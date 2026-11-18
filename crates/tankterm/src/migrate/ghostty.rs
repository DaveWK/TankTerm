//! Ghostty: `key = value` files with `config-file` includes and themes.

use std::collections::{BTreeMap, HashSet};
use std::env;
use std::path::{Path, PathBuf};

use toml_edit::{InlineTable, Value};

use crate::migrate::dirs;
use crate::migrate::output::{self, Binding, Doc, Mods, Plan, Target, ansi_key, color};

/// Ghostty's configuration files, in the order it reads them.
#[cfg_attr(windows, allow(unused_mut))]
pub fn locations() -> Vec<PathBuf> {
    let mut locations = Vec::new();

    #[cfg(not(windows))]
    if let Some(config_home) = dirs::config_home() {
        locations.push(config_home.join("ghostty").join("config"));
        locations.push(config_home.join("ghostty").join("config.ghostty"));
    }

    #[cfg(target_os = "macos")]
    if let Some(app_support) = dirs::app_support() {
        locations.push(app_support.join("com.mitchellh.ghostty").join("config"));
        locations.push(app_support.join("com.mitchellh.ghostty").join("config.ghostty"));
    }

    locations
}

/// Plan the conversion of Ghostty's configuration at `path`.
///
/// When the file was found in Ghostty's own locations, the other files there are read too,
/// in Ghostty's order, as Ghostty does.
pub fn plan(path: &Path, target: &Path, detected: bool) -> Result<Plan, String> {
    let files: Vec<PathBuf> = if detected {
        locations().into_iter().filter(|path| path.is_file()).collect()
    } else {
        vec![path.to_owned()]
    };

    let mut lines = Vec::new();
    let mut seen = HashSet::new();
    for file in &files {
        read(file, true, &mut lines, &mut seen)?;
    }

    let doc = convert(&lines, &theme_dirs());
    Ok(Plan::single("Ghostty", path, target, doc))
}

/// A `key = value` line, and where it came from.
#[derive(Clone, Debug)]
struct Line {
    key: String,
    value: String,
    /// `file:line`, for the report.
    location: String,
}

/// Read a file's lines, followed by those of the files it includes with `config-file`.
fn read(
    path: &Path,
    required: bool,
    lines: &mut Vec<Line>,
    seen: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    if !seen.insert(path.to_owned()) {
        return Ok(());
    }
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(_) if !required => return Ok(()),
        Err(err) => return Err(format!("{}: {err}", path.display())),
    };

    let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let mut includes = Vec::new();
    for line in parse(&contents, &name) {
        if line.key == "config-file" {
            let (optional, file) = match line.value.strip_prefix('?') {
                Some(file) => (true, file),
                None => (false, line.value.as_str()),
            };
            let file = expand_home(file);
            let file = if file.is_relative() {
                path.parent().unwrap_or(Path::new("")).join(file)
            } else {
                file
            };
            includes.push((file, !optional));
        } else {
            lines.push(line);
        }
    }

    // Included files load after the whole file that includes them.
    for (file, required) in includes {
        read(&file, required, lines, seen)?;
    }
    Ok(())
}

/// The `key = value` lines of a file.
fn parse(contents: &str, name: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    for (i, line) in contents.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=').unwrap_or((line, ""));
        let value = value.trim();
        let value =
            value.strip_prefix('"').and_then(|value| value.strip_suffix('"')).unwrap_or(value);
        lines.push(Line {
            key: key.trim().to_owned(),
            value: value.to_owned(),
            location: format!("{name}:{}", i + 1),
        });
    }
    lines
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), dirs::home()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}

/// Where Ghostty looks for themes by name.
fn theme_dirs() -> Vec<PathBuf> {
    let mut theme_dirs = Vec::new();
    theme_dirs.extend(dirs::config_home().map(|dir| dir.join("ghostty").join("themes")));
    theme_dirs
        .extend(env::var_os("GHOSTTY_RESOURCES_DIR").map(|dir| PathBuf::from(dir).join("themes")));
    theme_dirs.push(PathBuf::from("/usr/share/ghostty/themes"));
    theme_dirs.push(PathBuf::from("/usr/local/share/ghostty/themes"));
    theme_dirs.push(PathBuf::from("/Applications/Ghostty.app/Contents/Resources/ghostty/themes"));
    theme_dirs
}

/// The keys of colors, which themes set.
const COLOR_KEYS: &[(&str, &str)] = &[
    ("background", "colors.primary.background"),
    ("foreground", "colors.primary.foreground"),
    ("cursor-color", "colors.cursor.cursor"),
    ("cursor-text", "colors.cursor.text"),
    ("selection-background", "colors.selection.background"),
    ("selection-foreground", "colors.selection.text"),
];

