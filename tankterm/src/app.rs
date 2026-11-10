//! The iced application hosting the terminal.

use std::io;

use iced::widget::container;
use iced::{Element, Length, Subscription, Task, window};
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
}

impl App {
    /// Create the terminal and spawn its shell.
    pub fn new(title: String, dynamic_title: bool, settings: Settings) -> io::Result<Self> {
        let terminal = iced_term::Terminal::new(TERMINAL_ID, settings)?;
        Ok(Self { title, dynamic_title, terminal })
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
        container(TerminalView::show(&self.terminal).map(Message::Terminal))
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
