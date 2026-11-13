//! `tankTerm msg` messages through the real `App`, with `cat` as the shell.

use iced::widget::pane_grid::{Axis, Node};

use super::tab_tests::{app_with, focused, perform, wait_for_text};
use super::*;
use crate::cli::{
    ActivateTabOptions, GetTextOptions, IpcGetConfig, ListOptions, PaneOptions, ProgramOptions,
    SendTextOptions, SetTabTitleOptions, SocketMessage, SpawnOptions, SplitOptions, ZoomOptions,
};
use crate::hotkeys::Hotkeys;
use crate::ipc::{PaneInfo, SocketReply};
use crate::session::ordered_panes;

fn app() -> App {
    app_with("", Hotkeys::default())
}

fn answer(app: &mut App, message: SocketMessage) -> SocketReply {
    let (reply, _) = app.answer(message);
    // The application looks at the tabs after every message, as it does after this one.
    let _ = app.update(Message::Tick);
    reply
}

/// A program for a new tab or pane: `sh` running `script`, then `cat`.
fn script(script: &str) -> ProgramOptions {
    let command = ["sh", "-c", &format!("{script}; exec cat")].map(String::from);
    ProgramOptions { cwd: None, command: command.to_vec() }
}

fn spawn(app: &mut App, program: ProgramOptions) -> SocketReply {
    answer(app, SocketMessage::Spawn(SpawnOptions { profile: None, program }))
}

fn split(app: &mut App, options: SplitOptions) -> SocketReply {
    answer(app, SocketMessage::SplitPane(options))
}

fn pane(pane_id: Option<u64>) -> PaneOptions {
    PaneOptions { pane_id }
}

fn panes(app: &mut App) -> Vec<PaneInfo> {
    match answer(app, SocketMessage::List(ListOptions::default())) {
        SocketReply::Panes(panes) => panes,
        reply => panic!("not a list of panes: {reply:?}"),
    }
}

/// Terminal IDs of the panes of the tab at `index`, in layout order.
fn layout(app: &App, index: usize) -> Vec<u64> {
    let tab = &app.tabs[index];
    ordered_panes(tab.panes.layout()).iter().map(|pane| *tab.panes.get(*pane).unwrap()).collect()
}

/// The axis and ratio of the outermost split of the tab at `index`.
fn outer_split(app: &App, index: usize) -> (Axis, f32) {
    match app.tabs[index].panes.layout() {
        Node::Split { axis, ratio, .. } => (*axis, *ratio),
        Node::Pane(_) => panic!("the tab isn't split"),
    }
}

fn is_error(reply: &SocketReply, text: &str) -> bool {
    matches!(reply, SocketReply::Error(err) if err.contains(text))
}

#[test]
fn panes_are_listed_tab_by_tab() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab, AppAction::Split(Axis::Vertical)]);
    perform(&mut app, &[AppAction::ToggleZoom]);

    let summary: Vec<_> = panes(&mut app)
        .into_iter()
        .map(|pane| (pane.tab_id, pane.tab_index, pane.pane_id, pane.is_active, pane.is_zoomed))
        .collect();
    assert_eq!(summary, [(0, 0, 0, true, false), (1, 1, 1, false, false), (1, 1, 2, true, true)]);

    let pane = &panes(&mut app)[0];
    assert_eq!((pane.title.as_str(), pane.tab_title.as_str()), ("test", "test"));
    assert!(!pane.is_active_tab && pane.size.rows > 0 && pane.size.cols > 0, "{pane:?}");
    #[cfg(target_os = "linux")]
    assert!(std::path::Path::new(&pane.cwd).is_dir(), "{pane:?}");
}

#[test]
fn list_json_uses_the_field_names_of_wezterm() {
    let mut app = app();
    let json = serde_json::to_value(&panes(&mut app)[0]).unwrap();

    for field in ["window_id", "tab_id", "pane_id", "title", "cwd", "tab_title", "is_active"] {
        assert!(json.get(field).is_some(), "no {field} in {json}");
    }
    assert!(json["size"]["rows"].is_u64() && json["size"]["cols"].is_u64(), "{json}");
}

