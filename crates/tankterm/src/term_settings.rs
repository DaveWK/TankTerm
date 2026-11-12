//! Conversion from the TankTerm configuration to `iced_tterm` settings.

use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use iced::font::Family;
use iced::{Color, Font, Padding, Size};
use iced_core::alignment::Vertical;
use iced_core::text::{Alignment, LineHeight, Paragraph as _, Shaping, Text, Wrapping};
use iced_graphics::text::paragraph::Paragraph;
use iced_tterm::ColorPalette;
use iced_tterm::settings::{
    BackendSettings, CursorBlinking, CursorSettings, FontSettings, ScrollSettings, Settings,
    ThemeSettings,
};
use log::warn;

use crate::cli::Options;
use crate::color::{DIM_FACTOR, Rgb};
use crate::config::UiConfig;
use crate::config::color::Colors;
use crate::config::font::FontDescription;
use crate::config::profile::Profile;
use crate::config::ui_config::Program;

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
        // The application draws the background itself, behind the terminal and its padding in
        // one piece, so that it is equally transparent everywhere with `window.opacity`.
        theme: ThemeSettings {
            color_pallete: Box::new(palette(&config.colors)),
            background_opacity: 0.,
        },
        backend: backend_settings(config, options, std::env::var("SHELL").ok()),
        scroll: scroll_settings(config),
        cursor: cursor_settings(config),
    }
}

/// The cursor's shape and blinking.
fn cursor_settings(config: &UiConfig) -> CursorSettings {
    let cursor = &config.cursor;
    let style = cursor.style();

    // What the application in the terminal asks for decides unless the config forces blinking
    // on or off.
    let blinking = match (cursor.style.blinking_override(), style.blinking) {
        (Some(false), _) => CursorBlinking::Never,
        (Some(true), _) => CursorBlinking::Always,
        (None, false) => CursorBlinking::Off,
        (None, true) => CursorBlinking::On,
    };
    let timeout = cursor.blink_timeout();

    CursorSettings {
        shape: style.shape,
        blinking,
        blink_interval: Duration::from_millis(cursor.blink_interval()),
        blink_timeout: (!timeout.is_zero()).then_some(timeout),
        unfocused_hollow: cursor.unfocused_hollow,
        thickness: cursor.thickness(),
    }
}

/// The scrollback size and the lines scrolled per step of the mouse wheel.
fn scroll_settings(config: &UiConfig) -> ScrollSettings {
    ScrollSettings {
        history: config.scrolling.history() as usize,
        multiplier: f32::from(config.scrolling.multiplier),
    }
}

/// Settings for the terminals of new tabs and panes.
///
/// These run the configured shell, not the command the first terminal was started with on the
/// command line.
pub fn tab_settings(config: &UiConfig, options: &Options) -> Settings {
    let mut settings = settings(config, options);
    let (program, args) = shell(None, config, std::env::var("SHELL").ok());
    settings.backend.program = program;
    settings.backend.args = args;

    settings
}

fn font_settings(config: &UiConfig) -> FontSettings {
    FontSettings {
        size: config.font.size() * PX_PER_PT,
        font_type: font(config.font.normal()),
        ..FontSettings::default()
    }
}

/// Convert a config font description into an iced font.
///
/// iced replaces families it can't find with a proportional font, which breaks the terminal grid.
/// Missing families are replaced with the system monospace font instead, like the OpenGL renderer
/// did.
fn font(description: &FontDescription) -> Font {
    let family = family(&description.family, family_installed).unwrap_or_else(|| {
        let system = FontDescription::system();
        warn!("Font {:?} not found, using {:?} instead", description.family, system.family);
        family(&system.family, family_installed).unwrap_or(Family::Monospace)
    });

    Font { family, ..Font::MONOSPACE }
}

/// Resolve a configured family name, or `None` if it isn't installed.
fn family(name: &str, installed: impl Fn(&str) -> bool) -> Option<Family> {
    if name.eq_ignore_ascii_case("monospace") {
        Some(Family::Monospace)
    } else if installed(name) {
        Some(Family::Name(static_name(name)))
    } else {
        None
    }
}

