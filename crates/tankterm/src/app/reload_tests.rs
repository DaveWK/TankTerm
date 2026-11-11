//! Live config reload through the real `App`, with configuration files in a temporary
//! directory and `cat` or `sh` as the shell.

use std::fs;
use std::path::Path;

use clap::Parser;
use iced::keyboard::{Key, Modifiers};
use iced::{Size, Theme};
use iced_test::simulator::Simulator;
use iced_tterm::CursorShape;
use tempfile::TempDir;

use super::option_tests::{cells, cursor, oldest_line, wheel_step};
use super::tab_tests::{
    Warnings, mouse_pastes_with, perform, type_keys, wait_for_text, wait_until,
};
use super::*;
use crate::cli::Options;
use crate::config::{self, UiConfig};
use crate::setup::{ConfigSource, Setup, setup};

/// The shell of the test configurations, as their first line.
const CAT: &str = "terminal.shell = \"cat\"\n";

/// A configuration file in a directory of its own.
struct ConfigFile {
    directory: TempDir,
}

impl ConfigFile {
    /// Write a configuration file with `cat` as its shell.
    fn new(config: &str) -> Self {
        let file = Self { directory: tempfile::tempdir().unwrap() };
        file.write(config);
        file
    }

    fn path(&self) -> PathBuf {
        self.directory.path().join("tankterm.toml")
    }

    /// Replace the configuration, keeping `cat` as its shell.
    fn write(&self, config: &str) {
        self.write_raw(&format!("{CAT}{config}"));
    }

    /// Replace the content of the configuration file.
    fn write_raw(&self, content: &str) {
        fs::write(self.path(), content).unwrap();
    }

    /// Write a file next to the configuration file.
    fn write_import(&self, name: &str, content: &str) -> PathBuf {
        let path = self.directory.path().join(name);
        fs::write(&path, content).unwrap();
        path
    }

    /// Start the application the way `main` does.
    fn start(&self) -> App {
        start(&self.path())
    }

    /// The configuration as it is in the file right now.
    fn config(&self) -> UiConfig {
        config::load(&mut options(&self.path()))
    }
}

fn options(config_file: &Path) -> Options {
    let config_file = config_file.to_str().unwrap();
    Options::try_parse_from(["tankTerm", "--config-file", config_file]).unwrap()
}

/// Start the application with a configuration file, the way `main` does.
fn start(config_file: &Path) -> App {
    let mut options = options(config_file);
    let config = config::load(&mut options);
    let Setup { options: app_options, first, settings } = setup(&config, &mut options);

    let mut app = App::new(app_options, first, settings).unwrap();
    if let Some(source) = ConfigSource::new(&config, options) {
        app.watch_config(source);
    }

    app
}

/// A configuration file changed.
fn changed(app: &mut App) {
    let _ = app.update(Message::ConfigChanged);
}

/// Whether two applications draw the same picture.
fn same_picture(one: &App, other: &App) -> bool {
    let window = Size::new(800., 400.);
    let directory = tempfile::tempdir().unwrap();
    let picture = directory.path().join("picture");

    let mut ui = Simulator::with_size(Default::default(), window, one.view());
    let first = ui.snapshot(&Theme::Dark).unwrap();
    // The first picture is kept as what the second one is compared with.
    assert!(first.matches_hash(&picture).unwrap());

    let mut ui = Simulator::with_size(Default::default(), window, other.view());
    ui.snapshot(&Theme::Dark).unwrap().matches_hash(&picture).unwrap()
}

/// A configuration that looks nothing like the default one.
const RESTYLED: &str = r##"
[colors.primary]
background = "#202040"
foreground = "#e0e0a0"

[font]
size = 17.0

[window]
padding = { x = 12, y = 6 }

[cursor]
style = "Underline"
thickness = 0.5

[tabs]
show = "Always"
position = "Bottom"
"##;

#[test]
fn changed_configuration_is_drawn_like_a_window_started_with_it() {
    let file = ConfigFile::new("");
    let mut app = file.start();
    let before = ConfigFile::new("").start();
    assert!(same_picture(&app, &before));

    file.write(RESTYLED);
    assert!(same_picture(&app, &before), "the file was read before it was said to have changed");

    changed(&mut app);
    let after = ConfigFile::new(RESTYLED).start();
    assert!(same_picture(&app, &after));
    assert!(!same_picture(&app, &before));
    assert_eq!(cursor(&app, 0), CursorShape::Underline);

    // And back again.
    file.write("");
    changed(&mut app);
    assert!(same_picture(&app, &before));
}

