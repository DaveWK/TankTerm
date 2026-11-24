//! Layout through the real `App` view, rendered headlessly.

use iced::{Size, Theme};
use iced_test::simulator::Simulator;
use iced_tterm::settings::BackendSettings;

use super::*;
use crate::cli::Options;
use crate::config::UiConfig;
use crate::term_settings;

/// Size the terminal widget is laid out at in a window of `window` with `config`'s padding.
fn terminal_size(config: &str, window: Size) -> (Size, Padding) {
    let mut config: UiConfig = toml::from_str(config).unwrap();
    // The system's title bar, so a single tab shows no tab bar.
    config.window.decorations = crate::config::window::Decorations::Full;
    let mut settings = term_settings::settings(&config, &Options::default());
    settings.backend = BackendSettings { program: "cat".into(), ..BackendSettings::default() };

    let cell = term_settings::cell_size(&settings.font);
    let layout = Layout {
        padding: term_settings::padding(&config, cell),
        background: term_settings::background(&config),
        foreground: term_settings::foreground(&config),
        tab_bar: term_settings::tab_bar_styles(&config),
    };
    let options = AppOptions {
        title: "test".into(),
        dynamic_title: true,
        layout,
        tabs: config.tabs.clone(),
        status: crate::app::status::segments(&config),
        frameless: config.window.decorations == crate::config::window::Decorations::None,
        // Tests don't depend on the desktop they run on.
        title_buttons: crate::title_buttons::TitleButtons::new(
            config.window.buttons,
            Default::default,
        ),
        hotkeys: Hotkeys::default(),
        config_file: None,
        session_file: None,
        profiles: Vec::new(),
        confirm_multiline_paste: true,
        shell_integration: false,
        copy_on_select: false,
        bindings: Vec::new(),
        bell: crate::bell::Bell::from_config(&config.bell),
        opacity: config.window_opacity(),
        resize_increments: config.window.resize_increments,
    };
    let app = App::new(options, settings.clone(), settings).unwrap();

    let mut ui = Simulator::with_size(Default::default(), window, app.view());
    ui.snapshot(&Theme::Dark).unwrap();

    // The terminal widget publishes its laid out size to resize the PTY.
    let size = ui
        .into_messages()
        .find_map(|message| match message {
            Message::Terminal(iced_tterm::Event::BackendCall(
                _,
                iced_tterm::BackendCommand::Resize(Some(size), _),
            )) => Some(size),
            _ => None,
        })
        .expect("terminal widget didn't publish its size");

    (size, layout.padding)
}

#[test]
fn terminal_fills_window_without_padding() {
    let window = Size::new(800., 400.);
    let (size, _) = terminal_size("[window]\npadding = { x = 0, y = 0 }\n", window);

    assert_eq!(size, window);
}

#[test]
fn terminal_keeps_off_the_edges_by_default() {
    let window = Size::new(800., 400.);
    let (size, _) = terminal_size("", window);

    assert_eq!(size, Size::new(800. - 2. * 8., 400. - 2. * 4.));
}

#[test]
fn terminal_is_inset_by_cell_padding() {
    let window = Size::new(800., 400.);
    let (size, padding) = terminal_size("[window]\npadding = [1, 2, 3, 4]\n", window);

    assert!(padding.top > 0. && padding.left > padding.right);
    assert_eq!(size.width, window.width - padding.left - padding.right);
    assert_eq!(size.height, window.height - padding.top - padding.bottom);
}

#[test]
fn terminal_is_inset_by_pixel_padding() {
    let window = Size::new(800., 400.);
    let (size, _) = terminal_size("[window]\npadding = { x = 10, y = 20 }\n", window);

    assert_eq!(size, Size::new(780., 360.));
}

#[test]
fn window_dimensions_open_that_many_cells() {
    // Sizes whose cells aren't whole pixels, and one whose are.
    for size in ["11.25", "11", "9", "12"] {
        let text = format!(
            "[font]\nsize = {size}\n[window]\npadding = {{ x = 8, y = 4 }}\ndimensions = {{ \
             columns = 80, lines = 24 }}\n"
        );
        let config: UiConfig = toml::from_str(&text).unwrap();
        let settings = term_settings::settings(&config, &Options::default());
        let cell = term_settings::cell_size(&settings.font);
        let padding = term_settings::padding(&config, cell);
        let window = term_settings::window_size(&config, cell, padding).unwrap();

        let (terminal, _) = terminal_size(&text, window);
        // The terminal counts the whole pixels of its cells.
        let measured = settings.font.cell_size();
        let columns = (terminal.width / measured.width.trunc()).floor();
        let lines = (terminal.height / measured.height.trunc()).floor();
        assert_eq!((columns, lines), (80., 24.), "font size {size}");
    }
}
