//! Conversion from the TankTerm configuration to `iced_term` settings.

use iced::Font;
use iced::font::Family;
use iced_term::ColorPalette;
use iced_term::settings::{BackendSettings, FontSettings, Settings, ThemeSettings};

use crate::cli::Options;
use crate::config::UiConfig;
use crate::config::color::Colors;
use crate::config::font::FontDescription;
use crate::display::color::{DIM_FACTOR, Rgb};

/// Logical pixels per typographic point.
///
/// TankTerm font sizes are in points, while iced sizes text in logical pixels at 96 DPI.
const PX_PER_PT: f32 = 96. / 72.;

/// Shell used when neither the CLI, the config nor `$SHELL` specify one.
#[cfg(not(windows))]
const FALLBACK_SHELL: &str = "/bin/sh";
#[cfg(windows)]
const FALLBACK_SHELL: &str = "powershell";

/// Build the terminal widget settings from the config and CLI options.
pub fn settings(config: &UiConfig, options: &Options) -> Settings {
    Settings {
        font: font_settings(config),
        theme: ThemeSettings::new(Box::new(palette(&config.colors))),
        backend: backend_settings(config, options, std::env::var("SHELL").ok()),
    }
}

fn font_settings(config: &UiConfig) -> FontSettings {
    FontSettings {
        size: config.font.size().as_pt() * PX_PER_PT,
        font_type: font(config.font.normal()),
        ..FontSettings::default()
    }
}

/// Convert a config font description into an iced font.
fn font(description: &FontDescription) -> Font {
    let family = if description.family.eq_ignore_ascii_case("monospace") {
        Family::Monospace
    } else {
        // iced needs a `'static` family name. Fonts are only resolved on startup, so leaking the
        // name once is fine.
        Family::Name(Box::leak(description.family.clone().into_boxed_str()))
    };

    Font { family, ..Font::MONOSPACE }
}

/// Convert the configured colors into an `iced_term` palette.
fn palette(colors: &Colors) -> ColorPalette {
    let hex = |color: Rgb| color.to_string();
    let primary = &colors.primary;
    let (normal, bright) = (&colors.normal, &colors.bright);

    // Like the OpenGL renderer, derive dim colors from the normal ones when they aren't set.
    let dim = |configured: Option<Rgb>, normal: Rgb| hex(configured.unwrap_or(normal * DIM_FACTOR));
    let dim_colors = colors.dim.as_ref();

    ColorPalette {
        foreground: hex(primary.foreground),
        background: hex(primary.background),
        black: hex(normal.black),
        red: hex(normal.red),
        green: hex(normal.green),
        yellow: hex(normal.yellow),
        blue: hex(normal.blue),
        magenta: hex(normal.magenta),
        cyan: hex(normal.cyan),
        white: hex(normal.white),
        bright_black: hex(bright.black),
        bright_red: hex(bright.red),
        bright_green: hex(bright.green),
        bright_yellow: hex(bright.yellow),
        bright_blue: hex(bright.blue),
        bright_magenta: hex(bright.magenta),
        bright_cyan: hex(bright.cyan),
        bright_white: hex(bright.white),
        bright_foreground: primary.bright_foreground.map(hex),
        dim_foreground: dim(primary.dim_foreground, primary.foreground),
        dim_black: dim(dim_colors.map(|d| d.black), normal.black),
        dim_red: dim(dim_colors.map(|d| d.red), normal.red),
        dim_green: dim(dim_colors.map(|d| d.green), normal.green),
        dim_yellow: dim(dim_colors.map(|d| d.yellow), normal.yellow),
        dim_blue: dim(dim_colors.map(|d| d.blue), normal.blue),
        dim_magenta: dim(dim_colors.map(|d| d.magenta), normal.magenta),
        dim_cyan: dim(dim_colors.map(|d| d.cyan), normal.cyan),
        dim_white: dim(dim_colors.map(|d| d.white), normal.white),
    }
}

/// Resolve the shell, working directory and environment for the terminal.
///
/// The shell is taken from the CLI command first, then `terminal.shell` in the config, then
/// `env_shell` (the `$SHELL` variable).
fn backend_settings(
    config: &UiConfig,
    options: &Options,
    env_shell: Option<String>,
) -> BackendSettings {
    let terminal_options = &options.window_options.terminal_options;

    let (program, args) = match terminal_options.command().or_else(|| config.terminal.shell.clone())
    {
        Some(program) => (program.program().to_owned(), program.args().to_vec()),
        None => (env_shell.unwrap_or_else(|| FALLBACK_SHELL.into()), Vec::new()),
    };

    let working_directory = terminal_options
        .working_directory
        .clone()
        .filter(|dir| dir.is_dir())
        .or_else(|| config.general.working_directory.clone());

    BackendSettings { program, args, env: config.env.clone(), working_directory }
}

/// Window title, with the CLI `--title` taking precedence over the config.
pub fn window_title(config: &UiConfig, options: &Options) -> String {
    let mut identity = config.window.identity.clone();
    options.window_options.window_identity.override_identity_config(&mut identity);
    identity.title
}