#[test]
fn every_terminal_gets_the_changed_configuration() {
    let file = ConfigFile::new("");
    let mut app = file.start();
    perform(&mut app, &[
        AppAction::Split(iced::widget::pane_grid::Axis::Vertical),
        AppAction::NewTab,
    ]);

    file.write(RESTYLED);
    changed(&mut app);

    let after = ConfigFile::new(RESTYLED).start();
    let cell = cells(&after)[&0];
    let background = after.options.layout.background;
    perform(&mut app, &[AppAction::SelectTab(0)]);
    assert_eq!(cells(&app), HashMap::from([(0, cell), (1, cell)]));
    perform(&mut app, &[AppAction::SelectTab(1)]);
    assert_eq!(cells(&app), HashMap::from([(2, cell)]));
    for id in 0..3 {
        assert_eq!(app.terminals[&id].background, background);
        assert_ne!(cursor(&app, id), CursorShape::Block);
    }

    // So do the terminals opened afterwards.
    perform(&mut app, &[AppAction::NewTab]);
    assert_eq!(cells(&app), HashMap::from([(3, cell)]));
    assert_eq!(cursor(&app, 3), CursorShape::Underline);
}

#[test]
fn changed_font_size_keeps_the_distance_the_font_size_actions_set() {
    let file = ConfigFile::new("");
    let mut app = file.start();
    perform(&mut app, &[AppAction::IncreaseFontSize, AppAction::IncreaseFontSize]);

    file.write("[font]\nsize = 17.0\n");
    changed(&mut app);

    let mut larger = ConfigFile::new("[font]\nsize = 17.0\n").start();
    let configured = cells(&larger)[&0];
    perform(&mut larger, &[AppAction::IncreaseFontSize, AppAction::IncreaseFontSize]);
    assert_eq!(cells(&app)[&0], cells(&larger)[&0]);

    perform(&mut app, &[AppAction::ResetFontSize]);
    assert_eq!(cells(&app)[&0], configured);
}

#[test]
fn changed_scrolling_applies_to_running_terminals() {
    let counting = "terminal.shell = { program = \"sh\", args = [\"-c\", \"seq 1 200; printf \
                    done; exec cat\"] }\n";
    let file = ConfigFile::new("");
    file.write_raw(counting);
    let mut app = file.start();
    wait_for_text(&app, 0, "done");
    assert_eq!((oldest_line(&app), wheel_step(&app, 1.)), (1, vec![3]));

    file.write_raw(&format!("{counting}[scrolling]\nhistory = 0\nmultiplier = 7\n"));
    changed(&mut app);

    assert!(oldest_line(&app) > 100);
    assert_eq!(wheel_step(&app, 1.), [7]);
    // The shell kept running; it wasn't started again for the new configuration.
    assert_eq!(app.terminals.len(), 1);
    assert!(app.terminals[&0].terminal.text().concat().contains("done"));
}

#[test]
fn changed_key_bindings_and_presets_take_effect() {
    let file = ConfigFile::new("");
    let mut app = file.start();
    let alt_y = [Some((Key::Character("y".into()), Modifiers::ALT))];
    let ctrl_shift_1 = [Some((Key::Character("1".into()), Modifiers::CTRL | Modifiers::SHIFT))];
    assert_eq!(type_keys(&app, &alt_y).0, []);
    assert_eq!(type_keys(&app, &ctrl_shift_1).0, []);

    file.write(
        "[keyboard]\npreset = \"WezTerm\"\nbindings = [{ key = \"Y\", mods = \"Alt\", action = \
         \"Copy\" }]\n",
    );
    changed(&mut app);

    assert_eq!(app.options.hotkeys, Hotkeys::from_config(&file.config()));
    assert_eq!(type_keys(&app, &alt_y).0, [AppAction::Copy]);
    assert_eq!(type_keys(&app, &ctrl_shift_1).0, [AppAction::SelectTab(0)]);

    // The leader preset comes with its leader key.
    file.write("[keyboard]\npreset = \"Leader\"\n");
    changed(&mut app);
    let keys = [
        Some((Key::Character("a".into()), Modifiers::CTRL)),
        Some((Key::Character("c".into()), Modifiers::empty())),
    ];
    assert_eq!(type_keys(&app, &keys), (vec![AppAction::NewTab], Vec::new()));
    assert_eq!(type_keys(&app, &alt_y).0, []);
}