/// Colors by TankTerm key: the source key and the color.
type Colors = BTreeMap<String, (String, String)>;

/// Add a color line to `colors`; false when it isn't a color line.
fn add_color(colors: &mut Colors, line: &Line) -> Result<bool, String> {
    if line.key == "palette" {
        let (index, value) = line.value.split_once('=').ok_or("not N=color")?;
        let index: usize = index.trim().parse().map_err(|_| "not N=color")?;
        let value = color(value).ok_or("unknown color")?;
        let key = match index {
            0..=15 => ansi_key(index),
            16..=255 => format!("colors.indexed_colors.{index}"),
            _ => return Err("no such color".into()),
        };
        colors.insert(key, (format!("palette {index}"), value));
        return Ok(true);
    }

    let Some((_, to)) = COLOR_KEYS.iter().find(|(key, _)| *key == line.key) else {
        return Ok(false);
    };
    let value = match line.value.as_str() {
        "cell-foreground" => "CellForeground".to_owned(),
        "cell-background" => "CellBackground".to_owned(),
        value => color(value).ok_or("unknown color")?,
    };
    colors.insert(to.to_string(), (line.key.clone(), value));
    Ok(true)
}

/// The colors of the theme named `name`.
fn theme_colors(name: &str, theme_dirs: &[PathBuf]) -> Result<Colors, String> {
    let path = if Path::new(name).is_absolute() {
        Some(PathBuf::from(name))
    } else {
        theme_dirs.iter().map(|dir| dir.join(name)).find(|path| path.is_file())
    };
    let path =
        path.ok_or_else(|| format!("theme {name:?} not found in Ghostty's theme directories"))?;
    let contents =
        std::fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))?;

    let mut colors = Colors::new();
    for line in parse(&contents, name) {
        let _ = add_color(&mut colors, &line);
    }
    Ok(colors)
}

fn convert(lines: &[Line], theme_dirs: &[PathBuf]) -> Doc {
    let mut doc = Doc::default();

    // The last line of a key wins, apart from the keys that add to a list.
    let mut last: BTreeMap<&str, &Line> = BTreeMap::new();
    let mut families: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut theme = None;
    let mut colors = Colors::new();
    let mut bindings = Vec::new();
    let mut env = Vec::new();

    for line in lines {
        let skip_from = format!("{} ({})", line.key, line.location);
        match line.key.as_str() {
            key if key.starts_with("font-family") => {
                let list = families.entry(key).or_default();
                if line.value.is_empty() {
                    list.clear();
                } else {
                    list.push(&line.value);
                }
            },
            "theme" => theme = Some(line),
            "keybind" => keybind(&mut doc, &mut bindings, line),
            "env" => env.push(line),
            _ => match add_color(&mut colors, line) {
                Ok(true) => (),
                Ok(false) => {
                    last.insert(&line.key, line);
                },
                Err(err) => doc.skip(skip_from, err),
            },
        }
    }

    for (key, list) in families {
        let to = match key {
            "font-family" => "font.normal.family",
            "font-family-bold" => "font.bold.family",
            "font-family-italic" => "font.italic.family",
            "font-family-bold-italic" => "font.bold_italic.family",
            _ => {
                doc.skip(key, "no TankTerm equivalent");
                continue;
            },
        };
        if let Some(family) = list.first() {
            doc.set(key, to, *family);
        }
        if list.len() > 1 {
            doc.skip(format!("{key} {}", list[1..].join(", ")), "TankTerm has no fallback fonts");
        }
    }

    if let Some(theme) = theme {
        theme_and_colors(&mut doc, theme, colors, theme_dirs);
    } else {
        write_colors(&mut doc, colors);
    }

    pairs(&mut doc, &mut last);
    for (key, line) in last {
        setting(&mut doc, key, line);
    }
    for line in env {
        match line.value.split_once('=') {
            Some((name, value)) if !value.is_empty() => {
                doc.set_at(&format!("env {name}"), &["env", name], value)
            },
            _ => doc.skip(
                format!("env ({})", line.location),
                "only variables with a value are converted",
            ),
        }
    }

    // Ghostty has no preset of its own here; compare with TankTerm's default keys.
    output::push_bindings(&mut doc, None, bindings);
    doc
}

