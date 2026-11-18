//! WezTerm: a static reading of `wezterm.lua`, without running it.
//!
//! Literal assignments to the configuration table are converted. Anything computed,
//! conditional or unknown is reported as skipped with its line.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

use toml_edit::{InlineTable, Table, Value};

use crate::migrate::dirs;
use crate::migrate::lua::{self, Expr, Field, Stmt};
use crate::migrate::output::{self, ANSI_NAMES, Binding, Doc, Mods, Plan, Target, color};

/// WezTerm's configuration files, in the order it looks for them.
pub fn locations() -> Vec<PathBuf> {
    let mut locations = Vec::new();
    locations.extend(env::var_os("WEZTERM_CONFIG_FILE").map(PathBuf::from));

    #[cfg(not(windows))]
    {
        if let Some(config_home) = dirs::config_home() {
            locations.push(config_home.join("wezterm").join("wezterm.lua"));
        }
        for dir in dirs::config_dirs() {
            locations.push(dir.join("wezterm").join("wezterm.lua"));
        }
    }

    locations.extend(dirs::home().map(|home| home.join(".wezterm.lua")));
    locations
}

/// Plan the conversion of the WezTerm configuration at `path`.
pub fn plan(path: &Path, target: &Path) -> Result<Plan, String> {
    let source =
        std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let mut scheme_dirs = vec![path.parent().unwrap_or(Path::new("")).join("colors")];
    scheme_dirs.extend(dirs::config_home().map(|dir| dir.join("wezterm").join("colors")));
    let doc = convert(&source, &scheme_dirs).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(Plan::single("WezTerm", path, target, doc))
}

/// A setting of the configuration table, and its line.
struct Setting {
    key: String,
    value: Expr,
    line: usize,
}

fn convert(source: &str, scheme_dirs: &[PathBuf]) -> Result<Doc, String> {
    let statements = lua::parse(source)?;
    let mut doc = Doc::default();

    // Local names and what they hold, and the names of configuration tables.
    let mut locals: HashMap<String, Expr> = HashMap::new();
    let mut configs: HashSet<String> = HashSet::new();
    let mut settings: Vec<Setting> = Vec::new();

    for statement in statements {
        match statement {
            Stmt::Local { name, value, line } => {
                let value = resolve(&value, &locals);
                configs.remove(&name);
                match &value {
                    Expr::Call(path, _) if path == &["wezterm", "config_builder"] => {
                        configs.insert(name.clone());
                    },
                    Expr::Table(fields)
                        if fields.iter().all(|(field, _)| matches!(field, Field::Name(_))) =>
                    {
                        let is_config = fields
                            .iter()
                            .any(|(field, _)| matches!(field, Field::Name(key) if is_known(key)));
                        if fields.is_empty() || is_config {
                            configs.insert(name.clone());
                            settings.extend(table_settings(&value, line));
                        }
                    },
                    _ => (),
                }
                locals.insert(name, value);
            },
            Stmt::Assign { target, value, line } => {
                let value = resolve(&value, &locals);
                match target.split_first() {
                    Some((head, rest)) if configs.contains(head) && !rest.is_empty() => {
                        settings.push(nested(rest, value, line));
                    },
                    Some((head, [])) => {
                        locals.insert(head.clone(), value);
                    },
                    _ => doc.skip(
                        format!("line {line}: {}", target.join(".")),
                        "an assignment to something else than the configuration",
                    ),
                }
            },
            Stmt::Insert { target, value, line } => match target.split_first() {
                Some((head, rest)) if configs.contains(head) && !rest.is_empty() => {
                    let value = resolve(&value, &locals);
                    let list = Expr::Table(vec![(Field::Positional, value)]);
                    settings.push(nested(rest, list, line));
                },
                _ => doc.skip(format!("line {line}: table.insert"), "not into the configuration"),
            },
            Stmt::Return { value, line } => match &value {
                Expr::Name(path) if path.len() == 1 && configs.contains(&path[0]) => (),
                _ => match resolve(&value, &locals) {
                    table @ Expr::Table(_) => settings.extend(table_settings(&table, line)),
                    _ => doc.skip(format!("line {line}: return"), "returns something computed"),
                },
            },
            Stmt::Other { what, line } => {
                doc.skip(format!("line {line}"), format!("{what} isn't read"))
            },
        }
    }

    apply(&mut doc, settings, scheme_dirs);
    Ok(doc)
}

/// The settings of a literal configuration table.
fn table_settings(table: &Expr, line: usize) -> Vec<Setting> {
    let Expr::Table(fields) = table else { return Vec::new() };
    fields
        .iter()
        .filter_map(|(field, value)| match field {
            Field::Name(key) => Some(Setting { key: key.clone(), value: value.clone(), line }),
            _ => None,
        })
        .collect()
}

/// `config.a.b = value` as a setting of `a`, with the value nested in tables.
fn nested(path: &[String], value: Expr, line: usize) -> Setting {
    let value = path[1..]
        .iter()
        .rev()
        .fold(value, |value, key| Expr::Table(vec![(Field::Name(key.clone()), value)]));
    Setting { key: path[0].clone(), value, line }
}

/// Replace local names with what they hold.
fn resolve(expr: &Expr, locals: &HashMap<String, Expr>) -> Expr {
    let resolve_path = |path: &[String]| -> Option<Expr> {
        let (head, rest) = path.split_first()?;
        let base = locals.get(head)?;
        let base = match base {
            Expr::Call(path, args)
                if path == &["require"] && args.first() == Some(&Expr::Str("wezterm".into())) =>
            {
                Expr::Name(vec!["wezterm".into()])
            },
            base => base.clone(),
        };
        Some(rest.iter().fold(base, |base, field| match base {
            Expr::Name(mut path) => {
                path.push(field.clone());
                Expr::Name(path)
            },
            table @ Expr::Table(_) => table.get(field).cloned().unwrap_or(Expr::Nil),
            _ => Expr::Unknown,
        }))
    };

    match expr {
        Expr::Name(path) => resolve_path(path).unwrap_or_else(|| expr.clone()),
        Expr::Call(path, args) => {
            let args = args.iter().map(|arg| resolve(arg, locals)).collect();
            match resolve_path(path) {
                Some(Expr::Name(path)) => Expr::Call(path, args),
                Some(_) => Expr::Unknown,
                None if path == &["require"] && args == [Expr::Str("wezterm".into())] => {
                    Expr::Name(vec!["wezterm".into()])
                },
                None => Expr::Call(path.clone(), args),
            }
        },
        Expr::Table(fields) => Expr::Table(
            fields.iter().map(|(field, value)| (field.clone(), resolve(value, locals))).collect(),
        ),
        _ => expr.clone(),
    }
}