#[test]
fn changed_mouse_bindings_take_effect() {
    use iced::mouse::Button;

    let file = ConfigFile::new(
        "[mouse]\nbindings = [{ mouse = \"Right\", action = \"Paste\" }, { mouse = \"Right\", \
         mods = \"Shift\", action = \"Paste\" }]\n",
    );
    let mut app = file.start();
    let pastes = |app: &App, modifiers| mouse_pastes_with(app, Button::Right, modifiers, "text");
    assert_eq!(pastes(&app, Modifiers::empty()), ["text"]);
    assert_eq!(pastes(&app, Modifiers::SHIFT), ["text"]);

    // Buttons whose bindings are gone don't paste any longer.
    file.write("");
    changed(&mut app);
    assert!(pastes(&app, Modifiers::empty()).is_empty());
    assert!(pastes(&app, Modifiers::SHIFT).is_empty());
    let middle = mouse_pastes_with(&app, Button::Middle, Modifiers::empty(), "text");
    assert_eq!(middle, ["text"]);
}

#[test]
fn changed_tabs_bell_and_window_options_take_effect() {
    let file = ConfigFile::new("");
    let mut app = file.start();
    assert_eq!(app.title(), "TankTerm");
    assert!(app.options.bell.command.is_none() && !app.options.copy_on_select);

    file.write(concat!(
        "[window]\ntitle = \"Renamed\"\n",
        "[tabs]\nshow = \"Always\"\ninherit_working_directory = false\n",
        "[bell]\ncommand = \"true\"\n",
        "[selection]\nsave_to_clipboard = true\n",
    ));
    changed(&mut app);

    assert_eq!(app.title(), "Renamed");
    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    assert!(ui.find("1: Renamed").is_ok());
    assert!(!app.options.tabs.inherit_working_directory);
    assert_eq!(app.options.bell, crate::bell::Bell::from_config(&file.config().bell));
    assert!(app.options.bell.command.is_some() && app.options.copy_on_select);
}

/// Open the profile at `index` among the configured ones in a new tab.
fn open_profile(app: &mut App, index: usize) {
    perform(app, &[AppAction::ShowProfiles]);
    // The first entry of the profile selector is the regular shell.
    let _ = app.update(Message::PaletteChoose(index + 1));
}

#[test]
fn changed_profiles_take_effect_and_recolor_their_terminals() {
    let profile = |theme: &str| {
        format!("[[profiles]]\nname = \"Themed\"\nshell = \"cat\"\ntheme = \"{theme}\"\n")
    };
    let file = ConfigFile::new(&profile("dracula"));
    let mut app = file.start();
    open_profile(&mut app, 0);
    let default = app.terminals[&0].background;
    let dracula = app.terminals[&1].background;
    assert_ne!(dracula, default);

    // The terminal opened from the profile follows the profile's theme.
    file.write(&format!(
        "{}[[profiles]]\nname = \"Plain\"\nshell = \"cat\"\n",
        profile("catppuccin")
    ));
    changed(&mut app);
    let catppuccin = app.terminals[&1].background;
    assert_eq!(catppuccin, app.options.profiles[0].background);
    assert!(catppuccin != dracula && catppuccin != default);
    assert_eq!(app.terminals[&0].background, default);

    // New profiles can be opened.
    assert_eq!(app.options.profiles.len(), 2);
    open_profile(&mut app, 1);
    assert_eq!(app.terminals[&2].background, default);

    // A terminal whose profile is gone looks like the others.
    file.write("");
    changed(&mut app);
    assert!(app.options.profiles.is_empty());
    assert_eq!(app.terminals[&1].background, default);
}

#[test]
fn broken_configuration_is_logged_and_the_running_one_kept() {
    Warnings::keep();

    let file = ConfigFile::new(RESTYLED);
    let mut app = file.start();
    let before = ConfigFile::new(RESTYLED).start();
    let name = file.path().display().to_string();
    assert_eq!(Warnings::mentioning(&name), 0);

    file.write_raw("[colors.primary\nbackground = ");
    changed(&mut app);
    assert!(same_picture(&app, &before));
    assert_eq!(app.options.hotkeys, before.options.hotkeys);
    assert_eq!(Warnings::mentioning(&name), 1);

    // The file is still watched, and its next version is read.
    assert!(app.config.is_some());
    file.write("");
    changed(&mut app);
    assert!(same_picture(&app, &ConfigFile::new("").start()));

    // A configuration file that is gone changes nothing either.
    fs::remove_file(file.path()).unwrap();
    changed(&mut app);
    assert!(same_picture(&app, &ConfigFile::new("").start()));
    assert!(app.config.is_some());
}

