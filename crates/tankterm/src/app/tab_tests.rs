//! Tabs and panes through the real `App`, with `cat` as the shell.

use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use iced::keyboard::{Key, Modifiers};
use iced::widget::pane_grid::{Axis, Direction, Node};
use iced::{Size, Theme};
use iced_test::simulator::{self, Simulator};
use iced_tterm::settings::BackendSettings;
use iced_tterm::{AlacrittyEvent, BackendCommand, TermMode};

use super::icons::{self, Icon};
use super::*;
use crate::cli::Options;
use crate::config::UiConfig;
use crate::config::terminal::ShellIntegration;
use crate::session::ordered_panes;
use crate::term_settings;

fn app_with_session(config: &str, hotkeys: Hotkeys, session_file: Option<PathBuf>) -> App {
    app_from(config, hotkeys, session_file, |_| ())
}

/// An application for a configuration, with `first` changing the settings of its first terminal.
pub(super) fn app_from(
    config: &str,
    hotkeys: Hotkeys,
    session_file: Option<PathBuf>,
    first: impl FnOnce(&mut Settings),
) -> App {
    let mut config: UiConfig = toml::from_str(config).unwrap();
    // Loading a configuration resolves its styles.
    config.style.resolve(&config.colors);
    let mut settings = term_settings::settings(&config, &Options::default());
    // Tests run `cat` unless their configuration names a shell.
    if config.terminal.shell.is_none() {
        settings.backend = BackendSettings { program: "cat".into(), ..BackendSettings::default() };
    }

    // Profiles of the test configuration, told apart by a white background.
    let profiles = config.profiles.iter().map(|profile| Profile {
        name: profile.name.clone(),
        detail: String::new(),
        settings: term_settings::profile_settings(profile, &config, &Options::default()),
        background: Color::WHITE,
    });
    let profiles = profiles.collect();

    let options = AppOptions {
        title: "test".into(),
        dynamic_title: config.window.dynamic_title,
        layout: Layout {
            padding: Padding::ZERO,
            background: term_settings::background(&config),
            foreground: term_settings::foreground(&config),
            tab_bar: term_settings::tab_bar_styles(&config),
        },
        tabs: config.tabs.clone(),
        status: crate::app::status::segments(&config),
        frameless: config.window.decorations == crate::config::window::Decorations::None,
        // Tests don't depend on the desktop they run on.
        title_buttons: crate::title_buttons::TitleButtons::new(
            config.window.buttons,
            Default::default,
        ),
        hotkeys,
        config_file: None,
        session_file,
        profiles,
        confirm_multiline_paste: config.terminal.confirm_multiline_paste,
        shell_integration: config.terminal.shell_integration == ShellIntegration::Auto,
        copy_on_select: config.selection.save_to_clipboard,
        bindings: crate::hotkeys::mouse_bindings(config.mouse_bindings()),
        bell: crate::bell::Bell::from_config(&config.bell),
        opacity: config.window_opacity(),
    };

    let mut first_settings = settings.clone();
    first(&mut first_settings);
    App::new(options, first_settings, settings).unwrap()
}

pub(super) fn app_with(config: &str, hotkeys: Hotkeys) -> App {
    app_with_session(config, hotkeys, None)
}

fn app() -> App {
    app_with("", Hotkeys::default())
}

pub(super) fn perform(app: &mut App, actions: &[AppAction]) {
    for action in actions {
        let _ = app.update(Message::Action(action.clone()));
    }
}

/// Terminal IDs of the panes of the tab at `index`, in layout order.
fn terminals(app: &App, index: usize) -> Vec<u64> {
    let tab = &app.tabs[index];
    ordered_panes(tab.panes.layout()).iter().map(|pane| *tab.panes.get(*pane).unwrap()).collect()
}

/// Terminal ID of the focused pane of the tab that is shown.
pub(super) fn focused(app: &App) -> u64 {
    let tab = &app.tabs[app.active];
    *tab.panes.get(tab.focus).unwrap()
}

/// Text drawn by the application's view.
pub(super) fn has_text(app: &App, text: &str) -> bool {
    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    ui.find(text).is_ok()
}

/// The text the tab bar shows for the tab at `index`, to find the tab by: its label without
/// the tab's number, which is drawn on its own.
pub(super) fn tab_text(app: &App, index: usize) -> String {
    let parts = app.tab_label_parts(index, &app.tabs[index]).into_iter();
    parts.filter(|(_, number)| !number).map(|(text, _)| text).collect()
}

/// Whether the tab bar shows the tab at `index`.
pub(super) fn shows_tab(app: &App, index: usize) -> bool {
    has_text(app, &tab_text(app, index))
}

/// Forget where the shells are, so tabs nothing titled show the configured title, `test`,
/// until the next tick looks again.
pub(super) fn forget_directories(app: &mut App) {
    for shell in app.terminals.values_mut() {
        shell.working_directory = None;
    }
}

#[test]
fn new_tabs_are_added_after_the_last_and_shown() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab, AppAction::NewTab]);

    assert_eq!(app.tabs.len(), 3);
    assert_eq!(app.active, 2);
    assert_eq!(app.terminals.len(), 3);
    assert_eq!(focused(&app), 2);
}

#[test]
fn tabs_are_selected_by_position_and_cycle() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab, AppAction::NewTab]);

    perform(&mut app, &[AppAction::NextTab]);
    assert_eq!(app.active, 0);
    perform(&mut app, &[AppAction::PreviousTab]);
    assert_eq!(app.active, 2);
    perform(&mut app, &[AppAction::SelectTab(1)]);
    assert_eq!(app.active, 1);
    // There is no tenth tab.
    perform(&mut app, &[AppAction::SelectTab(9)]);
    assert_eq!(app.active, 1);
    perform(&mut app, &[AppAction::LastTab]);
    assert_eq!(app.active, 2);
}

#[test]
fn tabs_move_without_wrapping_around() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab, AppAction::NewTab, AppAction::MoveTabLeft]);

    assert_eq!(app.active, 1);
    assert_eq!([terminals(&app, 0), terminals(&app, 1), terminals(&app, 2)], [[0], [2], [1]]);

    perform(&mut app, &[AppAction::MoveTabRight, AppAction::MoveTabRight]);
    assert_eq!(app.active, 2);
    assert_eq!(terminals(&app, 2), [2]);
}

#[test]
fn closing_a_tab_closes_its_terminals_and_shows_a_neighbor() {
    let mut app = app();
    perform(&mut app, &[
        AppAction::NewTab,
        AppAction::Split(Axis::Vertical),
        AppAction::NewTab,
        AppAction::SelectTab(1),
        AppAction::CloseTab,
    ]);

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active, 1);
    assert_eq!(focused(&app), 3);
    let mut open: Vec<_> = app.terminals.keys().copied().collect();
    open.sort_unstable();
    assert_eq!(open, [0, 3]);

    // Closing a tab before the shown one keeps the shown one.
    let _ = app.update(Message::CloseTab(0));
    assert_eq!(app.active, 0);
    assert_eq!(focused(&app), 3);
}

#[test]
fn splits_focus_the_new_pane() {
    let mut app = app();
    perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::Split(Axis::Horizontal)]);

    assert_eq!(app.tabs.len(), 1);
    assert_eq!(terminals(&app, 0), [0, 1, 2]);
    assert_eq!(focused(&app), 2);
}

#[test]
fn focus_moves_between_panes() {
    let mut app = app();
    // Terminal 0 on the left, 1 above 2 on the right.
    perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::Split(Axis::Horizontal)]);

    perform(&mut app, &[AppAction::FocusPane(Direction::Up)]);
    assert_eq!(focused(&app), 1);
    perform(&mut app, &[AppAction::FocusPane(Direction::Left)]);
    assert_eq!(focused(&app), 0);
    // There is no pane further left.
    perform(&mut app, &[AppAction::FocusPane(Direction::Left)]);
    assert_eq!(focused(&app), 0);

    perform(&mut app, &[AppAction::PreviousPane]);
    assert_eq!(focused(&app), 2);
    perform(&mut app, &[AppAction::NextPane, AppAction::NextPane]);
    assert_eq!(focused(&app), 1);
}

#[test]
fn zoom_shows_only_the_focused_pane_until_focus_moves() {
    let mut app = app();
    perform(&mut app, &[AppAction::ToggleZoom]);
    assert_eq!(app.tabs[0].panes.maximized(), None);

    perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::ToggleZoom]);
    assert_eq!(app.tabs[0].panes.maximized(), Some(app.tabs[0].focus));

    perform(&mut app, &[AppAction::FocusPane(Direction::Left)]);
    assert_eq!(app.tabs[0].panes.maximized(), None);
    assert_eq!(focused(&app), 0);
}

#[test]
fn resizing_a_zoomed_pane_shows_the_divider_it_moves() {
    let mut app = app();
    perform(&mut app, &[
        AppAction::Split(Axis::Vertical),
        AppAction::ToggleZoom,
        AppAction::ResizePane(Direction::Left),
    ]);

    assert_eq!(app.tabs[0].panes.maximized(), None);
    assert!(matches!(app.tabs[0].panes.layout(), Node::Split { ratio, .. } if *ratio == 0.45));
    assert_eq!(focused(&app), 1);

    // A pane without a divider to move is shown among the others again as well, like one
    // without a neighbor to move focus to.
    perform(&mut app, &[AppAction::ToggleZoom, AppAction::ResizePane(Direction::Up)]);
    assert_eq!(app.tabs[0].panes.maximized(), None);
    assert!(matches!(app.tabs[0].panes.layout(), Node::Split { ratio, .. } if *ratio == 0.45));
}

