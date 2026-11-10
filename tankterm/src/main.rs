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

use iced::{Task, window};
use log::info;
#[cfg(windows)]
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, FreeConsole};

use tankterm_terminal::tty;

mod app;
mod chrome;
mod cli;
// Most configuration options belong to the previous OpenGL front end and aren't read by the iced
// front end yet, but are still parsed so existing config files keep loading without errors.
#[allow(dead_code, unused_imports)]
mod config;
mod display;
mod logging;
#[cfg(target_os = "macos")]
mod macos;
mod migrate;
#[cfg(windows)]
mod panic;
mod term_settings;

use crate::app::App;
use crate::chrome::Chrome;
use crate::cli::{Options, Subcommands};
use crate::config::UiConfig;
#[cfg(target_os = "macos")]
use crate::macos::locale;

/// Window icon, used on X11. Wayland and macOS take the icon from the desktop entry or bundle,
/// and Windows from the executable's resources.
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
const WINDOW_ICON: &[u8] = include_bytes!("../extra/logo/compat/alacritty-term.png");

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
    let title = term_settings::window_title(&config, &options);
    let settings = term_settings::settings(&config, &options);
    let primary = &config.colors.primary;
    let chrome =
        Chrome::new(config.style.resolve(), &settings.font, primary.foreground, primary.background);
    let app = App::new(title, config.window.dynamic_title, settings, chrome)?;

    // iced takes a reusable boot function, but the application is only booted once.
    let app = Mutex::new(Some(app));
    let boot = move || {
        let app = app.lock().unwrap().take().expect("application booted twice");
        (app, Task::none())
    };

    let result = iced::application(boot, App::update, App::view)
        .title(App::title)
        .subscription(App::subscription)
        .window(window_settings(&config, &options))
        .run();

    // Without explicitly detaching the console cmd won't redraw it's prompt.
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
    } else if options.embed.is_some() {
        Err("--embed")
    } else if options.print_events {
        Err("--print-events")
    } else if options.ref_test {
        Err("--ref-test")
    } else if options.window_options.terminal_options.hold {
        Err("--hold")
    } else {
        Ok(())
    }
}

/// Settings for the main window.
#[allow(unused_variables)]
fn window_settings(config: &UiConfig, options: &Options) -> window::Settings {
    window::Settings {
        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        icon: window_icon(),
        #[cfg(not(any(target_os = "macos", windows)))]
        platform_specific: window::settings::PlatformSpecific {
            application_id: term_settings::window_class(config, options),
            ..Default::default()
        },
        ..Default::default()
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

    #[test]
    fn unsupported_options_are_rejected() {
        assert_eq!(unsupported_options(&options(&["--daemon"])), Err("--daemon"));
        assert_eq!(unsupported_options(&options(&["--hold"])), Err("--hold"));
        assert_eq!(unsupported_options(&options(&["--print-events"])), Err("--print-events"));
        #[cfg(unix)]
        assert_eq!(unsupported_options(&options(&["--socket", "/tmp/s"])), Err("--socket"));
    }

    #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
    #[test]
    fn window_icon_decodes() {
        assert!(window_icon().is_some());
    }
}
