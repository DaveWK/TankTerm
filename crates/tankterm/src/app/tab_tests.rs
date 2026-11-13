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

use super::*;
use crate::cli::Options;
use crate::config::UiConfig;
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
        dynamic_title: true,
        layout: Layout {
            padding: Padding::ZERO,
            background: term_settings::background(&config),
            foreground: term_settings::foreground(&config),
        },
        tabs: config.tabs,
        hotkeys,
        session_file,
        profiles,
        confirm_multiline_paste: config.terminal.confirm_multiline_paste,
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
    assert_eq!(app.title(), "test");
    let _ = app.update(title(1, "second"));
    assert_eq!(app.title(), "second");

    perform(&mut app, &[AppAction::FocusPane(Direction::Left)]);
    assert_eq!(app.title(), "first");
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
    assert!(has_text(&app, "1: test *") && !has_text(&app, "1: test"));
    let _ = app.update(event(0, AlacrittyEvent::Bell));
    assert!(has_text(&app, "1: test !"));

    // Output of the tab that is shown isn't pointed out.
    let _ = app.update(event(1, AlacrittyEvent::Wakeup));
    let _ = app.update(event(1, AlacrittyEvent::Bell));
    assert!(has_text(&app, "2: test"));

    // Showing a tab clears its markers.
    perform(&mut app, &[AppAction::SelectTab(0)]);
    assert!(has_text(&app, "1: test") && has_text(&app, "2: test"));
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
    assert!(has_text(&app, "2: test"));

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
const PATIENCE: Duration = Duration::from_secs(60);

/// Wait until `condition` holds, failing with `what` if it doesn't in time.
pub(super) fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + PATIENCE;
    while !condition() {
        assert!(Instant::now() < deadline, "{what}");
        thread::sleep(Duration::from_millis(10));
    }
}

/// Wait for a terminal to show `text`.
pub(super) fn wait_for_text(app: &App, terminal: u64, text: &str) {
    wait_until(&format!("terminal never showed {text:?}"), || {
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
    wait_until("bracketed paste was never enabled", || {
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
    assert!(has_text(&app, "1: test !"));

    // Looking at it is enough.
    perform(&mut app, &[AppAction::SelectTab(0)]);
    assert!(!app.terminals[&0].attention);
    assert!(has_text(&app, "1: test"));

    // What finishes in front of the user isn't pointed out.
    run_to_the_end(&mut app, 0);
    assert!(!app.terminals[&0].attention);

    // An unfocused pane of the shown tab is.
    perform(&mut app, &[AppAction::Split(Axis::Vertical)]);
    run_to_the_end(&mut app, 0);
    assert!(app.terminals[&0].attention && !app.terminals[&2].attention);
    assert!(has_text(&app, "1: test !"));
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

/// The directory the shell of a terminal is in right now.
#[cfg(target_os = "linux")]
fn directory(app: &App, terminal: u64) -> Option<PathBuf> {
    working_directory(app.terminals[&terminal].terminal.child_pid().unwrap())
}

/// Have the shell of a terminal change its directory and wait until it did, without the
/// application looking at the shell.
#[cfg(target_os = "linux")]
pub(super) fn change_directory(app: &mut App, terminal: u64, path: &std::path::Path) {
    run(app, terminal, &format!("cd {}", path.display()));
    wait_until("the shell never changed directory", || {
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
    let mut app = app_with(INTERACTIVE_SHELL, Hotkeys::default());
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
    assert!(!has_text(&app, "1: test"));

    perform(&mut app, &[AppAction::NewTab]);
    assert!(has_text(&app, "1: test") && has_text(&app, "2: test"));

    perform(&mut app, &[AppAction::CloseTab]);
    assert!(!has_text(&app, "1: test"));
}

#[test]
fn tab_bar_visibility_is_configurable() {
    let mut always =
        app_with("[tabs]\nshow = \"Always\"\nposition = \"Bottom\"\n", Hotkeys::default());
    assert!(has_text(&always, "1: test"));
    perform(&mut always, &[AppAction::NewTab]);
    assert!(has_text(&always, "2: test"));

    let mut never = app_with("[tabs]\nshow = \"Never\"\n", Hotkeys::default());
    perform(&mut never, &[AppAction::NewTab]);
    assert!(!has_text(&never, "1: test"));
}

#[test]
fn tab_bar_buttons_select_close_and_add_tabs() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab]);

    let click = |app: &App, text: &str| {
        let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
        ui.click(text).unwrap();
        // The terminal also publishes its size when it is laid out.
        ui.into_messages()
            .filter(|message| !matches!(message, Message::Terminal(_)))
            .collect::<Vec<_>>()
    };

    let messages = click(&app, "1: test");
    assert!(matches!(messages[..], [Message::Action(AppAction::SelectTab(0))]));
    let messages = click(&app, "+");
    assert!(matches!(messages[..], [Message::Action(AppAction::NewTab)]));
    // Both tabs have a close button; the first one found belongs to the first tab.
    let messages = click(&app, "\u{d7}");
    assert!(matches!(messages[..], [Message::CloseTab(0)]));
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
