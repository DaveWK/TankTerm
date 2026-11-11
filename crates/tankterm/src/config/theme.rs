//! Themes: configuration files layered underneath the user's configuration.
//!
//! Like Glamour styles and Huh themes, a theme is selected by name or by path. Built-in themes
//! are TOML files in `extra/themes` using TankTerm's own config syntax, so they can use light/dark
//! color pairs, CharmTone color names and `[style]` blocks. Everything the user's configuration
//! sets takes precedence over the theme.

use std::path::{Path, PathBuf};

use toml::Value;

use crate::config::serde_utils;

/// Built-in themes, by name.
const BUILTIN: &[(&str, &str)] = &[
    ("green_screen", include_str!("../../extra/themes/green_screen.toml")),
    ("charm", include_str!("../../extra/themes/charm.toml")),
    ("dracula", include_str!("../../extra/themes/dracula.toml")),
    ("catppuccin", include_str!("../../extra/themes/catppuccin.toml")),
    ("base16", include_str!("../../extra/themes/base16.toml")),
];

/// Names of the built-in themes.
pub fn builtin_names() -> impl Iterator<Item = &'static str> {
    BUILTIN.iter().map(|(name, _)| *name)
}

/// Layer the selected theme underneath `config`.
///
/// The theme comes from `cli` (the `--theme` flag or `TANKTERM_THEME`), or else the
/// configuration's own `theme` key. Relative theme paths are resolved against `config_dir`.
pub fn apply(config: Value, cli: Option<&str>, config_dir: Option<&Path>) -> Result<Value, String> {
    let selected = cli
        .map(ToOwned::to_owned)
        .or_else(|| config.get("theme").and_then(Value::as_str).map(ToOwned::to_owned));

    match selected {
        Some(theme) => Ok(serde_utils::merge(load(&theme, config_dir)?, config)),
        None => Ok(config),
    }
}

/// Load a theme by built-in name or file path.
fn load(theme: &str, config_dir: Option<&Path>) -> Result<Value, String> {
    let name = theme.to_lowercase().replace(['-', ' '], "_");
    if let Some((_, content)) = BUILTIN.iter().find(|(builtin, _)| *builtin == name) {
        return toml::from_str(content).map_err(|err| format!("invalid theme {theme:?}: {err}"));
    }

    let path = theme_path(theme, config_dir);
    if !path.is_file() {
        let builtin = builtin_names().collect::<Vec<_>>().join(", ");
        return Err(format!("unknown theme {theme:?}; expected a file or one of: {builtin}"));
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|err| format!("unable to read theme {path:?}: {err}"))?;
    toml::from_str(&content).map_err(|err| format!("invalid theme {path:?}: {err}"))
}

/// Expand `~/` and resolve relative paths against the configuration's directory.
fn theme_path(theme: &str, config_dir: Option<&Path>) -> PathBuf {
    if let (Some(stripped), Some(home)) = (theme.strip_prefix("~/"), home::home_dir()) {
        return home.join(stripped);
    }

    let path = PathBuf::from(theme);
    match config_dir {
        Some(dir) if path.is_relative() => dir.join(path),
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(toml: &str) -> Value {
        toml::from_str(toml).unwrap()
    }

    #[test]
    fn builtin_themes_parse() {
        for name in builtin_names() {
            load(name, None).unwrap_or_else(|err| panic!("{name}: {err}"));
        }
    }

    #[test]
    fn no_theme_keeps_config() {
        let config = value("[colors.primary]\nforeground = '#ffffff'\n");
        assert_eq!(apply(config.clone(), None, None).unwrap(), config);
    }

    #[test]
    fn config_overrides_theme() {
        let config = value("theme = 'dracula'\n[colors.primary]\nforeground = '#123456'\n");
        let merged = apply(config, None, None).unwrap();

        // Set by the user.
        assert_eq!(merged["colors"]["primary"]["foreground"].as_str(), Some("#123456"));
        // Set by the theme.
        assert_eq!(merged["colors"]["primary"]["background"].as_str(), Some("#282a36"));
    }

    #[test]
    fn cli_overrides_config_theme() {
        let config = value("theme = 'dracula'\n");
        let merged = apply(config, Some("base16"), None).unwrap();

        assert_eq!(merged["colors"]["primary"]["background"].as_str(), Some("#181818"));
    }

    #[test]
    fn names_are_normalized() {
        assert!(load("Green-Screen", None).is_ok());
        assert!(load("CATPPUCCIN", None).is_ok());
    }

    #[test]
    fn theme_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mine.toml"), "[colors.primary]\nbackground = '#010203'\n")
            .unwrap();

        // Relative to the configuration's directory.
        let relative = apply(value("theme = 'mine.toml'"), None, Some(dir.path())).unwrap();
        assert_eq!(relative["colors"]["primary"]["background"].as_str(), Some("#010203"));

        // Absolute.
        let absolute = dir.path().join("mine.toml");
        let absolute = apply(value(""), absolute.to_str(), None).unwrap();
        assert_eq!(absolute["colors"]["primary"]["background"].as_str(), Some("#010203"));
    }

    #[test]
    fn unknown_theme_is_an_error() {
        let err = apply(value("theme = 'nope'"), None, None).unwrap_err();
        assert!(err.contains("unknown theme") && err.contains("charm"), "{err}");
    }
}
