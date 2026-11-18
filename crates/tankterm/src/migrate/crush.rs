//! Crush: the theme in use, from Crush's global JSON files and its theme files.
//!
//! Crush keeps user themes as JSON files in `themes/` of its configuration directory: a flat
//! object of palette keys, with an optional `base` theme they inherit from, Crush's default
//! `charmtone-panther` when it's left out. Of these keys, the base colors and the 16 ANSI colors
//! are terminal colors; the rest color Crush's own interface. The theme in use is
//! `options.tui.active_theme`, which Crush's Themes dialog saves in its global data file.
//! Crush's built-in themes are Go code, so only `charmtone-panther` has a TankTerm
//! counterpart: the `charm` theme, made from the same CharmTone palette.
//!
//! Crush's `crushrc` (Bash) has no theme setting, so it isn't read; neither is anything else of
//! the JSON files, which hold API keys.

use std::collections::HashSet;
use std::env;
use std::path::{Path, PathBuf};

use serde_json::Value as Json;

use crate::color::charmtone;
use crate::migrate::dirs;
use crate::migrate::output::{ANSI_NAMES, Doc, Plan, color};

/// Crush's default theme.
const DEFAULT_THEME: &str = "charmtone-panther";

/// Crush's global configuration directory: `$CRUSH_GLOBAL_CONFIG`, or `crush` in
/// `$XDG_CONFIG_HOME` or `~/.config`, on every platform.
fn config_dir() -> Option<PathBuf> {
    match env::var_os("CRUSH_GLOBAL_CONFIG").filter(|dir| !dir.is_empty()) {
        Some(dir) => Some(PathBuf::from(dir)),
        None => dirs::config_home().map(|dir| dir.join("crush")),
    }
}

/// Crush's global data file, where it saves the theme chosen in its Themes dialog.
fn data_file() -> Option<PathBuf> {
    let var = |name: &str| env::var_os(name).filter(|value| !value.is_empty()).map(PathBuf::from);
    if let Some(dir) = var("CRUSH_GLOBAL_DATA") {
        return Some(dir.join("crush.json"));
    }
    if let Some(dir) = var("XDG_DATA_HOME") {
        return Some(dir.join("crush").join("crush.json"));
    }

    #[cfg(windows)]
    let dir =
        var("LOCALAPPDATA").or_else(|| Some(var("USERPROFILE")?.join("AppData").join("Local")))?;
    #[cfg(not(windows))]
    let dir = dirs::home()?.join(".local").join("share");

    Some(dir.join("crush").join("crush.json"))
}

/// Crush's global JSON files, in the order it reads them; a later file's theme wins.
pub fn locations() -> Vec<PathBuf> {
    let mut locations: Vec<PathBuf> =
        config_dir().map(|dir| dir.join("crush.json")).into_iter().collect();
    locations.extend(data_file());
    locations
}

/// Plan the conversion of Crush's theme: the one the JSON configuration at `path` selects, or
/// the theme file at `path`.
///
/// When the file was found in Crush's own locations, the other global file is read too, as
/// Crush does.
pub fn plan(path: &Path, target: &Path, detected: bool) -> Result<Plan, String> {
    let files: Vec<PathBuf> = if detected {
        locations().into_iter().filter(|file| file.is_file()).collect()
    } else {
        vec![path.to_owned()]
    };

    // Crush looks for themes in its configuration directory; a file given by hand may have its
    // themes next to it.
    let dir = path.parent().unwrap_or(Path::new(""));
    let mut theme_dirs = vec![dir.to_owned(), dir.join("themes")];
    theme_dirs.extend(config_dir().map(|dir| dir.join("themes")));

    plan_files(path, target, &files, &theme_dirs)
}

/// [`plan`], with the configuration files to read and the theme directories given.
fn plan_files(
    path: &Path,
    target: &Path,
    files: &[PathBuf],
    theme_dirs: &[PathBuf],
) -> Result<Plan, String> {
    let json = read(path)?;
    let mut doc = Doc::default();

    let name = path.file_name().unwrap_or_default();
    let is_config = files.len() > 1
        || name == "crush.json"
        || name == ".crush.json"
        || json.as_object().is_some_and(|fields| fields.values().any(Json::is_object));

    if is_config {
        let mut theme = None;
        for file in files {
            let json = if file == path { json.clone() } else { read(file)? };
            if let Some(name) = active_theme(&json) {
                theme = Some(name.to_owned());
            }
        }
        let (name, from) = match &theme {
            Some(name) => (name.as_str(), format!("options.tui.active_theme {name}")),
            None => (DEFAULT_THEME, format!("options.tui.active_theme (unset: {DEFAULT_THEME})")),
        };
        // A theme file next to the configuration itself isn't one of Crush's.
        theme_by_name(&mut doc, name, &from, &theme_dirs[1..], &mut HashSet::new())?;
    } else {
        theme_file(&mut doc, path, theme_dirs, &mut HashSet::new())?;
    }

    Ok(Plan::single("Crush", path, target, doc))
}

