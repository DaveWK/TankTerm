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
    let config: UiConfig = toml::from_str(config).unwrap();
    let mut settings = term_settings::settings(&config, &Options::default());
    settings.backend = BackendSettings { program: "cat".into(), ..BackendSettings::default() };

    let cell = term_settings::cell_size(&settings.font);
    let layout = Layout {
        padding: term_settings::padding(&config, cell),
        background: term_settings::background(&config),
        foreground: term_settings::foreground(&config),
    };
    let options = AppOptions {
        title: "test".into(),
        dynamic_title: true,
        layout,
        tabs: config.tabs,
        hotkeys: Hotkeys::default(),
        config_file: None,
        session_file: None,
        profiles: Vec::new(),
        confirm_multiline_paste: true,
        copy_on_select: false,
        bindings: Vec::new(),
        bell: crate::bell::Bell::from_config(&config.bell),
        opacity: config.window_opacity(),
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
    let (size, _) = terminal_size("", window);

    assert_eq!(size, window);
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