#[test]
fn panes_are_resized_at_the_nearest_divider() {
    let ratios = |app: &App| {
        let mut ratios = Vec::new();
        let mut nodes = vec![app.tabs[0].panes.layout()];
        while let Some(node) = nodes.pop() {
            if let Node::Split { axis, ratio, a, b, .. } = node {
                ratios.push((*axis, *ratio));
                nodes.extend([&**b, &**a]);
            }
        }
        ratios
    };

    let mut app = app();
    perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::Split(Axis::Horizontal)]);
    assert_eq!(ratios(&app), [(Axis::Vertical, 0.5), (Axis::Horizontal, 0.5)]);

    perform(&mut app, &[
        AppAction::ResizePane(Direction::Left),
        AppAction::ResizePane(Direction::Down),
    ]);
    assert_eq!(ratios(&app), [(Axis::Vertical, 0.45), (Axis::Horizontal, 0.55)]);

    // The left pane has no horizontal divider to move.
    perform(&mut app, &[
        AppAction::FocusPane(Direction::Left),
        AppAction::ResizePane(Direction::Up),
    ]);
    assert_eq!(ratios(&app), [(Axis::Vertical, 0.45), (Axis::Horizontal, 0.55)]);

    // Dividers stop before a pane disappears.
    perform(&mut app, &vec![AppAction::ResizePane(Direction::Left); 20]);
    assert_eq!(ratios(&app)[0], (Axis::Vertical, RESIZE_LIMITS.0));
}

#[test]
fn closing_panes_focuses_a_sibling_and_closes_the_tab_with_the_last() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab, AppAction::Split(Axis::Vertical)]);

    perform(&mut app, &[AppAction::ClosePane]);
    assert_eq!(terminals(&app, 1), [1]);
    assert_eq!(focused(&app), 1);
    assert_eq!(app.terminals.len(), 2);

    perform(&mut app, &[AppAction::ClosePane]);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(focused(&app), 0);
}

#[test]
fn a_pane_closes_when_its_shell_exits() {
    let mut app = app();
    perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::NewTab]);

    // The shell of an unfocused pane in a tab that isn't shown exits.
    let exit = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Exit);
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(0, exit.clone())));

    assert_eq!(terminals(&app, 0), [1]);
    assert_eq!(app.active, 1);
    assert_eq!(focused(&app), 2);

    // Its last pane takes the tab with it.
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(1, exit.clone())));
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(focused(&app), 2);

    // The last tab leaves nothing to show; the window is closed.
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(2, exit)));
    assert!(app.tabs.is_empty() && app.terminals.is_empty());
    let _ = app.view();
}

#[test]
fn actions_do_nothing_after_the_last_tab_is_closed() {
    let mut app = app();
    perform(&mut app, &[AppAction::CloseTab]);
    assert!(app.tabs.is_empty());

    // Keys pressed before the window is gone still arrive.
    perform(&mut app, &[
        AppAction::NewTab,
        AppAction::CloseTab,
        AppAction::NextTab,
        AppAction::PreviousTab,
        AppAction::SelectTab(0),
        AppAction::LastTab,
        AppAction::MoveTabLeft,
        AppAction::MoveTabRight,
        AppAction::Split(Axis::Vertical),
        AppAction::ClosePane,
        AppAction::FocusPane(Direction::Left),
        AppAction::NextPane,
        AppAction::PreviousPane,
        AppAction::ToggleZoom,
        AppAction::ResizePane(Direction::Left),
        AppAction::ShowCommands,
        AppAction::ShowProfiles,
        AppAction::PaletteNext,
        AppAction::PalettePrevious,
        AppAction::PaletteClose,
        AppAction::PasteConfirm,
        AppAction::PasteCancel,
        AppAction::Copy,
        AppAction::Paste,
        AppAction::PasteSelection,
        AppAction::IncreaseFontSize,
        AppAction::DecreaseFontSize,
        AppAction::ResetFontSize,
        AppAction::Scroll(crate::hotkeys::ScrollTo::PageUp),
        AppAction::ClearHistory,
        AppAction::ToggleFullscreen,
        AppAction::ToggleMaximized,
    ]);

    assert!(app.tabs.is_empty() && app.terminals.is_empty());
    assert_eq!(app.active, 0);
    assert!(app.palette.is_none());
    let _ = app.view();
}

#[test]
fn titles_follow_the_focused_terminal() {
    let mut app = app();
    forget_directories(&mut app);
    assert_eq!(app.title(), "test");

    let title = |id, title: &str| {
        let event = AlacrittyEvent::Title(title.into());
        Message::Terminal(iced_tterm::Event::BackendCall(
            id,
            BackendCommand::ProcessAlacrittyEvent(event),
        ))
    };

    let _ = app.update(title(0, "first"));
    assert_eq!(app.title(), "first");

    perform(&mut app, &[AppAction::Split(Axis::Vertical)]);
    forget_directories(&mut app);
    assert_eq!(app.title(), "test");
    let _ = app.update(title(1, "second"));
    assert_eq!(app.title(), "second");

    perform(&mut app, &[AppAction::FocusPane(Direction::Left)]);
    assert_eq!(app.title(), "first");
}

#[test]
fn untitled_shells_are_named_after_where_they_are() {
    let mut app = app();
    let focused = app.focused_terminal().unwrap();
    let home = home::home_dir().unwrap();
    let (user, host) = (status::user_name().unwrap(), status::host_name().unwrap());

    // Until the directory is known, the configured title.
    app.terminals.get_mut(&focused).unwrap().working_directory = None;
    assert_eq!(app.tab_title(&app.tabs[0]), "test");

    let shell = app.terminals.get_mut(&focused).unwrap();
    shell.working_directory = Some(home.join("src"));
    shell.process = Some("bash".into());
    assert_eq!(app.tab_title(&app.tabs[0]), format!("{user}@{host}:~/src"));
    assert_eq!(app.title(), format!("{user}@{host}:~/src"));

    // ssh's local directory says nothing about the remote one.
    app.terminals.get_mut(&focused).unwrap().process = Some("ssh".into());
    assert_eq!(app.tab_title(&app.tabs[0]), "test");
}

#[test]
fn titles_programs_set_win_over_the_directory() {
    let mut app = app();
    let focused = app.focused_terminal().unwrap();
    let event = AlacrittyEvent::Title("vim".into());
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(
        focused,
        BackendCommand::ProcessAlacrittyEvent(event),
    )));

    app.terminals.get_mut(&focused).unwrap().working_directory = Some("/etc".into());
    assert_eq!(app.tab_title(&app.tabs[0]), "vim");
}

#[test]
fn without_dynamic_titles_the_configured_title_stays() {
    let mut app = app_with("[window]\ndynamic_title = false\n", Hotkeys::default());
    let focused = app.focused_terminal().unwrap();
    app.terminals.get_mut(&focused).unwrap().working_directory = Some("/etc".into());
    assert_eq!(app.tab_title(&app.tabs[0]), "test");
}

#[cfg(target_os = "linux")]
#[test]
fn directories_and_processes_are_polled_for_the_fallback_title() {
    let mut app = app();
    tick_until(&mut app, "the directory and process to be known", |app| {
        let shell = &app.terminals[&app.focused_terminal().unwrap()];
        shell.working_directory.is_some() && shell.process.is_some()
    });
}

#[test]
fn background_tabs_show_activity_and_bells() {
    let event = |id, event| {
        Message::Terminal(iced_tterm::Event::BackendCall(
            id,
            BackendCommand::ProcessAlacrittyEvent(event),
        ))
    };

    let mut app = app();
    perform(&mut app, &[AppAction::NewTab]);

    let _ = app.update(event(0, AlacrittyEvent::Wakeup));
    assert!(tab_text(&app, 0).ends_with(" *") && shows_tab(&app, 0));
    let _ = app.update(event(0, AlacrittyEvent::Bell));
    assert!(tab_text(&app, 0).ends_with(" !") && shows_tab(&app, 0));

    // Output of the tab that is shown isn't pointed out.
    let _ = app.update(event(1, AlacrittyEvent::Wakeup));
    let _ = app.update(event(1, AlacrittyEvent::Bell));
    assert!(!tab_text(&app, 1).ends_with(['*', '!']) && shows_tab(&app, 1));

    // Showing a tab clears its markers.
    perform(&mut app, &[AppAction::SelectTab(0)]);
    assert!(!tab_text(&app, 0).ends_with(['*', '!']) && shows_tab(&app, 0) && shows_tab(&app, 1));
}

#[test]
fn tabs_and_panes_are_restored_from_the_session_file() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("session.json");

    let mut app = app_with_session("", Hotkeys::default(), Some(file.clone()));
    perform(&mut app, &[
        AppAction::Split(Axis::Vertical),
        AppAction::Split(Axis::Horizontal),
        AppAction::ResizePane(Direction::Left),
        AppAction::FocusPane(Direction::Up),
        AppAction::NewTab,
        AppAction::SelectTab(0),
    ]);
    let session = app.session();
    assert_eq!(session.tabs.len(), 2);
    drop(app);

    let mut app = app_with_session("", Hotkeys::default(), Some(file.clone()));
    assert_eq!(app.session(), session);
    assert_eq!((app.tabs.len(), app.active, app.terminals.len()), (2, 0, 4));
    // The pane above the last one on the right has focus again.
    assert_eq!(terminals(&app, 0), [0, 1, 2]);
    assert_eq!(focused(&app), 1);

    // The first prompt of a restored shell isn't activity.
    let wakeup = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup);
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(3, wakeup)));
    assert!(!tab_text(&app, 1).ends_with(['*', '!']) && shows_tab(&app, 1));

    // Closing the window's last tab leaves that tab to come back to.
    perform(&mut app, &[AppAction::CloseTab, AppAction::CloseTab]);
    assert!(app.tabs.is_empty());
    drop(app);

    let app = app_with_session("", Hotkeys::default(), Some(file));
    assert_eq!((app.tabs.len(), app.terminals.len()), (1, 1));
}

#[test]
fn sessions_are_not_written_without_a_session_file() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab]);

    assert_eq!(app.saved_session, None);
}