/// Write colors, with a theme underneath.
fn theme_and_colors(doc: &mut Doc, theme: &Line, colors: Colors, theme_dirs: &[PathBuf]) {
    let from = format!("theme {}", theme.value);

    let pairs: Option<(&str, &str)> = theme
        .value
        .split(',')
        .try_fold((None, None), |(light, dark), part| {
            let part = part.trim();
            if let Some(name) = part.strip_prefix("light:") {
                Some((Some(name), dark))
            } else {
                part.strip_prefix("dark:").map(|name| (light, Some(name)))
            }
        })
        .and_then(|(light, dark)| Some((light?, dark?)));

    let mut theme_colors_by_key: BTreeMap<String, Value> = BTreeMap::new();
    match pairs {
        Some((light, dark)) => {
            match (theme_colors(light, theme_dirs), theme_colors(dark, theme_dirs)) {
                (Ok(light), Ok(dark)) => {
                    for (key, (_, dark_color)) in &dark {
                        let value: Value = match light.get(key) {
                            Some((_, light_color)) if light_color != dark_color => {
                                let mut pair = InlineTable::new();
                                pair.insert("light", light_color.as_str().into());
                                pair.insert("dark", dark_color.as_str().into());
                                Value::InlineTable(pair)
                            },
                            _ => dark_color.as_str().into(),
                        };
                        theme_colors_by_key.insert(key.clone(), value);
                    }
                },
                (Err(err), _) | (_, Err(err)) => doc.skip(&from, err),
            }
        },
        None => match theme_colors(&theme.value, theme_dirs) {
            Ok(colors) => {
                for (key, (_, value)) in colors {
                    theme_colors_by_key.insert(key, value.into());
                }
            },
            Err(err) => doc.skip(&from, err),
        },
    }

    // The configuration's own colors win over the theme's.
    for (key, (source, value)) in colors {
        theme_colors_by_key.remove(&key);
        set_color(doc, &source, &key, value.into());
    }
    for (key, value) in theme_colors_by_key {
        set_color(doc, &from, &key, value);
    }
}

fn write_colors(doc: &mut Doc, colors: Colors) {
    for (key, (source, value)) in colors {
        set_color(doc, &source, &key, value.into());
    }
}

/// Set a color; indexed colors go to `colors.indexed_colors`.
fn set_color(doc: &mut Doc, from: &str, key: &str, value: Value) {
    let Some(index) = key.strip_prefix("colors.indexed_colors.") else {
        doc.set(from, key, value);
        return;
    };

    let mut entry = InlineTable::new();
    entry.insert("index", index.parse::<i64>().unwrap_or_default().into());
    entry.insert("color", value);
    let colors = doc.doc["colors"].or_insert(toml_edit::table());
    if let Some(colors) = colors.as_table_mut() {
        colors.set_implicit(true);
        let array =
            colors.entry("indexed_colors").or_insert(toml_edit::value(toml_edit::Array::new()));
        if let Some(array) = array.as_array_mut() {
            array.push(entry);
        }
    }
    doc.report.convert(from, "colors.indexed_colors");
}

