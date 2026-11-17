//! Hints through the real `App`: the keys that label them and choose one, clicks, and the vi
//! cursor, with `cat` as the shell.

use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::{Event, mouse};
use iced_tterm::{BackendCommand, HintCommand, TermMode, ViCommand};

use super::option_tests::{click, ui_events};
use super::search_tests::{named, press, typed};
use super::tab_tests::{app_with, perform, wait_for_text};
use super::*;
use crate::config::UiConfig;

const URL: &str = "https://one.example";

/// Two hints for URLs: Control+Shift+U selects one, the mouse too, and Control+Shift+Y pastes
/// one.
const HINTS: &str = r#"
[hints]
alphabet = "jf"

[[hints.enabled]]
regex = "https://[a-z.]+"
action = "Select"
binding = { key = "U", mods = "Control|Shift" }
mouse = { enabled = true }

[[hints.enabled]]
regex = "https://[a-z.]+"
action = "Paste"
binding = { key = "Y", mods = "Control|Shift" }
"#;

/// An application with `config`, whose terminal shows the URL twice: `cat` echoes the line typed
/// into it. The cursor is on the line after them.
fn app_showing_url(config: &str) -> App {
    let hotkeys = Hotkeys::from_config(&toml::from_str::<UiConfig>(config).unwrap());
    let mut app = app_with(config, hotkeys);
    let write = BackendCommand::Write(format!("{URL}\r").into_bytes());
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(0, write)));
    wait_for_text(&app, 0, &format!("{URL}{URL}"));
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

fn hint_active(app: &App) -> bool {
    app.terminals[&0].terminal.hint_active()
}

fn selection(app: &App) -> Option<String> {
    app.terminals[&0].terminal.selection()
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
fn keys_label_the_hints_and_typing_a_label_chooses_one() {
    let mut app = app_showing_url(HINTS);
    let control_shift = Modifiers::CTRL | Modifiers::SHIFT;

    let start = press(&app, &[(Key::Character("u".into()), control_shift)]);
    assert!(matches!(&start[..], [Message::Action(AppAction::Hint(0))]));
    update(&mut app, start);
    assert!(hint_active(&app));

    // The keys type into the labels, whatever they are bound to, and none reach the shell. The
    // first URL's label is "fj", the second's "j".
    let messages = press(&app, &[(Key::Character("u".into()), control_shift)]);
    assert!(messages.is_empty(), "the binding ran: {messages:?}");
    let messages = press(&app, &typed("xf"));
    assert!(!writes(&messages));
    assert!(matches!(
        &messages[..],
        [
            Message::Terminal(iced_tterm::Event::BackendCall(
                0,
                BackendCommand::Hint(HintCommand::Input(x))
            )),
            Message::Terminal(iced_tterm::Event::BackendCall(
                0,
                BackendCommand::Hint(HintCommand::Input(f))
            )),
        ] if x == "x" && f == "f"
    ));
    update(&mut app, messages);
    assert!(hint_active(&app));
    assert_eq!(selection(&app), None);

    press_and_update(&mut app, &typed("j"));
    assert!(!hint_active(&app));
    assert_eq!(selection(&app).as_deref(), Some(URL));
    let first_line = app.terminals[&0].terminal.lines(0, 0);
    assert_eq!(first_line, [URL]);

    // Keys reach the shell again.
    assert!(writes(&press(&app, &typed("x"))));
}

#[test]
fn escape_takes_the_labels_away_and_pasting_hints_type_their_text() {
    let mut app = app_showing_url(HINTS);
    let control_shift = Modifiers::CTRL | Modifiers::SHIFT;

    perform(&mut app, &[AppAction::Hint(1)]);
    assert!(hint_active(&app));
    press_and_update(&mut app, &[named(Named::Escape)]);
    assert!(!hint_active(&app));

    press_and_update(&mut app, &[(Key::Character("y".into()), control_shift)]);
    assert!(hint_active(&app));
    press_and_update(&mut app, &typed("j"));
    assert!(!hint_active(&app));
    // `cat` gets the URL; the terminal shows it as it is typed.
    wait_for_text(&app, 0, &URL.repeat(3));
}

#[test]
fn clicking_a_hint_chooses_it_when_its_modifiers_are_held() {
    let shell =
        "[terminal]\nshell = { program = \"sh\", args = [\"-c\", \"printf 'https://one.example'; \
         exec cat\"] }\n";
    let clicked = |mods: &str| {
        let config = format!("{shell}{}", HINTS.replace("{ enabled = true }", mods));
        let mut app = app_with(&config, Hotkeys::default());
        wait_for_text(&app, 0, URL);

        let (_, messages) = ui_events(&app, &[((1., 1.), click(mouse::Button::Left))]);
        let opened = messages.iter().any(|message| {
            matches!(
                message,
                Message::Terminal(iced_tterm::Event::BackendCall(
                    0,
                    BackendCommand::Hint(HintCommand::OpenHovered)
                ))
            )
        });
        assert!(opened, "the click chose no hint: {messages:?}");
        update(&mut app, messages);
        app
    };

    // Without modifiers to hold, a click selects the URL. Without a selection, the pointer
    // shows that a click would.
    let mut app = clicked("{ enabled = true }");
    assert_eq!(selection(&app).as_deref(), Some(URL));
    perform(&mut app, &[AppAction::ClearSelection]);
    let moved = Event::Mouse(mouse::Event::CursorMoved { position: (2., 1.).into() });
    let (interactions, _) = ui_events(&app, &[((2., 1.), vec![moved])]);
    assert_eq!(interactions.last(), Some(&mouse::Interaction::Pointer));

    // With Control to hold, a plain click doesn't.
    let app = clicked("{ enabled = true, mods = \"Control\" }");
    assert_eq!(selection(&app), None);
}

#[test]
fn enter_chooses_the_hint_under_the_vi_cursor() {
    let mut app = app_showing_url(HINTS);
    perform(&mut app, &[AppAction::Vi(ViCommand::Toggle)]);
    assert!(app.terminals[&0].terminal.mode().contains(TermMode::VI));

    // Off the URLs, nothing happens; up on the second, the first hint for the mouse selects it.
    press_and_update(&mut app, &[named(Named::Enter)]);
    assert_eq!(selection(&app), None);
    press_and_update(&mut app, &typed("k"));
    let open = press(&app, &[named(Named::Enter)]);
    assert!(matches!(&open[..], [Message::Action(AppAction::Vi(ViCommand::Open))]));
    update(&mut app, open);
    assert_eq!(selection(&app).as_deref(), Some(URL));
}
