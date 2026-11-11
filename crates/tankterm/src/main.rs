//! TankTerm - A fast, cross-platform terminal emulator.

#![warn(rust_2018_idioms, future_incompatible)]
#![deny(clippy::all, clippy::if_not_else, clippy::enum_glob_use)]
#![cfg_attr(clippy, deny(warnings))]
// With the default subsystem, 'console', windows creates an additional console
// window for the program.
// This is silently ignored on non-windows systems.
// See https://msdn.microsoft.com/en-us/library/4cc7ya5b.aspx for more details.
#![windows_subsystem = "windows"]

#[cfg(not(any(feature = "x11", feature = "wayland", target_os = "macos", windows)))]
compile_error!(r#"at least one of the "x11"/"wayland" features must be enabled"#);

use std::error::Error;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Mutex;
use std::{env, fs};

use iced::{Point, Size, Task, window};
use log::info;
#[cfg(windows)]
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, FreeConsole};

use tankterm_terminal::tty;

mod app;
mod bell;
mod cli;
mod color;
// Most configuration options belong to the previous OpenGL front end and aren't read by the iced
// front end yet, but are still parsed so existing config files keep loading without errors.
#[allow(dead_code, unused_imports)]
mod config;
mod daemon;
mod hotkeys;
mod logging;
#[cfg(target_os = "macos")]
mod macos;
mod migrate;
mod palette;
#[cfg(windows)]
mod panic;
mod session;
mod setup;
mod term_settings;
mod watch;

use crate::app::App;
use crate::cli::{Options, Subcommands};
use crate::config::UiConfig;
use crate::config::window::{Decorations, StartupMode, WindowLevel};
#[cfg(target_os = "macos")]
use crate::macos::locale;
use crate::setup::{ConfigSource, Setup};

/// Window icon, used on X11. Wayland and macOS take the icon from the desktop entry or bundle,
/// and Windows from the executable's resources.
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
const WINDOW_ICON: &[u8] = include_bytes!("../extra/logo/compat/tankterm-64.png");

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(windows)]
    panic::attach_handler();

    // When linked with the windows subsystem windows won't automatically attach
    // to the console of the parent process, so we do it explicitly. This fails
    // silently if the parent has no console.
    #[cfg(windows)]
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }

    // Load command line options.
    let options = Options::new();

    match options.subcommands {
        #[cfg(unix)]
        Some(Subcommands::Msg(_)) => {
            return Err("`tankTerm msg` is not supported by the iced front end yet".into());
        },
        Some(Subcommands::Migrate(options)) => migrate::migrate(options),
        None => tankterm(options)?,
    }

    Ok(())
}

/// Temporary files stored for TankTerm.
///
/// This stores temporary files to automate their destruction through its `Drop` implementation.
struct TemporaryFiles {
    log_file: Option<PathBuf>,
}

impl Drop for TemporaryFiles {
    fn drop(&mut self) {
        // Clean up logfile.
        if let Some(log_file) = &self.log_file
            && fs::remove_file(log_file).is_ok()
        {
            let _ = writeln!(io::stdout(), "Deleted log file at \"{}\"", log_file.display());
        }
    }
}