/// Convert a setting that isn't a color, a font family, a key binding or a variable.
fn setting(doc: &mut Doc, key: &str, line: &Line) {
    let value = line.value.as_str();
    let skip_from = format!("{key} ({})", line.location);
    let boolean = || match value {
        "true" | "" => Some(true),
        "false" => Some(false),
        _ => None,
    };

    match key {
        "font-size" => match value.parse::<f64>() {
            Ok(size) => doc.set(key, "font.size", size),
            Err(_) => doc.skip(skip_from, "not a number"),
        },
        "font-style" | "font-style-bold" | "font-style-italic" | "font-style-bold-italic" => {
            let to = match key {
                "font-style" => "font.normal.style",
                "font-style-bold" => "font.bold.style",
                "font-style-italic" => "font.italic.style",
                _ => "font.bold_italic.style",
            };
            if value != "false" && value != "true" && !value.is_empty() {
                doc.set(key, to, value);
            }
        },
        "background-opacity" => match value.parse::<f64>() {
            Ok(opacity) => doc.set(key, "window.opacity", opacity.clamp(0., 1.)),
            Err(_) => doc.skip(skip_from, "not a number"),
        },
        "background-blur" | "background-blur-radius" => {
            if !matches!(value, "false" | "0") {
                doc.set(key, "window.blur", true);
            }
        },
        "cursor-style" => match value {
            "block" => doc.set(key, "cursor.style.shape", "Block"),
            "bar" => doc.set(key, "cursor.style.shape", "Beam"),
            "underline" => doc.set(key, "cursor.style.shape", "Underline"),
            "block_hollow" => {
                doc.set(key, "cursor.style.shape", "Block");
                doc.skip(skip_from, "TankTerm draws a hollow cursor only in panes without focus");
            },
            _ => doc.skip(skip_from, "unknown cursor style"),
        },
        "cursor-style-blink" => match boolean() {
            _ if value.is_empty() => (),
            Some(blink) => doc.set(key, "cursor.style.blinking", if blink { "On" } else { "Off" }),
            None => doc.skip(skip_from, "not true or false"),
        },
        "window-padding-balance" => {
            if value != "false" {
                doc.set(key, "window.dynamic_padding", true);
            }
        },
        "window-decoration" => {
            if matches!(value, "none" | "false") {
                doc.set(key, "window.decorations", "None");
                doc.set(key, "window.buttons", "None");
            }
        },
        "title" => {
            doc.set(key, "window.title", value);
            doc.set(key, "window.dynamic_title", false);
        },
        "command" => doc.set(key, "terminal.shell", command(value)),
        "working-directory" => match value {
            "home" => (),
            "inherit" => doc.skip(skip_from, "TankTerm starts in the home directory"),
            path => doc.set(
                key,
                "general.working_directory",
                expand_home(path).to_string_lossy().as_ref(),
            ),
        },
        "window-inherit-working-directory" | "tab-inherit-working-directory" => match boolean() {
            Some(inherit) => doc.set(key, "tabs.inherit_working_directory", inherit),
            None => doc.skip(skip_from, "not true or false"),
        },
        "scrollback-limit-lines" => match value.parse::<u64>() {
            Ok(lines) => doc.set(key, "scrolling.history", lines.min(100_000) as i64),
            Err(_) => doc.skip(skip_from, "not a number"),
        },
        "scrollback-limit" | "scrollback-limit-bytes" => {
            doc.skip(skip_from, "a limit in bytes; TankTerm's scrolling.history counts lines")
        },
        "copy-on-select" => match value {
            "clipboard" | "both" => doc.set(key, "selection.save_to_clipboard", true),
            "true" | "primary" | "" => (),
            _ => {
                doc.skip(skip_from, "TankTerm always puts the selection on the selection clipboard")
            },
        },
        "mouse-hide-while-typing" => match boolean() {
            Some(hide) => doc.set(key, "mouse.hide_when_typing", hide),
            None => doc.skip(skip_from, "not true or false"),
        },
        "bold-is-bright" => match boolean() {
            Some(bright) => doc.set(key, "colors.draw_bold_text_with_bright_colors", bright),
            None => doc.skip(skip_from, "not true or false"),
        },
        "bold-color" if value == "bright" => {
            doc.set(key, "colors.draw_bold_text_with_bright_colors", true)
        },
        "clipboard-paste-protection" => match boolean() {
            Some(confirm) => doc.set(key, "terminal.confirm_multiline_paste", confirm),
            None => doc.skip(skip_from, "not true or false"),
        },
        "fullscreen" => {
            if value != "false" {
                doc.set(key, "window.startup_mode", "Fullscreen");
            }
        },
        "maximize" => {
            if boolean() == Some(true) {
                doc.set(key, "window.startup_mode", "Maximized");
            }
        },
        _ => doc.skip(skip_from, "no TankTerm equivalent"),
    }
}

/// The settings made of two keys: padding, dimensions and clipboard access.
fn pairs(doc: &mut Doc, last: &mut BTreeMap<&str, &Line>) {
    // Ghostty's default padding is 2 points, on each side.
    let x = last.remove("window-padding-x");
    let y = last.remove("window-padding-y");
    if x.is_some() || y.is_some() {
        let side = |line: Option<&Line>| -> Result<i64, String> {
            let Some(line) = line else { return Ok(2) };
            let values: Option<Vec<i64>> =
                line.value.split(',').map(|part| part.trim().parse().ok()).collect();
            match values.as_deref() {
                Some([one]) => Ok(*one),
                Some([a, b]) if a == b => Ok(*a),
                _ => Err(format!("{} ({})", line.key, line.location)),
            }
        };
        match (side(x), side(y)) {
            (Ok(x), Ok(y)) => {
                let mut padding = InlineTable::new();
                padding.insert("x", x.into());
                padding.insert("y", y.into());
                doc.set("window-padding-x/window-padding-y", "window.padding", padding);
            },
            (Err(from), _) | (_, Err(from)) => {
                doc.skip(from, "TankTerm's padding in pixels is the same on opposite sides")
            },
        }
    }

    // Ghostty uses the size only when both are set.
    let width = last.remove("window-width");
    let height = last.remove("window-height");
    match (
        width.map(|line| line.value.parse::<i64>()),
        height.map(|line| line.value.parse::<i64>()),
    ) {
        (Some(Ok(columns)), Some(Ok(lines))) => {
            let mut dimensions = InlineTable::new();
            dimensions.insert("columns", columns.into());
            dimensions.insert("lines", lines.into());
            doc.set("window-width/window-height", "window.dimensions", dimensions);
        },
        (None, None) => (),
        _ => doc.skip("window-width/window-height", "Ghostty needs both, as whole numbers"),
    }

    // Ghostty asks before reading the clipboard, and allows writing it, by default.
    let read = last.remove("clipboard-read");
    let write = last.remove("clipboard-write");
    if read.is_some() || write.is_some() {
        let read = read.is_some_and(|line| line.value == "allow");
        let write = write.is_none_or(|line| line.value == "allow");
        let osc52 = match (read, write) {
            (true, true) => "CopyPaste",
            (false, true) => "OnlyCopy",
            (true, false) => "OnlyPaste",
            (false, false) => "Disabled",
        };
        doc.set("clipboard-read/clipboard-write", "terminal.osc52", osc52);
    }
}