/// Keeps the warnings and errors logged by the tests of this binary.
pub(super) struct Warnings(Mutex<Vec<String>>);

static WARNINGS: Warnings = Warnings(Mutex::new(Vec::new()));

impl log::Log for Warnings {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            self.0.lock().unwrap().push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

impl Warnings {
    /// Start keeping what is logged. The tests install no other logger.
    pub(super) fn keep() {
        static KEEP: std::sync::Once = std::sync::Once::new();

        KEEP.call_once(|| {
            log::set_logger(&WARNINGS).unwrap();
            log::set_max_level(log::LevelFilter::Warn);
        });
    }

    /// How many of the kept warnings and errors mention `text`.
    pub(super) fn mentioning(text: &str) -> usize {
        WARNINGS.0.lock().unwrap().iter().filter(|warning| warning.contains(text)).count()
    }
}

#[test]
fn failed_session_writes_are_tried_again_with_the_ticks() {
    Warnings::keep();

    // A file where the session file's directory belongs keeps the session from being written.
    let directory = tempfile::tempdir().unwrap();
    let blocker = directory.path().join("state");
    std::fs::write(&blocker, "").unwrap();
    let file = blocker.join("session.json");
    let name = file.display().to_string();

    let mut app = app_with_session("", Hotkeys::default(), Some(file.clone()));
    perform(&mut app, &[AppAction::NewTab]);
    assert_eq!(app.saved_session, None);
    assert_eq!(Warnings::mentioning(&name), 1);

    // Every tick tries again, without repeating what went wrong.
    let _ = app.update(Message::Tick);
    let _ = app.update(Message::Tick);
    assert_eq!(app.saved_session, None);
    assert!(app.session_error.is_some());
    assert_eq!(Warnings::mentioning(&name), 1);

    // Once the file can be written the next tick writes it, and nothing before that tick.
    std::fs::remove_file(&blocker).unwrap();
    perform(&mut app, &[AppAction::SelectTab(0)]);
    assert_eq!(Session::load(&file), None);
    let _ = app.update(Message::Tick);
    assert_eq!(Session::load(&file), Some(app.session()));
    assert_eq!(app.saved_session, Some(app.session()));
    assert_eq!(app.session_error, None);
    assert_eq!(Warnings::mentioning(&name), 1);

    // Later changes are written right away again, and a new failure is reported.
    perform(&mut app, &[AppAction::NewTab]);
    assert_eq!(Session::load(&file).map(|session| session.tabs.len()), Some(3));
    std::fs::remove_dir_all(&blocker).unwrap();
    std::fs::write(&blocker, "").unwrap();
    perform(&mut app, &[AppAction::NewTab]);
    assert_eq!(app.saved_session.as_ref().map(|session| session.tabs.len()), Some(3));
    assert_eq!(Warnings::mentioning(&name), 2);
}

#[test]
fn command_palette_filters_and_runs_commands() {
    let mut app = app();
    perform(&mut app, &[AppAction::ShowCommands]);
    assert!(has_text(&app, "Split Right") && has_text(&app, "New Tab"));

    let _ = app.update(Message::PaletteQuery("split".into()));
    assert!(has_text(&app, "Split Down") && !has_text(&app, "New Tab"));

    perform(&mut app, &[AppAction::PaletteNext]);
    let _ = app.update(Message::PaletteSubmit);
    assert!(app.palette.is_none());
    // "Split Down" put the new terminal below the first one.
    assert_eq!(terminals(&app, 0), [0, 1]);
    assert!(matches!(app.tabs[0].panes.layout(), Node::Split { axis: Axis::Horizontal, .. }));

    // Clicking an item runs it as well.
    perform(&mut app, &[AppAction::ShowCommands]);
    let _ = app.update(Message::PaletteChoose(0));
    assert_eq!((app.tabs.len(), app.palette.is_none()), (2, true));
}

#[test]
fn palette_takes_the_keyboard_until_it_is_closed() {
    use iced::keyboard::key::Named;

    let hotkeys =
        Hotkeys::default().with(Key::Named(Named::F5), Modifiers::empty(), AppAction::NewTab);
    let mut app = app_with("", hotkeys);
    perform(&mut app, &[AppAction::ShowCommands]);

    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    ui.tap_key(Named::F5);
    ui.tap_key(Named::ArrowDown);
    ui.tap_key(Named::Escape);
    // Clicks don't reach the terminal underneath either.
    ui.point_at((400., 300.));
    ui.simulate(simulator::click());

    let actions: Vec<_> = ui
        .into_messages()
        .filter_map(|message| match message {
            Message::Action(action) => Some(action),
            Message::PaneClicked(_) => panic!("click went through the palette"),
            _ => None,
        })
        .collect();
    assert_eq!(actions, [AppAction::PaletteNext, AppAction::PaletteClose]);

    perform(&mut app, &[AppAction::PaletteClose]);
    assert!(app.palette.is_none());
    assert_eq!(app.tabs.len(), 1);
}

#[test]
fn profiles_open_in_a_new_tab() {
    let config = "[[profiles]]\nname = \"Cat\"\nshell = \"cat\"\n";
    let mut app = app_with(config, Hotkeys::default());

    perform(&mut app, &[AppAction::ShowProfiles]);
    assert!(has_text(&app, "Default") && has_text(&app, "Cat"));

    let _ = app.update(Message::PaletteQuery("cat".into()));
    let _ = app.update(Message::PaletteSubmit);
    assert_eq!((app.tabs.len(), app.active, focused(&app)), (2, 1, 1));
    assert_eq!(app.terminals[&1].background, Color::WHITE);

    // The default entry opens the regular shell.
    perform(&mut app, &[AppAction::ShowProfiles]);
    let _ = app.update(Message::PaletteSubmit);
    assert_eq!(app.tabs.len(), 3);
    assert_ne!(app.terminals[&2].background, Color::WHITE);
}

/// How long the tests wait for a shell to do something before they give up.
///
/// Only a test that is about to fail waits this long, so this is generous: a shell on a busy
/// machine can take a while to start.
pub(super) const PATIENCE: Duration = Duration::from_secs(60);

/// Poll `condition` until it holds, failing with `what` if it doesn't in time.
///
/// Only for what nothing sends an event about: which process group has a terminal, or what a
/// command run outside of any terminal did. Wait for a program's output with
/// [`wait_for_terminal`].
pub(super) fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !condition() {
        assert!(Instant::now() < deadline, "{what}");
        thread::sleep(Duration::from_millis(10));
    }
}

/// Wait until `condition` holds, checking it again after each event a terminal sends: its PTY
/// event loop sends a wakeup once it parsed a program's output. Fails with `what` if it doesn't
/// hold in time.
///
/// The events are taken from the terminal, so the application doesn't see them.
pub(super) fn wait_for_terminal(
    app: &App,
    terminal: u64,
    what: &str,
    mut condition: impl FnMut() -> bool,
) {
    let deadline = Instant::now() + PATIENCE;
    while !condition() {
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "{what}");
        app.terminals[&terminal].terminal.wait_for_event(left);
    }
}

/// Like [`wait_for_terminal`], with the application handling each event the terminal sends, as
/// it does in a window.
pub(super) fn handle_until(
    app: &mut App,
    terminal: u64,
    what: &str,
    mut condition: impl FnMut(&App) -> bool,
) {
    let deadline = Instant::now() + PATIENCE;
    while !condition(app) {
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "{what}");
        let Some(shell) = app.terminals.get(&terminal) else {
            panic!("{what}: terminal {terminal} closed");
        };
        if let Some(event) = shell.terminal.wait_for_event(left) {
            let _ = app.update(terminal_event(terminal, event));
        }
    }
}

/// Hand the application the events its terminals sent so far, without waiting for more.
pub(super) fn handle_pending(app: &mut App) {
    let terminals: Vec<u64> = app.terminals.keys().copied().collect();
    for terminal in terminals {
        let pending = |app: &App| {
            let shell = app.terminals.get(&terminal)?;
            shell.terminal.wait_for_event(Duration::ZERO)
        };
        while let Some(event) = pending(app) {
            let _ = app.update(terminal_event(terminal, event));
        }
    }
}

/// Wait for a terminal to show `text`.
pub(super) fn wait_for_text(app: &App, terminal: u64, text: &str) {
    wait_for_terminal(app, terminal, &format!("terminal never showed {text:?}"), || {
        app.terminals[&terminal].terminal.text().concat().contains(text)
    });
}

fn paste(terminal: u64, text: &str) -> Message {
    Message::Terminal(iced_tterm::Event::BackendCall(terminal, BackendCommand::Paste(text.into())))
}

#[test]
fn pastes_with_line_breaks_wait_for_an_answer() {
    let mut app = app();

    // A single line is pasted right away.
    let _ = app.update(paste(0, "one"));
    assert!(app.pending_paste.is_none());
    wait_for_text(&app, 0, "one");

    let _ = app.update(paste(0, "two\nthree\n"));
    assert!(app.pending_paste.is_some());
    assert!(has_text(&app, "Paste 2 lines? Every line break runs a line."));
    assert!(has_text(&app, "two") && has_text(&app, "three"));

    // Cancelling pastes nothing.
    perform(&mut app, &[AppAction::PasteCancel]);
    assert!(app.pending_paste.is_none());

    let _ = app.update(paste(0, "four\nfive\n"));
    perform(&mut app, &[AppAction::PasteConfirm]);
    assert!(app.pending_paste.is_none());
    wait_for_text(&app, 0, "five");
    assert!(!app.terminals[&0].terminal.text().concat().contains("two"));
}

