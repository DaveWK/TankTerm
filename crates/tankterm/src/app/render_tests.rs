//! Writes screenshots of tabs and split panes, rendered headlessly, for looking at by hand.
//!
//! Run with `TANKTERM_SNAPSHOT_DIR=<dir> cargo test -p tankterm render_tests -- --ignored`.

use std::path::PathBuf;
use std::time::Duration;
use std::{env, thread};

use iced::widget::pane_grid::Axis;
use iced::{Size, Theme};
use iced_test::simulator::Simulator;
use iced_tterm::settings::BackendSettings;
use iced_tterm::{AlacrittyEvent, BackendCommand};

use super::*;
use crate::cli::Options;
use crate::config::UiConfig;
use crate::term_settings;

#[test]
#[ignore = "writes screenshots for manual inspection"]
fn tabs_and_panes() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(900., 420.);

    let config = UiConfig::default();
    let mut settings = term_settings::settings(&config, &Options::default());
    settings.backend = BackendSettings {
        program: "sh".into(),
        args: vec!["-c".into(), "echo \"pane $$\"; exec cat".into()],
        ..BackendSettings::default()
    };

    let options = AppOptions {
        title: "TankTerm".into(),
        dynamic_title: true,
        layout: Layout {
            padding: Padding::new(4.),
            background: term_settings::background(&config),
            foreground: term_settings::foreground(&config),
        },
        tabs: config.tabs,
        hotkeys: Hotkeys::from_config(&config),
        session_file: None,
        profiles: Vec::new(),
        confirm_multiline_paste: true,
        copy_on_select: false,
        bindings: Vec::new(),
    };
    let mut app = App::new(options, settings.clone(), settings).unwrap();

    for action in [
        AppAction::NewTab,
        AppAction::Split(Axis::Vertical),
        AppAction::Split(Axis::Horizontal),
        AppAction::NewTab,
        AppAction::SelectTab(1),
    ] {
        let _ = app.update(Message::Action(action));
    }

    // Lay the terminals out once so they get their size, then let the shells print.
    let resizes: Vec<_> = {
        let mut ui = Simulator::with_size(Default::default(), window, app.view());
        ui.snapshot(&Theme::Dark).unwrap();
        ui.into_messages().collect()
    };
    for message in resizes {
        let _ = app.update(message);
    }
    thread::sleep(Duration::from_millis(500));
    for id in 0..app.next_terminal {
        let wakeup = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup);
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(id, wakeup)));
    }

    let mut ui = Simulator::with_size(Default::default(), window, app.view());
    let snapshot = ui.snapshot(&Theme::Dark).unwrap();
    assert!(snapshot.matches_image(dir.join("tabs-and-panes")).unwrap());
    drop(ui);

    // The command palette over the terminals, with a background tab showing activity.
    let wakeup = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup);
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(0, wakeup)));
    let _ = app.update(Message::Action(AppAction::ShowCommands));
    let _ = app.update(Message::PaletteQuery("pane".into()));
    let _ = app.update(Message::Action(AppAction::PaletteNext));

    let mut ui = Simulator::with_size(Default::default(), window, app.view());
    let snapshot = ui.snapshot(&Theme::Dark).unwrap();
    assert!(snapshot.matches_image(dir.join("command-palette")).unwrap());
}

/// Lay the terminals out so they get their size, then let them show what their shells printed.
fn settle(app: &mut App, window: Size) {
    let resizes: Vec<_> = {
        let mut ui = Simulator::with_size(Default::default(), window, app.view());
        ui.snapshot(&Theme::Dark).unwrap();
        ui.into_messages().collect()
    };
    for message in resizes {
        let _ = app.update(message);
    }

    thread::sleep(Duration::from_millis(500));
    for id in 0..app.next_terminal {
        let wakeup = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup);
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(id, wakeup)));
    }
}

/// Write a screenshot of the application to `name` in the snapshot directory.
fn snapshot(app: &App, window: Size, dir: &std::path::Path, name: &str) {
    let mut ui = Simulator::with_size(Default::default(), window, app.view());
    let snapshot = ui.snapshot(&Theme::Dark).unwrap();
    assert!(snapshot.matches_image(dir.join(name)).unwrap());
}

/// An application with three panes whose `cat` was sent a line each.
fn three_panes(config: &str, lines: [&str; 3]) -> App {
    let mut app = super::tab_tests::app_with(config, Hotkeys::default());
    for action in [AppAction::Split(Axis::Vertical), AppAction::Split(Axis::Horizontal)] {
        let _ = app.update(Message::Action(action));
    }

    for (id, line) in lines.into_iter().enumerate() {
        let write = BackendCommand::Write(format!("{line}\n").into_bytes());
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(id as u64, write)));
    }

    app
}

#[test]
#[ignore = "writes screenshots for manual inspection"]
fn cursors() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(900., 420.);

    // `cat` repeats the DECSCUSR sequence it is sent, which sets the shape of its cursor.
    let shapes = ["block \x1b[2 q", "underline \x1b[4 q", "beam \x1b[6 q"];
    let mut app = three_panes("[cursor]\nunfocused_hollow = false\n", shapes);
    settle(&mut app, window);
    snapshot(&app, window, &dir, "cursor-shapes");

    // Thicker lines, and hollow cursors in the panes without the keyboard focus.
    let mut app = three_panes("[cursor]\nthickness = 0.4\n", shapes);
    settle(&mut app, window);
    snapshot(&app, window, &dir, "cursor-unfocused");

    let _ =
        app.update(Message::Action(AppAction::FocusPane(iced::widget::pane_grid::Direction::Up)));
    snapshot(&app, window, &dir, "cursor-thick-underline");
}
