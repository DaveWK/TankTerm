//! What the application takes from the configuration, when it starts and whenever the
//! configuration is read again.

use std::path::PathBuf;

use iced_tterm::settings::Settings;

use crate::app::{AppOptions, Layout, Profile};
use crate::bell::Bell;
use crate::cli::Options;
use crate::config::{self, UiConfig};
use crate::hotkeys::{self, Hotkeys};
use crate::{session, term_settings};

/// The application's settings for a configuration.
pub struct Setup {
    pub options: AppOptions,
    /// Settings for the first terminal, which runs the command given on the command line.
    pub first: Settings,
    /// Settings for the terminals of new tabs and panes.
    pub settings: Settings,
}

/// Where the configuration comes from, for reading it again when its files change.
pub struct ConfigSource {
    /// The command line options the configuration is loaded with.
    pub cli: Options,
    /// The configuration file, followed by the files it imports.
    pub paths: Vec<PathBuf>,
}

impl ConfigSource {
    /// The source of a configuration, if it asks to be read again when its files change and
    /// has files that could.
    pub fn new(config: &UiConfig, cli: Options) -> Option<Self> {
        let watched = config.live_config_reload() && !config.config_paths.is_empty();
        watched.then(|| Self { cli, paths: config.config_paths.clone() })
    }

    /// Read the configuration again.
    ///
    /// Without a configuration that can be used there are no settings: what is wrong with it
    /// is logged, and the application keeps the settings it has. The files of the new
    /// configuration are watched from then on, or none if it turns live reload off.
    pub fn reload(&mut self) -> Option<Setup> {
        let path = self.paths.first()?.clone();
        let config = config::reload(&path, &mut self.cli).ok()?;
        log::set_max_level(config.debug.log_level);

        self.paths =
            if config.live_config_reload() { config.config_paths.clone() } else { Vec::new() };

        Some(setup(&config, &mut self.cli))
    }
}

/// The application's settings for a configuration and the command line options.
pub fn setup(config: &UiConfig, options: &mut Options) -> Setup {
    let title = term_settings::window_title(config, options);
    let settings = term_settings::settings(config, options);
    let cell = term_settings::cell_size(&settings.font);

    // Profiles with a theme of their own get their colors from the configuration loaded with it.
    let profiles = config.profiles.clone().into_iter().map(|profile| {
        let themed = profile.theme.as_deref().map(|theme| config::load_with_theme(options, theme));
        let config = themed.as_ref().unwrap_or(config);
        let settings = term_settings::profile_settings(&profile, config, options);
        let detail = [settings.backend.program.clone()]
            .into_iter()
            .chain(settings.backend.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");

        Profile {
            name: profile.name,
            detail,
            settings,
            background: term_settings::background(config),
        }
    });
    let profiles: Vec<_> = profiles.collect();

    let app_options = AppOptions {
        title,
        dynamic_title: config.window.dynamic_title,
        layout: Layout {
            padding: term_settings::padding(config, cell),
            background: term_settings::background(config),
            foreground: term_settings::foreground(config),
        },
        tabs: config.tabs,
        hotkeys: Hotkeys::from_config(config),
        // A window started for a single command isn't a session worth remembering.
        session_file: session::default_path().filter(|_| {
            config.tabs.restore && options.window_options.terminal_options.command().is_none()
        }),
        profiles,
        confirm_multiline_paste: config.terminal.confirm_multiline_paste,
        copy_on_select: config.selection.save_to_clipboard,
        bindings: hotkeys::mouse_bindings(config.mouse_bindings()),
        bell: Bell::from_config(&config.bell),
        opacity: config.window_opacity(),
    };

    Setup {
        options: app_options,
        first: settings,
        settings: term_settings::tab_settings(config, options),
    }
}
