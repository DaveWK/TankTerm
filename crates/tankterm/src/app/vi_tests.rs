//! Vi mode through the real `App`: the keys that enter and leave it, move the vi cursor, select,
//! copy and search, with `cat` as the shell.

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced_tterm::{
    BackendCommand, SearchCommand, SearchDirection, SelectionType, TermMode, ViCommand, ViMotion,
};

use super::search_tests::{named, press, typed};
use super::tab_tests::{app_with, perform, wait_for_text};
use super::*;
use crate::config::UiConfig;
use crate::hotkeys::ScrollTo;

fn character(text: &str, modifiers: Modifiers) -> (Key, Modifiers) {
    (Key::Character(text.into()), modifiers)
}

/// `app` with `text` typed into its terminal: `cat` shows the line twice, and the cursor is on
/// the line after them.
fn app_showing(text: &str) -> App {
    let mut app = app_with("", Hotkeys::from_config(&UiConfig::default()));
    let write = BackendCommand::Write(format!("{text}\r").into_bytes());
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(0, write)));
    wait_for_text(&app, 0, &format!("{text}{text}"));
    app
}

fn update(app: &mut App, messages: Vec<Message>) {
    for message in messages {
        let _ = app.update(message);
    }
}

/// Keys pressed in the focused terminal of `app`, with what they lead to done.
fn press_and_update(app: &mut App, keys: &[(Key, Modifiers)]) {
    let messages = press(app, keys);
    update(app, messages);
}

fn vi_mode(app: &App) -> bool {
    app.terminals[&0].terminal.mode().contains(TermMode::VI)
}

fn vi_cursor(app: &App) -> Option<(i32, usize)> {
    let point = app.terminals[&0].terminal.vi_cursor()?;
    Some((point.line.0, point.column.0))
}

fn writes(messages: &[Message]) -> bool {
    messages.iter().any(|message| {
        matches!(
            message,
            Message::Terminal(iced_tterm::Event::BackendCall(_, BackendCommand::Write(_)))
        )
    })
}

#[test]
fn keys_select_and_copy_in_vi_mode_and_nothing_reaches_the_shell() {
    let mut app = app_showing("one needle two");

    let toggle = press(&app, &[(Key::Named(Named::Space), Modifiers::CTRL | Modifiers::SHIFT)]);
    assert!(matches!(&toggle[..], [Message::Action(AppAction::Vi(ViCommand::Toggle))]));
    update(&mut app, toggle);
    assert!(vi_mode(&app));
    assert_eq!(vi_cursor(&app), Some((2, 0)));

    // Keys without a binding do nothing.
    let messages = press(&app, &typed("xq"));
    assert!(messages.is_empty(), "unbound keys did something: {messages:?}");

    // Search backward to the second needle, and select it to the end of the word. The keys go
    // in groups, after which the binding mode changes, as the view is built again.
    press_and_update(&mut app, &[character("?", Modifiers::SHIFT)]);
    assert!(app.terminals[&0].terminal.search_active());
    let mut keys = typed("needle");
    keys.push(named(Named::Enter));
    for keys in [keys, typed("ve")] {
        let messages = press(&app, &keys);
        assert!(!writes(&messages), "keys reached the shell: {messages:?}");
        update(&mut app, messages);
    }
    assert!(!app.terminals[&0].terminal.search_active());
    assert_eq!(app.terminals[&0].terminal.selection().as_deref(), Some("needle"));
    assert_eq!(vi_cursor(&app), Some((1, 9)));

    // `n` goes on backward, to the start of this needle and then to the first one, taking the
    // selection along.
    press_and_update(&mut app, &typed("n"));
    assert_eq!(vi_cursor(&app), Some((1, 4)));
    press_and_update(&mut app, &typed("n"));
    assert_eq!(vi_cursor(&app), Some((0, 4)));
    assert_eq!(app.terminals[&0].terminal.selection().as_deref(), Some("needle two\none n"));

    // `y` copies and drops the selection.
    let messages = press(&app, &typed("y"));
    assert!(matches!(&messages[..], [
        Message::Action(AppAction::Copy),
        Message::Action(AppAction::ClearSelection)
    ]));
    update(&mut app, messages);
    assert_eq!(app.terminals[&0].terminal.selection(), None);

    // `i` leaves vi mode, and keys reach the shell again.
    let messages = press(&app, &typed("i"));
    assert!(matches!(&messages[..], [
        Message::Action(AppAction::Vi(ViCommand::Toggle)),
        Message::Action(AppAction::Scroll(ScrollTo::Bottom))
    ]));
    update(&mut app, messages);
    assert!(!vi_mode(&app));
    assert!(writes(&press(&app, &typed("x"))));
}