#[test]
fn a_wrong_value_is_logged_and_the_rest_of_the_configuration_applied() {
    Warnings::keep();

    let file = ConfigFile::new("");
    let mut app = file.start();

    file.write("[scrolling]\nmultiplier = \"a lot\"\n[colors.primary]\nbackground = \"#202040\"\n");
    changed(&mut app);

    assert!(Warnings::mentioning("a lot") >= 1);
    assert_eq!(app.options.layout.background, iced::Color::from_rgb8(0x20, 0x20, 0x40));
    assert_eq!(wheel_step(&app, 1.), [3]);
}

#[test]
fn imported_files_are_watched_and_applied() {
    let file = ConfigFile::new("");
    let colors = file.write_import("colors.toml", "[colors.primary]\nbackground = \"#101010\"\n");
    file.write_raw(&format!("{CAT}general.import = [\"colors.toml\"]\n"));
    let mut app = file.start();

    let watched = |app: &App| app.config.as_ref().map(|source| source.paths.clone());
    assert_eq!(watched(&app), Some(vec![file.path(), colors.clone()]));
    assert_eq!(app.options.layout.background, iced::Color::from_rgb8(0x10, 0x10, 0x10));

    file.write_import("colors.toml", "[colors.primary]\nbackground = \"#303030\"\n");
    changed(&mut app);
    assert_eq!(app.options.layout.background, iced::Color::from_rgb8(0x30, 0x30, 0x30));
    assert_eq!(app.terminals[&0].background, iced::Color::from_rgb8(0x30, 0x30, 0x30));

    // Files that are imported from now on are watched from now on.
    let font = file.write_import("font.toml", "[font]\nsize = 17.0\n");
    file.write_raw(&format!("{CAT}general.import = [\"colors.toml\", \"font.toml\"]\n"));
    changed(&mut app);
    assert_eq!(watched(&app), Some(vec![file.path(), colors, font]));

    // And files that aren't imported any longer are not.
    file.write("");
    changed(&mut app);
    assert_eq!(watched(&app), Some(vec![file.path()]));
}

#[test]
fn configuration_is_not_watched_without_live_config_reload() {
    let file = ConfigFile::new("general.live_config_reload = false\n");
    assert!(file.start().config.is_none());

    // Nor is there anything to watch without a configuration file.
    assert!(ConfigSource::new(&UiConfig::default(), Options::default()).is_none());

    // Turning it off is the last change that is read.
    let file = ConfigFile::new("");
    let mut app = file.start();
    assert!(app.config.is_some());
    file.write("general.live_config_reload = false\n[colors.primary]\nbackground = \"#202040\"\n");
    changed(&mut app);
    assert!(app.config.is_none());
    assert_eq!(app.options.layout.background, iced::Color::from_rgb8(0x20, 0x20, 0x40));

    file.write("");
    changed(&mut app);
    assert_eq!(app.options.layout.background, iced::Color::from_rgb8(0x20, 0x20, 0x40));
}

#[test]
fn watched_files_change_the_window_without_being_told() {
    use iced::futures::executor::block_on;
    use iced::futures::{StreamExt, future, stream};
    use iced_winit::runtime::futures::subscription;

    let file = ConfigFile::new("");
    let mut app = file.start();

    // Run the application's subscriptions, the one watching the files among them, the way
    // iced would. Nothing happens in the window meanwhile.
    let recipes = subscription::into_recipes(app.subscription());
    let mut streams: Vec<_> =
        recipes.into_iter().map(|recipe| recipe.stream(Box::pin(stream::pending()))).collect();

    file.write("[colors.primary]\nbackground = \"#202040\"\n");
    wait_until("the change was never noticed", || {
        let changed = streams.iter_mut().any(|messages| {
            let message = block_on(future::poll_immediate(messages.next()));
            matches!(message, Some(Some(Message::ConfigChanged)))
        });
        if changed {
            let _ = app.update(Message::ConfigChanged);
        }
        changed
    });

    assert_eq!(app.options.layout.background, iced::Color::from_rgb8(0x20, 0x20, 0x40));
}