#[test]
fn pastes_are_ignored_while_the_paste_warning_is_shown() {
    let mut app = app();
    let _ = app.update(paste(0, "one\ntwo\n"));

    // Neither text that would be asked about nor text that wouldn't gets past the warning.
    let _ = app.update(paste(0, "three\nfour\n"));
    let _ = app.update(paste(0, "five"));
    assert!(has_text(&app, "Paste 2 lines? Every line break runs a line."));
    assert!(has_text(&app, "one") && !has_text(&app, "three"));

    // The answer is about the text that is shown.
    perform(&mut app, &[AppAction::PasteConfirm]);
    assert!(app.pending_paste.is_none());
    let _ = app.update(paste(0, "six"));
    wait_for_text(&app, 0, "six");

    let text = app.terminals[&0].terminal.text().concat();
    assert!(text.contains("one") && text.contains("two"), "{text:?}");
    assert!(!text.contains("three") && !text.contains("five"), "{text:?}");
}

/// A clipboard with the same text on the regular and the selection clipboard.
struct TestClipboard(&'static str);

impl iced_core::Clipboard for TestClipboard {
    fn read(&self, _kind: iced_core::clipboard::Kind) -> Option<String> {
        Some(self.0.into())
    }

    fn write(&mut self, _kind: iced_core::clipboard::Kind, _contents: String) {}
}

/// Press a mouse button over the terminal with `text` on the clipboards, which the simulator
/// has none of, and return the text of the pastes that asks for.
fn mouse_pastes(app: &App, button: iced::mouse::Button, text: &'static str) -> Vec<String> {
    mouse_pastes_with(app, button, Modifiers::empty(), text)
}

/// [`mouse_pastes`] with modifier keys held down, in a terminal that was clicked first so it
/// hears about them.
pub(super) fn mouse_pastes_with(
    app: &App,
    button: iced::mouse::Button,
    modifiers: Modifiers,
    text: &'static str,
) -> Vec<String> {
    use iced::mouse;
    use iced_core::renderer::Headless;
    use iced_winit::runtime::{UserInterface, user_interface};

    let renderer =
        <iced::Renderer as Headless>::new(iced::Font::MONOSPACE, iced::Pixels(16.), None);
    let mut renderer = iced::futures::executor::block_on(renderer).unwrap();
    let cache = user_interface::Cache::default();
    let mut ui = UserInterface::build(app.view(), Size::new(800., 400.), cache, &mut renderer);

    let held = [
        iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)),
    ];
    let held = if modifiers.is_empty() { &held[..0] } else { &held[..] };
    let press = [iced::Event::Mouse(mouse::Event::ButtonPressed(button))];

    let mut messages = Vec::new();
    let _ = ui.update(
        &[held, &press].concat(),
        mouse::Cursor::Available((400., 300.).into()),
        &mut renderer,
        &mut TestClipboard(text),
        &mut messages,
    );

    let pastes = messages.into_iter().filter_map(|message| match message {
        Message::Terminal(iced_tterm::Event::BackendCall(_, BackendCommand::Paste(text))) => {
            Some(text)
        },
        _ => None,
    });
    pastes.collect()
}

#[test]
fn paste_warning_takes_the_mouse() {
    use iced::mouse::Button;

    let config = "[mouse]\nbindings = [{ mouse = \"Right\", action = \"Paste\" }]\n";
    let mut app = app_with(config, Hotkeys::default());
    assert_eq!(mouse_pastes(&app, Button::Middle, "ls\n"), ["ls\n"]);
    assert_eq!(mouse_pastes(&app, Button::Right, "ls\n"), ["ls\n"]);

    // The buttons that paste don't reach the terminal under the warning.
    let _ = app.update(paste(0, "ls\n"));
    assert!(app.pending_paste.is_some());
    assert!(mouse_pastes(&app, Button::Middle, "rm\n").is_empty());
    assert!(mouse_pastes(&app, Button::Right, "rm\n").is_empty());
}

#[test]
fn paste_warning_takes_the_keyboard() {
    use iced::keyboard::key::Named;

    let mut app = app();
    let _ = app.update(paste(0, "ls\n"));
    assert!(has_text(&app, "Paste this line? Its line break runs it."));

    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    ui.typewrite("y");
    ui.tap_key(Named::Enter);

    let mut actions = Vec::new();
    for message in ui.into_messages() {
        match message {
            Message::Action(action) => actions.push(action),
            Message::Terminal(iced_tterm::Event::BackendCall(_, BackendCommand::Write(_))) => {
                panic!("a key reached the terminal")
            },
            _ => (),
        }
    }
    assert_eq!(actions, [AppAction::PasteConfirm]);
}

#[test]
fn pastes_are_not_questioned_with_bracketed_paste_or_when_turned_off() {
    // The shell asks for bracketed paste, like interactive shells and editors do.
    let config = "[terminal]\nshell = { program = \"sh\", args = [\"-c\", \"printf \
                  '\\\\033[?2004h'; exec cat\"] }\n";
    let mut app = app_with(config, Hotkeys::default());
    wait_for_terminal(&app, 0, "bracketed paste was never enabled", || {
        app.terminals[&0].terminal.mode().contains(TermMode::BRACKETED_PASTE)
    });

    let _ = app.update(paste(0, "one\ntwo\n"));
    assert!(app.pending_paste.is_none());
    wait_for_text(&app, 0, "^[[200~one");

    let mut app = app_with("[terminal]\nconfirm_multiline_paste = false\n", Hotkeys::default());
    let _ = app.update(paste(0, "one\ntwo\n"));
    assert!(app.pending_paste.is_none());
    wait_for_text(&app, 0, "two");
}

#[test]
fn finished_selections_are_reported_with_their_text() {
    use iced::mouse;

    let config = "[terminal]\nshell = { program = \"sh\", args = [\"-c\", \"printf 'hello world'; \
                  exec cat\"] }\n";
    let mut app = app_with(config, Hotkeys::default());
    let window = Size::new(800., 400.);

    // Lay the terminal out so it knows its size, and let the shell print.
    let resizes: Vec<_> = {
        let mut ui = Simulator::with_size(Default::default(), window, app.view());
        ui.snapshot(&Theme::Dark).unwrap();
        ui.into_messages().collect()
    };
    for message in resizes {
        let _ = app.update(message);
    }
    wait_for_text(&app, 0, "hello world");
    let wakeup = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup);
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(0, wakeup)));
    assert_eq!(app.terminals[&0].terminal.selection(), None);

    // Drag over the start of the first line.
    let drag: Vec<_> = {
        let mut ui = Simulator::with_size(Default::default(), window, app.view());
        ui.point_at((2., 5.));
        ui.simulate([iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))]);
        ui.point_at((60., 5.));
        ui.simulate([
            iced::Event::Mouse(mouse::Event::CursorMoved { position: (60., 5.).into() }),
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ]);
        ui.into_messages().collect()
    };
    assert!(matches!(
        drag.last(),
        Some(Message::Terminal(iced_tterm::Event::SelectionFinished(0)))
    ));
    for message in drag {
        let _ = app.update(message);
    }

    let selection = app.terminals[&0].terminal.selection().expect("nothing was selected");
    assert!("hello world".starts_with(&selection), "{selection:?}");
}

#[test]
fn mouse_paste_bindings_come_from_the_configuration() {
    use iced::mouse::Button;

    let bindings = |config: &str| {
        let config: UiConfig = toml::from_str(config).unwrap();
        crate::hotkeys::mouse_bindings(config.mouse_bindings())
            .into_iter()
            .filter_map(|(binding, action)| match binding.target {
                InputKind::Mouse(button) => Some((button, binding.modifiers, action)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };

    // By default the middle button pastes and the right one expands the selection.
    let default = bindings("");
    assert!(default.contains(&(Button::Middle, Modifiers::empty(), BindingAction::PasteSelection)));
    for modifiers in [Modifiers::empty(), Modifiers::CTRL] {
        assert!(default.contains(&(Button::Right, modifiers, BindingAction::ExpandSelection)));
    }
    // `Alt` and the right button select a command's output.
    let output = (Button::Right, Modifiers::ALT, BindingAction::SelectCommandOutput);
    assert!(default.contains(&output));

    let right_click = bindings(
        "[mouse]\nbindings = [{ mouse = \"Right\", action = \"Paste\" }, { mouse = \"Middle\", \
         action = \"None\" }]\n",
    );
    assert!(right_click.contains(&(Button::Right, Modifiers::empty(), BindingAction::Paste)));
    assert!(right_click.contains(&(Button::Middle, Modifiers::empty(), BindingAction::Ignore)));
    assert!(!right_click.iter().any(|(.., action)| *action == BindingAction::PasteSelection));
}

/// Configuration running an interactive shell, which gives every command the terminal.
pub(super) const INTERACTIVE_SHELL: &str =
    "[terminal]\nshell = { program = \"sh\", args = [\"-i\"] }\n";

/// Type into the shell of a terminal.
fn type_into(app: &mut App, terminal: u64, bytes: &[u8]) {
    let write = BackendCommand::Write(bytes.to_vec());
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(terminal, write)));
}

/// Type a line into the shell of a terminal.
fn run(app: &mut App, terminal: u64, line: &str) {
    type_into(app, terminal, format!("{line}\n").as_bytes());
}

/// Output of a terminal, as the application is told about it.
pub(super) fn wakeup(terminal: u64) -> Message {
    let wakeup = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup);
    Message::Terminal(iced_tterm::Event::BackendCall(terminal, wakeup))
}

/// Look at the shells until `condition` holds.
fn tick_until(app: &mut App, what: &str, condition: impl Fn(&App) -> bool) {
    wait_until(what, || {
        let _ = app.update(Message::Tick);
        condition(app)
    });
}

/// Start a command in a terminal and wait for the application to see it running.
///
/// The command is `cat`, which runs until [`end_command`] ends it: how long it runs is up to
/// the test, not to the clock.
#[cfg(unix)]
fn start_command(app: &mut App, terminal: u64) {
    run(app, terminal, "cat");
    tick_until(app, "the command never started", |app| app.terminals[&terminal].busy);
}

