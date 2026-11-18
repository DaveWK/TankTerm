//! Tabby: `config.yaml`.

use std::path::{Path, PathBuf};

use serde_yaml::Value as Yaml;
use toml_edit::{InlineTable, Table, Value};

use crate::migrate::dirs;
use crate::migrate::output::{self, Binding, Doc, Mods, Plan, Target, ansi_key, color};

/// Tabby's configuration file.
pub fn locations() -> Vec<PathBuf> {
    let mut locations = Vec::new();

    #[cfg(not(any(windows, target_os = "macos")))]
    locations.extend(dirs::config_home().map(|dir| dir.join("tabby").join("config.yaml")));

    #[cfg(target_os = "macos")]
    locations.extend(dirs::app_support().map(|dir| dir.join("tabby").join("config.yaml")));

    #[cfg(windows)]
    locations.extend(dirs::app_data().map(|dir| dir.join("tabby").join("config.yaml")));

    locations
}

/// Plan the conversion of Tabby's configuration at `path`.
pub fn plan(path: &Path, target: &Path) -> Result<Plan, String> {
    let contents =
        std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let doc = convert(&contents).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(Plan::single("Tabby", path, target, doc))
}

fn convert(contents: &str) -> Result<Doc, String> {
    let yaml: Yaml = serde_yaml::from_str(contents).map_err(|err| format!("YAML error: {err}"))?;
    let Yaml::Mapping(root) = yaml else {
        return Err("not a Tabby configuration".into());
    };

    let mut doc = Doc::default();
    for (key, value) in &root {
        let key = key.as_str().unwrap_or_default();
        match key {
            "terminal" => terminal(&mut doc, value, &root),
            "appearance" => appearance(&mut doc, value),
            "profiles" => profiles(&mut doc, value),
            "hotkeys" => hotkeys(&mut doc, value),
            "version" => (),
            _ => skip_unless_empty(&mut doc, key, value, "no TankTerm equivalent"),
        }
    }
    Ok(doc)
}

/// Record a skipped key, unless it holds nothing.
fn skip_unless_empty(doc: &mut Doc, key: &str, value: &Yaml, reason: &str) {
    let empty = match value {
        Yaml::Mapping(map) => map.is_empty(),
        Yaml::Sequence(seq) => seq.is_empty(),
        Yaml::Null => true,
        _ => false,
    };
    if !empty {
        doc.skip(key, reason);
    }
}

fn terminal(doc: &mut Doc, terminal: &Yaml, root: &serde_yaml::Mapping) {
    let Yaml::Mapping(terminal) = terminal else { return };

    let mode = root
        .get("appearance")
        .and_then(|appearance| appearance.get("colorSchemeMode"))
        .and_then(Yaml::as_str)
        .unwrap_or("dark");
    let dark = terminal.get("colorScheme");
    let light = terminal.get("lightColorScheme");
    match (mode, dark, light) {
        ("light", _, Some(light)) => color_scheme(doc, "terminal.lightColorScheme", light, None),
        ("auto", Some(dark), Some(light)) => {
            color_scheme(doc, "terminal.colorScheme", dark, Some(light));
        },
        (_, Some(dark), _) => color_scheme(doc, "terminal.colorScheme", dark, None),
        _ => (),
    }

    for (key, value) in terminal {
        let key = key.as_str().unwrap_or_default();
        let from = format!("terminal.{key}");
        match (key, value) {
            ("colorScheme" | "lightColorScheme", _) => (),
            ("customColorSchemes", _) => {
                skip_unless_empty(doc, &from, value, "only the scheme in use is converted")
            },
            ("font", Yaml::String(font)) => {
                let family =
                    font.split(',').next().unwrap_or_default().trim().trim_matches(['"', '\'']);
                doc.set(&from, "font.normal.family", family);
            },
            ("fontSize", _) => match value.as_f64() {
                // Tabby's sizes are CSS pixels, TankTerm's are points.
                Some(size) => doc.set(&format!("{from} ({size} px)"), "font.size", size * 0.75),
                None => doc.skip(&from, "not a number"),
            },
            ("fontWeight", _) => font_weight(doc, &from, value, "font.normal.style"),
            ("fontWeightBold", _) => font_weight(doc, &from, value, "font.bold.style"),
            ("cursor", Yaml::String(shape)) => match shape.as_str() {
                "block" => doc.set(&from, "cursor.style.shape", "Block"),
                "beam" => doc.set(&from, "cursor.style.shape", "Beam"),
                "underline" => doc.set(&from, "cursor.style.shape", "Underline"),
                _ => doc.skip(&from, format!("unknown cursor {shape:?}")),
            },
            ("cursorBlink", Yaml::Bool(blink)) => {
                doc.set(&from, "cursor.style.blinking", if *blink { "On" } else { "Off" });
            },
            ("bell", Yaml::String(bell)) => match bell.as_str() {
                "visual" => doc.set(&from, "bell.duration", 100),
                "off" => (),
                _ => doc.skip(&from, "TankTerm has no audible bell; bell.command can play a sound"),
            },
            ("copyOnSelect", Yaml::Bool(copy)) => {
                doc.set(&from, "selection.save_to_clipboard", *copy)
            },
            ("scrollbackLines", _) => match value.as_u64() {
                Some(lines) => doc.set(&from, "scrolling.history", lines.min(100_000) as i64),
                None => doc.skip(&from, "not a number"),
            },
            ("drawBoldTextInBrightColors", Yaml::Bool(bright)) => {
                doc.set(&from, "colors.draw_bold_text_with_bright_colors", *bright);
            },
            ("ligatures", _) => doc.skip(&from, "TankTerm doesn't draw ligatures"),
            ("profile", Yaml::String(profile)) => default_profile(doc, &from, profile, root),
            _ => skip_unless_empty(doc, &from, value, "no TankTerm equivalent"),
        }
    }
}