/// Settings that are converted, or deliberately skipped.
const KNOWN: &[&str] = &[
    "font",
    "font_size",
    "color_scheme",
    "color_schemes",
    "colors",
    "window_background_opacity",
    "kde_window_background_blur",
    "macos_window_background_blur",
    "win32_system_backdrop",
    "window_padding",
    "window_decorations",
    "integrated_buttons_alignment",
    "initial_cols",
    "initial_rows",
    "default_prog",
    "default_cwd",
    "set_environment_variables",
    "launch_menu",
    "ssh_domains",
    "default_cursor_style",
    "cursor_blink_rate",
    "visual_bell",
    "audible_bell",
    "scrollback_lines",
    "hide_tab_bar_if_only_one_tab",
    "enable_tab_bar",
    "tab_bar_at_bottom",
    "hide_mouse_cursor_when_typing",
    "automatically_reload_config",
    "leader",
    "keys",
    "key_tables",
    "disable_default_key_bindings",
    "mouse_bindings",
    "bold_brightens_ansi_colors",
];

fn is_known(key: &str) -> bool {
    KNOWN.contains(&key)
}

/// Convert the settings, in order; later settings of a key win.
fn apply(doc: &mut Doc, settings: Vec<Setting>, scheme_dirs: &[PathBuf]) {
    let mut schemes: BTreeMap<String, Expr> = BTreeMap::new();
    let mut scheme = None;
    let mut colors = Vec::new();
    let mut bindings = Vec::new();
    let mut table_options: BTreeMap<String, Expr> = BTreeMap::new();
    let mut key_tables = Vec::new();
    let mut padding = None;
    let mut decorations = None;
    let mut buttons = None;
    let mut dimensions = (None, None);
    let mut bell_color = None;

    doc.set("WezTerm's default keys", "keyboard.preset", "WezTerm");
    for setting in settings {
        let Setting { key, value, line } = setting;
        let from = key.as_str();
        let skip_from = format!("{key} (line {line})");
        if value == Expr::Unknown
            || matches!(&value, Expr::Call(path, _) if path.first().is_some_and(|head| head != "wezterm"))
        {
            doc.skip(skip_from, "computed; not read");
            continue;
        }

        match key.as_str() {
            "font" => font(doc, &skip_from, &value),
            "font_size" => number(doc, &skip_from, from, "font.size", &value),
            "color_scheme" => scheme = Some((value, line)),
            "color_schemes" => {
                if let Expr::Table(fields) = &value {
                    for (field, scheme) in fields {
                        let name = match field {
                            Field::Name(name) => name.clone(),
                            Field::Index(Expr::Str(name)) => name.clone(),
                            _ => continue,
                        };
                        schemes.insert(name, scheme.clone());
                    }
                }
            },
            "colors" => colors.push((value, line)),
            "window_background_opacity" => number(doc, &skip_from, from, "window.opacity", &value),
            "kde_window_background_blur" => {
                if value.as_bool() == Some(true) {
                    doc.set(from, "window.blur", true);
                }
            },
            "macos_window_background_blur" => {
                if value.as_f64().is_some_and(|radius| radius > 0.) {
                    doc.set(from, "window.blur", true);
                }
            },
            "win32_system_backdrop" => {
                if value
                    .as_str()
                    .is_some_and(|backdrop| backdrop != "Auto" && backdrop != "Disable")
                {
                    doc.set(from, "window.blur", true);
                }
            },
            "window_padding" => padding = Some((value, line)),
            "window_decorations" => decorations = Some((value, line)),
            "integrated_buttons_alignment" => buttons = value.as_str().map(ToOwned::to_owned),
            "initial_cols" => dimensions.0 = value.as_f64(),
            "initial_rows" => dimensions.1 = value.as_f64(),
            "default_prog" => match program(&value) {
                Some(shell) => doc.set(from, "terminal.shell", shell),
                None => doc.skip(skip_from, "not a list of strings"),
            },
            "default_cwd" => match value.as_str() {
                Some(cwd) => doc.set(from, "general.working_directory", cwd),
                None => doc.skip(skip_from, "not a string"),
            },
            "set_environment_variables" => {
                if let Expr::Table(fields) = &value {
                    for (field, value) in fields {
                        let name = match field {
                            Field::Name(name) => name.clone(),
                            Field::Index(Expr::Str(name)) => name.clone(),
                            _ => continue,
                        };
                        match value.as_str() {
                            Some(value) => {
                                doc.set_at(&format!("{from}.{name}"), &["env", &name], value)
                            },
                            None => {
                                doc.skip(format!("{from}.{name} (line {line})"), "not a string")
                            },
                        }
                    }
                }
            },
            "launch_menu" => launch_menu(doc, &value, line),
            "ssh_domains" => ssh_domains(doc, &value, line),
            "default_cursor_style" => match value.as_str() {
                Some(style) => cursor_style(doc, &skip_from, from, style),
                None => doc.skip(skip_from, "not a string"),
            },
            "cursor_blink_rate" => match value.as_f64() {
                Some(rate) if rate > 0. => doc.set(from, "cursor.blink_interval", rate as i64),
                Some(_) => doc.set(from, "cursor.style.blinking", "Never"),
                None => doc.skip(skip_from, "not a number"),
            },
            "visual_bell" => visual_bell(doc, &skip_from, &value),
            "audible_bell" => {
                if value.as_str() != Some("Disabled") {
                    doc.skip(
                        skip_from,
                        "TankTerm has no audible bell; bell.command can play a sound",
                    );
                }
            },
            "scrollback_lines" => match value.as_f64() {
                Some(lines) => doc.set(from, "scrolling.history", (lines as i64).clamp(0, 100_000)),
                None => doc.skip(skip_from, "not a number"),
            },
            "hide_tab_bar_if_only_one_tab" => {
                if value.as_bool() == Some(true) && !doc.is_set("tabs.show") {
                    doc.set(from, "tabs.show", "Multiple");
                }
            },
            "enable_tab_bar" => {
                if value.as_bool() == Some(false) {
                    doc.set(from, "tabs.show", "Never");
                }
            },
            "tab_bar_at_bottom" => {
                if value.as_bool() == Some(true) {
                    doc.set(from, "tabs.position", "Bottom");
                }
            },
            "hide_mouse_cursor_when_typing" => match value.as_bool() {
                Some(hide) => doc.set(from, "mouse.hide_when_typing", hide),
                None => doc.skip(skip_from, "not true or false"),
            },
            "automatically_reload_config" => match value.as_bool() {
                Some(reload) => doc.set(from, "general.live_config_reload", reload),
                None => doc.skip(skip_from, "not true or false"),
            },
            "bold_brightens_ansi_colors" => match &value {
                Expr::Bool(bright) => {
                    doc.set(from, "colors.draw_bold_text_with_bright_colors", *bright)
                },
                Expr::Str(mode) if mode == "BrightAndBold" => {
                    doc.set(from, "colors.draw_bold_text_with_bright_colors", true)
                },
                _ => doc.skip(skip_from, "TankTerm brightens bold text or doesn't"),
            },
            "leader" => leader(doc, &skip_from, &value),
            "keys" => {
                for (i, entry) in value.items().into_iter().enumerate() {
                    let entry_from = format!("keys[{}] (line {line})", i + 1);
                    match key_entry(entry, &mut table_options) {
                        Ok(binding) => bindings.push((entry_from, binding)),
                        Err(reason) => doc.skip(entry_from, reason),
                    }
                }
            },
            "key_tables" => key_tables.push((value, line)),
            "disable_default_key_bindings" => {
                if value.as_bool() == Some(true) {
                    doc.skip(
                        skip_from,
                        "TankTerm's own keys stay; unbind them with action = \"ReceiveChar\"",
                    );
                }
            },
            "mouse_bindings" => doc.skip(skip_from, "mouse bindings aren't converted"),
            _ => doc.skip(skip_from, "no TankTerm equivalent"),
        }
    }

    if let Some((value, line)) = scheme {
        color_scheme(doc, &value, line, &schemes, scheme_dirs);
    }
    for (value, line) in colors {
        scheme_colors(doc, "colors", &value, line, &mut bell_color);
    }
    if let Some(bell_color) = bell_color {
        doc.set("colors.visual_bell", "bell.color", bell_color);
    }
    if let Some((value, line)) = padding {
        window_padding(doc, &value, line);
    }
    if let Some((value, line)) = decorations {
        window_decorations(doc, &value, line, buttons.as_deref());
    }
    match dimensions {
        (Some(columns), Some(lines)) => {
            let mut table = InlineTable::new();
            table.insert("columns", (columns as i64).into());
            table.insert("lines", (lines as i64).into());
            doc.set("initial_cols/initial_rows", "window.dimensions", table);
        },
        (None, None) => (),
        _ => doc.skip("initial_cols/initial_rows", "TankTerm needs both"),
    }

    output::push_bindings(doc, Some("WezTerm"), bindings);
    for (value, line) in key_tables {
        tables(doc, &value, line, &table_options);
    }
}