/// End the command started by [`start_command`] and wait for its shell to have the terminal
/// again, without the application looking at the shell.
#[cfg(unix)]
fn end_command(app: &mut App, terminal: u64) {
    // Ctrl+D at the start of a line is the end of the input `cat` waits for.
    type_into(app, terminal, b"\x04");

    let shell = &app.terminals[&terminal].terminal;
    wait_until("the command never ended", || shell.foreground_process_group() == shell.child_pid());
}

/// Run a command in a terminal and wait for the application to see it finish.
#[cfg(unix)]
fn run_to_the_end(app: &mut App, terminal: u64) {
    start_command(app, terminal);
    end_command(app, terminal);
    tick_until(app, "the command never finished", |app| !app.terminals[&terminal].busy);
}

#[cfg(unix)]
#[test]
fn finished_commands_are_pointed_out_unless_looked_at() {
    let mut app = app_with(INTERACTIVE_SHELL, Hotkeys::default());
    perform(&mut app, &[AppAction::NewTab]);

    // A command finishes in the tab that isn't shown.
    run_to_the_end(&mut app, 0);
    assert!(app.terminals[&0].attention);
    assert!(tab_text(&app, 0).ends_with(" !") && shows_tab(&app, 0));

    // Looking at it is enough.
    perform(&mut app, &[AppAction::SelectTab(0)]);
    assert!(!app.terminals[&0].attention);
    assert!(!tab_text(&app, 0).ends_with(['*', '!']) && shows_tab(&app, 0));

    // What finishes in front of the user isn't pointed out.
    run_to_the_end(&mut app, 0);
    assert!(!app.terminals[&0].attention);

    // An unfocused pane of the shown tab is.
    perform(&mut app, &[AppAction::Split(Axis::Vertical)]);
    run_to_the_end(&mut app, 0);
    assert!(app.terminals[&0].attention && !app.terminals[&2].attention);
    assert!(tab_text(&app, 0).ends_with(" !") && shows_tab(&app, 0));
    perform(&mut app, &[AppAction::FocusPane(Direction::Left)]);
    assert!(!app.terminals[&0].attention);

    // So is the focused pane while the window isn't looked at.
    let _ = app.update(Message::WindowFocused(false));
    run_to_the_end(&mut app, 0);
    assert!(app.terminals[&0].attention);
    let _ = app.update(Message::WindowFocused(true));
    assert!(!app.terminals[&0].attention);
}

#[cfg(unix)]
#[test]
fn finished_commands_are_noticed_with_the_output_that_follows_them() {
    let mut app = app_with(INTERACTIVE_SHELL, Hotkeys::default());
    perform(&mut app, &[AppAction::NewTab]);

    start_command(&mut app, 0);
    end_command(&mut app, 0);
    assert!(app.terminals[&0].busy && !app.terminals[&0].attention);

    // The shell's next prompt is output of the terminal; it doesn't take a tick.
    let _ = app.update(wakeup(0));
    assert!(!app.terminals[&0].busy && app.terminals[&0].attention);
}

/// An event of a terminal, as the application is told about it.
pub(super) fn terminal_event(terminal: u64, event: AlacrittyEvent) -> Message {
    let event = BackendCommand::ProcessAlacrittyEvent(event);
    Message::Terminal(iced_tterm::Event::BackendCall(terminal, event))
}

#[cfg(unix)]
#[test]
fn commands_the_shell_marks_are_noticed_without_watching_its_process() {
    let mut app = app_with(INTERACTIVE_SHELL, Hotkeys::default());
    perform(&mut app, &[AppAction::NewTab]);

    let _ = app.update(terminal_event(0, AlacrittyEvent::CommandStarted));
    assert!(app.terminals[&0].busy && !app.terminals[&0].attention);

    // The shell has the terminal, which would end the command if its process was watched.
    let _ = app.update(wakeup(0));
    let _ = app.update(Message::Tick);
    assert!(app.terminals[&0].busy && !app.terminals[&0].attention);

    let _ = app.update(terminal_event(0, AlacrittyEvent::CommandFinished(Some(1))));
    assert!(!app.terminals[&0].busy && app.terminals[&0].attention);

    // Commands the shell doesn't mark aren't looked for either.
    run(&mut app, 0, "cat");
    let shell = &app.terminals[&0].terminal;
    wait_until("the command never started", || {
        shell.foreground_process_group() != shell.child_pid()
    });
    let _ = app.update(Message::Tick);
    assert!(!app.terminals[&0].busy);
}

#[cfg(target_os = "linux")]
#[test]
fn directories_the_shell_reports_come_first() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().canonicalize().unwrap();

    let mut app = app_with(INTERACTIVE_SHELL, Hotkeys::default());
    let start = self::directory(&app, 0);
    assert_ne!(start.as_ref(), Some(&path));

    let reported = AlacrittyEvent::WorkingDirectory(path.clone());
    let _ = app.update(terminal_event(0, reported));
    assert_eq!(app.terminals[&0].working_directory, Some(path.clone()));

    perform(&mut app, &[AppAction::NewTab]);
    assert_eq!(self::directory(&app, 1), Some(path));

    // A reported directory that is gone since leaves the one the shell's process is in.
    drop(directory);
    perform(&mut app, &[AppAction::SelectTab(0), AppAction::NewTab]);
    assert_eq!(self::directory(&app, 2), start);
}

/// The directory the shell of a terminal is in right now.
#[cfg(target_os = "linux")]
fn directory(app: &App, terminal: u64) -> Option<PathBuf> {
    working_directory(app.terminals[&terminal].terminal.child_pid().unwrap())
}

/// Have the shell of a terminal change its directory and wait until it did, without the
/// application looking at the shell. The shell prints its prompt once it is there.
#[cfg(target_os = "linux")]
pub(super) fn change_directory(app: &mut App, terminal: u64, path: &std::path::Path) {
    run(app, terminal, &format!("cd {}", path.display()));
    wait_for_terminal(app, terminal, "the shell never changed directory", || {
        directory(app, terminal).as_deref() == Some(path)
    });
}

#[cfg(target_os = "linux")]
#[test]
fn new_tabs_and_panes_start_in_the_focused_shells_directory() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().canonicalize().unwrap();

    let mut app = app_with(INTERACTIVE_SHELL, Hotkeys::default());
    change_directory(&mut app, 0, &path);

    perform(&mut app, &[AppAction::NewTab, AppAction::Split(Axis::Vertical)]);
    assert_eq!(self::directory(&app, 1), Some(path.clone()));
    assert_eq!(self::directory(&app, 2), Some(path.clone()));

    // Without the option, new shells start where the first one did.
    let config = format!("{INTERACTIVE_SHELL}\n[tabs]\ninherit_working_directory = false\n");
    let mut app = app_with(&config, Hotkeys::default());
    let start = self::directory(&app, 0);
    change_directory(&mut app, 0, &path);

    perform(&mut app, &[AppAction::NewTab]);
    assert_eq!(self::directory(&app, 1), start);
}

#[cfg(target_os = "linux")]
#[test]
fn directories_are_looked_up_with_the_ticks_for_the_session_file() {
    let state = tempfile::tempdir().unwrap();
    let file = state.path().join("session.json");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().canonicalize().unwrap();
    let saved = |app: &App| match &app.session().tabs[0].layout {
        crate::session::SavedNode::Pane { working_directory } => working_directory.clone(),
        crate::session::SavedNode::Split { .. } => panic!("the tab was never split"),
    };

    let mut app = app_with_session(INTERACTIVE_SHELL, Hotkeys::default(), Some(file.clone()));
    change_directory(&mut app, 0, &path);

    // Output of a terminal doesn't have the directory of its shell looked up.
    let _ = app.update(wakeup(0));
    assert_ne!(saved(&app), Some(path.clone()));

    // The next tick does, and the session file is written with what it found.
    let _ = app.update(Message::Tick);
    assert_eq!(saved(&app), Some(path.clone()));
    assert_eq!(Session::load(&file), Some(app.session()));

    // Without a session file there is nothing to look the directories up for.
    // Nor for the tab titles, without dynamic titles.
    let config = format!("[window]\ndynamic_title = false\n{INTERACTIVE_SHELL}");
    let mut app = app_with(&config, Hotkeys::default());
    change_directory(&mut app, 0, &path);
    let _ = app.update(Message::Tick);
    assert_ne!(saved(&app), Some(path));
}

#[cfg(target_os = "linux")]
#[test]
fn restored_panes_start_in_their_directories() {
    let state = tempfile::tempdir().unwrap();
    let file = state.path().join("session.json");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().canonicalize().unwrap();
    let config = format!("{INTERACTIVE_SHELL}\n[tabs]\ninherit_working_directory = false\n");

    let mut app = app_with_session(&config, Hotkeys::default(), Some(file.clone()));
    perform(&mut app, &[AppAction::Split(Axis::Vertical)]);
    run(&mut app, 1, &format!("cd {}", path.display()));
    tick_until(&mut app, "the shell never changed directory", |app| {
        app.terminals[&1].working_directory.as_ref() == Some(&path)
    });
    let start = self::directory(&app, 0);
    drop(app);

    let app = app_with_session(&config, Hotkeys::default(), Some(file.clone()));
    assert_eq!(terminals(&app, 0), [0, 1]);
    assert_eq!(self::directory(&app, 0), start);
    assert_eq!(self::directory(&app, 1), Some(path));
    drop(app);

    // A directory that is gone doesn't keep its pane from coming back.
    drop(directory);
    let app = app_with_session(&config, Hotkeys::default(), Some(file));
    assert_eq!(terminals(&app, 0), [0, 1]);
    assert_eq!(self::directory(&app, 1), start);
}

#[test]
fn tab_bar_is_shown_with_more_than_one_tab() {
    let mut app = app();
    assert!(!shows_tab(&app, 0));

    perform(&mut app, &[AppAction::NewTab]);
    assert!(shows_tab(&app, 0) && shows_tab(&app, 1));

    perform(&mut app, &[AppAction::CloseTab]);
    assert!(!shows_tab(&app, 0));
}