/// The theme a Crush configuration selects, with the old `theme` name as a fallback.
fn active_theme(json: &Json) -> Option<&str> {
    json.pointer("/options/tui/active_theme")
        .and_then(Json::as_str)
        .filter(|name| !name.is_empty())
        .or_else(|| json.pointer("/options/tui/theme").and_then(Json::as_str))
        .filter(|name| !name.is_empty())
}

fn read(path: &Path) -> Result<Json, String> {
    let text = std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
    serde_json::from_str(&text).map_err(|err| format!("{}: JSON error: {err}", path.display()))
}

/// A theme name as Crush looks it up: lowercase, with its old names renamed.
fn normalize(name: &str) -> String {
    match name.to_lowercase().as_str() {
        "charmtone" => DEFAULT_THEME.to_owned(),
        name => name.to_owned(),
    }
}

/// The theme file of `name`; Crush matches the file name without regard to case.
fn find_theme(name: &str, theme_dirs: &[PathBuf]) -> Option<PathBuf> {
    let file_name = format!("{name}.json");
    theme_dirs.iter().find_map(|dir| {
        std::fs::read_dir(dir).ok()?.flatten().map(|entry| entry.path()).find(|path| {
            path.is_file()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(&file_name))
        })
    })
}

/// Convert the theme `name`: a user theme file, which wins over a built-in theme of the same
/// name, or a built-in theme.
fn theme_by_name(
    doc: &mut Doc,
    name: &str,
    from: &str,
    theme_dirs: &[PathBuf],
    seen: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    let name = normalize(name);
    match find_theme(&name, theme_dirs).filter(|file| !seen.contains(file)) {
        Some(file) => theme_file(doc, &file, theme_dirs, seen),
        None => {
            builtin(doc, &name, from);
            Ok(())
        },
    }
}

/// A built-in Crush theme.
fn builtin(doc: &mut Doc, theme: &str, from: &str) {
    if theme == DEFAULT_THEME {
        doc.set(from, "theme", "charm");
    } else {
        doc.skip(from, "a theme built into Crush; its colors are in Crush's code, not in a file");
    }
}

/// Convert a theme file, after its base.
fn theme_file(
    doc: &mut Doc,
    path: &Path,
    theme_dirs: &[PathBuf],
    seen: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    seen.insert(path.to_owned());
    let json = read(path)?;
    let Some(fields) = json.as_object() else {
        return Err(format!("{}: not a Crush theme", path.display()));
    };
    let name = path.file_stem().unwrap_or_default().to_string_lossy().into_owned();

    let base = fields.get("base").and_then(Json::as_str).filter(|base| !base.is_empty());
    let base_from = match base {
        Some(base) => format!("{name}.base {base}"),
        None => format!("{name}.base (unset: {DEFAULT_THEME})"),
    };
    theme_by_name(doc, base.unwrap_or(DEFAULT_THEME), &base_from, theme_dirs, seen)?;

    for (key, value) in fields {
        let from = format!("{name}.{key}");
        let to = match key.as_str() {
            "base" | "name" | "$schema" => continue,
            "bg_base" => "colors.primary.background".to_owned(),
            "fg_base" => "colors.primary.foreground".to_owned(),
            key => match ansi(key) {
                Some(to) => to,
                None => {
                    doc.skip(from, "a color of Crush's own interface");
                    continue;
                },
            },
        };
        match theme_color(value) {
            // An ANSI color given as one of the first 16 colors would refer to itself.
            Some(value) if to.starts_with("colors.normal") || to.starts_with("colors.bright") => {
                if value.parse::<u8>().is_ok_and(|index| index < 16) {
                    doc.skip(from, "refers to the terminal's own ANSI colors");
                } else {
                    doc.set(&from, &to, value);
                }
            },
            Some(value) => doc.set(&from, &to, value),
            None => doc.skip(from, "not a color"),
        }
    }
    Ok(())
}