fn number(doc: &mut Doc, skip_from: &str, from: &str, to: &str, value: &Expr) {
    match value.as_f64() {
        Some(n) => doc.set(from, to, n),
        None => doc.skip(skip_from, "not a number"),
    }
}

/// A WezTerm font weight as a word of a TankTerm font style.
fn weight(weight: &str) -> &str {
    match weight {
        "DemiLight" => "Light",
        "Book" => "Regular",
        "DemiBold" => "SemiBold",
        "ExtraBlack" => "Black",
        weight => weight,
    }
}

/// `wezterm.font(…)` and `wezterm.font_with_fallback{…}`.
fn font(doc: &mut Doc, skip_from: &str, value: &Expr) {
    let (family, attributes, fallbacks) = match value {
        Expr::Call(path, args) if path == &["wezterm", "font"] => {
            let (family, attributes) = font_args(args);
            (family, attributes, Vec::new())
        },
        Expr::Call(path, args) if path == &["wezterm", "font_with_fallback"] => {
            let fonts = args.first().map(Expr::items).unwrap_or_default();
            let first = fonts.first().map(|font| match font {
                Expr::Str(_) => font_args(std::slice::from_ref(*font)),
                table => (
                    table.get("family").and_then(Expr::as_str).map(ToOwned::to_owned),
                    Some((*table).clone()),
                ),
            });
            let fallbacks = fonts[1.min(fonts.len())..]
                .iter()
                .filter_map(|font| {
                    font.as_str().or_else(|| font.get("family").and_then(Expr::as_str))
                })
                .map(ToOwned::to_owned)
                .collect();
            let (family, attributes) = first.unwrap_or((None, None));
            (family, attributes, fallbacks)
        },
        _ => return doc.skip(skip_from, "not wezterm.font or wezterm.font_with_fallback"),
    };

    let Some(family) = family else {
        return doc.skip(skip_from, "no font family");
    };
    doc.set("font", "font.normal.family", family);

    let attributes = attributes.unwrap_or(Expr::Nil);
    let mut style = Vec::new();
    if let Some(weight_name) = attributes.get("weight").and_then(Expr::as_str) {
        style.push(weight(weight_name).to_owned());
    }
    if let Some(stretch) =
        attributes.get("stretch").and_then(Expr::as_str).filter(|s| *s != "Normal")
    {
        style.push(stretch.to_owned());
    }
    if attributes.get("italic").and_then(Expr::as_bool) == Some(true) {
        style.push("Italic".to_owned());
    }
    if !style.is_empty() {
        doc.set("font", "font.normal.style", style.join(" "));
    }
    if !fallbacks.is_empty() {
        doc.skip(
            format!("font fallbacks {}", fallbacks.join(", ")),
            "TankTerm has no fallback fonts",
        );
    }
}

/// The family and attributes of `wezterm.font('X', { … })` or `wezterm.font { family = 'X' }`.
fn font_args(args: &[Expr]) -> (Option<String>, Option<Expr>) {
    match args.first() {
        Some(Expr::Str(family)) => (Some(family.clone()), args.get(1).cloned()),
        Some(table @ Expr::Table(_)) => {
            (table.get("family").and_then(Expr::as_str).map(ToOwned::to_owned), Some(table.clone()))
        },
        _ => (None, None),
    }
}