#[test]
fn inline_search_takes_the_next_key_whatever_it_is_bound_to() {
    let mut app = app_showing("kayak");
    perform(&mut app, &[AppAction::Vi(ViCommand::Toggle)]);

    // Up to the line `cat` printed.
    press_and_update(&mut app, &typed("k"));
    assert_eq!(vi_cursor(&app), Some((1, 0)));

    // After `f`, `k` is the character to go to, not a motion.
    press_and_update(&mut app, &typed("f"));
    let messages = press(&app, &typed("k"));
    assert!(matches!(
        &messages[..],
        [Message::Terminal(iced_tterm::Event::BackendCall(
            _,
            BackendCommand::Vi(ViCommand::InlineSearchInput(text))
        ))] if text == "k"
    ));
    update(&mut app, messages);
    assert_eq!(vi_cursor(&app), Some((1, 4)));

    // `,` goes back to the one before, and `k` is a motion again.
    press_and_update(&mut app, &typed(",k"));
    assert_eq!(vi_cursor(&app), Some((0, 0)));
}

#[test]
fn selecting_with_the_vi_cursor_fills_the_selection_clipboard() {
    let mut app = app_showing("abc");
    perform(&mut app, &[
        AppAction::Vi(ViCommand::Toggle),
        AppAction::Vi(ViCommand::Motion(ViMotion::Up)),
    ]);
    assert_eq!(vi_cursor(&app), Some((1, 0)));

    // A command that changes the selection copies it; one that leaves it as it was doesn't, and
    // its task is only the one that focuses the terminal.
    let select = Message::Action(AppAction::Vi(ViCommand::ToggleSelection(SelectionType::Lines)));
    let copied = app.update(select);
    assert_eq!(app.terminals[&0].terminal.selection().as_deref(), Some("abc\n"));
    let unchanged = app.update(Message::Action(AppAction::Vi(ViCommand::SearchNext)));
    assert!(copied.units() > unchanged.units());
}

#[test]
fn paging_takes_the_vi_cursor_along_and_the_palette_toggles_vi_mode() {
    let mut app = app_showing("abc");
    perform(&mut app, &[AppAction::ShowCommands]);
    let _ = app.update(Message::PaletteQuery("vi mode".into()));
    let _ = app.update(Message::PaletteSubmit);
    assert!(app.palette.is_none());
    assert!(vi_mode(&app));

    perform(&mut app, &[AppAction::Scroll(ScrollTo::Top)]);
    assert_eq!(vi_cursor(&app), Some((0, 0)));
    perform(&mut app, &[AppAction::Scroll(ScrollTo::Bottom)]);
    let bottom = app.terminals[&0].terminal.screen_lines() as i32 - 1;
    assert_eq!(app.terminals[&0].terminal.vi_cursor().map(|point| point.line.0), Some(bottom));

    // Searching from vi mode starts at the vi cursor, not at the top of the view.
    perform(&mut app, &[
        AppAction::Scroll(ScrollTo::Top),
        AppAction::Vi(ViCommand::Motion(ViMotion::Down)),
        AppAction::Search(SearchCommand::Start(SearchDirection::Forward)),
        AppAction::Search(SearchCommand::Input("abc".into())),
    ]);
    assert_eq!(vi_cursor(&app), Some((1, 0)));
}
