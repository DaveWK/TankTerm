//! The iced application hosting the terminal.

use std::io;

use iced::widget::container;
use iced::{Color, Element, Length, Padding, Subscription, Task, window};
use iced_term::TerminalView;
use iced_term::actions::Action;
use iced_term::settings::Settings;

/// ID of the only terminal; `iced_term` uses it to route backend events.
const TERMINAL_ID: u64 = 0;

/// Messages handled by the application.
#[derive(Debug, Clone)]
pub enum Message {
    Terminal(iced_term::Event),
}

/// Application state: a single window with one terminal.
pub struct App {
    title: String,
    dynamic_title: bool,
    terminal: iced_term::Terminal,
    layout: Layout,
}

/// How the terminal is placed in the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// Space around the terminal, in logical pixels.
    pub padding: Padding,
    /// Color shown in the padding.
    pub background: Color,
}

impl App {
    /// Create the terminal and spawn its shell.
    pub fn new(
        title: String,
        dynamic_title: bool,
        settings: Settings,
        layout: Layout,
    ) -> io::Result<Self> {
        let terminal = iced_term::Terminal::new(TERMINAL_ID, settings)?;
        Ok(Self { title, dynamic_title, terminal, layout })
    }

    pub fn title(&self) -> String {
        self.title.clone()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        self.terminal.subscription().map(Message::Terminal)
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let Message::Terminal(iced_term::Event::BackendCall(_, command)) = message;

        match self.terminal.handle(iced_term::Command::ProxyToBackend(command)) {
            // The shell exited, close the window and with it the application.
            Action::Shutdown => return window::latest().and_then(window::close),
            Action::ChangeTitle(title) if self.dynamic_title => self.title = title,
            Action::ChangeTitle(_) | Action::Ignore => (),
        }

        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let background = self.layout.background;
        container(TerminalView::show(&self.terminal).map(Message::Terminal))
            .padding(self.layout.padding)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(background.into()),
                ..Default::default()
            })
            .into()
    }
}

/// Layout through the real `App` view, rendered headlessly.
#[cfg(all(test, unix))]
mod layout_tests {
    use iced::{Size, Theme};
    use iced_term::settings::BackendSettings;
    use iced_test::simulator::Simulator;

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
        };
        let app = App::new("test".into(), true, settings, layout).unwrap();

        let mut ui = Simulator::with_size(Default::default(), window, app.view());
        ui.snapshot(&Theme::Dark).unwrap();

        // The terminal widget publishes its laid out size to resize the PTY.
        let size = ui
            .into_messages()
            .find_map(|message| match message {
                Message::Terminal(iced_term::Event::BackendCall(
                    _,
                    iced_term::BackendCommand::Resize(Some(size), _),
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
}