fn cursor_style(doc: &mut Doc, skip_from: &str, from: &str, style: &str) {
    let (blinking, shape) = if let Some(shape) = style.strip_prefix("Blinking") {
        ("On", shape)
    } else if let Some(shape) = style.strip_prefix("Steady") {
        ("Off", shape)
    } else {
        return doc.skip(skip_from, "unknown cursor style");
    };
    let shape = match shape {
        "Block" => "Block",
        "Underline" => "Underline",
        "Bar" => "Beam",
        _ => return doc.skip(skip_from, "unknown cursor style"),
    };
    doc.set(from, "cursor.style.shape", shape);
    doc.set(from, "cursor.style.blinking", blinking);
}

fn visual_bell(doc: &mut Doc, skip_from: &str, value: &Expr) {
    let fade_in = value.get("fade_in_duration_ms").and_then(Expr::as_f64).unwrap_or(0.);
    let fade_out = value.get("fade_out_duration_ms").and_then(Expr::as_f64).unwrap_or(0.);
    let duration = (fade_in + fade_out) as i64;
    if duration > 0 {
        doc.set("visual_bell", "bell.duration", duration);
    }
    match value.get("fade_out_function").and_then(Expr::as_str) {
        Some(function @ ("Linear" | "Ease" | "EaseOut")) => {
            doc.set("visual_bell.fade_out_function", "bell.animation", function)
        },
        Some(_) => {
            doc.skip(format!("{skip_from} fade_out_function"), "TankTerm has no such animation")
        },
        None => (),
    }
    if value.get("target").and_then(Expr::as_str) == Some("CursorColor") {
        doc.skip(format!("{skip_from} target"), "TankTerm flashes the whole pane");
    }
}

/// A list of strings as a TankTerm shell.
fn program(value: &Expr) -> Option<Value> {
    let words: Option<Vec<String>> =
        value.items().into_iter().map(|item| item.as_str().map(ToOwned::to_owned)).collect();
    let words = words?;
    let (program, args) = words.split_first()?;
    Some(output::program(program, args))
}

fn launch_menu(doc: &mut Doc, value: &Expr, line: usize) {
    for (i, item) in value.items().into_iter().enumerate() {
        let from = format!("launch_menu[{}] (line {line})", i + 1);
        let shell = item.get("args").and_then(program);
        let name = item.get("label").and_then(Expr::as_str).map(ToOwned::to_owned).or_else(|| {
            item.get("args").and_then(|args| {
                args.items().first().and_then(|arg| arg.as_str()).map(ToOwned::to_owned)
            })
        });
        let Some(name) = name else {
            doc.skip(from, "no label or args");
            continue;
        };

        let mut profile = Table::new();
        profile.insert("name", toml_edit::value(name));
        if let Some(shell) = shell {
            profile.insert("shell", toml_edit::value(shell));
        }
        if let Some(cwd) = item.get("cwd").and_then(Expr::as_str) {
            profile.insert("working_directory", toml_edit::value(cwd));
        }
        if let Some(Expr::Table(fields)) = item.get("set_environment_variables") {
            let mut env = InlineTable::new();
            for (field, value) in fields {
                if let (Field::Name(name), Some(value)) = (field, value.as_str()) {
                    env.insert(name, value.into());
                }
            }
            profile.insert("env", toml_edit::value(env));
        }
        doc.push_profile(&from, profile);
    }
}

fn ssh_domains(doc: &mut Doc, value: &Expr, line: usize) {
    for (i, domain) in value.items().into_iter().enumerate() {
        let from = format!("ssh_domains[{}] (line {line})", i + 1);
        let (Some(name), Some(address)) = (
            domain.get("name").and_then(Expr::as_str),
            domain.get("remote_address").and_then(Expr::as_str),
        ) else {
            doc.skip(from, "no name or remote_address");
            continue;
        };

        let mut args = Vec::new();
        let host = match address.rsplit_once(':') {
            Some((host, port))
                if port.chars().all(|c| c.is_ascii_digit()) && !host.contains(':') =>
            {
                args.extend(["-p".to_owned(), port.to_owned()]);
                host
            },
            _ => address,
        };
        args.push(match domain.get("username").and_then(Expr::as_str) {
            Some(user) => format!("{user}@{host}"),
            None => host.to_owned(),
        });

        let mut profile = Table::new();
        profile.insert("name", toml_edit::value(name));
        profile.insert("shell", toml_edit::value(output::program("ssh", &args)));
        doc.push_profile(&from, profile);
    }
}

/// A color scheme by name: from `color_schemes`, a scheme file, or a built-in TankTerm theme.
fn color_scheme(
    doc: &mut Doc,
    value: &Expr,
    line: usize,
    schemes: &BTreeMap<String, Expr>,
    scheme_dirs: &[PathBuf],
) {
    let Some(name) = value.as_str() else {
        return doc.skip(format!("color_scheme (line {line})"), "computed; not read");
    };
    let from = format!("color_scheme {name}");

    if let Some(scheme) = schemes.get(name) {
        let mut bell_color = None;
        scheme_colors(doc, &from, scheme, line, &mut bell_color);
        return;
    }

    if let Some(colors) = scheme_file(name, scheme_dirs) {
        let mut bell_color = None;
        scheme_colors(doc, &from, &colors, line, &mut bell_color);
        return;
    }

    let theme = match name {
        "Dracula" | "Dracula (Official)" | "Dracula (base16)" | "Dracula (Gogh)" => "dracula",
        "Catppuccin Mocha" | "Catppuccin Mocha (Gogh)" => "catppuccin",
        "Default Dark (base16)" | "Base16 Default Dark (base16)" => "base16",
        _ => {
            return doc.skip(
                format!("{from} (line {line})"),
                "a WezTerm built-in scheme; TankTerm doesn't have its colors",
            );
        },
    };
    doc.set(&from, "theme", theme);
}

/// The `[colors]` of a WezTerm scheme file named `name`, as a Lua-like table.
fn scheme_file(name: &str, scheme_dirs: &[PathBuf]) -> Option<Expr> {
    for dir in scheme_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "toml") {
                continue;
            }
            let Some(scheme) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| text.parse::<toml::Table>().ok())
            else {
                continue;
            };
            let named = scheme
                .get("metadata")
                .and_then(|meta| meta.get("name"))
                .and_then(toml::Value::as_str)
                == Some(name);
            let stem = path.file_stem().is_some_and(|stem| stem == name);
            if named || stem {
                return scheme.get("colors").map(toml_to_expr);
            }
        }
    }
    None
}