/// A Tabby (CSS) font weight as a TankTerm font style.
fn font_weight(doc: &mut Doc, from: &str, value: &Yaml, to: &str) {
    let style = match value.as_u64() {
        Some(100) => "Thin",
        Some(200) => "ExtraLight",
        Some(300) => "Light",
        Some(400) => "Regular",
        Some(500) => "Medium",
        Some(600) => "SemiBold",
        Some(700) => "Bold",
        Some(800) => "ExtraBold",
        Some(900) => "Black",
        _ => return doc.skip(from, "not a font weight"),
    };
    doc.set(from, to, style);
}

/// Convert a color scheme, as light/dark pairs when `light` is given too.
fn color_scheme(doc: &mut Doc, from: &str, dark: &Yaml, light: Option<&Yaml>) {
    let get = |scheme: &Yaml, key: &str| scheme.get(key).and_then(Yaml::as_str).and_then(color);
    let set = |doc: &mut Doc,
               key: &str,
               to: &str,
               dark_color: Option<String>,
               light_color: Option<String>| {
        let value: Value = match (dark_color, light_color) {
            (Some(dark), Some(light)) if light != dark => {
                let mut pair = InlineTable::new();
                pair.insert("light", light.into());
                pair.insert("dark", dark.into());
                Value::InlineTable(pair)
            },
            (Some(dark), _) => dark.into(),
            _ => return,
        };
        doc.set(&format!("{from}.{key}"), to, value);
    };

    let keys = [
        ("foreground", "colors.primary.foreground"),
        ("background", "colors.primary.background"),
        ("cursor", "colors.cursor.cursor"),
        ("cursorAccent", "colors.cursor.text"),
        ("selection", "colors.selection.background"),
        ("selectionForeground", "colors.selection.text"),
    ];
    for (key, to) in keys {
        set(doc, key, to, get(dark, key), light.and_then(|light| get(light, key)));
    }

    let ansi = |scheme: &Yaml, i: usize| {
        scheme.get("colors").and_then(|colors| colors.get(i)).and_then(Yaml::as_str).and_then(color)
    };
    for i in 0..16 {
        set(
            doc,
            &format!("colors[{i}]"),
            &ansi_key(i),
            ansi(dark, i),
            light.and_then(|light| ansi(light, i)),
        );
    }
}