#[test]
fn spawn_opens_a_tab_whose_shell_knows_its_pane() {
    let mut app = app();
    let directory = tempfile::tempdir().unwrap();
    let directory = directory.path().canonicalize().unwrap();

    let mut program = script("echo pane=$TANKTERM_PANE_ID; pwd");
    program.cwd = Some(directory.clone());
    assert_eq!(spawn(&mut app, program), SocketReply::Pane(1));

    assert_eq!(app.tabs.len(), 2);
    assert_eq!(app.active, 1);
    wait_for_text(&app, 1, "pane=1");
    wait_for_text(&app, 1, &directory.display().to_string());
}

#[test]
fn spawn_opens_profiles_by_name() {
    let config = "[[profiles]]\nname = \"Plain\"\nshell = \"cat\"\n";
    let mut app = app_with(config, Hotkeys::default());

    let spawn = |app: &mut App, name: &str| {
        let profile = Some(name.to_owned());
        let options = SpawnOptions { profile, program: ProgramOptions::default() };
        answer(app, SocketMessage::Spawn(options))
    };

    assert_eq!(spawn(&mut app, "Plain"), SocketReply::Pane(1));
    assert_eq!(app.terminals[&1].profile.as_deref(), Some("Plain"));
    assert_eq!(app.terminals[&1].background, Color::WHITE);

    assert!(is_error(&spawn(&mut app, "Missing"), "no profile named \"Missing\""));
    assert_eq!(app.tabs.len(), 2);
}

#[test]
fn split_pane_splits_the_pane_it_is_given_or_the_focused_one() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab]);

    // The first tab gets a pane below its pane, by default, while the second tab stays shown.
    let options = SplitOptions { pane_id: Some(0), ..SplitOptions::default() };
    assert_eq!(split(&mut app, options), SocketReply::Pane(2));
    assert_eq!(app.active, 1);
    assert_eq!(outer_split(&app, 0), (Axis::Horizontal, 0.5));
    assert_eq!(layout(&app, 0), [0, 2]);
    let tab = &app.tabs[0];
    assert_eq!(tab.panes.get(tab.focus), Some(&2));

    // Without a pane, the focused pane is split.
    let options = SplitOptions { right: true, ..SplitOptions::default() };
    assert_eq!(split(&mut app, options), SocketReply::Pane(3));
    assert_eq!(outer_split(&app, 1).0, Axis::Vertical);
    assert_eq!(focused(&app), 3);

    let options = SplitOptions { pane_id: Some(42), ..SplitOptions::default() };
    assert!(is_error(&split(&mut app, options), "no pane with ID 42"));
    assert_eq!(app.terminals.len(), 4);
}

#[test]
fn split_pane_puts_the_new_pane_on_any_side_with_its_share() {
    let mut app = app();

    let options = SplitOptions { left: true, percent: Some(30), ..SplitOptions::default() };
    assert_eq!(split(&mut app, options), SocketReply::Pane(1));
    assert_eq!(layout(&app, 0), [1, 0]);
    let (axis, ratio) = outer_split(&app, 0);
    assert_eq!(axis, Axis::Vertical);
    assert!((ratio - 0.3).abs() < 1e-6, "{ratio}");
    assert_eq!(focused(&app), 1);

    let mut app = self::app();
    let options = SplitOptions { bottom: true, percent: Some(25), ..SplitOptions::default() };
    assert_eq!(split(&mut app, options), SocketReply::Pane(1));
    assert_eq!(layout(&app, 0), [0, 1]);
    let (axis, ratio) = outer_split(&app, 0);
    assert_eq!(axis, Axis::Horizontal);
    assert!((ratio - 0.75).abs() < 1e-6, "{ratio}");

    let options = SplitOptions { top: true, pane_id: Some(0), ..SplitOptions::default() };
    assert_eq!(split(&mut app, options), SocketReply::Pane(2));
    assert_eq!(layout(&app, 0), [2, 0, 1]);
}