#[test]
fn tab_bar_visibility_is_configurable() {
    let mut always =
        app_with("[tabs]\nshow = \"Always\"\nposition = \"Bottom\"\n", Hotkeys::default());
    assert!(shows_tab(&always, 0));
    perform(&mut always, &[AppAction::NewTab]);
    assert!(shows_tab(&always, 1));

    let mut never = app_with("[tabs]\nshow = \"Never\"\n", Hotkeys::default());
    perform(&mut never, &[AppAction::NewTab]);
    assert!(!shows_tab(&never, 0));
}

#[test]
fn tab_bar_buttons_select_close_and_add_tabs() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab]);

    let messages = click_on(&app, tab_text(&app, 0));
    assert!(matches!(messages[..], [Message::Action(AppAction::SelectTab(0))]));
    let messages = click_on(&app, icons::id(Icon::Plus));
    assert!(matches!(messages[..], [Message::Action(AppAction::NewTab)]));
    let messages = click_on(&app, icons::close_tab_id(0));
    assert!(matches!(messages[..], [Message::CloseTab(0)]));
}

#[test]
fn hovered_tabs_handle_a_click_once() {
    use iced::mouse::{Button, Event as MouseEvent};

    // A tab under the mouse is drawn twice, with and without the hovered style, and its
    // padding changes when hovered.
    let mut app = app_with("[style.tab_hovered]\npadding = 0\n", Hotkeys::default());
    perform(&mut app, &[AppAction::NewTab, AppAction::NewTab]);

    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    let tab = ui.find(tab_text(&app, 1)).unwrap();
    ui.point_at(iced_test::selector::Bounded::visible_bounds(&tab).unwrap().center());
    ui.simulate([
        iced::Event::Mouse(MouseEvent::ButtonPressed(Button::Middle)),
        iced::Event::Mouse(MouseEvent::ButtonReleased(Button::Middle)),
    ]);
    ui.simulate(simulator::click());

    let messages: Vec<_> =
        ui.into_messages().filter(|message| !matches!(message, Message::Terminal(_))).collect();
    assert!(matches!(messages[..], [
        Message::CloseTab(1),
        Message::Action(AppAction::SelectTab(1))
    ]));
}

#[test]
fn status_segments_show_while_they_have_something_to_show() {
    let config = r#"
        [tabs]
        show = "Always"

        [[tabs.status]]
        segment = "Cwd"
        format = "in {basename} {unknown}"

        [[tabs.status]]
        segment = "Battery"
        format = "{percent}% {status}"

        [[tabs.status]]
        segment = "Clock"
        format = "%a %H:%M"
    "#;
    let mut app = app_with(config, Hotkeys::default());

    // The clock is looked at when the window opens.
    assert!(app.now.is_some());
    app.now = Some("2026-11-12T09:41:00[UTC]".parse().unwrap());
    app.battery = None;
    let focused = app.focused_terminal().unwrap();
    app.terminals.get_mut(&focused).unwrap().working_directory = Some("/srv/www".into());

    assert!(has_text(&app, "Thu 09:41"));
    // A placeholder the segment doesn't know stays as it is written.
    assert!(has_text(&app, "in www {unknown}"));
    // There is no battery.
    assert_eq!(app.segment_text(&app.options.status[1]), None);

    app.battery = Some(status::Battery { percent: 81, status: "Charging".into() });
    assert!(has_text(&app, "81% Charging"));

    app.terminals.get_mut(&focused).unwrap().working_directory = None;
    assert_eq!(app.segment_text(&app.options.status[0]), None);
}

#[test]
fn tab_titles_follow_the_title_format() {
    let mut app = app();
    perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::NewTab]);
    forget_directories(&mut app);

    // The default shows the position, the title and the markers.
    assert_eq!(app.tab_label(0, &app.tabs[0]), "1  test");
    let parts = app.tab_label_parts(0, &app.tabs[0]);
    assert_eq!(parts, [("1".into(), true), ("  test".into(), false)]);
    app.tabs[0].activity = true;
    assert_eq!(app.tab_label(0, &app.tabs[0]), "1  test *");
    perform(&mut app, &[AppAction::SelectTab(0), AppAction::ToggleZoom]);
    assert_eq!(app.tab_label(0, &app.tabs[0]), "1  test Z");
    app.terminals.get_mut(&0).unwrap().attention = true;
    assert_eq!(app.tab_label(0, &app.tabs[0]), "1  test Z !");

    let config = "[tabs]\nshow = \"Always\"\ntitle = \"{{{index}}} {process}@{cwd} [{panes}] \
                  {path} {nope}\"\n";
    let mut app = app_with(config, Hotkeys::default());
    perform(&mut app, &[AppAction::Split(Axis::Vertical)]);
    let focused = app.focused_terminal().unwrap();
    let shell = app.terminals.get_mut(&focused).unwrap();
    shell.working_directory = Some("/srv/www".into());
    shell.process = Some("vim".into());
    assert_eq!(app.tab_label(0, &app.tabs[0]), "{1} vim@www [2] /srv/www {nope}");
    // The number is drawn on its own, between the rest.
    assert!(
        has_text(&app, "{")
            && has_text(&app, "1")
            && has_text(&app, "} vim@www [2] /srv/www {nope}")
    );

    // What can't be told is left empty.
    let shell = app.terminals.get_mut(&focused).unwrap();
    shell.working_directory = None;
    shell.process = None;
    assert_eq!(app.tab_label(0, &app.tabs[0]), "{1} @ [2]  {nope}");
}

#[cfg(target_os = "linux")]
#[test]
fn process_names_are_looked_at_with_the_ticks_while_the_title_shows_them() {
    // Without dynamic titles, which fall back to where the shells are unless they run `ssh`.
    let mut app = app_with("[window]\ndynamic_title = false\n", Hotkeys::default());
    let _ = app.update(Message::Tick);
    assert_eq!(app.terminals[&0].process, None);

    let mut app = app_with("[tabs]\ntitle = \"{process}\"\n", Hotkeys::default());
    let _ = app.update(Message::Tick);
    assert_eq!(app.terminals[&0].process.as_deref(), Some("cat"));
    assert_eq!(app.tab_label(0, &app.tabs[0]), "cat");
}

#[test]
fn only_the_key_table_is_a_segment_by_default() {
    let app = app_with("[tabs]\nshow = \"Always\"\n", Hotkeys::default());

    assert_eq!(app.options.status.len(), 1);
    assert_eq!(app.options.status[0].kind, crate::config::tabs::SegmentKind::KeyTable);
    assert!(app.now.is_none() && app.battery.is_none());
}

/// What the status segment at `index` shows.
fn segment(app: &App, index: usize) -> Option<String> {
    app.segment_text(&app.options.status[index])
}

#[test]
fn zoom_count_and_text_segments_follow_the_tabs() {
    let config = r#"
        [tabs]
        show = "Always"

        [[tabs.status]]
        segment = "Zoom"

        [[tabs.status]]
        segment = "Count"
        format = "tab {tab} of {tabs}, {panes} panes"

        [[tabs.status]]
        segment = "Text"
        format = "|"

        [[tabs.status]]
        segment = "Text"

        [[tabs.status]]
        segment = "Host"
        format = "{user}@{host} {fqdn}"
    "#;
    let mut app = app_with(config, Hotkeys::default());

    assert_eq!(segment(&app, 0), None);
    assert_eq!(segment(&app, 1).as_deref(), Some("tab 1 of 1, 1 panes"));
    assert_eq!(segment(&app, 2).as_deref(), Some("|"));
    // A segment with nothing to show isn't drawn.
    assert_eq!(segment(&app, 3), None);
    assert!(has_text(&app, "|") && !has_text(&app, "ZOOM"));

    perform(&mut app, &[AppAction::NewTab, AppAction::Split(Axis::Vertical)]);
    assert_eq!(segment(&app, 1).as_deref(), Some("tab 2 of 2, 2 panes"));
    assert!(has_text(&app, "tab 2 of 2, 2 panes"));

    perform(&mut app, &[AppAction::ToggleZoom]);
    assert_eq!(segment(&app, 0).as_deref(), Some("ZOOM"));
    assert!(has_text(&app, "ZOOM"));
    perform(&mut app, &[AppAction::SelectTab(0)]);
    assert_eq!(segment(&app, 0), None);

    // Who and where are known wherever the environment and the system say.
    if std::env::var("USER").is_ok_and(|user| !user.is_empty()) {
        let host = segment(&app, 4).unwrap();
        assert!(host.contains('@') && !host.ends_with(' '), "{host}");
    }
}

#[cfg(target_os = "linux")]
#[test]
fn process_segment_shows_while_a_command_runs() {
    let config = format!(
        "{INTERACTIVE_SHELL}\n[tabs]\nshow = \"Always\"\n[[tabs.status]]\nsegment = \"Process\"\n"
    );
    let mut app = app_with(&config, Hotkeys::default());

    // At the prompt the shell has the terminal, and there is nothing to show.
    let _ = app.update(Message::Tick);
    assert_eq!(segment(&app, 0), None);

    start_command(&mut app, 0);
    let _ = app.update(Message::Tick);
    assert_eq!(segment(&app, 0).as_deref(), Some("cat"));
    assert!(has_text(&app, "cat"));

    end_command(&mut app, 0);
    tick_until(&mut app, "the command never finished", |app| !app.terminals[&0].busy);
    assert_eq!(segment(&app, 0), None);
}

#[cfg(unix)]
#[test]
fn load_segment_shows_the_load_average() {
    let config = "[[tabs.status]]\nsegment = \"Load\"\nformat = \"{load1} {load5} {load15}\"\n";
    let mut app = app_with(config, Hotkeys::default());

    app.load = Some([0.5, 1.25, 2.]);
    assert_eq!(segment(&app, 0).as_deref(), Some("0.50 1.25 2.00"));

    // The ticks look at it again.
    let _ = app.update(Message::Tick);
    assert!(app.load.is_some_and(|load| load != [0.5, 1.25, 2.]));
}

