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
            tab_bar: term_settings::tab_bar_styles(&config),
        },
        tabs: config.tabs.clone(),
        status: crate::app::status::segments(&config),
        frameless: config.window.decorations == crate::config::window::Decorations::None,
        window_buttons: config.window.buttons,
        hotkeys: Hotkeys::from_config(&config),
        config_file: None,
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
    drop(ui);

    // The labels of the panes, while one is chosen by typing its label.
    let _ = app.update(Message::Action(AppAction::PaletteClose));
    let _ = app.update(Message::Action(AppAction::PaneSelect(PaneSelectMode::Focus)));

    let mut ui = Simulator::with_size(Default::default(), window, app.view());
    let snapshot = ui.snapshot(&Theme::Dark).unwrap();
    assert!(snapshot.matches_image(dir.join("pane-select")).unwrap());
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
fn prompt_marks() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(600., 200.);

    // A command that succeeds, one that fails, and the prompt waiting for the next one, with
    // their output selected.
    let config = r#"
        [terminal.shell]
        program = "sh"
        args = ["-c", '''
        a='\033]133;A\007~/src $ \033]133;B\007'
        printf "${a}ls\n\033]133;C\007Cargo.toml  crates  docs\n\033]133;D;0\007"
        printf "${a}false\n\033]133;C\007\033]133;D;1\007"
        printf "${a}make\n\033]133;C\007make: *** No targets.  Stop.\n\033]133;D;2\007"
        printf "${a}"
        exec cat
        ''']
    "#;
    let mut app = super::tab_tests::app_with(config, Hotkeys::default());
    settle(&mut app, window);
    let select = iced_tterm::Command::ProxyToBackend(BackendCommand::SelectCommandOutput);
    app.terminals.get_mut(&0).unwrap().terminal.handle(select);
    snapshot(&app, window, &dir, "prompt-marks");
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

/// The picture at the top of the README: real commands in tabs and split panes.
///
/// Needs `bash`, `bat`, `git` and `tree`. Run with
/// `TANKTERM_SNAPSHOT_DIR=<dir> cargo test -p tankterm readme_screenshot -- --ignored`, then copy
/// `<dir>/readme-wgpu.png` to `extra/promo/tankterm-readme.png`.
#[test]
#[ignore = "writes the README screenshot"]
fn readme_screenshot() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(1100., 560.);
    let repo =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();

    // A fixed prompt, so no user or host name ends up in the picture, and tool options in the
    // environment, so the commands typed stay short.
    let config = format!(
        r#"
        [general]
        working_directory = '{}'
        [terminal]
        shell = {{ program = "bash", args = ["--norc", "--noprofile", "-i"] }}
        [env]
        PS1 = '\[\e[1;32m\]tank\[\e[0m\]:\[\e[1;36m\]~/tankterm\[\e[0m\]$ '
        COLORTERM = "truecolor"
        TERM = "xterm-256color"
        BAT_PAGING = "never"
        BAT_STYLE = "numbers,header"
        GIT_PAGER = "cat"
        GIT_CONFIG_COUNT = "1"
        GIT_CONFIG_KEY_0 = "alias.lg"
        GIT_CONFIG_VALUE_0 = "log --graph --color=always --format='%C(yellow)%h%C(reset) %<(42,trunc)%s'"
        [tabs]
        show = "Always"
        inherit_working_directory = false
        "#,
        repo.display()
    );
    let mut app = super::tab_tests::app_with(&config, Hotkeys::default());
    app.options.title = "TankTerm".into();
    for action in [
        AppAction::Split(Axis::Vertical),
        AppAction::Split(Axis::Horizontal),
        AppAction::NewTab,
        AppAction::NewTab,
        AppAction::SelectTab(0),
    ] {
        let _ = app.update(Message::Action(action));
    }
    settle(&mut app, window);

    let commands =
        ["bat -r 5:27 extra/themes/charm.toml", "git lg -10 main", "tree -C -L 1 crates"];
    let write = |app: &mut App, id: usize, text: &str| {
        let write = BackendCommand::Write(text.as_bytes().to_vec());
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(id as u64, write)));
    };
    for (id, command) in commands.into_iter().enumerate() {
        // Typed once the prompt is there, as someone would.
        super::tab_tests::wait_for_text(&app, id as u64, "tankterm$");
        write(&mut app, id, &format!("{command}\n"));
    }

    thread::sleep(Duration::from_secs(2));
    for id in 0..app.next_terminal {
        let wakeup = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup);
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(id, wakeup)));
    }
    snapshot(&app, window, &dir, "readme");
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