fn appearance(doc: &mut Doc, appearance: &Yaml) {
    let Yaml::Mapping(appearance) = appearance else { return };
    for (key, value) in appearance {
        let key = key.as_str().unwrap_or_default();
        let from = format!("appearance.{key}");
        match (key, value) {
            ("opacity", _) => match value.as_f64() {
                Some(opacity) => doc.set(&from, "window.opacity", opacity),
                None => doc.skip(&from, "not a number"),
            },
            ("vibrancy", Yaml::Bool(blur)) => doc.set(&from, "window.blur", *blur),
            ("frame", Yaml::String(frame)) => match frame.as_str() {
                "native" => (),
                // Tabby draws its own title bar with the tabs in it.
                _ => doc.set(&from, "window.decorations", "None"),
            },
            ("tabsLocation", Yaml::String(location)) => match location.as_str() {
                "top" => doc.set(&from, "tabs.position", "Top"),
                "bottom" => doc.set(&from, "tabs.position", "Bottom"),
                _ => doc.skip(&from, "TankTerm's tabs are at the top or the bottom"),
            },
            ("colorSchemeMode", _) => (),
            _ => skip_unless_empty(doc, &from, value, "no TankTerm equivalent"),
        }
    }
}

/// The default profile's shell, for a custom local profile.
fn default_profile(doc: &mut Doc, from: &str, id: &str, root: &serde_yaml::Mapping) {
    if !id.contains(":custom:") {
        return;
    }
    let profile = root.get("profiles").and_then(Yaml::as_sequence).and_then(|profiles| {
        profiles.iter().find(|profile| profile.get("id").and_then(Yaml::as_str) == Some(id))
    });
    match profile.filter(|profile| profile.get("type").and_then(Yaml::as_str) == Some("local")) {
        Some(profile) => match local_shell(profile.get("options")) {
            Some(shell) => doc.set(from, "terminal.shell", shell),
            None => doc.skip(from, "the profile has no command"),
        },
        None => doc.skip(from, "only a local profile can be the default shell"),
    }
}

/// The program of a local profile.
fn local_shell(options: Option<&Yaml>) -> Option<Value> {
    let options = options?;
    let command =
        options.get("command").and_then(Yaml::as_str).filter(|command| !command.is_empty())?;
    Some(output::program(command, &strings(options.get("args"))))
}

/// A list of strings, leaving out anything else.
fn strings(value: Option<&Yaml>) -> Vec<String> {
    value
        .and_then(Yaml::as_sequence)
        .map(|seq| seq.iter().filter_map(yaml_scalar).collect())
        .unwrap_or_default()
}