/// A family name that lives as long as the program, which is what iced needs.
///
/// Every name is only leaked once, however often the configuration is read.
fn static_name(name: &str) -> &'static str {
    static NAMES: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

    let mut names = NAMES.lock().unwrap_or_else(PoisonError::into_inner);
    match names.iter().find(|known| **known == name) {
        Some(known) => known,
        None => {
            let leaked: &'static str = Box::leak(name.to_owned().into_boxed_str());
            names.push(leaked);
            leaked
        },
    }
}

/// Check whether iced's text renderer has a font with this family name.
///
/// This uses iced's own font database, so it sees the same fonts the renderer will.
fn family_installed(name: &str) -> bool {
    let Ok(mut font_system) = iced_graphics::text::font_system().write() else {
        // Let iced pick on its own if the font database is unavailable.
        return true;
    };

    font_system
        .raw()
        .db()
        .faces()
        .any(|face| face.families.iter().any(|(family, _)| family.eq_ignore_ascii_case(name)))
}

/// Settings for the terminals of a profile.
///
/// `config` is the configuration with the profile's theme, if it has one. What the profile
/// doesn't set is the same as for other new tabs.
pub fn profile_settings(profile: &Profile, config: &UiConfig, options: &Options) -> Settings {
    let mut settings = tab_settings(config, options);

    if let Some(shell) = &profile.shell {
        settings.backend.program = shell.program().to_owned();
        settings.backend.args = shell.args().to_vec();
    }
    if let Some(directory) = &profile.working_directory {
        settings.backend.working_directory = Some(directory.clone());
    }
    settings.backend.env.extend(profile.env.clone());

    settings
}

/// Size of a terminal cell, measured the same way `iced_tterm` measures its grid.
pub fn cell_size(font: &FontSettings) -> Size {
    Paragraph::with_text(Text {
        content: "m",
        font: font.font_type,
        size: iced::Pixels(font.size),
        align_y: Vertical::Center,
        align_x: Alignment::Center,
        shaping: Shaping::Advanced,
        line_height: LineHeight::Relative(font.scale_factor),
        bounds: Size::INFINITE,
        wrapping: Wrapping::Glyph,
    })
    .min_bounds()
}

/// Padding around the terminal, in logical pixels.
///
/// iced lays out in logical pixels and applies the display's scale factor itself, so pixel
/// padding isn't scaled here.
pub fn padding(config: &UiConfig, cell: Size) -> Padding {
    let padding = config.window.padding(1., cell.width, cell.height);
    Padding { top: padding.top, right: padding.right, bottom: padding.bottom, left: padding.left }
}

/// Size of a window showing the configured `window.dimensions`, in logical pixels.
///
/// `None` without dimensions, which leaves the size to the default. The tab bar isn't counted, so
/// a window that always shows it has slightly fewer lines.
pub fn window_size(config: &UiConfig, cell: Size, padding: Padding) -> Option<Size> {
    let dimensions = config.window.dimensions()?;

    Some(Size::new(
        dimensions.columns as f32 * cell.width + padding.left + padding.right,
        dimensions.lines as f32 * cell.height + padding.top + padding.bottom,
    ))
}

/// Background color of the window, drawn behind the terminals and their padding.
pub fn background(config: &UiConfig) -> Color {
    let background = config.colors.primary.background;
    Color::from_rgb8(background.r, background.g, background.b)
}

/// Text color of the window, used for the tab bar and pane frames.
pub fn foreground(config: &UiConfig) -> Color {
    let foreground = config.colors.primary.foreground;
    Color::from_rgb8(foreground.r, foreground.g, foreground.b)
}

/// Convert the configured colors into an `iced_tterm` palette.
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

    let (program, args) = shell(terminal_options.command(), config, env_shell);

    let working_directory = terminal_options
        .working_directory
        .clone()
        .filter(|dir| dir.is_dir())
        .or_else(|| config.general.working_directory.clone());

    BackendSettings { program, args, env: config.env.clone(), working_directory }
}