#[test]
#[ignore = "writes screenshots for manual inspection"]
fn tab_bar_styles() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(900., 120.);

    // Four tabs: the second is shown, the third printed something unseen, and the fourth rang
    // the bell. The mouse is over the first one in the `-hovered` pictures.
    let shots = |name: &str, config: &str| {
        let mut app = super::tab_tests::app_with(config, Hotkeys::default());
        for action in
            [AppAction::NewTab, AppAction::NewTab, AppAction::NewTab, AppAction::SelectTab(1)]
        {
            let _ = app.update(Message::Action(action));
        }
        settle(&mut app, window);
        for (index, tab) in app.tabs.iter_mut().enumerate() {
            tab.activity = index == 2;
        }
        app.terminals.get_mut(&3).unwrap().attention = true;

        snapshot(&app, window, &dir, name);

        let mut ui = Simulator::with_size(Default::default(), window, app.view());
        let tab = ui.find("1: test").unwrap();
        ui.point_at(iced_test::selector::Bounded::visible_bounds(&tab).unwrap().center());
        let hovered = ui.snapshot(&Theme::Dark).unwrap();
        assert!(hovered.matches_image(dir.join(format!("{name}-hovered"))).unwrap());
        drop(ui);

        app.window_focused = false;
        snapshot(&app, window, &dir, &format!("{name}-blurred"));
    };

    shots("tab-bar-default", "");
    shots(
        "tab-bar-styled",
        r##"
        [style.defs.pill]
        border = "Rounded"
        padding = [2, 12]
        margin = [0, 2]

        [style.tab_bar]
        background = "#1b1b24"
        padding = [4, 6]

        [style.tab_active]
        inherit = "pill"
        foreground = "#000000"
        background = "#00ff00"
        border_foreground = "#00ff00"
        bold = true

        [style.tab_inactive]
        inherit = "pill"
        foreground = "#9a9aa8"
        border_foreground = "#3a3a4a"

        [style.tab_hovered]
        foreground = "#ffffff"
        border_foreground = "#00ff00"

        [style.tab_activity]
        italic = true

        [style.tab_attention]
        foreground = "#ff4d5a"
        underline = true

        [style.tab_button]
        foreground = "#00ff00"

        [style.blurred.tab_active]
        foreground = "#00ff00"
        background = "#1b1b24"
        bold = false
        "##,
    );
}

#[test]
#[ignore = "writes screenshots for manual inspection"]
fn status_segments() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(900., 120.);

    // The default segment style, then segments with styles of their own.
    let shots = |name: &str, styles: [&str; 4]| {
        let config = format!(
            r##"
            [keyboard.tables.resize]
            bindings = []

            [[tabs.status]]
            segment = "KeyTable"
            style = {{ {} }}

            [[tabs.status]]
            segment = "Cwd"
            style = {{ {} }}

            [[tabs.status]]
            segment = "Battery"
            format = "{{percent}}% {{status}}"
            style = {{ {} }}

            [[tabs.status]]
            segment = "Clock"
            format = "%a %d %b %H:%M"
            style = {{ {} }}
            "##,
            styles[0], styles[1], styles[2], styles[3]
        );
        let parsed: UiConfig = toml::from_str(&config).unwrap();
        let mut app = super::tab_tests::app_with(&config, Hotkeys::from_config(&parsed));
        let _ = app.update(Message::Action(AppAction::NewTab));
        let _ = app.update(Message::Action(AppAction::ActivateKeyTable("resize".into())));
        assert!(!app.key_tables.is_empty());
        settle(&mut app, window);

        app.now = Some("2026-11-12T09:41:00[UTC]".parse().unwrap());
        app.battery = Some(status::Battery { percent: 81, status: "Charging".into() });
        let focused = app.focused_terminal().unwrap();
        let home = home::home_dir().unwrap_or_default();
        app.terminals.get_mut(&focused).unwrap().working_directory =
            Some(home.join("src/tankterm"));
        snapshot(&app, window, &dir, name);
    };

    shots("status-segments", ["", "", "", ""]);
    shots("status-segments-styled", [
        r##"foreground = "#000000", background = "#f4d35e", bold = true"##,
        r##"foreground = "#00ff00", italic = true"##,
        r##"foreground = "#000000", background = "#00aa00""##,
        r##"foreground = "#000000", background = "#00ff00", border = "Rounded", margin = [0, 4]"##,
    ]);
}

#[test]
#[ignore = "writes screenshots for manual inspection"]
fn tab_title_formats() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(900., 120.);

    // The program and directory come from a tick, as they would in a window: `cat`, in the
    // directory the tests run in. The second tab is split, with a pane zoomed.
    let config = r#"
        [tabs]
        show = "Always"
        title = "{index} {process} in {cwd} [{panes}]{zoom}{marker}"
    "#;
    let mut app = super::tab_tests::app_with(config, Hotkeys::default());
    for action in [
        AppAction::NewTab,
        AppAction::Split(Axis::Vertical),
        AppAction::ToggleZoom,
        AppAction::NewTab,
        AppAction::SelectTab(1),
    ] {
        let _ = app.update(Message::Action(action));
    }
    settle(&mut app, window);
    let _ = app.update(Message::Tick);
    for (index, tab) in app.tabs.iter_mut().enumerate() {
        tab.activity = index == 2;
    }

    snapshot(&app, window, &dir, "tab-title-format");
}

#[test]
#[ignore = "writes screenshots for manual inspection"]
fn window_buttons() {
    let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
    let window = Size::new(900., 120.);

    // A window without the system's title bar, with one tab: the tab bar is shown anyway, with
    // the window's buttons. The mouse is over the close button in the `-hovered` picture.
    let shot = |name: &str, config: &str, close: f32| {
        let config = format!("[window]\ndecorations = \"None\"\n{config}");
        let mut app = super::tab_tests::app_with(&config, Hotkeys::default());
        settle(&mut app, window);
        snapshot(&app, window, &dir, name);

        let mut ui = Simulator::with_size(Default::default(), window, app.view());
        let tab = ui.find("1: test").unwrap();
        let y = iced_test::selector::Bounded::visible_bounds(&tab).unwrap().center_y();
        ui.point_at((close, y));
        let hovered = ui.snapshot(&Theme::Dark).unwrap();
        assert!(hovered.matches_image(dir.join(format!("{name}-hovered"))).unwrap());
    };

    shot("window-buttons-right", "", window.width - 4.);
    shot("window-buttons-left", "buttons = \"Left\"\n", 4.);
}