/// Ghostty's `command` as a TankTerm shell.
fn command(command: &str) -> Value {
    if let Some(direct) = command.strip_prefix("direct:") {
        let mut words = direct.split_whitespace().map(ToOwned::to_owned);
        let program = words.next().unwrap_or_default();
        return output::program(&program, &words.collect::<Vec<_>>());
    }
    let command = command.strip_prefix("shell:").unwrap_or(command);
    if command.contains(char::is_whitespace) {
        output::program("/bin/sh", &["-c".to_owned(), command.to_owned()])
    } else {
        command.into()
    }
}

/// Convert a `keybind` line.
fn keybind(doc: &mut Doc, bindings: &mut Vec<(String, Binding)>, line: &Line) {
    let skip_from = format!("keybind {} ({})", line.value, line.location);
    if line.value == "clear" {
        return doc.skip(skip_from, "TankTerm keeps its own default keys");
    }

    // The `=` after the trigger; `ctrl+==…` binds the `=` key.
    let split = line
        .value
        .char_indices()
        .filter(|(_, c)| *c == '=')
        .map(|(i, _)| i)
        .find(|&i| i > 0 && !line.value[..i].ends_with('+'));
    let Some(split) = split else {
        return doc.skip(skip_from, "not trigger=action");
    };
    let (mut trigger, action) = (&line.value[..split], &line.value[split + 1..]);

    loop {
        if let Some(rest) = trigger.strip_prefix("global:") {
            return doc.skip(format!("keybind {rest}={action}"), "TankTerm has no global keys");
        }
        match ["all:", "unconsumed:", "performable:"]
            .iter()
            .find_map(|prefix| trigger.strip_prefix(prefix))
        {
            Some(rest) => trigger = rest,
            None => break,
        }
    }
    if trigger.contains('>') {
        return doc.skip(skip_from, "key sequences aren't converted; see keyboard.leader");
    }
    if trigger.contains('/') {
        return doc.skip(skip_from, "key tables aren't converted; see keyboard.tables");
    }

    let Some((key, mods)) = parse_trigger(trigger) else {
        return doc.skip(skip_from, "can't read the key");
    };
    let target = match parse_action(action) {
        Ok(target) => target,
        Err(reason) => return doc.skip(skip_from, reason),
    };
    bindings.push((format!("keybind {}", line.value), Binding {
        key,
        mods,
        leader: false,
        target,
    }));
}

/// A trigger like `ctrl+shift+t` as a TankTerm key and modifiers.
fn parse_trigger(trigger: &str) -> Option<(String, Mods)> {
    let (mods, key) = if trigger == "+" {
        ("", "+")
    } else if let Some(mods) = trigger.strip_suffix("++") {
        (mods, "+")
    } else {
        trigger.rsplit_once('+').unwrap_or(("", trigger))
    };

    let mut parsed = Mods::default();
    for name in mods.split('+').filter(|name| !name.is_empty()) {
        if !parsed.add(name) {
            return None;
        }
    }
    Some((key_name(key)?, parsed))
}

