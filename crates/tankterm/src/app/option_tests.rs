//! Configuration options through the real `App`, with `cat` or `sh` as the shell.

use iced::futures::StreamExt;
use iced::futures::executor::block_on;
use iced::keyboard::key::Named;
use iced::keyboard::{Key, Modifiers};
use iced::mouse::{self, ScrollDelta};
use iced::{Event, Size, Task, Theme, window};
use iced_core::clipboard::Kind;
use iced_test::simulator::Simulator;
use iced_tterm::settings::ScrollSettings;
use iced_tterm::{BackendCommand, SelectionType, TermMode};
use iced_winit::runtime;

use super::tab_tests::{app_with, key, perform, type_keys, wait_for_text, wait_until, wakeup};
use super::*;
use crate::config::UiConfig;
use crate::hotkeys::ScrollTo;

/// The lines the terminal under the pointer is asked to scroll by for one step of the wheel.
fn wheel_step(app: &App, y: f32) -> Vec<i32> {
    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    ui.point_at((400., 200.));
    let delta = ScrollDelta::Lines { x: 0., y };
    ui.simulate([Event::Mouse(mouse::Event::WheelScrolled { delta })]);

    let scrolls = ui.into_messages().filter_map(|message| match message {
        Message::Terminal(iced_tterm::Event::BackendCall(_, BackendCommand::Scroll(lines))) => {
            Some(lines)
        },
        _ => None,
    });
    scrolls.collect()
}

#[test]
fn wheel_scrolls_by_the_scrolling_multiplier() {
    let app = app_with("", Hotkeys::default());
    assert_eq!(wheel_step(&app, 1.), [3]);
    assert_eq!(wheel_step(&app, -1.), [-3]);

    let app = app_with("[scrolling]\nmultiplier = 5\n", Hotkeys::default());
    assert_eq!(wheel_step(&app, 1.), [5]);

    let app = app_with("[scrolling]\nmultiplier = 0\n", Hotkeys::default());
    assert!(wheel_step(&app, 1.).is_empty());
}

/// An application whose shell prints the numbers 1 to 200, once it has printed them all.
fn app_counting(config: &str) -> App {
    let shell = "[terminal]\nshell = { program = \"sh\", args = [\"-c\", \"seq 1 200; printf \
                 done; exec cat\"] }\n";
    let app = app_with(&format!("{config}{shell}"), Hotkeys::default());
    wait_for_text(&app, 0, "done");

    app
}

/// The oldest of the numbers 1 to 200 the first terminal still has.
fn oldest_line(app: &App) -> u32 {
    let text = app.terminals[&0].terminal.text();
    text[0].parse().unwrap_or_else(|_| panic!("unexpected first line in {text:?}"))
}

#[test]
fn scrollback_keeps_scrolling_history_lines() {
    assert_eq!(oldest_line(&app_counting("")), 1);

    // Without history only the screen is left, and that many lines less with history.
    let screen = oldest_line(&app_counting("[scrolling]\nhistory = 0\n"));
    assert!(screen > 100, "the screen starts at line {screen}");
    assert_eq!(oldest_line(&app_counting("[scrolling]\nhistory = 10\n")), screen - 10);
    assert_eq!(oldest_line(&app_counting("[scrolling]\nhistory = 75\n")), screen - 75);
}

#[test]
fn scroll_settings_change_for_a_running_terminal() {
    let mut app = app_counting("[scrolling]\nhistory = 75\n");
    let before = oldest_line(&app);

    let scroll = ScrollSettings { history: 20, multiplier: 2. };
    let terminal = &mut app.terminals.get_mut(&0).unwrap().terminal;
    terminal.handle(iced_tterm::Command::ChangeScroll(scroll));

    assert_eq!(oldest_line(&app), before + 55);
    assert_eq!(wheel_step(&app, 1.), [2]);
}

/// An application with the key bindings of its configuration, the default ones included.
fn app_configured(config: &str) -> App {
    let hotkeys = Hotkeys::from_config(&toml::from_str::<UiConfig>(config).unwrap());
    app_with(config, hotkeys)
}

const CTRL_SHIFT: Modifiers = Modifiers::CTRL.union(Modifiers::SHIFT);

fn with(character: &str, modifiers: Modifiers) -> Option<(Key, Modifiers)> {
    Some((Key::Character(character.into()), modifiers))
}

fn named(key: Named, modifiers: Modifiers) -> Option<(Key, Modifiers)> {
    Some((Key::Named(key), modifiers))
}