fn toml_to_expr(value: &toml::Value) -> Expr {
    match value {
        toml::Value::String(s) => Expr::Str(s.clone()),
        toml::Value::Integer(i) => Expr::Number(*i as f64),
        toml::Value::Float(f) => Expr::Number(*f),
        toml::Value::Boolean(b) => Expr::Bool(*b),
        toml::Value::Array(items) => {
            Expr::Table(items.iter().map(|item| (Field::Positional, toml_to_expr(item))).collect())
        },
        toml::Value::Table(table) => Expr::Table(
            table
                .iter()
                .map(|(key, value)| (Field::Name(key.clone()), toml_to_expr(value)))
                .collect(),
        ),
        toml::Value::Datetime(_) => Expr::Unknown,
    }
}

/// The colors of a `colors` table or a scheme.
fn scheme_colors(
    doc: &mut Doc,
    from: &str,
    colors: &Expr,
    line: usize,
    bell_color: &mut Option<String>,
) {
    let Expr::Table(fields) = colors else {
        return doc.skip(format!("{from} (line {line})"), "not a table");
    };
    for (field, value) in fields {
        let Field::Name(key) = field else { continue };
        let key_from = format!("{from}.{key}");
        let to = match key.as_str() {
            "foreground" => "colors.primary.foreground",
            "background" => "colors.primary.background",
            "cursor_bg" => "colors.cursor.cursor",
            "cursor_fg" => "colors.cursor.text",
            "selection_fg" => "colors.selection.text",
            "selection_bg" => "colors.selection.background",
            "ansi" | "brights" => {
                let table = if key == "ansi" { "normal" } else { "bright" };
                for (i, item) in value.items().into_iter().enumerate().take(8) {
                    match item.as_str().and_then(color) {
                        Some(c) => doc.set(
                            &format!("{key_from}[{}]", i + 1),
                            &format!("colors.{table}.{}", ANSI_NAMES[i]),
                            c,
                        ),
                        None => {
                            doc.skip(format!("{key_from}[{}] (line {line})", i + 1), "not a color")
                        },
                    }
                }
                continue;
            },
            "indexed" => {
                if let Expr::Table(entries) = value {
                    for (index, value) in entries {
                        let (Field::Index(Expr::Number(index)), Some(c)) =
                            (index, value.as_str().and_then(color))
                        else {
                            continue;
                        };
                        let mut entry = InlineTable::new();
                        entry.insert("index", (*index as i64).into());
                        entry.insert("color", c.into());
                        let colors = doc.doc["colors"].or_insert(toml_edit::table());
                        if let Some(colors) = colors.as_table_mut() {
                            colors.set_implicit(true);
                            let array = colors
                                .entry("indexed_colors")
                                .or_insert(toml_edit::value(toml_edit::Array::new()));
                            if let Some(array) = array.as_array_mut() {
                                array.push(entry);
                            }
                        }
                        doc.report.convert(format!("{key_from}[{index}]"), "colors.indexed_colors");
                    }
                }
                continue;
            },
            "visual_bell" => {
                *bell_color = value.as_str().and_then(color);
                continue;
            },
            _ => {
                doc.skip(format!("{key_from} (line {line})"), "no TankTerm equivalent");
                continue;
            },
        };
        match value.as_str().and_then(color) {
            Some(c) => doc.set(&key_from, to, c),
            None => doc.skip(format!("{key_from} (line {line})"), "not a color"),
        }
    }
}

/// `window_padding` with sizes like `8`, `"8px"` or `"1cell"`.
fn window_padding(doc: &mut Doc, value: &Expr, line: usize) {
    enum Size {
        Pixels(i64),
        Cells(i64),
    }
    let size = |side: &str| -> Option<Size> {
        match value.get(side) {
            None => Some(Size::Pixels(0)),
            Some(Expr::Number(n)) => Some(Size::Pixels(*n as i64)),
            Some(Expr::Str(s)) => {
                if let Some(px) = s.strip_suffix("px") {
                    px.trim().parse::<f64>().ok().map(|px| Size::Pixels(px as i64))
                } else if let Some(cells) = s.strip_suffix("cell") {
                    cells
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|c| c.fract() == 0.)
                        .map(|c| Size::Cells(c as i64))
                } else {
                    s.trim().parse::<f64>().ok().map(|px| Size::Pixels(px as i64))
                }
            },
            _ => None,
        }
    };
    let skip_from = format!("window_padding (line {line})");
    let sides: Option<Vec<Size>> =
        ["top", "right", "bottom", "left"].iter().map(|side| size(side)).collect();
    let Some(sides) = sides else {
        return doc.skip(skip_from, "sizes in percent or fractions of cells aren't converted");
    };

    match sides.as_slice() {
        [Size::Pixels(top), Size::Pixels(right), Size::Pixels(bottom), Size::Pixels(left)] => {
            if top != bottom || left != right {
                return doc
                    .skip(skip_from, "TankTerm's padding in pixels is the same on opposite sides");
            }
            let mut padding = InlineTable::new();
            padding.insert("x", (*left).into());
            padding.insert("y", (*top).into());
            doc.set("window_padding", "window.padding", padding);
        },
        sides if sides.iter().all(|side| matches!(side, Size::Cells(_) | Size::Pixels(0))) => {
            let cells: toml_edit::Array = sides
                .iter()
                .map(|side| match side {
                    Size::Cells(cells) => *cells,
                    Size::Pixels(_) => 0,
                })
                .collect();
            doc.set("window_padding", "window.padding", cells);
        },
        _ => doc.skip(skip_from, "TankTerm's padding is all in pixels or all in cells"),
    }
}

fn window_decorations(doc: &mut Doc, value: &Expr, line: usize, alignment: Option<&str>) {
    let Some(flags) = value.as_str() else {
        return doc.skip(format!("window_decorations (line {line})"), "computed; not read");
    };
    let flags: Vec<&str> = flags.split('|').map(str::trim).collect();
    if flags.contains(&"TITLE") {
        return;
    }
    doc.set("window_decorations", "window.decorations", "None");
    let buttons = if flags.contains(&"INTEGRATED_BUTTONS") {
        match alignment {
            Some("Left") => "Left",
            Some("Right") => "Right",
            _ => "Desktop",
        }
    } else {
        "None"
    };
    doc.set("window_decorations", "window.buttons", buttons);
}

