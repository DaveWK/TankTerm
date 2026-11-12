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
use iced_tterm::{BackendCommand, CursorShape, SelectionType, TermMode};
use iced_winit::runtime;

use super::reload_tests::same_picture;
use super::tab_tests::{app_with, key, perform, type_keys, wait_for_text, wait_until, wakeup};
use super::*;
use crate::config::UiConfig;
use crate::hotkeys::ScrollTo;

/// The lines the terminal under the pointer is asked to scroll by for one step of the wheel.
pub(super) fn wheel_step(app: &App, y: f32) -> Vec<i32> {
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
pub(super) fn oldest_line(app: &App) -> u32 {
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
pub(super) fn cells(app: &App) -> HashMap<u64, Size> {
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

    perform(&mut app, &vec![AppAction::DecreaseFontSize; 3]);
    let smaller = cells(&app);
    assert!(smaller[&0].height < configured.height);
    assert_eq!(smaller[&0], smaller[&2]);

    perform(&mut app, &[AppAction::ResetFontSize]);
    assert_eq!(cells(&app), HashMap::from([(0, configured), (1, configured), (2, configured)]));

    // The size stops shrinking at a few pixels and comes back from there a step at a time.
    perform(&mut app, &vec![AppAction::DecreaseFontSize; 100]);
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
    Minimize,
    Close,
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
            runtime::Action::Window(runtime::window::Action::Minimize(id, true)) => {
                assert_eq!(id, window);
                asked.push(Asked::Minimize);
            },
            runtime::Action::Window(runtime::window::Action::Close(id)) => {
                assert_eq!(id, window);
                asked.push(Asked::Close);
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

#[test]
fn window_actions_minimize_hide_and_quit() {
    let mut app = app_with("", Hotkeys::default());
    perform(&mut app, &[AppAction::NewTab]);

    assert_eq!(run_action(&mut app, AppAction::Minimize, [None, None]), [Asked::Minimize]);
    assert_eq!(run_action(&mut app, AppAction::Hide, [None, None]), [Asked::SetMode(
        window::Mode::Hidden
    )]);
    // Quitting closes the window with every tab in it, which ends TankTerm.
    assert_eq!(run_action(&mut app, AppAction::Quit, [None, None]), [Asked::Close]);
}

#[test]
fn chars_are_typed_into_the_focused_terminal() {
    let mut app = app_with("", Hotkeys::default());
    perform(&mut app, &[AppAction::Split(iced::widget::pane_grid::Axis::Vertical)]);

    perform(&mut app, &[AppAction::Chars("typed".into())]);
    wait_for_text(&app, 1, "typed");
    assert_eq!(app.terminals[&0].terminal.text().concat(), "");
}

#[test]
fn selection_actions_copy_and_clear_the_selection_of_the_focused_terminal() {
    let config =
        "[terminal]\nshell = { program = \"sh\", args = [\"-c\", \"printf hello; exec cat\"] }\n";
    let mut app = app_with(config, Hotkeys::default());
    wait_for_text(&app, 0, "hello");
    assert_eq!(run_action(&mut app, AppAction::CopySelection, [None, None]), []);

    let terminal = &mut app.terminals.get_mut(&0).unwrap().terminal;
    for select in [
        BackendCommand::SelectStart(SelectionType::Simple, (0., 0.)),
        BackendCommand::SelectUpdate((10_000., 0.)),
    ] {
        terminal.handle(iced_tterm::Command::ProxyToBackend(select));
    }

    // CopySelection goes to the selection clipboard, Copy to the regular one.
    let asked = run_action(&mut app, AppAction::CopySelection, [None, None]);
    assert_eq!(asked, [Asked::Write(Kind::Primary, "hello".into())]);

    perform(&mut app, &[AppAction::ClearSelection]);
    assert_eq!(app.terminals[&0].terminal.selection(), None);
    assert_eq!(run_action(&mut app, AppAction::Copy, [None, None]), []);
}

#[cfg(target_os = "linux")]
#[test]
fn commands_run_in_the_directory_of_the_focused_shell() {
    use super::tab_tests::{INTERACTIVE_SHELL, change_directory};

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().canonicalize().unwrap();
    let log = path.join("ran");
    let mut app = app_with(INTERACTIVE_SHELL, Hotkeys::default());
    change_directory(&mut app, 0, &path);

    let program = crate::config::ui_config::Program::WithArgs {
        program: "sh".into(),
        args: vec!["-c".into(), format!("pwd > '{}'", log.display())],
    };
    perform(&mut app, &[AppAction::RunCommand(program)]);

    let ran = || std::fs::read_to_string(&log).unwrap_or_default();
    wait_until("the command never ran", || !ran().is_empty());
    assert_eq!(std::path::Path::new(ran().trim()), path);

    // A program that can't be started is logged, not fatal.
    let missing = crate::config::ui_config::Program::Just("/does/not/exist/tankterm".into());
    perform(&mut app, &[AppAction::RunCommand(missing)]);
    assert_eq!(app.terminals.len(), 1);
}

/// The shape the cursor of a terminal is drawn with.
pub(super) fn cursor(app: &App, terminal: u64) -> CursorShape {
    app.terminals[&terminal].terminal.cursor_shape()
}

/// The blink interval of a terminal's cursor passed.
fn blink(app: &mut App, terminal: u64) {
    let blink = iced_tterm::Event::BackendCall(terminal, BackendCommand::BlinkCursor);
    let _ = app.update(Message::Terminal(blink));
}

#[test]
fn cursor_is_hollow_without_the_keyboard_focus() {
    let mut app = app_with("", Hotkeys::default());
    assert_eq!(cursor(&app, 0), CursorShape::Block);

    // Only the pane that is typed into has a solid cursor.
    perform(&mut app, &[AppAction::Split(iced::widget::pane_grid::Axis::Vertical)]);
    assert_eq!((cursor(&app, 0), cursor(&app, 1)), (CursorShape::HollowBlock, CursorShape::Block));
    perform(&mut app, &[AppAction::PreviousPane]);
    assert_eq!((cursor(&app, 0), cursor(&app, 1)), (CursorShape::Block, CursorShape::HollowBlock));

    // No pane is typed into while the window doesn't have focus, or while a list or the
    // paste warning takes the keys.
    let _ = app.update(Message::WindowFocused(false));
    assert_eq!(cursor(&app, 0), CursorShape::HollowBlock);
    let _ = app.update(Message::WindowFocused(true));
    assert_eq!(cursor(&app, 0), CursorShape::Block);

    perform(&mut app, &[AppAction::ShowCommands]);
    assert_eq!(cursor(&app, 0), CursorShape::HollowBlock);
    perform(&mut app, &[AppAction::PaletteClose]);
    assert_eq!(cursor(&app, 0), CursorShape::Block);

    // The terminals of new tabs are told as well, and those of tabs that aren't shown.
    perform(&mut app, &[AppAction::NewTab]);
    assert_eq!((cursor(&app, 0), cursor(&app, 2)), (CursorShape::HollowBlock, CursorShape::Block));
}

#[test]
fn cursor_keeps_its_shape_without_unfocused_hollow() {
    let config = "[cursor]\nstyle = \"Underline\"\nunfocused_hollow = false\n";
    let mut app = app_with(config, Hotkeys::default());
    perform(&mut app, &[AppAction::Split(iced::widget::pane_grid::Axis::Vertical)]);
    let _ = app.update(Message::WindowFocused(false));

    assert_eq!(cursor(&app, 0), CursorShape::Underline);
    assert_eq!(cursor(&app, 1), CursorShape::Underline);
}

/// Configuration whose shell sends `sequence` and then waits.
fn shell_sending(sequence: &str) -> String {
    format!(
        "[terminal]\nshell = {{ program = \"sh\", args = [\"-c\", \"printf '{sequence}'; exec \
         cat\"] }}\n"
    )
}

/// Let the first terminal show what its shell printed until `condition` holds.
fn shown_until(app: &mut App, what: &str, condition: impl Fn(&App) -> bool) {
    wait_until(what, || {
        let _ = app.update(wakeup(0));
        condition(app)
    });
}

#[test]
fn applications_set_the_shape_of_the_cursor() {
    // DECSCUSR 4 is a steady underline, DECSCUSR 6 a steady beam.
    let config = format!("[cursor]\nstyle = \"Beam\"\n{}", shell_sending("\\\\033[4 q"));
    let mut app = app_with(&config, Hotkeys::default());
    shown_until(&mut app, "the cursor never became an underline", |app| {
        cursor(app, 0) == CursorShape::Underline
    });

    let mut app = app_with(&shell_sending("\\\\033[6 q"), Hotkeys::default());
    shown_until(&mut app, "the cursor never became a beam", |app| {
        cursor(app, 0) == CursorShape::Beam
    });

    // A hidden cursor (DECTCEM) has no shape.
    let mut app = app_with(&shell_sending("\\\\033[?25l"), Hotkeys::default());
    shown_until(&mut app, "the cursor was never hidden", |app| {
        cursor(app, 0) == CursorShape::Hidden
    });
}

#[test]
fn cursor_blinks_in_the_terminal_with_the_keyboard_focus() {
    let config = "[cursor]\nstyle = { shape = \"Beam\", blinking = \"Always\" }\n";
    let mut app = app_with(config, Hotkeys::default());
    perform(&mut app, &[AppAction::Split(iced::widget::pane_grid::Axis::Vertical)]);
    let blinks = |app: &App, terminal: u64| app.terminals[&terminal].terminal.cursor_blinks();
    assert!(!blinks(&app, 0) && blinks(&app, 1));

    // Every interval turns the cursor off or on.
    blink(&mut app, 1);
    assert_eq!(cursor(&app, 1), CursorShape::Hidden);
    blink(&mut app, 1);
    assert_eq!(cursor(&app, 1), CursorShape::Beam);

    // Typing lights the cursor, and it stays lit for the interval that follows.
    blink(&mut app, 1);
    assert_eq!(cursor(&app, 1), CursorShape::Hidden);
    let typed = iced_tterm::Event::BackendCall(1, BackendCommand::Write(b"x".to_vec()));
    let _ = app.update(Message::Terminal(typed));
    assert_eq!(cursor(&app, 1), CursorShape::Beam);
    blink(&mut app, 1);
    assert_eq!(cursor(&app, 1), CursorShape::Beam);

    // The blinking moves with the focus, and the cursor left behind is lit.
    blink(&mut app, 1);
    blink(&mut app, 1);
    perform(&mut app, &[AppAction::PreviousPane]);
    assert!(blinks(&app, 0) && !blinks(&app, 1));
    assert_eq!(cursor(&app, 1), CursorShape::HollowBlock);
    blink(&mut app, 1);
    assert_eq!(cursor(&app, 1), CursorShape::HollowBlock);
}

#[test]
fn applications_turn_blinking_on_unless_it_is_never() {
    let blinks = |app: &App| app.terminals[&0].terminal.cursor_blinks();

    // DECSCUSR 5 is a blinking beam.
    let mut app = app_with(&shell_sending("\\\\033[5 q"), Hotkeys::default());
    shown_until(&mut app, "the cursor never blinked", blinks);
    assert_eq!(cursor(&app, 0), CursorShape::Beam);

    // DECSCUSR 2 is a steady block, which turns the configured blinking off.
    let config =
        format!("[cursor]\nstyle = {{ blinking = \"On\" }}\n{}", shell_sending("\\\\033[2 q"));
    let mut app = app_with(&config, Hotkeys::default());
    shown_until(&mut app, "the cursor never stopped blinking", |app| !blinks(app));

    let config =
        format!("[cursor]\nstyle = {{ blinking = \"Never\" }}\n{}", shell_sending("\\\\033[5 q"));
    let mut app = app_with(&config, Hotkeys::default());
    shown_until(&mut app, "the cursor never became a beam", |app| {
        cursor(app, 0) == CursorShape::Beam
    });
    assert!(!blinks(&app));
}

/// The bell of a terminal rang.
fn bell(app: &mut App, terminal: u64) {
    let bell = BackendCommand::ProcessAlacrittyEvent(iced_tterm::AlacrittyEvent::Bell);
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(terminal, bell)));
}

#[test]
fn bell_command_runs_with_every_bell() {
    let directory = tempfile::tempdir().unwrap();
    let log = directory.path().join("bells");
    let config = format!(
        "[bell]\ncommand = {{ program = \"sh\", args = [\"-c\", \"echo rang >> '{}'\"] }}\n",
        log.display()
    );
    let mut app = app_with(&config, Hotkeys::default());
    perform(&mut app, &[AppAction::NewTab]);
    let bells = || std::fs::read_to_string(&log).unwrap_or_default().lines().count();

    // The terminal that is typed into and one in a tab that isn't shown.
    bell(&mut app, 1);
    wait_until("the bell command never ran", || bells() == 1);
    bell(&mut app, 0);
    wait_until("the bell command never ran again", || bells() == 2);

    // No command, no process: a bell without one is only pointed out.
    let mut app = app_with("", Hotkeys::default());
    bell(&mut app, 0);
    assert!(!app.is_flashing());
}

#[test]
fn bell_command_that_cannot_run_is_survived() {
    let mut app = app_with("[bell]\ncommand = \"/does/not/exist/tankterm\"\n", Hotkeys::default());
    bell(&mut app, 0);
    assert_eq!(app.terminals.len(), 1);
}

#[test]
fn visual_bell_flashes_the_pane_that_rang_until_it_fades() {
    let mut app = app_with("[bell]\nduration = 200\n", Hotkeys::default());
    perform(&mut app, &[AppAction::Split(iced::widget::pane_grid::Axis::Vertical)]);
    assert!(!app.is_flashing());

    // A pane that isn't typed into flashes as well; the other one doesn't.
    bell(&mut app, 0);
    let rang = app.terminals[&0].rang.expect("the pane doesn't flash");
    assert!(app.terminals[&1].rang.is_none());
    assert!(app.is_flashing());

    // Frames are drawn while the flash fades, and stop once it has.
    let _ = app.update(Message::Frame(rang + Duration::from_millis(199)));
    assert!(app.is_flashing());
    let _ = app.update(Message::Frame(rang + Duration::from_millis(200)));
    assert!(!app.is_flashing());
    let _ = app.view();

    // Ringing again while it flashes starts the flash over.
    bell(&mut app, 0);
    bell(&mut app, 0);
    let again = app.terminals[&0].rang.unwrap();
    assert!(again >= rang);
    let _ = app.view();

    // A terminal in a tab that isn't shown has nothing to flash.
    perform(&mut app, &[AppAction::NewTab]);
    let _ = app.update(Message::Frame(again + Duration::from_secs(1)));
    bell(&mut app, 1);
    assert!(!app.is_flashing());
    assert!(app.terminals[&1].attention);
}

#[test]
fn visual_bell_is_off_without_a_duration_and_when_accessible() {
    let mut app = app_with("[bell]\ncolor = \"#ff0000\"\n", Hotkeys::default());
    bell(&mut app, 0);
    assert!(!app.is_flashing());

    let mut config: UiConfig = toml::from_str("[bell]\nduration = 200\n").unwrap();
    config.apply_accessibility(true);
    app.options.bell = crate::bell::Bell::from_config(&config.bell);
    bell(&mut app, 0);
    assert!(!app.is_flashing());
}

/// An application with a background color and opacity.
fn app_with_background(color: &str, opacity: f32) -> App {
    let config = format!(
        "[colors.primary]\nbackground = \"{color}\"\n[window]\nopacity = {opacity:?}\npadding = \
         {{ x = 10, y = 10 }}\n"
    );
    app_with(&config, Hotkeys::default())
}

#[test]
fn window_opacity_makes_the_background_transparent() {
    let app = app_with_background("#c02020", 0.5);
    let shell = &app.terminals[&0];
    assert_eq!(app.pane_background(shell), Color { a: 0.5, ..Color::from_rgb8(0xc0, 0x20, 0x20) });

    // Opaque backgrounds are told apart by their color, and from a transparent one.
    let red = app_with_background("#c02020", 1.);
    assert_eq!(red.pane_background(&red.terminals[&0]).a, 1.);
    assert!(!same_picture(&red, &app_with_background("#2020c0", 1.)));
    assert!(!same_picture(&red, &app));

    // A background that isn't drawn at all looks the same in every color.
    let clear = app_with_background("#c02020", 0.);
    assert!(same_picture(&clear, &app_with_background("#2020c0", 0.)));
    assert!(!same_picture(&clear, &app));
}

#[test]
fn window_is_only_cleared_to_a_color_when_it_is_opaque() {
    let transparent = app_with_background("#c02020", 0.5);
    assert_eq!(transparent.style(&Theme::Dark).background_color, Color::TRANSPARENT);
    assert_eq!(transparent.opacity(), 0.5);

    let opaque = app_with("", Hotkeys::default());
    let style = opaque.style(&Theme::Dark);
    assert_eq!(style.background_color.a, 1.);
    assert_eq!(style.text_color, transparent.style(&Theme::Dark).text_color);
    assert_eq!(opaque.opacity(), 1.);
}