fn yaml_scalar(value: &Yaml) -> Option<String> {
    match value {
        Yaml::String(s) => Some(s.clone()),
        Yaml::Number(n) => Some(n.to_string()),
        Yaml::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn profiles(doc: &mut Doc, profiles: &Yaml) {
    let Some(profiles) = profiles.as_sequence() else { return };
    for (i, profile) in profiles.iter().enumerate() {
        let name = profile.get("name").and_then(Yaml::as_str).unwrap_or("");
        let from = format!("profiles[{i}] \"{name}\"");
        let options = profile.get("options");
        let option = |key: &str| options.and_then(|options| options.get(key));

        let mut table = Table::new();
        table.insert("name", toml_edit::value(name));
        match profile.get("type").and_then(Yaml::as_str) {
            Some("local") => {
                if let Some(shell) = local_shell(options) {
                    table.insert("shell", toml_edit::value(shell));
                }
            },
            Some("ssh") => {
                let Some(host) = option("host").and_then(Yaml::as_str) else {
                    doc.skip(&from, "an SSH profile without a host");
                    continue;
                };
                let mut args = Vec::new();
                if let Some(port) = option("port").and_then(Yaml::as_u64).filter(|port| *port != 22)
                {
                    args.extend(["-p".to_owned(), port.to_string()]);
                }
                for key in strings(option("privateKeys")) {
                    let key = key.strip_prefix("file://").unwrap_or(&key).to_owned();
                    args.extend(["-i".to_owned(), key]);
                }
                if option("jumpHost").and_then(Yaml::as_str).is_some_and(|jump| !jump.is_empty()) {
                    doc.skip(
                        format!("{from}.options.jumpHost"),
                        "jump hosts aren't converted; add -J to the profile's args",
                    );
                }
                let user = option("user").and_then(Yaml::as_str).filter(|user| !user.is_empty());
                args.push(match user {
                    Some(user) => format!("{user}@{host}"),
                    None => host.to_owned(),
                });
                table.insert("shell", toml_edit::value(output::program("ssh", &args)));
            },
            Some(kind) => {
                doc.skip(&from, format!("TankTerm has no {kind} profiles"));
                continue;
            },
            None => continue,
        }

        if let Some(cwd) = option("cwd").and_then(Yaml::as_str).filter(|cwd| !cwd.is_empty()) {
            table.insert("working_directory", toml_edit::value(cwd));
        }
        if let Some(env) = option("env").and_then(Yaml::as_mapping).filter(|env| !env.is_empty()) {
            let mut env_table = InlineTable::new();
            for (key, value) in env {
                if let (Some(key), Some(value)) = (key.as_str(), yaml_scalar(value)) {
                    env_table.insert(key, value.into());
                }
            }
            table.insert("env", toml_edit::value(env_table));
        }
        if profile.get("terminalColorScheme").is_some() {
            doc.skip(
                format!("{from}.terminalColorScheme"),
                "a profile's theme is a TankTerm theme file; set it by hand",
            );
        }
        doc.push_profile(&from, table);
    }
}

/// Tabby's hotkeys and the TankTerm actions they become.
const ACTIONS: &[(&str, &str)] = &[
    ("copy", "Copy"),
    ("paste", "Paste"),
    ("clear", "ClearHistory"),
    ("zoom-in", "IncreaseFontSize"),
    ("zoom-out", "DecreaseFontSize"),
    ("reset-zoom", "ResetFontSize"),
    ("search", "SearchForward"),
    ("scroll-to-top", "ScrollToTop"),
    ("scroll-page-up", "ScrollPageUp"),
    ("scroll-up", "ScrollLineUp"),
    ("scroll-down", "ScrollLineDown"),
    ("scroll-page-down", "ScrollPageDown"),
    ("scroll-to-bottom", "ScrollToBottom"),
    ("settings", "OpenConfig"),
    ("new-tab", "CreateNewTab"),
    ("new-window", "CreateNewWindow"),
    ("toggle-fullscreen", "ToggleFullscreen"),
    ("close-tab", "CloseTab"),
    ("rename-tab", "RenameTab"),
    ("next-tab", "SelectNextTab"),
    ("previous-tab", "SelectPreviousTab"),
    ("move-tab-left", "MoveTabLeft"),
    ("move-tab-right", "MoveTabRight"),
    ("tab-1", "SelectTab1"),
    ("tab-2", "SelectTab2"),
    ("tab-3", "SelectTab3"),
    ("tab-4", "SelectTab4"),
    ("tab-5", "SelectTab5"),
    ("tab-6", "SelectTab6"),
    ("tab-7", "SelectTab7"),
    ("tab-8", "SelectTab8"),
    ("tab-9", "SelectTab9"),
    ("split-right", "SplitRight"),
    ("split-bottom", "SplitDown"),
    ("pane-nav-left", "FocusPaneLeft"),
    ("pane-nav-right", "FocusPaneRight"),
    ("pane-nav-up", "FocusPaneUp"),
    ("pane-nav-down", "FocusPaneDown"),
    ("pane-nav-previous", "FocusPreviousPane"),
    ("pane-nav-next", "FocusNextPane"),
    ("pane-maximize", "TogglePaneZoom"),
    ("close-pane", "ClosePane"),
    ("rearrange-panes", "PaneSelect"),
    ("profile-selector", "ShowProfiles"),
    ("command-selector", "ShowCommandPalette"),
];

/// Why a Tabby hotkey without a TankTerm action is skipped.
fn no_action(id: &str) -> &'static str {
    match id {
        "ctrl-c" => "copies the selection or sends Ctrl+C; TankTerm leaves Ctrl+C to the terminal",
        "home"
        | "end"
        | "previous-word"
        | "next-word"
        | "delete-previous-word"
        | "delete-next-word"
        | "delete-line"
        | "insert-new-line" => {
            "sends a shell editing sequence; TankTerm leaves the key to the terminal"
        },
        "toggle-window" => "a global hotkey; TankTerm has none",
        "split-left" | "split-top" => "TankTerm splits to the right and down only",
        _ if id.starts_with("tab-") => "TankTerm selects tabs 1 to 9 only",
        _ if id.starts_with("pane-increase") || id.starts_with("pane-decrease") => {
            "TankTerm moves dividers instead of growing panes; see ResizePane*"
        },
        _ => "no TankTerm action",
    }
}

fn hotkeys(doc: &mut Doc, hotkeys: &Yaml) {
    let Some(hotkeys) = hotkeys.as_mapping() else { return };

    doc.set("Tabby's default keys", "keyboard.preset", "Tabby");
    let mut bindings = Vec::new();
    for (id, keys) in hotkeys {
        let id = id.as_str().unwrap_or_default();
        let from = format!("hotkeys.{id}");
        let strokes: Vec<&Yaml> = match keys {
            Yaml::String(_) => vec![keys],
            Yaml::Sequence(keys) => keys.iter().collect(),
            _ => continue,
        };
        if strokes.is_empty() {
            continue;
        }

        let Some((_, action)) = ACTIONS.iter().find(|(name, _)| *name == id) else {
            doc.skip(&from, no_action(id));
            continue;
        };

        for stroke in strokes {
            let Some(stroke) = stroke.as_str() else {
                doc.skip(&from, "key sequences aren't converted; see keyboard.leader");
                continue;
            };
            match parse_stroke(stroke) {
                Ok((key, mods)) => bindings.push((format!("{from} {stroke}"), Binding {
                    key,
                    mods,
                    leader: false,
                    target: Target::Action(action),
                })),
                Err(reason) => doc.skip(format!("{from} {stroke}"), reason),
            }
        }
    }
    output::push_bindings(doc, Some("Tabby"), bindings);
}

/// A Tabby keystroke like `Ctrl-Shift-T` or `⌘-=`, as a TankTerm key and modifiers.
fn parse_stroke(stroke: &str) -> Result<(String, Mods), &'static str> {
    let (mods, key) = if stroke == "-" {
        ("", "-")
    } else if let Some(mods) = stroke.strip_suffix("--") {
        (mods, "-")
    } else {
        match stroke.rsplit_once('-') {
            Some((mods, key)) => (mods, key),
            None => ("", stroke),
        }
    };

    let mut parsed = Mods::default();
    for name in mods.split('-').filter(|name| !name.is_empty()) {
        if !parsed.add(name) {
            return Err("unknown modifier");
        }
    }
    // A hotkey of modifiers alone, like Tabby's `Ctrl-Shift` for rearranging panes.
    if parsed.add(key) {
        return Err("TankTerm can't bind modifiers alone");
    }

    Ok((key_name(key).ok_or("TankTerm doesn't bind mouse buttons to these actions")?, parsed))
}

