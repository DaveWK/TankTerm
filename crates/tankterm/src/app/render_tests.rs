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
use iced_tterm::{AlacrittyEvent, BackendCommand, SelectionType};

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
        bell: crate::bell::Bell::from_config(&config.bell),
        opacity: config.window_opacity(),
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

#[test]
#[ignore = "writes screenshots for manual inspection"]
fn selection_and_strikethrough() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(900., 160.);

    // The second line is selected, with a selection style that adds attributes.
    let config = r#"
        [terminal]
        shell = { program = "sh", args = ["-c", 'printf "plain \033[9mstruck out\033[0m plain\nselected line\n"; exec cat'] }
        [style.selection]
        bold = true
        italic = true
        underline = true
    "#;
    let mut app = super::tab_tests::app_with(config, Hotkeys::default());
    settle(&mut app, window);

    let cell = term_settings::cell_size(&app.settings.font);
    let select = BackendCommand::SelectStart(SelectionType::Lines, (1., cell.height * 1.5));
    app.terminals.get_mut(&0).unwrap().terminal.handle(iced_tterm::Command::ProxyToBackend(select));
    snapshot(&app, window, &dir, "selection-and-strikethrough");

    // Reverse instead: the selection's colors swapped.
    let config = config.replace("underline = true", "reverse = true");
    let mut app = super::tab_tests::app_with(&config, Hotkeys::default());
    settle(&mut app, window);
    let select = BackendCommand::SelectStart(SelectionType::Lines, (1., cell.height * 1.5));
    app.terminals.get_mut(&0).unwrap().terminal.handle(iced_tterm::Command::ProxyToBackend(select));
    snapshot(&app, window, &dir, "selection-reversed");
}

#[test]
#[ignore = "writes screenshots for manual inspection"]
fn box_drawing() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(1100., 560.);

    // Frames of light, heavy, double and rounded lines with their junctions, diagonals, blocks,
    // eighths, shades and quadrants, then dashes and half lines, all the sextants, powerline
    // segments, lines on a background, dim and reversed, and the cursor on a junction.
    let sextants: String = ('\u{1fb00}'..='\u{1fb3b}').collect();
    let [right, right_thin, left, left_thin] = ['\u{e0b0}', '\u{e0b1}', '\u{e0b2}', '\u{e0b3}'];
    let config = format!(
        r#"
        [font]
        size = 18
        [terminal.shell]
        program = "sh"
        args = ["-c", '''
        printf "┌──┬──┐ ┏━━┳━━┓ ╔══╦══╗ ╭──┬──╮ ┍━┯━┑ ╓─╥─╖ ╱╲╱╲ ▁▂▃▄▅▆▇█ ░▒▓█ ██\n"
        printf "│  │  │ ┃  ┃  ┃ ║  ║  ║ │  │  │ │ │ │ ║ ║ ║ ╲╱╲╱ █▇▆▅▄▃▂▁ ▀▄▌▐ ██\n"
        printf "├──┼──┤ ┣━━╋━━┫ ╠══╬══╣ ├──┼──┤ ┝━┿━┥ ╟─╫─╢ ╱╲╱╲ ▏▎▍▌▋▊▉█ ▔▕▖▗ ██\n"
        printf "│  │  │ ┃  ┃  ┃ ║  ║  ║ │  │  │ │ │ │ ║ ║ ║ ╲╱╲╱ 🮂🮃🮄🮅🮆🮇🮈🮉 ▘▝▙▚ ██\n"
        printf "└──┴──┘ ┗━━┻━━┛ ╚══╩══╝ ╰──┴──╯ ┕━┷━┙ ╙─╨─╜ ╳╳╳╳ 🮊🮋▛▜▞▟░▒ ▓▓▓▓ ██\n"
        printf "┄┄┄┅┅┅ ┈┈┈┉┉┉ ╌╌╌╍╍╍ ┆┇┊┋╎╏ ╴╶╵╷╸╹╺╻ ╼╽╾╿ ┞┟┠┡┢ ┭┮┯┰┱┲ ╭╮╭╮\n"
        printf "┆┇┊┋╎╏ ┆┇┊┋╎╏ ┆┇┊┋╎╏ ┆┇┊┋╎╏ ╴╶╵╷╸╹╺╻ ╼╽╾╿ ┦┧┨┩┪ ┵┶┷┸┹┺ ╰╯╰╯\n"
        printf "{sextants}\n"
        printf "\033[30;44m main \033[34;42m{right}\033[30m ~/src \033[32;49m{right}\033[0m {right_thin} {left_thin} \033[32m{left}\033[30;42m 12:00 \033[34;42m{left}\033[30;44m ok \033[0m\n"
        printf "\033[41m─┼─╋━╬═█▒\033[0m \033[2m─┼─╋━╬═█▒ dim\033[0m \033[7m┌─┐╔═╗▄▀\033[0m\n"
        printf "cursor ┌┼┐\033[2D"
        exec cat
        ''']
        "#
    );
    let mut app = super::tab_tests::app_with(&config, Hotkeys::default());
    settle(&mut app, window);
    snapshot(&app, window, &dir, "box-drawing");

    // The same characters from the font.
    let config = config.replace("size = 18", "size = 18\nbuiltin_box_drawing = false");
    let mut app = super::tab_tests::app_with(&config, Hotkeys::default());
    settle(&mut app, window);
    snapshot(&app, window, &dir, "box-drawing-font");
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

#[test]
#[ignore = "writes screenshots for manual inspection"]
fn visual_bell() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(900., 420.);

    let config = "[bell]\nduration = 60000\ncolor = \"#ff4040\"\n";
    let mut app = three_panes(config, ["quiet", "rang the bell", "quiet"]);
    settle(&mut app, window);

    // Halfway through the flash of the pane at the top right.
    let bell = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Bell);
    let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(1, bell)));
    let rang = app.terminals.get_mut(&1).unwrap().rang.as_mut().expect("the pane doesn't flash");
    *rang -= Duration::from_secs(30);
    snapshot(&app, window, &dir, "visual-bell");
}

#[test]
#[ignore = "writes screenshots for manual inspection"]
fn window_opacity() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(900., 420.);

    // The screenshot is taken over the gray of iced's dark theme, which stands in for what
    // is behind the window. Cells with a background of their own stay opaque.
    let config = "[window]\nopacity = 0.5\n[colors.primary]\nbackground = \"#c02020\"\n";
    let lines = ["plain", "\x1b[44m blue cells \x1b[0m plain", "\x1b[7m reversed \x1b[0m"];
    let mut app = three_panes(config, lines);
    settle(&mut app, window);
    snapshot(&app, window, &dir, "window-opacity");
}

#[test]
#[ignore = "writes screenshots for manual inspection"]
fn dynamic_padding() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(940., 430.);

    let mut app = three_panes("[window]\ndynamic_padding = true\n", ["", "", ""]);
    settle(&mut app, window);

    // Each grid is cleared to blue once it has its size, which shows where it is: every pane
    // leaves as much space on its left as on its right, and above as below.
    for id in 0..3 {
        let blue = BackendCommand::Write(b"\x1b[44m\x1b[2J\x1b[H blue grid\n".to_vec());
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(id, blue)));
    }
    settle(&mut app, window);
    snapshot(&app, window, &dir, "dynamic-padding");
}