#[cfg(not(target_os = "macos"))]
#[test]
fn bound_keys_run_their_action_once_and_stay_from_the_shell() {
    let app = app_configured("");

    let keys = [
        with("c", CTRL_SHIFT),
        with("v", CTRL_SHIFT),
        named(Named::Insert, Modifiers::SHIFT),
        with("=", Modifiers::CTRL),
        named(Named::PageUp, Modifiers::SHIFT),
        key("x"),
    ];
    let (actions, written) = type_keys(&app, &keys);

    assert_eq!(actions, [
        AppAction::Copy,
        AppAction::Paste,
        AppAction::PasteSelection,
        AppAction::IncreaseFontSize,
        AppAction::Scroll(ScrollTo::PageUp),
    ]);
    // Only the key without a binding is typed into the shell.
    assert_eq!(written, b"x");
}

#[cfg(not(target_os = "macos"))]
#[test]
fn unbound_keys_reach_the_shell_and_swallowed_keys_do_not() {
    let app = app_configured(
        r#"
        [keyboard]
        bindings = [
            { key = "V", mods = "Control|Shift", action = "ReceiveChar" },
            { key = "C", mods = "Control|Shift", action = "None" },
            { key = "B", action = "None" },
            { key = "Y", mods = "Alt", action = "Copy" },
        ]
        "#,
    );

    let keys = [
        with("v", CTRL_SHIFT),
        with("c", CTRL_SHIFT),
        key("b"),
        key("x"),
        with("y", Modifiers::ALT),
    ];
    let (actions, written) = type_keys(&app, &keys);

    assert_eq!(actions, [AppAction::Copy]);
    // `Control+Shift+V` is typed like `Control+V`; the terminal doesn't paste by itself.
    assert_eq!(written, b"\x16x");
}

#[test]
fn scrolling_keys_are_the_applications_on_the_alternate_screen() {
    let config = "[terminal]\nshell = { program = \"sh\", args = [\"-c\", \"printf \
                  '\\\\033[?1049h'; exec cat\"] }\n";
    let mut app = app_configured(config);
    wait_until("the alternate screen was never entered", || {
        app.terminals[&0].terminal.mode().contains(TermMode::ALT_SCREEN)
    });
    let _ = app.update(wakeup(0));

    let page_up = [named(Named::PageUp, Modifiers::SHIFT)];
    let (actions, written) = type_keys(&app, &page_up);
    assert_eq!(actions, []);
    assert_eq!(written, b"\x1b[5;2~");

    // The same key scrolls for a shell on the regular screen.
    let (actions, written) = type_keys(&app_configured(""), &page_up);
    assert_eq!(actions, [AppAction::Scroll(ScrollTo::PageUp)]);
    assert_eq!(written, b"");
}

/// How many lines the first terminal is scrolled back.
fn offset(app: &App) -> usize {
    app.terminals[&0].terminal.display_offset()
}

#[test]
fn scroll_actions_move_through_the_scrollback() {
    let mut app = app_counting("");
    // Every number is on a line of its own, and the cursor is on the line after them.
    let page = app.terminals[&0].terminal.screen_lines();
    let top = app.terminals[&0].terminal.text().len() - page;
    assert!(page >= 2 && top > 2 * page, "page of {page} lines, {top} lines of history");

    let mut scroll = |to| {
        perform(&mut app, &[AppAction::Scroll(to)]);
        offset(&app)
    };
    assert_eq!(scroll(ScrollTo::PageUp), page);
    assert_eq!(scroll(ScrollTo::LineUp), page + 1);
    assert_eq!(scroll(ScrollTo::HalfPageUp), page + 1 + page / 2);
    assert_eq!(scroll(ScrollTo::LineDown), page + page / 2);
    assert_eq!(scroll(ScrollTo::HalfPageDown), page);
    assert_eq!(scroll(ScrollTo::PageDown), 0);
    assert_eq!(scroll(ScrollTo::PageDown), 0);
    assert_eq!(scroll(ScrollTo::Top), top);
    assert_eq!(scroll(ScrollTo::PageUp), top);
    assert_eq!(scroll(ScrollTo::Bottom), 0);
}