/// A Tabby key name as TankTerm's.
fn key_name(key: &str) -> Option<String> {
    Some(match key {
        "Left" | "Right" | "Up" | "Down" => format!("Arrow{key}"),
        "MiddleClick" | "Mouse" => return None,
        _ if key.starts_with("Wheel") => return None,
        _ if key.chars().count() == 1 => key.to_uppercase(),
        _ => key.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::color::{CellRgb, Rgb};
    use crate::config::{Action, UiConfig};

    const CONFIG: &str = r##"
version: 8
profiles:
  - type: local
    name: Fish
    id: local:custom:1
    options:
      command: /usr/bin/fish
      args: [-l]
      env: { EDITOR: hx }
  - type: ssh
    name: Build
    options:
      host: build.example.com
      port: 2222
      user: me
  - type: serial
    name: Console
    options: {}
hotkeys:
  new-tab:
    - Ctrl-Shift-T
  zoom-out:
    - Ctrl--
  next-tab:
    - Ctrl-Shift-Right
  tab-10:
    - Alt-0
  home:
    - Home
  rearrange-panes:
    - Ctrl-Shift
  split-left: []
terminal:
  font: Fira Code
  fontSize: 16
  cursor: beam
  cursorBlink: false
  bell: visual
  profile: local:custom:1
  ligatures: true
  colorScheme:
    name: Test
    foreground: '#39FF14'
    background: '#000000'
    cursor: '#39FF14'
    selection: '#88888888'
    colors: ['#000000', '#FF0000', '#00FF00', '#FFFF00', '#0000FF', '#FF00FF', '#00FFFF', '#FFFFFF',
             '#111111', '#FF1111', '#11FF11', '#FFFF11', '#1111FF', '#FF11FF', '#11FFFF', '#EEEEEE']
appearance:
  opacity: 0.5
  vibrancy: true
  frame: thin
ssh:
  warnOnClose: true
hacks: {}
"##;

    #[test]
    fn converts_settings() {
        let doc = convert(CONFIG).unwrap();
        let toml = doc.doc.to_string();
        let config: UiConfig = toml::from_str(&toml).unwrap();

        assert_eq!(config.font.size(), 12.0);
        assert_eq!(config.font.normal().family, "Fira Code");
        assert_eq!(config.colors.primary.foreground, Rgb::new(0x39, 0xff, 0x14));
        assert_eq!(config.colors.bright.white, Rgb::new(0xee, 0xee, 0xee));
        assert_eq!(
            config.colors.selection.background,
            Some(CellRgb::Rgb(Rgb::new(0x88, 0x88, 0x88)))
        );
        assert_eq!(config.window_opacity(), 0.5);
        assert!(config.window.blur);
        assert_eq!(config.bell.duration(), std::time::Duration::from_millis(100));
        assert!(
            toml.contains("shell = { program = \"/usr/bin/fish\", args = [\"-l\"] }"),
            "{toml}"
        );

        assert_eq!(config.profiles.len(), 2);
        assert_eq!(config.profiles[0].env["EDITOR"], "hx");
        assert!(
            toml.contains("program = \"ssh\", args = [\"-p\", \"2222\", \"me@build.example.com\"]"),
            "{toml}"
        );

        let skipped: Vec<_> = doc.report.skipped.iter().map(|(key, _)| key.as_str()).collect();
        assert!(skipped.contains(&"profiles[2] \"Console\""), "{skipped:?}");
        assert!(skipped.contains(&"terminal.ligatures"), "{skipped:?}");
        assert!(skipped.contains(&"ssh"), "{skipped:?}");
        assert!(!skipped.contains(&"hacks"), "{skipped:?}");
    }

    #[test]
    fn converts_hotkeys_against_the_preset() {
        let doc = convert(CONFIG).unwrap();
        let toml = doc.doc.to_string();

        // Ctrl-Shift-T and Ctrl-- are TankTerm's keys already; Ctrl-Shift-Right isn't.
        assert!(!toml.contains("CreateNewTab"), "{toml}");
        assert!(!toml.contains("DecreaseFontSize"), "{toml}");
        assert!(
            toml.contains(
                "{ key = \"ArrowRight\", mods = \"Control|Shift\", action = \"SelectNextTab\" }"
            ),
            "{toml}"
        );

        let config: UiConfig = toml::from_str(&toml).unwrap();
        assert!(
            config.key_bindings().iter().any(|binding| binding.action == Action::SelectNextTab)
        );

        let skipped: Vec<_> = doc.report.skipped.iter().map(|(key, _)| key.as_str()).collect();
        assert!(skipped.contains(&"hotkeys.tab-10"), "{skipped:?}");
        assert!(skipped.contains(&"hotkeys.home"), "{skipped:?}");
        assert!(skipped.contains(&"hotkeys.rearrange-panes Ctrl-Shift"), "{skipped:?}");
        assert!(!skipped.iter().any(|key| key.contains("split-left")), "{skipped:?}");
    }

    #[test]
    fn strokes() {
        let control = Mods { control: true, ..Mods::default() };
        assert_eq!(parse_stroke("Ctrl--"), Ok(("-".into(), control)));
        assert_eq!(parse_stroke("Ctrl-="), Ok(("=".into(), control)));
        let command_alt = Mods { command: true, alt: true, ..Mods::default() };
        assert_eq!(parse_stroke("⌘-⌥-Left"), Ok(("ArrowLeft".into(), command_alt)));
        assert_eq!(parse_stroke("F11"), Ok(("F11".into(), Mods::default())));
        assert!(parse_stroke("Ctrl-Shift").is_err());
    }

    #[test]
    fn light_and_dark_schemes_become_pairs() {
        let doc = convert(
            "appearance:\n  colorSchemeMode: auto\nterminal:\n  colorScheme:\n    background: \
             '#000000'\n  lightColorScheme:\n    background: '#ffffff'\n",
        )
        .unwrap();
        assert_eq!(
            doc.doc.to_string(),
            "[colors.primary]\nbackground = { light = \"#ffffff\", dark = \"#000000\" }\n"
        );
    }
}