/// The TankTerm key of an `ansi_*` palette key.
fn ansi(key: &str) -> Option<String> {
    let name = key.strip_prefix("ansi_")?;
    let (table, name) = match name.strip_prefix("bright_") {
        Some(name) => ("bright", name),
        None => ("normal", name),
    };
    ANSI_NAMES.contains(&name).then(|| format!("colors.{table}.{name}"))
}

/// A Crush color: hex, an ANSI index, or a CharmTone name, which TankTerm reads as it is.
fn theme_color(value: &Json) -> Option<String> {
    match value {
        Json::Number(n) => n.as_u64().filter(|n| *n < 256).map(|n| n.to_string()),
        Json::String(s) if s.parse::<u8>().is_ok() => Some(s.clone()),
        Json::String(s) if charmtone::color(&s.to_lowercase()).is_some() => Some(s.to_lowercase()),
        Json::String(s) if s.starts_with('#') => color(s),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use crate::color::Rgb;
    use crate::config::UiConfig;

    /// Plan the file at `path` alone, with the theme directories in `dir`, without Crush's own.
    fn plan_one(path: &Path, target: &Path, dir: &Path) -> Plan {
        let theme_dirs = [dir.to_owned(), dir.join("themes")];
        plan_files(path, target, std::slice::from_ref(&path.to_owned()), &theme_dirs).unwrap()
    }

    #[test]
    fn theme_from_config() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("themes")).unwrap();
        fs::write(
            dir.path().join("crush.json"),
            r#"{ "providers": { "x": { "api_key": "secret" } }, "options": { "tui": { "active_theme": "Mine" } } }"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("themes/mine.json"),
            r##"{ "base": "parent", "bg_base": "#101010", "ansi_red": "Coral", "ansi_bright_black": "8", "ansi_white": "250", "primary": "#ff00ff", "fg_base": "nope" }"##,
        )
        .unwrap();
        fs::write(dir.path().join("themes/parent.json"), r##"{ "fg_base": "#eeeeee" }"##).unwrap();

        let config = dir.path().join("crush.json");
        let plan = plan_one(&config, &dir.path().join("t.toml"), dir.path());
        let contents = &plan.files[0].contents;
        assert!(!contents.contains("secret"), "{contents}");
        // The parent theme has no base, so it inherits from Crush's default theme.
        assert!(contents.contains("theme = \"charm\""), "{contents}");

        let config: UiConfig = toml::from_str(contents).unwrap();
        assert_eq!(config.colors.primary.background, Rgb::new(0x10, 0x10, 0x10));
        assert_eq!(config.colors.primary.foreground, Rgb::new(0xee, 0xee, 0xee));
        assert!(contents.contains("red = \"coral\""), "{contents}");
        assert!(contents.contains("white = \"250\""), "{contents}");

        let skipped: Vec<_> = plan.report.skipped.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(skipped, ["mine.ansi_bright_black", "mine.fg_base", "mine.primary"]);
    }

    #[test]
    fn builtin_themes() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("crush.json");
        let target = dir.path().join("t.toml");

        fs::write(&config, r#"{ "options": { "tui": { "active_theme": "rose-pine" } } }"#).unwrap();
        let plan = plan_one(&config, &target, dir.path());
        assert_eq!(plan.report.skipped.len(), 1);
        assert!(plan.report.converted.is_empty());

        // The old name of the default theme, under the old key.
        fs::write(&config, r#"{ "options": { "tui": { "theme": "charmtone" } } }"#).unwrap();
        let plan = plan_one(&config, &target, dir.path());
        assert_eq!(plan.files[0].contents.lines().last(), Some("theme = \"charm\""));

        // Without a theme, Crush uses its default.
        fs::write(&config, r#"{ "$schema": "https://charm.land/crush.json" }"#).unwrap();
        let plan = plan_one(&config, &target, dir.path());
        assert_eq!(plan.report.converted.len(), 1);
    }

    #[test]
    fn user_theme_shadows_a_builtin_and_can_extend_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("themes")).unwrap();
        fs::write(
            dir.path().join("themes/charmtone-panther.json"),
            r##"{ "base": "charmtone-panther", "ansi_blue": "#0000ff" }"##,
        )
        .unwrap();

        let theme = dir.path().join("themes/charmtone-panther.json");
        let plan = plan_one(&theme, &dir.path().join("t.toml"), dir.path());
        let contents = &plan.files[0].contents;
        assert!(contents.contains("theme = \"charm\""), "{contents}");
        assert!(contents.contains("blue = \"#0000ff\""), "{contents}");
    }
}