#[test]
fn status_segments_go_to_either_end_of_the_tab_bar() {
    let config = r#"
        [tabs]
        show = "Always"

        [[tabs.status]]
        segment = "Text"
        format = "left of the tabs"
        side = "Left"

        [[tabs.status]]
        segment = "Text"
        format = "right of the tabs"
    "#;
    let app = app_with(config, Hotkeys::default());

    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    let mut x = |text: &str| {
        let found = ui.find(text).unwrap();
        iced_test::selector::Bounded::visible_bounds(&found).unwrap().x
    };
    let (left, tab, right) = (x("left of the tabs"), x(&tab_text(&app, 0)), x("right of the tabs"));
    assert!(left < tab && tab < right, "{left} {tab} {right}");
}

#[test]
fn bound_keys_are_handled_and_kept_from_the_shell() {
    let hotkeys =
        Hotkeys::default().with(Key::Character("a".into()), Modifiers::empty(), AppAction::NewTab);
    let app = app_with("", hotkeys);

    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    // Click the terminal to give it keyboard focus.
    ui.point_at((400., 200.));
    ui.simulate(simulator::click());
    ui.typewrite("ab");

    let mut actions = Vec::new();
    let mut written = Vec::new();
    for message in ui.into_messages() {
        match message {
            Message::Action(action) => actions.push(action),
            Message::Terminal(iced_tterm::Event::BackendCall(_, BackendCommand::Write(bytes))) => {
                written.extend(bytes);
            },
            _ => (),
        }
    }

    assert_eq!(actions, [AppAction::NewTab]);
    assert_eq!(written, b"b");
}

/// How long [`type_keys`] pauses where a key is `None`.
///
/// A pause is never shorter than this, however busy the machine is, so a leader key timeout
/// below it has always run out after a pause.
const PAUSE: Duration = Duration::from_millis(80);

/// Press and release keys in a focused terminal of `app`, pausing where a key is `None`.
///
/// Returns the actions the keys ran and the bytes they sent to the shell.
pub(super) fn type_keys(app: &App, keys: &[Option<(Key, Modifiers)>]) -> (Vec<AppAction>, Vec<u8>) {
    use iced::keyboard::key::{NativeCode, Physical};
    use iced::keyboard::{Event, Location};

    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    // Click the terminal to give it keyboard focus.
    ui.point_at((400., 200.));
    ui.simulate(simulator::click());

    for key in keys {
        let Some((key, modifiers)) = key.clone() else {
            thread::sleep(PAUSE);
            continue;
        };

        let physical_key = Physical::Unidentified(NativeCode::Unidentified);
        let text = match &key {
            Key::Character(text) if modifiers.is_empty() => Some(text.clone()),
            _ => None,
        };
        ui.simulate([
            iced::Event::Keyboard(Event::ModifiersChanged(modifiers)),
            iced::Event::Keyboard(Event::KeyPressed {
                key: key.clone(),
                modified_key: key.clone(),
                physical_key,
                location: Location::Standard,
                modifiers,
                text,
                repeat: false,
            }),
            iced::Event::Keyboard(Event::KeyReleased {
                key: key.clone(),
                modified_key: key,
                physical_key,
                location: Location::Standard,
                modifiers,
            }),
            iced::Event::Keyboard(Event::ModifiersChanged(Modifiers::empty())),
        ]);
    }

    let mut actions = Vec::new();
    let mut written = Vec::new();
    for message in ui.into_messages() {
        match message {
            Message::Action(action) => actions.push(action),
            Message::Terminal(iced_tterm::Event::BackendCall(_, BackendCommand::Write(bytes))) => {
                written.extend(bytes)
            },
            _ => (),
        }
    }

    (actions, written)
}

pub(super) fn key(character: &str) -> Option<(Key, Modifiers)> {
    Some((Key::Character(character.into()), Modifiers::empty()))
}

fn ctrl(character: &str) -> Option<(Key, Modifiers)> {
    Some((Key::Character(character.into()), Modifiers::CTRL))
}

/// Leader key timeout, in milliseconds, for tests whose keys follow the leader key in time
/// however slowly the test runs.
const NEVER: u64 = 24 * 60 * 60 * 1000;

/// Hotkeys with `Ctrl+A` as the leader key and `c` after it bound to a new tab.
fn leader_hotkeys(timeout: u64) -> Hotkeys {
    Hotkeys::default()
        .with_leader(Key::Character("a".into()), Modifiers::CTRL, Duration::from_millis(timeout))
        .with_sequence(Key::Character("c".into()), Modifiers::empty(), AppAction::NewTab)
}

#[test]
fn leader_key_and_the_key_after_it_are_kept_from_the_shell() {
    let app = app_with("", leader_hotkeys(NEVER));

    let (actions, written) = type_keys(&app, &[ctrl("a"), key("c"), key("c")]);
    assert_eq!(actions, [AppAction::NewTab]);
    // Only the second `c` is typed into the shell.
    assert_eq!(written, b"c");
}

#[test]
fn unbound_key_after_the_leader_key_cancels_the_sequence() {
    use iced::keyboard::key::Named;

    let app = app_with("", leader_hotkeys(NEVER));

    let (actions, written) = type_keys(&app, &[ctrl("a"), key("q"), key("c")]);
    assert_eq!(actions, []);
    assert_eq!(written, b"c");

    // Reaching for a modifier on the way to the second key doesn't cancel.
    let shift = Some((Key::Named(Named::Shift), Modifiers::SHIFT));
    let (actions, written) = type_keys(&app, &[ctrl("a"), shift, key("c")]);
    assert_eq!(actions, [AppAction::NewTab]);
    assert_eq!(written, b"");
}

#[test]
fn leader_key_twice_sends_it_to_the_shell() {
    let app = app_with("", leader_hotkeys(NEVER));

    let (actions, written) = type_keys(&app, &[ctrl("a"), ctrl("a"), key("c")]);
    assert_eq!(actions, []);
    assert_eq!(written, b"\x01c");
}

#[test]
fn leader_key_gives_up_after_its_timeout() {
    // Shorter than the pause, which a busy machine only makes longer.
    let app = app_with("", leader_hotkeys(20));

    let (actions, written) = type_keys(&app, &[ctrl("a"), None, key("c")]);
    assert_eq!(actions, []);
    assert_eq!(written, b"c");
}

#[test]
fn leader_key_is_left_to_the_shell_without_leader_bindings() {
    let hotkeys = Hotkeys::default().with_leader(
        Key::Character("a".into()),
        Modifiers::CTRL,
        Duration::from_millis(NEVER),
    );
    let app = app_with("", hotkeys);

    let (actions, written) = type_keys(&app, &[ctrl("a"), key("c")]);
    assert_eq!(actions, []);
    assert_eq!(written, b"\x01c");
}

#[test]
fn tabs_share_the_bar_evenly_up_to_the_widest_tab() {
    // Where the button after the tabs starts, in a window 800 wide.
    let new_tab_button = |app: &App| {
        let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
        let plus = ui.find(icons::id(Icon::Plus)).unwrap();
        iced_test::selector::Bounded::visible_bounds(&plus).unwrap().x
    };

    // A few tabs are as wide as a tab gets, and leave the rest of the bar empty.
    let config = "[tabs]\nshow = \"Always\"\nwidth = \"Shared\"\n";
    let mut app = app_with(config, Hotkeys::default());
    let one = new_tab_button(&app);
    assert!((MAX_TAB_WIDTH..MAX_TAB_WIDTH + 20.).contains(&one), "{one}");
    perform(&mut app, &[AppAction::NewTab]);
    let two = new_tab_button(&app);
    assert!((two - one - MAX_TAB_WIDTH).abs() < 2., "{one} {two}");

    // Many share the bar, and the button stays in it.
    perform(&mut app, &vec![AppAction::NewTab; 10]);
    let crowded = new_tab_button(&app);
    assert!((600. ..800.).contains(&crowded), "{crowded}");
}

#[test]
fn tabs_are_as_wide_as_their_titles_by_default() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab]);
    app.tabs[0].title = Some("a".into());
    app.tabs[1].title = Some("a much longer title".into());

    let (first, second) = (text_x(&app, tab_text(&app, 0)), text_x(&app, tab_text(&app, 1)));
    let plus = text_x(&app, icons::id(Icon::Plus));
    let (first, second) = (second - first, plus - second);
    assert!(first < second && first < 100., "{first} {second}");
    assert!(second < MAX_TAB_WIDTH, "{second}");

    // The whole title shows.
    let label = tab_text(&app, 1);
    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    let shown = iced_test::selector::Bounded::visible_bounds(&ui.find(label.as_str()).unwrap());
    let cell = app.settings.font.cell_size().width;
    let shown = shown.unwrap().width;
    assert!(shown >= label.chars().count() as f32 * cell - 1., "{shown} {cell}");
}

#[test]
fn long_titles_and_crowded_bars_keep_the_buttons_on_the_bar() {
    let mut app = app_with("[tabs]\nshow = \"Always\"\n", Hotkeys::default());
    app.tabs[0].title = Some("x".repeat(200));
    let tab = text_x(&app, icons::id(Icon::Plus)) - text_x(&app, tab_text(&app, 0));
    assert!(tab <= MAX_TAB_WIDTH + 2., "{tab}");

    // Twelve tabs share the bar, and the button after them stays on it.
    perform(&mut app, &vec![AppAction::NewTab; 11]);
    let plus = text_x(&app, icons::id(Icon::Plus));
    assert!(plus < 790., "{plus}");
    let step = text_x(&app, tab_text(&app, 11)) - text_x(&app, tab_text(&app, 10));
    assert!(step < 800. / 12., "{step}");
}