/// Run main TankTerm entrypoint.
///
/// Loads the configuration, spawns the shell and runs the iced application until the shell exits
/// or the window is closed.
fn tankterm(mut options: Options) -> Result<(), Box<dyn Error>> {
    // Initialize the logger as soon as possible as to capture output from other subsystems.
    let log_file = logging::initialize(&options).expect("Unable to initialize logger");

    info!("Welcome to TankTerm");
    info!("Version {}", env!("VERSION"));

    unsupported_options(&options)
        .map_err(|option| format!("{option} is not supported by the iced front end yet"))?;

    // Load configuration file.
    let config = config::load(&mut options);
    log_config_path(&config);

    // Update the log level from config.
    log::set_max_level(config.debug.log_level);

    // Set tty environment variables.
    tty::setup_env();

    // Switch to home directory.
    #[cfg(target_os = "macos")]
    env::set_current_dir(home::home_dir().unwrap()).unwrap();

    // Set macOS locale.
    #[cfg(target_os = "macos")]
    locale::set_locale_environment();

    #[cfg(target_os = "macos")]
    macos::disable_autofill();

    // Setup automatic RAII cleanup for our files.
    let _files = TemporaryFiles { log_file: log_file.filter(|_| !config.debug.persistent_logging) };

    // Spawn the shell before starting iced, so failing to launch it is reported as an error.
    let Setup { options: app_options, first, settings } = setup::setup(&config, &mut options);
    let cell = term_settings::cell_size(&first.font);
    let window_size = term_settings::window_size(&config, cell, app_options.layout.padding);
    let window = window_settings(&config, &options, window_size);
    let mut app = App::new(app_options, first, settings)?;

    // The command line options are needed again whenever the configuration is.
    if let Some(source) = ConfigSource::new(&config, options) {
        app.watch_config(source);
    }

    // iced takes a reusable boot function, but the application is only booted once.
    let app = Mutex::new(Some(app));
    let boot = move || {
        let app = app.lock().unwrap().take().expect("application booted twice");
        (app, Task::none())
    };

    let result = iced::application(boot, App::update, App::view)
        .title(App::title)
        .style(App::style)
        .subscription(App::subscription)
        .window(window)
        .run();

    // Without explicitly detaching the console cmd won't redraw its prompt.
    #[cfg(windows)]
    unsafe {
        FreeConsole();
    }

    info!("Goodbye");

    result.map_err(Into::into)
}

/// Return the first CLI option the iced front end can't honor.
fn unsupported_options(options: &Options) -> Result<(), &'static str> {
    #[cfg(unix)]
    if options.socket.is_some() {
        return Err("--socket");
    }

    if options.daemon {
        Err("--daemon")
    } else if options.window_options.terminal_options.hold {
        Err("--hold")
    } else {
        Ok(())
    }
}

/// Settings for the main window.
///
/// `size` is the window size for `window.dimensions`, if they are configured.
#[allow(unused_variables)]
fn window_settings(config: &UiConfig, options: &Options, size: Option<Size>) -> window::Settings {
    let defaults = window::Settings::default();
    let window = &config.window;

    window::Settings {
        size: size.unwrap_or(defaults.size),
        position: match window.position {
            Some(position) => {
                window::Position::Specific(Point::new(position.x as f32, position.y as f32))
            },
            None => window::Position::Default,
        },
        maximized: window.startup_mode == StartupMode::Maximized,
        // iced has one kind of fullscreen; macOS' simple fullscreen uses it too.
        fullscreen: matches!(
            window.startup_mode,
            StartupMode::Fullscreen | StartupMode::SimpleFullscreen
        ),
        // The macOS title bar variants (`Transparent`, `Buttonless`) keep the decorations.
        decorations: window.decorations != Decorations::None,
        level: match window.level {
            WindowLevel::Normal => window::Level::Normal,
            WindowLevel::AlwaysOnTop => window::Level::AlwaysOnTop,
        },
        // Only a transparent window shows what is behind a background that isn't opaque.
        transparent: config.window_opacity() < 1.,
        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        icon: window_icon(),
        #[cfg(not(any(target_os = "macos", windows)))]
        platform_specific: window::settings::PlatformSpecific {
            application_id: term_settings::window_class(config, options),
            ..Default::default()
        },
        ..defaults
    }
}

/// Decode the embedded window icon.
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
fn window_icon() -> Option<window::Icon> {
    let mut decoder = png::Decoder::new(io::Cursor::new(WINDOW_ICON));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());
    window::icon::from_rgba(buf, info.width, info.height).ok()
}