fn leader(doc: &mut Doc, skip_from: &str, value: &Expr) {
    let Some(key) = value.get("key").and_then(Expr::as_str).and_then(key_name) else {
        return doc.skip(skip_from, "no key TankTerm knows");
    };
    let (mods, leader) = match value.get("mods").and_then(Expr::as_str).map(mods) {
        Some(Some(parsed)) => parsed,
        Some(None) => return doc.skip(skip_from, "unknown modifier"),
        None => (Mods::default(), false),
    };
    let mut table = InlineTable::new();
    table.insert("key", key.into());
    let mods = mods.to_tankterm();
    if !mods.is_empty() && !leader {
        table.insert("mods", mods.into());
    }
    if let Some(timeout) = value.get("timeout_milliseconds").and_then(Expr::as_f64) {
        table.insert("timeout", (timeout as i64).into());
    }
    doc.set("leader", "keyboard.leader", table);
}

/// WezTerm's modifiers, and whether `LEADER` is one of them.
fn mods(mods: &str) -> Option<(Mods, bool)> {
    let mut parsed = Mods::default();
    let mut leader = false;
    for name in mods.split('|').map(str::trim) {
        match name {
            "" | "NONE" => (),
            "LEADER" => leader = true,
            "SUPER" | "CMD" | "WIN" => parsed.command = true,
            "META" | "OPT" | "ALT" => parsed.alt = true,
            name => {
                if !parsed.add(name) {
                    return None;
                }
            },
        }
    }
    Some((parsed, leader))
}

/// A WezTerm key name as TankTerm's.
fn key_name(key: &str) -> Option<String> {
    let key = key.strip_prefix("phys:").or_else(|| key.strip_prefix("mapped:")).unwrap_or(key);
    Some(match key {
        "LeftArrow" => "ArrowLeft".into(),
        "RightArrow" => "ArrowRight".into(),
        "UpArrow" => "ArrowUp".into(),
        "DownArrow" => "ArrowDown".into(),
        "Return" => "Enter".into(),
        " " => "Space".into(),
        "Enter" | "Tab" | "Escape" | "Backspace" | "Delete" | "Insert" | "Home" | "End"
        | "PageUp" | "PageDown" | "Space" => key.into(),
        _ if key.chars().count() == 1 => key.to_uppercase(),
        _ if key.starts_with('F') && key[1..].parse::<u8>().is_ok() => key.into(),
        _ if key.starts_with("Numpad") && key[6..].parse::<u8>().is_ok() => key.into(),
        _ => return None,
    })
}

/// A `keys` entry as a binding.
fn key_entry(entry: &Expr, table_options: &mut BTreeMap<String, Expr>) -> Result<Binding, String> {
    let key = entry.get("key").and_then(Expr::as_str).ok_or("no key")?;
    let key = key_name(key).ok_or_else(|| format!("TankTerm doesn't know the key {key:?}"))?;
    let (mods, leader) = match entry.get("mods").and_then(Expr::as_str) {
        Some(names) => mods(names).ok_or("unknown modifier")?,
        None => (Mods::default(), false),
    };
    let action = entry.get("action").ok_or("no action")?;
    let target = action_target(action, table_options)?;
    Ok(Binding { key, mods, leader, target })
}