/// A Ghostty key name as TankTerm's.
fn key_name(key: &str) -> Option<String> {
    let named = |name: &str| -> Option<&'static str> {
        Some(match name {
            "arrow_up" | "up" | "ArrowUp" => "ArrowUp",
            "arrow_down" | "down" | "ArrowDown" => "ArrowDown",
            "arrow_left" | "left" | "ArrowLeft" => "ArrowLeft",
            "arrow_right" | "right" | "ArrowRight" => "ArrowRight",
            "page_up" | "PageUp" => "PageUp",
            "page_down" | "PageDown" => "PageDown",
            "home" | "Home" => "Home",
            "end" | "End" => "End",
            "insert" | "Insert" => "Insert",
            "delete" | "Delete" => "Delete",
            "backspace" | "Backspace" => "Backspace",
            "enter" | "Enter" => "Enter",
            "escape" | "Escape" => "Escape",
            "tab" | "Tab" => "Tab",
            "space" | "Space" => "Space",
            "bracket_left" | "left_bracket" | "BracketLeft" => "[",
            "bracket_right" | "right_bracket" | "BracketRight" => "]",
            "minus" | "Minus" => "-",
            "equal" | "Equal" => "=",
            "plus" => "+",
            "comma" | "Comma" => ",",
            "period" | "Period" => ".",
            "slash" | "Slash" => "/",
            "backslash" | "Backslash" => "\\",
            "semicolon" | "Semicolon" => ";",
            "quote" | "apostrophe" | "Quote" => "'",
            "backquote" | "grave_accent" | "Backquote" => "`",
            "zero" => "0",
            "one" => "1",
            "two" => "2",
            "three" => "3",
            "four" => "4",
            "five" => "5",
            "six" => "6",
            "seven" => "7",
            "eight" => "8",
            "nine" => "9",
            _ => return None,
        })
    };

    if let Some(name) = named(key) {
        return Some(name.to_owned());
    }
    if key.chars().count() == 1 {
        return Some(key.to_uppercase());
    }
    for prefix in ["key_", "Key", "digit_", "Digit"] {
        if let Some(rest) = key.strip_prefix(prefix).filter(|rest| rest.chars().count() == 1) {
            return Some(rest.to_uppercase());
        }
    }
    for prefix in ["numpad_", "kp_", "Numpad"] {
        if let Some(rest) = key.strip_prefix(prefix) {
            let rest = match rest {
                "add" => "Add",
                "subtract" => "Subtract",
                "multiply" => "Multiply",
                "divide" => "Divide",
                "decimal" => "Decimal",
                "enter" => "Enter",
                "equal" => "Equals",
                rest if rest.len() == 1 && rest.chars().all(|c| c.is_ascii_digit()) => rest,
                _ => return None,
            };
            return Some(format!("Numpad{rest}"));
        }
    }
    let function = key.strip_prefix('f').or_else(|| key.strip_prefix('F'))?;
    function.parse::<u8>().ok().map(|n| format!("F{n}"))
}