fn log_config_path(config: &UiConfig) {
    if config.config_paths.is_empty() {
        return;
    }

    let mut msg = String::from("Configuration files loaded from:");
    for path in &config.config_paths {
        let _ = write!(msg, "\n  {:?}", path.display());
    }

    info!("{msg}");
}

#[cfg(test)]
mod tests {
    use super::*;

    use clap::Parser;

    fn options(args: &[&str]) -> Options {
        Options::try_parse_from(["tankTerm"].iter().chain(args)).unwrap()
    }

    #[test]
    fn supported_options_are_accepted() {
        let options = options(&["--title", "x", "-e", "sh", "-c", "true"]);
        assert_eq!(unsupported_options(&options), Ok(()));
    }

    fn config(toml: &str) -> UiConfig {
        toml::from_str(toml).unwrap()
    }

    #[test]
    fn window_settings_default_to_iced_defaults() {
        let settings = window_settings(&config(""), &options(&[]), None);
        let defaults = window::Settings::default();

        assert_eq!(settings.size, defaults.size);
        assert!(matches!(settings.position, window::Position::Default));
        assert!(!settings.maximized && !settings.fullscreen);
        assert!(settings.decorations);
        assert_eq!(settings.level, window::Level::Normal);
        assert!(!settings.transparent);
    }

    #[test]
    fn window_is_transparent_with_an_opacity_below_one() {
        let transparent = |opacity: &str| {
            let config = config(&format!("[window]\nopacity = {opacity}\n"));
            window_settings(&config, &options(&[]), None).transparent
        };

        assert!(transparent("0.9") && transparent("0.0"));
        assert!(!transparent("1.0"));
        // The opacity is at most one.
        assert!(!transparent("3.0"));
    }

    #[test]
    fn window_settings_follow_the_window_config() {
        let config = config(concat!(
            "[window]\n",
            "position = { x = 30, y = 40 }\n",
            "startup_mode = \"Maximized\"\n",
            "decorations = \"None\"\n",
            "level = \"AlwaysOnTop\"\n",
        ));
        let settings = window_settings(&config, &options(&[]), Some(Size::new(800., 480.)));

        assert_eq!(settings.size, Size::new(800., 480.));
        assert!(matches!(
            settings.position,
            window::Position::Specific(point) if point == Point::new(30., 40.)
        ));
        assert!(settings.maximized && !settings.fullscreen);
        assert!(!settings.decorations);
        assert_eq!(settings.level, window::Level::AlwaysOnTop);
    }

    #[test]
    fn fullscreen_startup_modes_start_fullscreen() {
        for mode in ["Fullscreen", "SimpleFullscreen"] {
            let config = config(&format!("[window]\nstartup_mode = \"{mode}\"\n"));
            let settings = window_settings(&config, &options(&[]), None);

            assert!(settings.fullscreen && !settings.maximized);
        }
    }

    #[test]
    fn window_size_fits_the_configured_dimensions() {
        let cell = Size::new(10., 20.);
        let padding = iced::Padding { top: 1., right: 2., bottom: 3., left: 4. };

        let sized = config("[window]\ndimensions = { columns = 80, lines = 24 }\n");
        assert_eq!(
            term_settings::window_size(&sized, cell, padding),
            Some(Size::new(80. * 10. + 6., 24. * 20. + 4.))
        );

        // Without dimensions the size is left to the default.
        assert_eq!(term_settings::window_size(&config(""), cell, padding), None);
    }

    #[test]
    fn unsupported_options_are_rejected() {
        assert_eq!(unsupported_options(&options(&["--daemon"])), Err("--daemon"));
        assert_eq!(unsupported_options(&options(&["--hold"])), Err("--hold"));
        #[cfg(unix)]
        assert_eq!(unsupported_options(&options(&["--socket", "/tmp/s"])), Err("--socket"));
    }

    #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
    #[test]
    fn window_icon_decodes() {
        assert!(window_icon().is_some());
    }
}