/// The program to run in a terminal and its arguments.
///
/// This is `command` if there is one, then `terminal.shell` in the config, then `env_shell`.
fn shell(
    command: Option<Program>,
    config: &UiConfig,
    env_shell: Option<String>,
) -> (String, Vec<String>) {
    match command.or_else(|| config.terminal.shell.clone()) {
        Some(program) => (program.program().to_owned(), program.args().to_vec()),
        None => (env_shell.unwrap_or_else(|| FALLBACK_SHELL.into()), Vec::new()),
    }
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
        config.font = config.font.clone().with_size(12.);

        // 12pt at 96 DPI is 16 logical pixels.
        assert_eq!(font_settings(&config).size, 16.);
    }

    #[test]
    fn monospace_family_maps_to_generic_monospace() {
        let description = FontDescription { family: "MonoSpace".into(), style: None };
        assert_eq!(font(&description).family, Family::Monospace);
    }

    #[test]
    fn installed_family_is_kept() {
        assert_eq!(family("Iosevka Term", |_| true), Some(Family::Name("Iosevka Term")));
    }

    #[test]
    fn family_names_are_only_kept_once() {
        let first = static_name("TankTerm Test Family");
        let second = static_name(&String::from("TankTerm Test Family"));

        assert_eq!(first, "TankTerm Test Family");
        assert!(std::ptr::eq(first, second));
    }

    #[test]
    fn missing_family_is_none() {
        assert_eq!(family("Iosevka Term", |_| false), None);
    }

    #[test]
    fn missing_font_falls_back_to_monospace_font() {
        let description = FontDescription { family: "TankTerm Missing Font".into(), style: None };
        let family = font(&description).family;

        // The fallback is the system font, never a proportional one.
        let system = FontDescription::system().family;
        let expected = [Family::Monospace, Family::Name(Box::leak(system.into_boxed_str()))];
        assert!(expected.contains(&family), "unexpected fallback {family:?}");
    }

    #[test]
    fn iced_font_database_is_queried() {
        // iced always embeds its icon font, so it must be found on every machine.
        assert!(family_installed("Iced-Icons"));
        assert!(!family_installed("TankTerm Missing Font"));
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

    fn config(toml: &str) -> UiConfig {
        toml::from_str(toml).unwrap()
    }

    #[test]
    fn scrolling_follows_the_config() {
        let defaults = scroll_settings(&UiConfig::default());
        assert_eq!(defaults, ScrollSettings { history: 10_000, multiplier: 3. });

        let config = config("[scrolling]\nhistory = 0\nmultiplier = 7\n");
        assert_eq!(scroll_settings(&config), ScrollSettings { history: 0, multiplier: 7. });
        assert_eq!(settings(&config, &options(&[])).scroll, scroll_settings(&config));
    }

    #[test]
    fn cursor_follows_the_config() {
        use iced_tterm::CursorShape;

        assert_eq!(cursor_settings(&UiConfig::default()), CursorSettings {
            shape: CursorShape::Block,
            blinking: CursorBlinking::Off,
            blink_interval: Duration::from_millis(750),
            blink_timeout: Some(Duration::from_secs(5)),
            unfocused_hollow: true,
            thickness: 0.15,
        });

        let config = config(concat!(
            "[cursor]\n",
            "style = { shape = \"Beam\", blinking = \"Always\" }\n",
            "blink_interval = 200\n",
            "blink_timeout = 0\n",
            "unfocused_hollow = false\n",
            "thickness = 0.5\n",
        ));
        let cursor = cursor_settings(&config);
        assert_eq!(cursor, CursorSettings {
            shape: CursorShape::Beam,
            blinking: CursorBlinking::Always,
            blink_interval: Duration::from_millis(200),
            blink_timeout: None,
            unfocused_hollow: false,
            thickness: 0.5,
        });
        assert_eq!(settings(&config, &options(&[])).cursor, cursor);

        let blinking = |style: &str| {
            let config = self::config(&format!("[cursor]\nstyle = {style}\n"));
            let cursor = cursor_settings(&config);
            (cursor.shape, cursor.blinking)
        };
        assert_eq!(blinking("\"Underline\""), (CursorShape::Underline, CursorBlinking::Off));
        assert_eq!(blinking("{ blinking = \"On\" }"), (CursorShape::Block, CursorBlinking::On));
        assert_eq!(
            blinking("{ shape = \"Beam\", blinking = \"Never\" }"),
            (CursorShape::Beam, CursorBlinking::Never)
        );

        // The timeout is never shorter than a blink, and the interval never below 10 ms.
        let config = self::config("[cursor]\nblink_interval = 1\nblink_timeout = 1\n");
        let cursor = cursor_settings(&config);
        assert_eq!(cursor.blink_interval, Duration::from_millis(10));
        assert_eq!(cursor.blink_timeout, Some(Duration::from_secs(1)));
    }

    #[test]
    fn accessibility_keeps_the_cursor_from_blinking() {
        let mut config = config("[cursor]\nstyle = { shape = \"Beam\", blinking = \"Always\" }\n");
        config.apply_accessibility(true);

        let cursor = cursor_settings(&config);
        assert_eq!(
            (cursor.shape, cursor.blinking),
            (iced_tterm::CursorShape::Beam, CursorBlinking::Never)
        );
    }

    #[test]
    fn pixel_padding_is_logical() {
        let config = config("[window]\npadding = { x = 3, y = 5 }\n");
        let padding = padding(&config, Size::new(10., 20.));

        assert_eq!(padding, Padding { top: 5., right: 3., bottom: 5., left: 3. });
    }

    #[test]
    fn cell_padding_uses_cell_size() {
        let config = config("[window]\npadding = [1, 2, 3, 4]\n");
        let padding = padding(&config, Size::new(10., 20.));

        assert_eq!(padding, Padding { top: 20., right: 20., bottom: 60., left: 40. });
    }

    #[test]
    fn background_is_primary_background() {
        let config = config("[colors.primary]\nbackground = '#102030'\n");
        assert_eq!(background(&config), Color::from_rgb8(0x10, 0x20, 0x30));
    }

    #[test]
    fn terminals_leave_their_background_to_the_application() {
        for config in ["", "[window]\nopacity = 0.5\n"] {
            let settings = settings(&self::config(config), &options(&[]));
            assert_eq!(settings.theme.background_opacity, 0.);
        }
    }

    #[test]
    fn cell_size_is_measured() {
        let cell = cell_size(&FontSettings::default());
        assert!(cell.width > 0. && cell.height > cell.width, "unexpected cell {cell:?}");
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
    #[test]
    fn profiles_override_the_shell_directory_and_environment() {
        let config: UiConfig = toml::from_str(
            r#"
            [env]
            SHARED = "config"
            EDITOR = "vi"

            [terminal]
            shell = "zsh"

            [[profiles]]
            name = "Server"
            shell = { program = "ssh", args = ["server"] }
            working_directory = "/srv"
            env = { EDITOR = "nano" }

            [[profiles]]
            name = "Plain"
            "#,
        )
        .unwrap();
        // The command the window was started with isn't used for profiles.
        let options = options(&["-e", "htop"]);

        let server = profile_settings(&config.profiles[0], &config, &options).backend;
        assert_eq!(
            (server.program.as_str(), server.args.as_slice()),
            ("ssh", &["server".to_owned()][..])
        );
        assert_eq!(server.working_directory, Some(PathBuf::from("/srv")));
        assert_eq!(server.env["EDITOR"], "nano");
        assert_eq!(server.env["SHARED"], "config");

        let plain = profile_settings(&config.profiles[1], &config, &options).backend;
        assert_eq!((plain.program.as_str(), plain.args.len()), ("zsh", 0));
        assert_eq!(plain.env["EDITOR"], "vi");
    }
}