/// A Ghostty action as TankTerm's.
fn parse_action(action: &str) -> Result<Target, &'static str> {
    let (name, param) = action.split_once(':').unwrap_or((action, ""));
    let number = || param.trim().parse::<f64>().ok();
    let action = match (name, param) {
        ("new_tab", _) => "CreateNewTab",
        ("close_tab", "" | "this") => "CloseTab",
        ("close_surface", _) => "ClosePane",
        ("next_tab", _) => "SelectNextTab",
        ("previous_tab", _) => "SelectPreviousTab",
        ("last_tab", _) => "SelectLastTab",
        ("goto_tab", _) => match number() {
            Some(n) if (1.0..=9.0).contains(&n) && n.fract() == 0. => [
                "SelectTab1",
                "SelectTab2",
                "SelectTab3",
                "SelectTab4",
                "SelectTab5",
                "SelectTab6",
                "SelectTab7",
                "SelectTab8",
                "SelectTab9",
            ][n as usize - 1],
            _ => return Err("TankTerm selects tabs 1 to 9 only"),
        },
        ("move_tab", _) => match number() {
            Some(-1.) => "MoveTabLeft",
            Some(1.) => "MoveTabRight",
            _ => return Err("TankTerm moves tabs by one place"),
        },
        ("new_split", "right") => "SplitRight",
        ("new_split", "down") => "SplitDown",
        ("new_split", _) => return Err("TankTerm splits to the right and down only"),
        ("goto_split", "left") => "FocusPaneLeft",
        ("goto_split", "right") => "FocusPaneRight",
        ("goto_split", "up" | "top") => "FocusPaneUp",
        ("goto_split", "down" | "bottom") => "FocusPaneDown",
        ("goto_split", "previous") => "FocusPreviousPane",
        ("goto_split", "next") => "FocusNextPane",
        ("toggle_split_zoom", _) => "TogglePaneZoom",
        ("resize_split", _) => match param.split(',').next().unwrap_or_default() {
            "left" => "ResizePaneLeft",
            "right" => "ResizePaneRight",
            "up" => "ResizePaneUp",
            "down" => "ResizePaneDown",
            _ => return Err("unknown direction"),
        },
        ("copy_to_clipboard", _) => "Copy",
        ("paste_from_clipboard", _) => "Paste",
        ("paste_from_selection", _) => "PasteSelection",
        ("increase_font_size", _) => "IncreaseFontSize",
        ("decrease_font_size", _) => "DecreaseFontSize",
        ("reset_font_size", _) => "ResetFontSize",
        ("scroll_to_top", _) => "ScrollToTop",
        ("scroll_to_bottom", _) => "ScrollToBottom",
        ("scroll_page_up", _) => "ScrollPageUp",
        ("scroll_page_down", _) => "ScrollPageDown",
        ("scroll_page_fractional", _) => match number() {
            Some(-0.5) => "ScrollHalfPageUp",
            Some(0.5) => "ScrollHalfPageDown",
            Some(-1.) => "ScrollPageUp",
            Some(1.) => "ScrollPageDown",
            _ => return Err("TankTerm scrolls by whole and half pages"),
        },
        ("scroll_page_lines", _) => match number() {
            Some(-1.) => "ScrollLineUp",
            Some(1.) => "ScrollLineDown",
            _ => return Err("TankTerm scrolls by one line"),
        },
        ("jump_to_prompt", _) => match number() {
            Some(n) if n < 0. => "ScrollToPreviousPrompt",
            Some(n) if n > 0. => "ScrollToNextPrompt",
            _ => return Err("not a number of prompts"),
        },
        ("clear_screen", _) => "ClearHistory",
        ("toggle_fullscreen", _) => "ToggleFullscreen",
        ("toggle_maximize", _) => "ToggleMaximized",
        ("new_window", _) => "CreateNewWindow",
        ("quit" | "close_window", _) => "Quit",
        ("toggle_command_palette", _) => "ShowCommandPalette",
        ("toggle_tab_overview", _) => "ShowTabNavigator",
        ("open_config", _) => "OpenConfig",
        ("prompt_surface_title" | "prompt_tab_title", _) => "RenameTab",
        ("start_search", _) => "SearchForward",
        ("unbind", _) => "ReceiveChar",
        ("ignore", _) => "None",
        ("text", _) => return Ok(Target::Chars(unescape(param))),
        ("csi", _) => return Ok(Target::Chars(format!("\x1b[{param}"))),
        ("esc", _) => return Ok(Target::Chars(format!("\x1b{param}"))),
        _ => return Err("no TankTerm action"),
    };
    Ok(Target::Action(action))
}