#[test]
fn text_is_pasted_or_sent_as_it_is() {
    let mut app = app();
    // With bracketed paste on, `cat` shows the markers around pasted text.
    let reply = spawn(&mut app, script("printf '\\033[?2004h'; echo ready"));
    assert_eq!(reply, SocketReply::Pane(1));
    wait_for_text(&app, 1, "ready");

    let send = |app: &mut App, pane_id, text: &str, no_paste| {
        let text = Some(text.to_owned());
        answer(app, SocketMessage::SendText(SendTextOptions { pane_id, no_paste, text }))
    };

    assert_eq!(send(&mut app, None, "typed\n", true), SocketReply::Done);
    wait_for_text(&app, 1, "typed");
    assert!(!app.terminals[&1].terminal.text().concat().contains("~typed"));

    assert_eq!(send(&mut app, Some(1), "pasted", false), SocketReply::Done);
    wait_for_text(&app, 1, "^[[200~pasted^[[201~");

    // The first tab didn't get any of it.
    assert!(!app.terminals[&0].terminal.text().concat().contains("pasted"));
    assert!(is_error(&send(&mut app, Some(9), "lost", false), "no pane with ID 9"));
}

#[test]
fn text_is_read_from_the_screen_or_the_lines_asked_for() {
    let mut app = app();
    assert_eq!(spawn(&mut app, script("seq 1 5")), SocketReply::Pane(1));
    wait_for_text(&app, 1, "5");

    let text = |app: &mut App, start_line, end_line| {
        let options = GetTextOptions { pane_id: Some(1), start_line, end_line };
        match answer(app, SocketMessage::GetText(options)) {
            SocketReply::Text(lines) => lines,
            reply => panic!("not text: {reply:?}"),
        }
    };

    let screen = text(&mut app, None, None);
    assert_eq!(screen[..5], ["1", "2", "3", "4", "5"]);
    assert!(screen.len() <= app.terminals[&1].terminal.screen_lines());
    assert_eq!(text(&mut app, Some(1), Some(2)), ["2", "3"]);
    assert!(text(&mut app, Some(-5), Some(-1)).is_empty());
}

#[test]
fn activate_pane_shows_the_pane_and_its_tab() {
    let mut app = app();
    perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::ToggleZoom]);
    perform(&mut app, &[AppAction::NewTab]);

    assert_eq!(answer(&mut app, SocketMessage::ActivatePane(pane(Some(0)))), SocketReply::Done);
    assert_eq!((app.active, focused(&app)), (0, 0));
    // The zoomed pane made way for the activated one.
    assert!(app.tabs[0].panes.maximized().is_none());

    let reply = answer(&mut app, SocketMessage::ActivatePane(pane(Some(7))));
    assert!(is_error(&reply, "no pane with ID 7"));
}

#[test]
fn activate_tab_finds_tabs_by_id_position_or_distance() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab, AppAction::NewTab, AppAction::MoveTabLeft]);
    // Tab IDs stay with their tabs: the tabs are now 0, 2 and 1.
    assert_eq!(app.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>(), [0, 2, 1]);

    let mut activate =
        |options: ActivateTabOptions| match answer(&mut app, SocketMessage::ActivateTab(options)) {
            SocketReply::Done => Ok(app.active),
            reply => Err(reply),
        };
    let by_id = |id| ActivateTabOptions { tab_id: Some(id), ..Default::default() };
    let by_index = |index| ActivateTabOptions { tab_index: Some(index), ..Default::default() };
    let by_offset = |offset, no_wrap| ActivateTabOptions {
        tab_relative: Some(offset),
        no_wrap,
        ..Default::default()
    };

    assert_eq!(activate(by_id(1)), Ok(2));
    assert_eq!(activate(by_index(0)), Ok(0));
    assert_eq!(activate(by_index(-1)), Ok(2));
    assert_eq!(activate(by_offset(1, false)), Ok(0));
    assert_eq!(activate(by_offset(-1, true)), Ok(0));
    assert_eq!(activate(by_offset(-4, false)), Ok(2));

    assert!(activate(by_id(5)).is_err_and(|reply| is_error(&reply, "no tab with ID 5")));
    assert!(activate(by_index(3)).is_err_and(|reply| is_error(&reply, "no tab at position 3")));
    assert!(activate(by_index(-4)).is_err());
}