#[test]
fn clear_history_drops_the_scrollback_of_the_focused_terminal() {
    let mut app = app_counting("");
    perform(&mut app, &[AppAction::Split(iced::widget::pane_grid::Axis::Vertical)]);
    perform(&mut app, &[AppAction::PreviousPane, AppAction::Scroll(ScrollTo::PageUp)]);
    assert_eq!(oldest_line(&app), 1);
    assert!(offset(&app) > 0);

    perform(&mut app, &[AppAction::ClearHistory]);
    assert!(oldest_line(&app) > 100);
    assert_eq!(offset(&app), 0);

    perform(&mut app, &[AppAction::Scroll(ScrollTo::Top)]);
    assert_eq!(offset(&app), 0);
}

/// The cell size of every terminal that is shown, by terminal ID, as laid out in a window.
fn cells(app: &App) -> HashMap<u64, Size> {
    let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
    ui.snapshot(&Theme::Dark).unwrap();

    let cells = ui.into_messages().filter_map(|message| match message {
        Message::Terminal(iced_tterm::Event::BackendCall(
            id,
            BackendCommand::Resize(_, Some(cell)),
        )) => Some((id, cell)),
        _ => None,
    });
    cells.collect()
}

#[test]
fn font_size_actions_resize_every_terminal() {
    let mut app = app_with("", Hotkeys::default());
    perform(&mut app, &[AppAction::Split(iced::widget::pane_grid::Axis::Vertical)]);
    let configured = cells(&app)[&0];

    perform(&mut app, &[AppAction::IncreaseFontSize, AppAction::IncreaseFontSize]);
    let larger = cells(&app);
    assert!(larger[&0].height > configured.height && larger[&0].width > configured.width);
    assert_eq!(larger[&0], larger[&1]);

    // New terminals get the size the others have.
    perform(&mut app, &[AppAction::Split(iced::widget::pane_grid::Axis::Horizontal)]);
    assert_eq!(cells(&app)[&2], larger[&0]);

    perform(&mut app, &[AppAction::DecreaseFontSize; 3]);
    let smaller = cells(&app);
    assert!(smaller[&0].height < configured.height);
    assert_eq!(smaller[&0], smaller[&2]);

    perform(&mut app, &[AppAction::ResetFontSize]);
    assert_eq!(cells(&app), HashMap::from([(0, configured), (1, configured), (2, configured)]));

    // The size stops shrinking at a few pixels and comes back from there a step at a time.
    perform(&mut app, &[AppAction::DecreaseFontSize; 100]);
    let smallest = cells(&app)[&0];
    assert!(smallest.height > 0. && smallest.height < smaller[&0].height);
    perform(&mut app, &[AppAction::IncreaseFontSize]);
    assert!(cells(&app)[&0].height > smallest.height);
    perform(&mut app, &[AppAction::ResetFontSize]);
    assert_eq!(cells(&app)[&0], configured);
}

/// What a task asked iced's runtime to do.
#[derive(Debug, PartialEq)]
enum Asked {
    Read(Kind),
    Write(Kind, String),
    SetMode(window::Mode),
    ToggleMaximize,
}

/// Run a task the way iced's runtime would, with `clipboard` as the text of the regular and of
/// the selection clipboard and a window in `mode`.
///
/// Returns what the task asked the runtime to do and the messages it produced.
fn run_task(
    task: Task<Message>,
    clipboard: [Option<&str>; 2],
    mode: window::Mode,
) -> (Vec<Asked>, Vec<Message>) {
    let (mut asked, mut messages) = (Vec::new(), Vec::new());
    let Some(mut actions) = runtime::task::into_stream(task) else { return (asked, messages) };
    let window = window::Id::unique();

    while let Some(action) = block_on(actions.next()) {
        match action {
            runtime::Action::Output(message) => messages.push(message),
            runtime::Action::Clipboard(runtime::clipboard::Action::Read { target, channel }) => {
                let text = match target {
                    Kind::Standard => clipboard[0],
                    Kind::Primary => clipboard[1],
                };
                let _ = channel.send(text.map(str::to_owned));
                asked.push(Asked::Read(target));
            },
            runtime::Action::Clipboard(runtime::clipboard::Action::Write { target, contents }) => {
                asked.push(Asked::Write(target, contents));
            },
            runtime::Action::Window(runtime::window::Action::GetLatest(channel)) => {
                let _ = channel.send(Some(window));
            },
            runtime::Action::Window(runtime::window::Action::GetMode(id, channel)) => {
                assert_eq!(id, window);
                let _ = channel.send(mode);
            },
            runtime::Action::Window(runtime::window::Action::SetMode(id, mode)) => {
                assert_eq!(id, window);
                asked.push(Asked::SetMode(mode));
            },
            runtime::Action::Window(runtime::window::Action::ToggleMaximize(id)) => {
                assert_eq!(id, window);
                asked.push(Asked::ToggleMaximize);
            },
            // Giving a widget keyboard focus needs the widgets.
            runtime::Action::Widget(_) => (),
            action => panic!("unexpected {action:?}"),
        }
    }

    (asked, messages)
}