/// A WezTerm action as a TankTerm action, chars or key table.
fn action_target(
    action: &Expr,
    table_options: &mut BTreeMap<String, Expr>,
) -> Result<Target, String> {
    let (name, arg) = match action {
        // A plain name, as older configurations write actions without arguments.
        Expr::Str(name) => (name.as_str(), Expr::Nil),
        Expr::Name(path) if path.len() == 3 && path[..2] == ["wezterm", "action"] => {
            (path[2].as_str(), Expr::Nil)
        },
        Expr::Call(path, args) if path.len() == 3 && path[..2] == ["wezterm", "action"] => {
            (path[2].as_str(), args.first().cloned().unwrap_or(Expr::Nil))
        },
        // The old `wezterm.action { SpawnTab = 'CurrentPaneDomain' }` form.
        Expr::Call(path, args) if path == &["wezterm", "action"] => match args.first() {
            Some(Expr::Table(fields)) => match fields.first() {
                Some((Field::Name(name), arg)) => {
                    return action_target(
                        &Expr::Call(vec!["wezterm".into(), "action".into(), name.clone()], vec![
                            arg.clone(),
                        ]),
                        table_options,
                    );
                },
                Some((Field::Positional, Expr::Str(name))) => {
                    return action_target(
                        &Expr::Name(vec!["wezterm".into(), "action".into(), name.clone()]),
                        table_options,
                    );
                },
                _ => return Err("can't read the action".into()),
            },
            _ => return Err("can't read the action".into()),
        },
        Expr::Call(path, _) if path == &["wezterm", "action_callback"] => {
            return Err("a Lua callback; not read".into());
        },
        _ => return Err("computed; not read".into()),
    };

    let n = arg.as_f64();
    let s = arg.as_str();
    let action = match name {
        "SpawnTab" => "CreateNewTab",
        "SpawnWindow" => "CreateNewWindow",
        "CloseCurrentTab" => "CloseTab",
        "CloseCurrentPane" => "ClosePane",
        "ActivateTabRelative" | "ActivateTabRelativeNoWrap" => match n {
            Some(1.) => "SelectNextTab",
            Some(-1.) => "SelectPreviousTab",
            _ => return Err("TankTerm moves one tab at a time".into()),
        },
        "ActivateTab" => match n {
            Some(-1.) => "SelectLastTab",
            Some(n) if (0. ..=8.).contains(&n) && n.fract() == 0. => [
                "SelectTab1",
                "SelectTab2",
                "SelectTab3",
                "SelectTab4",
                "SelectTab5",
                "SelectTab6",
                "SelectTab7",
                "SelectTab8",
                "SelectTab9",
            ][n as usize],
            _ => return Err("TankTerm selects tabs 1 to 9 and the last one".into()),
        },
        "MoveTabRelative" => match n {
            Some(-1.) => "MoveTabLeft",
            Some(1.) => "MoveTabRight",
            _ => return Err("TankTerm moves tabs by one place".into()),
        },
        "SplitHorizontal" => "SplitRight",
        "SplitVertical" => "SplitDown",
        "SplitPane" => match arg.get("direction").and_then(Expr::as_str) {
            Some("Right") => "SplitRight",
            Some("Down") => "SplitDown",
            _ => return Err("TankTerm splits to the right and down only".into()),
        },
        "ActivatePaneDirection" => match s {
            Some("Left") => "FocusPaneLeft",
            Some("Right") => "FocusPaneRight",
            Some("Up") => "FocusPaneUp",
            Some("Down") => "FocusPaneDown",
            Some("Next") => "FocusNextPane",
            Some("Prev") => "FocusPreviousPane",
            _ => return Err("unknown direction".into()),
        },
        "AdjustPaneSize" => match arg.items().first().and_then(|dir| dir.as_str()) {
            Some("Left") => "ResizePaneLeft",
            Some("Right") => "ResizePaneRight",
            Some("Up") => "ResizePaneUp",
            Some("Down") => "ResizePaneDown",
            _ => return Err("unknown direction".into()),
        },
        "TogglePaneZoomState" => "TogglePaneZoom",
        "PaneSelect" => match arg.get("mode").and_then(Expr::as_str) {
            None | Some("Activate") => "PaneSelect",
            Some("SwapWithActive" | "SwapWithActiveKeepFocus") => "PaneSelectSwap",
            _ => return Err("TankTerm selects or swaps panes".into()),
        },
        "RotatePanes" => match s {
            Some("Clockwise") => "RotatePanesClockwise",
            Some("CounterClockwise") => "RotatePanesCounterClockwise",
            _ => return Err("unknown direction".into()),
        },
        "Copy" => "Copy",
        "Paste" => "Paste",
        "PastePrimarySelection" => "PasteSelection",
        "CopyTo" => match s {
            Some("PrimarySelection") => "CopySelection",
            _ => "Copy",
        },
        "PasteFrom" => match s {
            Some("PrimarySelection") => "PasteSelection",
            _ => "Paste",
        },
        "IncreaseFontSize" => "IncreaseFontSize",
        "DecreaseFontSize" => "DecreaseFontSize",
        "ResetFontSize" => "ResetFontSize",
        "ScrollByPage" => match n {
            Some(-1.) => "ScrollPageUp",
            Some(1.) => "ScrollPageDown",
            Some(-0.5) => "ScrollHalfPageUp",
            Some(0.5) => "ScrollHalfPageDown",
            _ => return Err("TankTerm scrolls by whole and half pages".into()),
        },
        "ScrollByLine" => match n {
            Some(-1.) => "ScrollLineUp",
            Some(1.) => "ScrollLineDown",
            _ => return Err("TankTerm scrolls by one line".into()),
        },
        "ScrollToTop" => "ScrollToTop",
        "ScrollToBottom" => "ScrollToBottom",
        "ScrollToPrompt" => match n {
            Some(n) if n < 0. => "ScrollToPreviousPrompt",
            Some(n) if n > 0. => "ScrollToNextPrompt",
            _ => return Err("not a number of prompts".into()),
        },
        "ClearScrollback" => "ClearHistory",
        "Search" => "SearchForward",
        "ActivateCopyMode" => "ToggleViMode",
        "ToggleFullScreen" => "ToggleFullscreen",
        "Hide" | "HideApplication" => "Hide",
        "QuitApplication" => "Quit",
        "ActivateCommandPalette" => "ShowCommandPalette",
        "ShowLauncher" => "ShowProfiles",
        "ShowLauncherArgs" => match arg.get("flags").and_then(Expr::as_str) {
            Some(flags) if flags.contains("TABS") && !flags.contains("LAUNCH_MENU_ITEMS") => {
                "ShowTabNavigator"
            },
            _ => "ShowProfiles",
        },
        "ShowTabNavigator" => "ShowTabNavigator",
        "PopKeyTable" => "PopKeyTable",
        "ClearKeyTableStack" => "ClearKeyTableStack",
        "Nop" => "None",
        "DisableDefaultAssignment" => "ReceiveChar",
        "SendString" => {
            return s.map(|s| Target::Chars(s.to_owned())).ok_or_else(|| "not a string".into());
        },
        "ActivateKeyTable" => {
            let name = arg.get("name").and_then(Expr::as_str).ok_or("no table name")?;
            table_options.entry(name.to_owned()).or_insert_with(|| arg.clone());
            return Ok(Target::Table(name.to_owned()));
        },
        "ActivateLastTab" => return Err("TankTerm has no action for the tab shown before".into()),
        "ReloadConfiguration" => {
            return Err("TankTerm reads its configuration again by itself".into());
        },
        _ => return Err(format!("no TankTerm action for {name}")),
    };
    Ok(Target::Action(action))
}