/// Where what `selector` finds, like a text or a drawn icon's ID, starts in the window, 800
/// wide.
pub(super) fn text_x<S>(app: &App, selector: S) -> f32
where
    S: iced_test::selector::Selector + Send,
    S::Output: iced_test::selector::Bounded + Clone + Send,
{
    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    let found = ui.find(selector).unwrap();
    iced_test::selector::Bounded::visible_bounds(&found).unwrap().x
}

/// The messages a click on what `selector` finds sends, apart from the terminals'.
pub(super) fn click_on<S>(app: &App, selector: S) -> Vec<Message>
where
    S: iced_test::selector::Selector + Send,
    S::Output: iced_test::selector::Bounded + Clone + Send + Sync + 'static,
{
    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    ui.click(selector).unwrap();
    // The terminal also publishes its size when it is laid out.
    ui.into_messages().filter(|message| !matches!(message, Message::Terminal(_))).collect()
}

#[test]
fn tab_widths_are_in_cells() {
    let config = "[tabs]\nshow = \"Always\"\n[style.tab_active]\nwidth = \
                  20\n[style.tab_inactive]\nmax_width = 10\n";
    let mut app = app_with(config, Hotkeys::default());
    let cell = app.settings.font.cell_size().width;

    // The shown tab is 20 cells wide, then a pixel, then the new tab button's padding, and its
    // icon is centered in a square as wide as the font is high.
    let size = app.settings.font.size;
    let centered = (size - (size * 0.75).round()) / 2.;
    let plus = text_x(&app, icons::id(Icon::Plus)) - centered;
    assert!((plus - (20. * cell).floor() - 1. - 14.).abs() < 1., "{plus} {cell}");

    // The second tab is shown, and the first is 10 cells wide.
    perform(&mut app, &[AppAction::NewTab]);
    let second = text_x(&app, tab_text(&app, 1)) - text_x(&app, tab_text(&app, 0));
    assert!((second - (10. * cell).floor() - 1.).abs() < 1., "{second} {cell}");
}

#[test]
fn tabs_with_a_width_keep_it_in_a_crowded_bar() {
    let config = "[tabs]\nshow = \"Always\"\n[style.tab_active]\nwidth = 30\n";
    let mut app = app_with(config, Hotkeys::default());
    perform(&mut app, &vec![AppAction::NewTab; 9]);
    perform(&mut app, &[AppAction::SelectTab(0)]);
    let cell = app.settings.font.cell_size().width;

    let shown = text_x(&app, tab_text(&app, 1)) - text_x(&app, tab_text(&app, 0));
    assert!((shown - (30. * cell).floor() - 1.).abs() < 1., "{shown} {cell}");

    // The other tabs share what is left.
    let other = text_x(&app, tab_text(&app, 2)) - text_x(&app, tab_text(&app, 1));
    assert!(other < shown / 2., "{other} {shown}");
}

/// Whether two applications draw the same picture with the mouse at `at`, if anywhere.
fn same_picture_at(one: &App, other: &App, at: Option<(f32, f32)>) -> bool {
    let window = Size::new(800., 400.);
    let directory = tempfile::tempdir().unwrap();
    let picture = directory.path().join("picture");
    let snapshot = |app: &App| {
        let mut ui = Simulator::with_size(Default::default(), window, app.view());
        if let Some(at) = at {
            ui.point_at(at);
        }
        ui.snapshot(&Theme::Dark).unwrap()
    };

    // The first picture is kept as what the second one is compared with.
    assert!(snapshot(one).matches_hash(&picture).unwrap());
    snapshot(other).matches_hash(&picture).unwrap()
}

/// Where the close button of the first tab is, in a bar that shows it.
fn close_button_center(app: &App) -> iced::Point {
    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    let close = ui.find(icons::close_tab_id(0)).unwrap();
    iced_test::selector::Bounded::visible_bounds(&close).unwrap().center()
}

#[test]
fn close_buttons_show_under_the_mouse() {
    let hover = app_with("[tabs]\nshow = \"Always\"\n", Hotkeys::default());
    let always =
        app_with("[tabs]\nshow = \"Always\"\nclose_button = \"Always\"\n", Hotkeys::default());

    // The room for the close button is kept.
    assert_eq!(text_x(&hover, icons::id(Icon::Plus)), text_x(&always, icons::id(Icon::Plus)));

    // It is only drawn under the mouse.
    let close = close_button_center(&always);
    assert!(!same_picture_at(&hover, &always, None));
    assert!(same_picture_at(&hover, &always, Some((close.x, close.y))));
}

#[test]
fn a_hidden_close_button_closes_its_tab_with_one_click() {
    let mut app = app_with("[tabs]\nshow = \"Always\"\n", Hotkeys::default());
    perform(&mut app, &[AppAction::NewTab]);
    let mut always =
        app_with("[tabs]\nshow = \"Always\"\nclose_button = \"Always\"\n", Hotkeys::default());
    perform(&mut always, &[AppAction::NewTab]);
    let close = close_button_center(&always);

    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    ui.point_at(close);
    ui.simulate(simulator::click());
    let messages: Vec<_> =
        ui.into_messages().filter(|message| !matches!(message, Message::Terminal(_))).collect();
    assert!(matches!(messages[..], [Message::CloseTab(0)]), "{messages:?}");
}

#[test]
fn close_buttons_can_be_left_out() {
    use iced::mouse::{Button, Event as MouseEvent};

    let hover = app_with("[tabs]\nshow = \"Always\"\n", Hotkeys::default());
    let never =
        app_with("[tabs]\nshow = \"Always\"\nclose_button = \"Never\"\n", Hotkeys::default());
    assert!(text_x(&never, icons::id(Icon::Plus)) < text_x(&hover, icons::id(Icon::Plus)));

    // Nothing along the tab closes it, but a middle click still does.
    let start = text_x(&never, tab_text(&never, 0));
    let end = text_x(&never, icons::id(Icon::Plus));
    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), never.view());
    let y = iced_test::selector::Bounded::visible_bounds(&ui.find(tab_text(&never, 0)).unwrap())
        .unwrap()
        .center_y();
    let mut x = start;
    while x < end - 2. {
        ui.point_at((x, y));
        ui.simulate(simulator::click());
        x += 2.;
    }
    let messages: Vec<_> = ui.into_messages().collect();
    assert!(!messages.iter().any(|message| matches!(message, Message::CloseTab(_))));

    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), never.view());
    ui.point_at((start + 4., y));
    ui.simulate([
        iced::Event::Mouse(MouseEvent::ButtonPressed(Button::Middle)),
        iced::Event::Mouse(MouseEvent::ButtonReleased(Button::Middle)),
    ]);
    assert!(ui.into_messages().any(|message| matches!(message, Message::CloseTab(0))));
}

#[test]
fn bordered_tabs_and_buttons_take_clicks() {
    // Every kind of border, on single sides, on everything with a border.
    for kind in ["Normal", "Rounded", "Thick", "Double", "Block", "ASCII"] {
        let config = format!(
            "[tabs]\nshow = \"Always\"\n[style.defs.lined]\nborder = \"{kind}\"\nborder_bottom = \
             true\nborder_left = true\nborder_left_foreground = \
             \"#ff0000\"\n[style.tab_bar]\ninherit = \"lined\"\n[style.tab_active]\ninherit = \
             \"lined\"\n[style.tab_inactive]\ninherit = \"lined\"\n[style.tab_button]\ninherit = \
             \"lined\"\n"
        );
        let mut app = app_with(&config, Hotkeys::default());
        perform(&mut app, &[AppAction::NewTab]);

        let click = |text: &str| {
            let mut ui =
                Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
            ui.click(text).unwrap();
            ui.into_messages()
                .filter(|message| !matches!(message, Message::Terminal(_)))
                .collect::<Vec<_>>()
        };
        let first = tab_text(&app, 0);
        assert!(matches!(click(&first)[..], [Message::Action(AppAction::SelectTab(0))]));
        let plus = click_on(&app, icons::id(Icon::Plus));
        assert!(matches!(plus[..], [Message::Action(AppAction::NewTab)]));
        assert!(matches!(click_on(&app, icons::close_tab_id(0))[..], [Message::CloseTab(0)]));
    }
}

#[test]
fn gap_is_the_space_between_tabs() {
    let config = "[tabs]\nshow = \"Always\"\nwidth = \"Shared\"\n[style.tab_bar]\ngap = 12\n";
    let mut app = app_with(config, Hotkeys::default());
    perform(&mut app, &[AppAction::NewTab]);

    let step = text_x(&app, tab_text(&app, 1)) - text_x(&app, tab_text(&app, 0));
    assert!((step - MAX_TAB_WIDTH - 12.).abs() < 1., "{step}");
}

/// Shells get the shell integration where they are started, unless it is turned off.
#[cfg(unix)]
#[test]
fn shells_are_started_with_the_shell_integration_unless_it_is_off() {
    use std::os::unix::fs::PermissionsExt;

    // A program named like zsh, which shows the directory zsh would read its files from.
    let directory = tempfile::tempdir().unwrap();
    let zsh = directory.path().join("zsh");
    std::fs::write(&zsh, "#!/bin/sh\necho \"zdotdir=${ZDOTDIR-}.\"\nexec cat\n").unwrap();
    std::fs::set_permissions(&zsh, std::fs::Permissions::from_mode(0o755)).unwrap();
    let shell = format!("terminal.shell = \"{}\"\n", zsh.display());

    let app = app_with(&shell, Hotkeys::default());
    wait_for_text(&app, 0, "/zsh.");
    let text = app.terminals[&0].terminal.text().concat();
    assert!(text.contains("/shell-integration/"), "{text}");

    let app =
        app_with(&format!("{shell}terminal.shell_integration = \"None\"\n"), Hotkeys::default());
    wait_for_text(&app, 0, ".");
    let text = app.terminals[&0].terminal.text().concat();
    assert!(!text.contains("/shell-integration/"), "{text}");
}