/// Ghostty's (Zig's) string escapes.
fn unescape(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('t') => result.push('\t'),
            Some('e') => result.push('\x1b'),
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                result.extend(u8::from_str_radix(&hex, 16).ok().map(char::from));
            },
            Some('u') => {
                let code: String =
                    chars.by_ref().skip_while(|c| *c == '{').take_while(|c| *c != '}').collect();
                result.extend(u32::from_str_radix(&code, 16).ok().and_then(char::from_u32));
            },
            Some(other) => result.push(other),
            None => result.push('\\'),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use crate::color::{CellRgb, Rgb};
    use crate::config::UiConfig;

    fn convert_str(contents: &str, theme_dirs: &[PathBuf]) -> Doc {
        convert(&parse(contents, "config"), theme_dirs)
    }

    #[test]
    fn converts_settings() {
        let doc = convert_str(
            "# comment\nfont-family = \"\"\nfont-family = Iosevka Term\nfont-family = Noto \
             Emoji\nfont-size = 13\nbackground = 101010\nforeground = \
             #c0c0c0\nselection-background = cell-foreground\npalette = 1=#ff0000\npalette = 9 = \
             #ff8080\npalette = 100=#123456\nbackground-opacity = 0.9\nbackground-blur = \
             20\ncursor-style = bar\ncursor-style-blink = false\nwindow-padding-x = \
             4\nwindow-padding-y = 6,6\nwindow-decoration = none\nwindow-width = \
             100\nwindow-height = 30\ncommand = direct:/bin/zsh -l\ncopy-on-select = \
             clipboard\nclipboard-read = allow\nenv = EDITOR=hx\nshell-integration = \
             zsh\nscrollback-limit = 10000000\n",
            &[],
        );
        let toml = doc.doc.to_string();
        let config: UiConfig = toml::from_str(&toml).unwrap();

        assert_eq!(config.font.normal().family, "Iosevka Term");
        assert_eq!(config.font.size(), 13.0);
        assert_eq!(config.colors.primary.background, Rgb::new(0x10, 0x10, 0x10));
        assert_eq!(config.colors.normal.red, Rgb::new(0xff, 0, 0));
        assert_eq!(config.colors.bright.red, Rgb::new(0xff, 0x80, 0x80));
        assert_eq!(config.colors.selection.background, Some(CellRgb::CellForeground));
        assert_eq!(config.window_opacity(), 0.9);
        assert!(config.window.blur);
        assert!(config.selection.save_to_clipboard);
        assert_eq!(config.env["EDITOR"], "hx");
        assert!(toml.contains("indexed_colors = [{ index = 100, color = \"#123456\" }]"), "{toml}");
        assert!(toml.contains("padding = { x = 4, y = 6 }"), "{toml}");
        assert!(toml.contains("dimensions = { columns = 100, lines = 30 }"), "{toml}");
        assert!(toml.contains("shell = { program = \"/bin/zsh\", args = [\"-l\"] }"), "{toml}");
        assert!(toml.contains("osc52 = \"CopyPaste\""), "{toml}");
        assert!(toml.contains("decorations = \"None\""), "{toml}");

        let skipped: Vec<_> = doc.report.skipped.iter().map(|(key, _)| key.as_str()).collect();
        assert!(skipped.contains(&"font-family Noto Emoji"), "{skipped:?}");
        assert!(skipped.contains(&"shell-integration (config:25)"), "{skipped:?}");
        assert!(skipped.contains(&"scrollback-limit (config:26)"), "{skipped:?}");
    }

    #[test]
    fn keybinds() {
        let doc = convert_str(
            "keybind = ctrl+shift+t=new_tab\nkeybind = ctrl+==increase_font_size:1\nkeybind = \
             super+arrow_left=goto_split:left\nkeybind = ctrl+a>c=new_tab\nkeybind = \
             global:ctrl+grave_accent=toggle_quick_terminal\nkeybind = \
             ctrl+shift+key_k=text:\\x0c\nkeybind = alt+digit_2=goto_tab:2\nkeybind = \
             ctrl+shift+o=new_split:auto\n",
            &[],
        );
        let toml = doc.doc.to_string();

        // Ctrl+Shift+T and Ctrl+= are TankTerm's keys already.
        assert!(!toml.contains("CreateNewTab"), "{toml}");
        assert!(!toml.contains("IncreaseFontSize"), "{toml}");
        assert!(
            toml.contains("{ key = \"ArrowLeft\", mods = \"Super\", action = \"FocusPaneLeft\" }"),
            "{toml}"
        );
        assert!(
            toml.contains("{ key = \"K\", mods = \"Control|Shift\", chars = \"\\f\" }"),
            "{toml}"
        );
        assert!(!toml.contains("SelectTab2"), "{toml}");
        toml::from_str::<UiConfig>(&toml).unwrap();

        let skipped: Vec<_> = doc
            .report
            .skipped
            .iter()
            .map(|(key, reason)| (key.as_str(), reason.as_str()))
            .collect();
        assert_eq!(skipped.len(), 3, "{skipped:?}");
    }

    #[test]
    fn themes_and_includes() {
        let dir = tempfile::tempdir().unwrap();
        let themes = dir.path().join("themes");
        fs::create_dir_all(&themes).unwrap();
        fs::write(
            themes.join("Dark"),
            "background = #000000\nforeground = #ffffff\npalette = 0=#111111\n",
        )
        .unwrap();
        fs::write(themes.join("Light"), "background = #ffffff\nforeground = #ffffff\n").unwrap();

        let config = dir.path().join("config.ghostty");
        fs::write(
            &config,
            "config-file = extra\nconfig-file = ?missing\ntheme = \
             light:Light,dark:Dark\nfont-size = 10\n",
        )
        .unwrap();
        fs::write(dir.path().join("extra"), "font-size = 15\nforeground = #00ff00\n").unwrap();

        let mut lines = Vec::new();
        read(&config, true, &mut lines, &mut HashSet::new()).unwrap();
        let doc = convert(&lines, &[themes]);
        let toml = doc.doc.to_string();

        assert!(
            toml.contains("background = { light = \"#ffffff\", dark = \"#000000\" }"),
            "{toml}"
        );
        // The included file comes after the including one, and the config's colors beat the
        // theme's.
        assert!(toml.contains("size = 15.0"), "{toml}");
        assert!(toml.contains("foreground = \"#00ff00\""), "{toml}");
        assert!(toml.contains("black = \"#111111\""), "{toml}");
        toml::from_str::<UiConfig>(&toml).unwrap();
    }

    #[test]
    fn missing_theme_is_skipped() {
        let doc = convert_str("theme = Nope\n", &[]);
        assert_eq!(doc.report.skipped.len(), 1);
        assert!(doc.report.skipped[0].1.contains("not found"));
    }
}