#[test]
fn kill_pane_closes_the_pane_and_its_tab_with_it() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab, AppAction::Split(Axis::Vertical)]);

    assert_eq!(answer(&mut app, SocketMessage::KillPane(pane(Some(2)))), SocketReply::Done);
    assert_eq!(layout(&app, 1), [1]);
    assert!(!app.terminals.contains_key(&2));

    assert_eq!(answer(&mut app, SocketMessage::KillPane(pane(Some(0)))), SocketReply::Done);
    assert_eq!(app.tabs.len(), 1);
    assert_eq!(layout(&app, 0), [1]);
}

#[test]
fn zoom_pane_zooms_unzooms_or_toggles() {
    let mut app = app();
    perform(&mut app, &[AppAction::Split(Axis::Vertical)]);

    let zoom = |app: &mut App, unzoom: bool, toggle: bool| {
        let options = ZoomOptions { pane_id: Some(0), zoom: false, unzoom, toggle };
        assert_eq!(answer(app, SocketMessage::ZoomPane(options)), SocketReply::Done);
        let tab = &app.tabs[0];
        tab.panes.maximized().map(|pane| *tab.panes.get(pane).unwrap())
    };

    assert_eq!(zoom(&mut app, false, false), Some(0));
    assert_eq!(focused(&app), 0);
    assert_eq!(zoom(&mut app, false, true), None);
    assert_eq!(zoom(&mut app, false, true), Some(0));
    assert_eq!(zoom(&mut app, true, false), None);
}

#[test]
fn set_tab_title_names_a_tab_until_the_title_is_empty() {
    let mut app = app();
    perform(&mut app, &[AppAction::NewTab]);

    let mut set = |pane_id, tab_id, title: &str| {
        let options = SetTabTitleOptions { pane_id, tab_id, title: title.into() };
        assert_eq!(answer(&mut app, SocketMessage::SetTabTitle(options)), SocketReply::Done);
        app.tabs.iter().map(|tab| app.tab_title(tab).to_owned()).collect::<Vec<_>>()
    };

    assert_eq!(set(None, None, "logs"), ["test", "logs"]);
    assert_eq!(set(Some(1), Some(0), "build"), ["build", "logs"]);
    assert_eq!(set(Some(1), None, ""), ["build", "test"]);
}

#[test]
fn window_messages_are_not_supported_yet() {
    let mut app = app();
    let reply = answer(&mut app, SocketMessage::GetConfig(IpcGetConfig::default()));
    assert!(is_error(&reply, "not supported by the iced front end yet"), "{reply:?}");
}

#[test]
fn messages_arrive_through_the_socket_and_are_answered() {
    use iced::futures::executor::block_on_stream;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("test.sock");
    let socket = crate::ipc::Listener::bind(Some(path.clone())).unwrap();
    let mut requests = block_on_stream(crate::ipc::accept(&socket));
    let mut app = app();

    let message = SocketMessage::SplitPane(SplitOptions { right: true, ..SplitOptions::default() });
    let client = std::thread::spawn(move || crate::ipc::request(Some(path), &message).unwrap());

    let _ = app.update(Message::Ipc(requests.next().unwrap()));
    assert_eq!(client.join().unwrap(), SocketReply::Pane(1));
    assert_eq!(focused(&app), 1);
}