/// Run an action and then the task it returns, with a window that isn't fullscreen.
fn run_action(app: &mut App, action: AppAction, clipboard: [Option<&str>; 2]) -> Vec<Asked> {
    let task = app.update(Message::Action(action));
    let (asked, messages) = run_task(task, clipboard, window::Mode::Windowed);
    for message in messages {
        let _ = app.update(message);
    }

    asked
}

#[test]
fn copy_puts_the_selection_of_the_focused_terminal_on_the_clipboard() {
    let config =
        "[terminal]\nshell = { program = \"sh\", args = [\"-c\", \"printf hello; exec cat\"] }\n";
    let mut app = app_with(config, Hotkeys::default());
    wait_for_text(&app, 0, "hello");

    // Without a selection the clipboard is left alone.
    assert_eq!(run_action(&mut app, AppAction::Copy, [None, None]), []);

    // Select the first line, from its first cell to wherever it ends.
    let terminal = &mut app.terminals.get_mut(&0).unwrap().terminal;
    for select in [
        BackendCommand::SelectStart(SelectionType::Simple, (0., 0.)),
        BackendCommand::SelectUpdate((10_000., 0.)),
    ] {
        terminal.handle(iced_tterm::Command::ProxyToBackend(select));
    }

    let asked = run_action(&mut app, AppAction::Copy, [None, None]);
    assert_eq!(asked, [Asked::Write(Kind::Standard, "hello".into())]);

    // The selection of a terminal that doesn't have focus isn't copied.
    perform(&mut app, &[AppAction::NewTab]);
    assert_eq!(run_action(&mut app, AppAction::Copy, [None, None]), []);
}

#[test]
fn paste_actions_paste_a_clipboard_into_the_focused_terminal() {
    let mut app = app_with("", Hotkeys::default());
    perform(&mut app, &[AppAction::Split(iced::widget::pane_grid::Axis::Vertical)]);

    let asked = run_action(&mut app, AppAction::Paste, [Some("copied"), Some("selected")]);
    assert_eq!(asked, [Asked::Read(Kind::Standard)]);
    wait_for_text(&app, 1, "copied");

    let asked = run_action(&mut app, AppAction::PasteSelection, [Some("copied"), Some("selected")]);
    assert_eq!(asked, [Asked::Read(Kind::Primary)]);
    wait_for_text(&app, 1, "selected");

    // The regular clipboard stands in for a selection clipboard the platform doesn't have.
    let asked = run_action(&mut app, AppAction::PasteSelection, [Some("regular"), None]);
    assert_eq!(asked, [Asked::Read(Kind::Primary), Asked::Read(Kind::Standard)]);
    wait_for_text(&app, 1, "regular");

    // Nothing was pasted into the other pane, and empty clipboards paste nothing.
    assert_eq!(app.terminals[&0].terminal.text().concat(), "");
    assert_eq!(run_action(&mut app, AppAction::Paste, [None, None]), [Asked::Read(Kind::Standard)]);
    assert_eq!(run_action(&mut app, AppAction::Paste, [Some(""), None]), [Asked::Read(
        Kind::Standard
    )]);
    assert!(app.pending_paste.is_none());

    // Pasted line breaks are asked about, like those pasted with the mouse.
    run_action(&mut app, AppAction::Paste, [Some("rm -r\n"), None]);
    assert!(app.pending_paste.is_some());
    perform(&mut app, &[AppAction::PasteCancel]);
    assert!(!app.terminals[&1].terminal.text().concat().contains("rm"));
}

#[test]
fn window_actions_toggle_fullscreen_and_maximized() {
    let mut app = app_with("", Hotkeys::default());
    let mut toggle = |action, mode| {
        let task = app.update(Message::Action(action));
        run_task(task, [None, None], mode).0
    };

    assert_eq!(toggle(AppAction::ToggleFullscreen, window::Mode::Windowed), [Asked::SetMode(
        window::Mode::Fullscreen
    )]);
    assert_eq!(toggle(AppAction::ToggleFullscreen, window::Mode::Fullscreen), [Asked::SetMode(
        window::Mode::Windowed
    )]);
    assert_eq!(toggle(AppAction::ToggleMaximized, window::Mode::Windowed), [Asked::ToggleMaximize]);
}
