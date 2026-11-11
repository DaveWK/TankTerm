//! Tabs and panes through the real `App`, with `cat` as the shell.

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
    let config: UiConfig = toml::from_str(config).unwrap();
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
    };

    App::new(options, settings.clone(), settings).unwrap()
}

fn app_with(config: &str, hotkeys: Hotkeys) -> App {
    app_with_session(config, hotkeys, None)
}

fn app() -> App {
    app_with("", Hotkeys::default())
}

fn perform(app: &mut App, actions: &[AppAction]) {
    for action in actions {
        let _ = app.update(Message::Action(*action));
    }
}

/// Terminal IDs of the panes of the tab at `index`, in layout order.
fn terminals(app: &App, index: usize) -> Vec<u64> {
    let tab = &app.tabs[index];
    ordered_panes(tab.panes.layout()).iter().map(|pane| *tab.panes.get(*pane).unwrap()).collect()
}

/// Terminal ID of the focused pane of the tab that is shown.
fn focused(app: &App) -> u64 {
    let tab = &app.tabs[app.active];
    *tab.panes.get(tab.focus).unwrap()
}

/// Text drawn by the application's view.
fn has_text(app: &App, text: &str) -> bool {
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
    perform(&mut app, &[AppAction::ResizePane(Direction::Left); 20]);
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

/// Wait for a terminal to show `text`.
fn wait_for_text(app: &App, terminal: u64, text: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !app.terminals[&terminal].terminal.text().concat().contains(text) {
        assert!(std::time::Instant::now() < deadline, "terminal never showed {text:?}");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
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
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !app.terminals[&0].terminal.mode().contains(TermMode::BRACKETED_PASTE) {
        assert!(std::time::Instant::now() < deadline, "bracketed paste was never enabled");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

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

    // By default only the middle button pastes.
    let default = bindings("");
    assert!(default.contains(&(Button::Middle, Modifiers::empty(), BindingAction::PasteSelection)));
    assert!(default.contains(&(Button::Right, Modifiers::empty(), BindingAction::Ignore)));

    let right_click = bindings(
        "[mouse]\nbindings = [{ mouse = \"Right\", action = \"Paste\" }, { mouse = \"Middle\", \
         action = \"None\" }]\n",
    );
    assert!(right_click.contains(&(Button::Right, Modifiers::empty(), BindingAction::Paste)));
    assert!(right_click.contains(&(Button::Middle, Modifiers::empty(), BindingAction::Ignore)));
    assert!(!right_click.iter().any(|(.., action)| *action == BindingAction::PasteSelection));
}

/// Configuration running an interactive shell, which gives every command the terminal.
const INTERACTIVE_SHELL: &str = "[terminal]\nshell = { program = \"sh\", args = [\"-i\"] }\n";

/// Type a line into the shell of a terminal.
fn run(app: &mut App, terminal: u64, line: &str) {
    let write = BackendCommand::Write(format!("{line}\n").into_bytes());
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(terminal, write)));
}

/// Look at the shells until `condition` holds.
fn tick_until(app: &mut App, what: &str, condition: impl Fn(&App) -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let _ = app.update(Message::Tick);
        if condition(app) {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "{what}");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Run a short command in a terminal and wait for it to finish.
#[cfg(unix)]
fn run_to_the_end(app: &mut App, terminal: u64) {
    run(app, terminal, "sleep 0.3");
    tick_until(app, "the command never started", |app| app.terminals[&terminal].busy);
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

/// The directory the shell of a terminal is in right now.
#[cfg(target_os = "linux")]
fn directory(app: &App, terminal: u64) -> Option<PathBuf> {
    working_directory(app.terminals[&terminal].terminal.child_pid().unwrap())
}

#[cfg(target_os = "linux")]
#[test]
fn new_tabs_and_panes_start_in_the_focused_shells_directory() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().canonicalize().unwrap();

    let mut app = app_with(INTERACTIVE_SHELL, Hotkeys::default());
    run(&mut app, 0, &format!("cd {}", path.display()));
    tick_until(&mut app, "the shell never changed directory", |app| {
        app.terminals[&0].working_directory.as_ref() == Some(&path)
    });

    perform(&mut app, &[AppAction::NewTab, AppAction::Split(Axis::Vertical)]);
    assert_eq!(self::directory(&app, 1), Some(path.clone()));
    assert_eq!(self::directory(&app, 2), Some(path.clone()));

    // Without the option, new shells start where the first one did.
    let config = format!("{INTERACTIVE_SHELL}\n[tabs]\ninherit_working_directory = false\n");
    let mut app = app_with(&config, Hotkeys::default());
    let start = self::directory(&app, 0);
    run(&mut app, 0, &format!("cd {}", path.display()));
    tick_until(&mut app, "the shell never changed directory", |app| {
        app.terminals[&0].working_directory.as_ref() == Some(&path)
    });

    perform(&mut app, &[AppAction::NewTab]);
    assert_eq!(self::directory(&app, 1), start);
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

/// Press and release keys in a focused terminal of `app`, pausing where a key is `None`.
///
/// Returns the actions the keys ran and the bytes they sent to the shell.
fn type_keys(app: &App, keys: &[Option<(Key, Modifiers)>]) -> (Vec<AppAction>, Vec<u8>) {
    use iced::keyboard::key::{NativeCode, Physical};
    use iced::keyboard::{Event, Location};

    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    // Click the terminal to give it keyboard focus.
    ui.point_at((400., 200.));
    ui.simulate(simulator::click());

    for key in keys {
        let Some((key, modifiers)) = key.clone() else {
            std::thread::sleep(std::time::Duration::from_millis(80));
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

fn key(character: &str) -> Option<(Key, Modifiers)> {
    Some((Key::Character(character.into()), Modifiers::empty()))
}

fn ctrl(character: &str) -> Option<(Key, Modifiers)> {
    Some((Key::Character(character.into()), Modifiers::CTRL))
}

/// Hotkeys with `Ctrl+A` as the leader key and `c` after it bound to a new tab.
fn leader_hotkeys(timeout: u64) -> Hotkeys {
    Hotkeys::default()
        .with_leader(
            Key::Character("a".into()),
            Modifiers::CTRL,
            std::time::Duration::from_millis(timeout),
        )
        .with_sequence(Key::Character("c".into()), Modifiers::empty(), AppAction::NewTab)
}

#[test]
fn leader_key_and_the_key_after_it_are_kept_from_the_shell() {
    let app = app_with("", leader_hotkeys(10_000));

    let (actions, written) = type_keys(&app, &[ctrl("a"), key("c"), key("c")]);
    assert_eq!(actions, [AppAction::NewTab]);
    // Only the second `c` is typed into the shell.
    assert_eq!(written, b"c");
}

#[test]
fn unbound_key_after_the_leader_key_cancels_the_sequence() {
    use iced::keyboard::key::Named;

    let app = app_with("", leader_hotkeys(10_000));

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
    let app = app_with("", leader_hotkeys(10_000));

    let (actions, written) = type_keys(&app, &[ctrl("a"), ctrl("a"), key("c")]);
    assert_eq!(actions, []);
    assert_eq!(written, b"\x01c");
}

#[test]
fn leader_key_gives_up_after_its_timeout() {
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
        std::time::Duration::from_secs(10),
    );
    let app = app_with("", hotkeys);

    let (actions, written) = type_keys(&app, &[ctrl("a"), key("c")]);
    assert_eq!(actions, []);
    assert_eq!(written, b"\x01c");
}