/// Window class (X11) or app ID (Wayland), with the CLI `--class` taking precedence.
#[cfg(not(any(target_os = "macos", windows)))]
pub fn window_class(config: &UiConfig, options: &Options) -> String {
    let mut identity = config.window.identity.clone();
    options.window_options.window_identity.override_identity_config(&mut identity);
    identity.class.general
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::PathBuf;

    use clap::Parser;

    use crate::config::color::DimColors;
    use crate::config::ui_config::Program;

    fn options(args: &[&str]) -> Options {
        let mut options = Options::try_parse_from(["tankTerm"].iter().chain(args)).unwrap();
        options.config_options = options.window_options.config_overrides();
        options
    }

    fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb::new(r, g, b)
    }

    #[test]
    fn font_size_points_to_pixels() {
        let mut config = UiConfig::default();
        config.font = config.font.clone().with_size(crossfont::Size::new(12.));

        // 12pt at 96 DPI is 16 logical pixels.
        assert_eq!(font_settings(&config).size, 16.);
    }

    #[test]
    fn monospace_family_maps_to_generic_monospace() {
        let description = FontDescription { family: "MonoSpace".into(), style: None };
        assert_eq!(font(&description).family, Family::Monospace);
    }

    #[test]
    fn named_family_is_kept() {
        let description = FontDescription { family: "JetBrains Mono".into(), style: None };
        assert_eq!(font(&description).family, Family::Name("JetBrains Mono"));
    }

    #[test]
    fn palette_uses_configured_colors() {
        let mut colors = Colors::default();
        colors.primary.foreground = rgb(0x11, 0x22, 0x33);
        colors.primary.background = rgb(0x44, 0x55, 0x66);
        colors.normal.red = rgb(0xff, 0x00, 0x00);
        colors.bright.blue = rgb(0x00, 0x00, 0xff);
        colors.primary.bright_foreground = Some(rgb(0xee, 0xee, 0xee));

        let palette = palette(&colors);

        assert_eq!(palette.foreground, "#112233");
        assert_eq!(palette.background, "#445566");
        assert_eq!(palette.red, "#ff0000");
        assert_eq!(palette.bright_blue, "#0000ff");
        assert_eq!(palette.bright_foreground.as_deref(), Some("#eeeeee"));
    }

    #[test]
    fn palette_derives_missing_dim_colors() {
        let mut colors = Colors::default();
        colors.primary.foreground = rgb(100, 200, 50);
        colors.primary.dim_foreground = None;
        colors.normal.green = rgb(0, 150, 0);
        colors.dim = None;

        let palette = palette(&colors);

        assert_eq!(palette.dim_foreground, (rgb(100, 200, 50) * DIM_FACTOR).to_string());
        assert_eq!(palette.dim_green, (rgb(0, 150, 0) * DIM_FACTOR).to_string());
    }

    #[test]
    fn palette_uses_configured_dim_colors() {
        let mut colors = Colors::default();
        colors.primary.dim_foreground = Some(rgb(1, 2, 3));
        colors.dim = Some(DimColors { cyan: rgb(4, 5, 6), ..DimColors::default() });

        let palette = palette(&colors);

        assert_eq!(palette.dim_foreground, "#010203");
        assert_eq!(palette.dim_cyan, "#040506");
    }

    #[test]
    fn shell_from_cli_command_wins() {
        let mut config = UiConfig::default();
        config.terminal.shell = Some(Program::Just("config-shell".into()));
        let options = options(&["-e", "htop", "-d", "10"]);

        let backend = backend_settings(&config, &options, Some("/bin/env-shell".into()));

        assert_eq!(backend.program, "htop");
        assert_eq!(backend.args, ["-d", "10"]);
    }

    #[test]
    fn shell_from_config_before_env() {
        let mut config = UiConfig::default();
        config.terminal.shell =
            Some(Program::WithArgs { program: "fish".into(), args: vec!["-l".into()] });

        let backend = backend_settings(&config, &options(&[]), Some("/bin/env-shell".into()));

        assert_eq!(backend.program, "fish");
        assert_eq!(backend.args, ["-l"]);
    }

    #[test]
    fn shell_from_env_then_fallback() {
        let config = UiConfig::default();

        let backend = backend_settings(&config, &options(&[]), Some("/bin/env-shell".into()));
        assert_eq!(backend.program, "/bin/env-shell");

        let backend = backend_settings(&config, &options(&[]), None);
        assert_eq!(backend.program, FALLBACK_SHELL);
        assert!(backend.args.is_empty());
    }

    #[test]
    fn working_directory_cli_then_config() {
        let cli_dir = tempfile::tempdir().unwrap();
        let mut config = UiConfig::default();
        config.general.working_directory = Some(PathBuf::from("/config/dir"));

        let cli = options(&["--working-directory", cli_dir.path().to_str().unwrap()]);
        let backend = backend_settings(&config, &cli, None);
        assert_eq!(backend.working_directory.as_deref(), Some(cli_dir.path()));

        // A CLI directory that doesn't exist falls back to the config.
        let missing = options(&["--working-directory", "/does/not/exist/tankterm"]);
        let backend = backend_settings(&config, &missing, None);
        assert_eq!(backend.working_directory, Some(PathBuf::from("/config/dir")));
    }

    #[test]
    fn env_is_passed_to_backend() {
        let mut config = UiConfig::default();
        config.env.insert("TANKTERM_TEST".into(), "1".into());

        let backend = backend_settings(&config, &options(&[]), None);

        assert_eq!(backend.env.get("TANKTERM_TEST").map(String::as_str), Some("1"));
    }

    #[test]
    fn title_cli_overrides_config() {
        let mut config = UiConfig::default();
        config.window.identity.title = "Config Title".into();

        assert_eq!(window_title(&config, &options(&[])), "Config Title");
        assert_eq!(window_title(&config, &options(&["--title", "CLI Title"])), "CLI Title");
    }

    #[cfg(not(any(target_os = "macos", windows)))]
    #[test]
    fn class_cli_overrides_config() {
        let config = UiConfig::default();

        assert_eq!(window_class(&config, &options(&[])), "TankTerm");
        assert_eq!(window_class(&config, &options(&["--class", "MyClass"])), "MyClass");
    }
}
