//! Searching a terminal's scrollback through the real `App`: the keys that open the search, type
//! into its bar and close it.

use iced::Size;
use iced::keyboard::key::{Named, NativeCode, Physical};
use iced::keyboard::{Event, Key, Location, Modifiers};
use iced_test::simulator::{self, Simulator};
use iced_tterm::{BackendCommand, SearchCommand, SearchDirection};

use super::tab_tests::{app_with, perform, wait_for_text};
use super::*;
use crate::config::UiConfig;

/// Keys pressed in the focused terminal of `app`, as the messages they lead to.
pub(super) fn press(app: &App, keys: &[(Key, Modifiers)]) -> Vec<Message> {
    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    // Click the terminal to give it keyboard focus.
    ui.point_at((400., 200.));
    ui.simulate(simulator::click());

    for (key, modifiers) in keys {
        let physical_key = Physical::Unidentified(NativeCode::Unidentified);
        let text = match key {
            Key::Character(text) if !modifiers.control() => Some(text.clone()),
            _ => None,
        };
        ui.simulate([
            iced::Event::Keyboard(Event::ModifiersChanged(*modifiers)),
            iced::Event::Keyboard(Event::KeyPressed {
                key: key.clone(),
                modified_key: key.clone(),
                physical_key,
                location: Location::Standard,
                modifiers: *modifiers,
                text,
                repeat: false,
            }),
            iced::Event::Keyboard(Event::KeyReleased {
                key: key.clone(),
                modified_key: key.clone(),
                physical_key,
                location: Location::Standard,
                modifiers: *modifiers,
            }),
            iced::Event::Keyboard(Event::ModifiersChanged(Modifiers::empty())),
        ]);
    }

    // The click's own messages aren't the keys'.
    ui.into_messages()
        .filter(|message| {
            matches!(
                message,
                Message::Action(_)
                    | Message::Terminal(iced_tterm::Event::BackendCall(
                        _,
                        BackendCommand::Write(_)
                            | BackendCommand::Search(_)
                            | BackendCommand::Vi(_)
                    ))
            )
        })
        .collect()
}

pub(super) fn typed(text: &str) -> Vec<(Key, Modifiers)> {
    text.chars().map(|c| (Key::Character(c.to_string().into()), Modifiers::empty())).collect()
}

pub(super) fn named(key: Named) -> (Key, Modifiers) {
    (Key::Named(key), Modifiers::empty())
}

fn search_active(app: &App) -> bool {
    app.terminals[&0].terminal.search_active()
}

#[test]
fn keys_open_the_search_type_into_it_and_close_it() {
    let mut app = app_with("", Hotkeys::from_config(&UiConfig::default()));
    let write = BackendCommand::Write(b"one needle\rtwo\r".to_vec());
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(0, write)));
    wait_for_text(&app, 0, "two");

    // The configured key opens the search.
    let open = press(&app, &[(Key::Character("f".into()), Modifiers::CTRL | Modifiers::SHIFT)]);
    let start = SearchCommand::Start(SearchDirection::Forward);
    assert!(
        matches!(&open[..], [Message::Action(AppAction::Search(command))] if *command == start)
    );
    for message in open {
        let _ = app.update(message);
    }
    assert!(search_active(&app));

    // Typed keys go into the bar, and nothing reaches the shell; Escape closes the bar.
    let mut keys = typed("needle");
    keys.push(named(Named::Escape));
    let messages = press(&app, &keys);
    assert!(
        !messages.iter().any(|message| matches!(
            message,
            Message::Terminal(iced_tterm::Event::BackendCall(_, BackendCommand::Write(_)))
        )),
        "keys reached the shell: {messages:?}"
    );
    assert!(matches!(
        messages.last(),
        Some(Message::Action(AppAction::Search(SearchCommand::Cancel)))
    ));
    for message in messages {
        let _ = app.update(message);
    }

    // The match is left selected, and keys reach the shell again.
    assert!(!search_active(&app));
    assert_eq!(app.terminals[&0].terminal.selection().as_deref(), Some("needle"));
    let messages = press(&app, &typed("x"));
    assert!(matches!(
        &messages[..],
        [Message::Terminal(iced_tterm::Event::BackendCall(_, BackendCommand::Write(bytes)))]
            if bytes == b"x"
    ));
}

#[test]
fn backspace_deletes_in_the_bar_and_unbound_keys_do_nothing() {
    let mut app = app_with("", Hotkeys::from_config(&UiConfig::default()));
    perform(&mut app, &[AppAction::Search(SearchCommand::Start(SearchDirection::Backward))]);

    let messages = press(&app, &[
        named(Named::Backspace),
        named(Named::ArrowLeft),
        named(Named::Tab),
        named(Named::F5),
    ]);
    let commands: Vec<_> = messages
        .iter()
        .map(|message| match message {
            Message::Terminal(iced_tterm::Event::BackendCall(_, command)) => format!("{command:?}"),
            Message::Action(action) => format!("{action:?}"),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(commands, [format!(
        "{:?}",
        BackendCommand::Search(SearchCommand::Input("\x08".into()))
    )]);
}

#[test]
fn pastes_go_into_the_bar_without_their_line_breaks() {
    let mut app = app_with("", Hotkeys::from_config(&UiConfig::default()));
    let paste = || {
        let paste = BackendCommand::Paste("one\ntwo".into());
        Message::Terminal(iced_tterm::Event::BackendCall(0, paste))
    };

    // Lines pasted into the shell are asked about.
    let _ = app.update(paste());
    assert!(app.pending_paste.is_some());
    perform(&mut app, &[AppAction::PasteCancel]);

    let write = BackendCommand::Write(b"onetwo\r".to_vec());
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(0, write)));
    wait_for_text(&app, 0, "onetwo");

    // Lines pasted into the bar aren't, and they search as one.
    perform(&mut app, &[AppAction::Search(SearchCommand::Start(SearchDirection::Backward))]);
    let _ = app.update(paste());
    assert!(app.pending_paste.is_none());
    perform(&mut app, &[AppAction::Search(SearchCommand::Cancel)]);
    assert_eq!(app.terminals[&0].terminal.selection().as_deref(), Some("onetwo"));
}

#[test]
fn the_command_palette_offers_the_search() {
    let mut app = app_with("", Hotkeys::from_config(&UiConfig::default()));
    perform(&mut app, &[AppAction::ShowCommands]);
    let _ = app.update(Message::PaletteQuery("search back".into()));
    let _ = app.update(Message::PaletteSubmit);

    assert!(app.palette.is_none());
    assert!(search_active(&app));
}
