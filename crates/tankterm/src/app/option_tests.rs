//! Configuration options through the real `App`, with `cat` or `sh` as the shell.

use iced::mouse::{self, ScrollDelta};
use iced::{Event, Size};
use iced_test::simulator::Simulator;
use iced_tterm::BackendCommand;
use iced_tterm::settings::ScrollSettings;

use super::tab_tests::{app_with, wait_for_text};
use super::*;

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