/// `key_tables` as `[keyboard.tables]`.
fn tables(doc: &mut Doc, value: &Expr, line: usize, options: &BTreeMap<String, Expr>) {
    let Expr::Table(fields) = value else { return };
    for (field, entries) in fields {
        let name = match field {
            Field::Name(name) => name.clone(),
            Field::Index(Expr::Str(name)) => name.clone(),
            _ => continue,
        };
        let mut nested_options = BTreeMap::new();
        for (i, entry) in entries.items().into_iter().enumerate() {
            let from = format!("key_tables.{name}[{}] (line {line})", i + 1);
            match key_entry(entry, &mut nested_options) {
                Ok(binding) => doc.push_binding(
                    &from,
                    &["keyboard", "tables", &name, "bindings"],
                    binding.to_inline_table(),
                ),
                Err(reason) => doc.skip(from, reason),
            }
        }

        // WezTerm sets these where a binding activates the table, TankTerm on the table.
        if let Some(arg) = options.get(&name) {
            for (wezterm, tankterm) in [
                ("one_shot", "one_shot"),
                ("until_unknown", "until_unknown"),
                ("prevent_fallback", "prevent_fallback"),
            ] {
                if let Some(value) = arg.get(wezterm).and_then(Expr::as_bool) {
                    doc.set_at(
                        &format!("ActivateKeyTable {name} {wezterm}"),
                        &["keyboard", "tables", &name, tankterm],
                        value,
                    );
                }
            }
            if let Some(timeout) = arg.get("timeout_milliseconds").and_then(Expr::as_f64) {
                doc.set_at(
                    &format!("ActivateKeyTable {name} timeout_milliseconds"),
                    &["keyboard", "tables", &name, "timeout"],
                    timeout as i64,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use crate::color::Rgb;
    use crate::config::UiConfig;

    const CONFIG: &str = r#"
local wezterm = require 'wezterm'
local act = wezterm.action
local config = wezterm.config_builder()
local size = 13

config.font = wezterm.font_with_fallback { { family = 'JetBrains Mono', weight = 'DemiBold' }, 'Noto Color Emoji' }
config.font_size = size
config.color_scheme = 'Dracula (Official)'
config.colors = {
  foreground = '#c0c0c0',
  ansi = { 'black', 'maroon', 'green', 'olive', 'navy', 'purple', 'teal', 'silver' },
  indexed = { [136] = '#af8700' },
  tab_bar = { background = '#000000' },
}
config.window_background_opacity = 0.85
config.window_padding = { left = 4, right = 4, top = 2, bottom = 2 }
config.window_decorations = 'RESIZE'
config.default_cursor_style = 'BlinkingBar'
config.hide_tab_bar_if_only_one_tab = true
config.default_prog = { '/bin/zsh', '-l' }
config.launch_menu = { { label = 'htop', args = { 'htop' } } }
config.leader = { key = 'a', mods = 'CTRL', timeout_milliseconds = 1000 }
config.front_end = 'WebGpu'
config.keys = {
  { key = 't', mods = 'CTRL|SHIFT', action = act.SpawnTab 'CurrentPaneDomain' },
  { key = '|', mods = 'LEADER|SHIFT', action = act.SplitHorizontal { domain = 'CurrentPaneDomain' } },
  { key = 'r', mods = 'LEADER', action = act.ActivateKeyTable { name = 'resize', one_shot = false } },
  { key = 'k', mods = 'CMD', action = act.ClearScrollback 'ScrollbackAndViewport' },
  { key = 'x', mods = 'ALT', action = wezterm.action_callback(function(win, pane) end) },
  { key = 'F1', action = wezterm.action { SendString = 'hi' } },
}
config.key_tables = {
  resize = {
    { key = 'LeftArrow', action = act.AdjustPaneSize { 'Left', 1 } },
    { key = 'Escape', action = 'PopKeyTable' },
  },
}
if wezterm.target_triple == 'x86_64-pc-windows-msvc' then
  config.default_prog = { 'pwsh.exe' }
end
config.scrollback_lines = 5000 * 2
return config
"#;

    #[test]
    fn converts_settings() {
        let doc = convert(CONFIG, &[]).unwrap();
        let toml = doc.doc.to_string();
        let config: UiConfig = toml::from_str(&toml).unwrap();

        assert_eq!(config.font.normal().family, "JetBrains Mono");
        assert!(toml.contains("style = \"SemiBold\""), "{toml}");
        assert_eq!(config.font.size(), 13.0);
        assert!(toml.contains("theme = \"dracula\""), "{toml}");
        assert_eq!(config.colors.primary.foreground, Rgb::new(0xc0, 0xc0, 0xc0));
        assert_eq!(config.colors.normal.blue, Rgb::new(0, 0, 0x80));
        assert!(toml.contains("indexed_colors = [{ index = 136, color = \"#af8700\" }]"), "{toml}");
        assert_eq!(config.window_opacity(), 0.85);
        assert!(toml.contains("padding = { x = 4, y = 2 }"), "{toml}");
        assert!(
            toml.contains("decorations = \"None\"") && toml.contains("buttons = \"None\""),
            "{toml}"
        );
        assert!(toml.contains("shape = \"Beam\"") && toml.contains("blinking = \"On\""), "{toml}");
        assert!(toml.contains("show = \"Multiple\""), "{toml}");
        assert!(toml.contains("shell = { program = \"/bin/zsh\", args = [\"-l\"] }"), "{toml}");
        assert_eq!(config.profiles[0].name, "htop");
        assert!(
            toml.contains("leader = { key = \"A\", mods = \"Control\", timeout = 1000 }"),
            "{toml}"
        );

        let skipped: Vec<_> = doc.report.skipped.iter().map(|(key, _)| key.as_str()).collect();
        assert!(!skipped.iter().any(|key| key.contains("return")), "{skipped:?}");
        for expected in [
            "font fallbacks Noto Color Emoji",
            "colors.tab_bar (line 10)",
            "front_end (line 24)",
            "keys[5] (line 25)",
            "line 39",
            "scrollback_lines (line 42)",
        ] {
            assert!(skipped.contains(&expected), "{expected} not in {skipped:?}");
        }
    }

    #[test]
    fn converts_keys() {
        let doc = convert(CONFIG, &[]).unwrap();
        let toml = doc.doc.to_string();

        // Ctrl+Shift+T is the WezTerm preset's key already.
        assert!(!toml.contains("CreateNewTab"), "{toml}");
        assert!(
            toml.contains(
                "{ key = \"|\", mods = \"Shift\", mode = \"Leader\", action = \"SplitRight\" }"
            ),
            "{toml}"
        );
        assert!(toml.contains("{ key = \"R\", mode = \"Leader\", table = \"resize\" }"), "{toml}");
        assert!(
            toml.contains("{ key = \"K\", mods = \"Super\", action = \"ClearHistory\" }"),
            "{toml}"
        );
        assert!(toml.contains("{ key = \"F1\", chars = \"hi\" }"), "{toml}");
        assert!(toml.contains("preset = \"WezTerm\""), "{toml}");
        assert!(toml.contains("[keyboard.tables.resize]"), "{toml}");
        assert!(toml.contains("one_shot = false"), "{toml}");
        assert!(toml.contains("{ key = \"ArrowLeft\", action = \"ResizePaneLeft\" }"), "{toml}");

        let config: UiConfig = toml::from_str(&toml).unwrap();
        assert_eq!(config.key_tables()["resize"].bindings.len(), 2);
        assert!(!config.key_tables()["resize"].one_shot);
    }

    #[test]
    fn return_table_and_scheme_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("mine.toml"),
            "[colors]\nbackground = '#101010'\nbrights = ['#111111']\n\n[metadata]\nname = 'My \
             Scheme'\n",
        )
        .unwrap();

        let doc = convert(
            "local wezterm = require 'wezterm'\nreturn {\n  color_scheme = 'My Scheme',\n  \
             font_size = 10,\n}\n",
            &[dir.path().to_owned()],
        )
        .unwrap();
        let config: UiConfig = toml::from_str(&doc.doc.to_string()).unwrap();

        assert_eq!(config.colors.primary.background, Rgb::new(0x10, 0x10, 0x10));
        assert_eq!(config.colors.bright.black, Rgb::new(0x11, 0x11, 0x11));
        assert_eq!(config.font.size(), 10.0);
    }

    #[test]
    fn unknown_scheme_is_skipped() {
        let doc = convert("return { color_scheme = 'Batman' }", &[]).unwrap();
        assert_eq!(doc.report.skipped.len(), 1);
    }
}
