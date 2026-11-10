//! The iced application hosting the terminal.

use std::io;

use iced::widget::{container, responsive};
use iced::{Element, Length, Subscription, Task, window};
use iced_term::TerminalView;
use iced_term::actions::Action;
use iced_term::settings::Settings;

use crate::chrome::Chrome;

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
    chrome: Chrome,
    /// Name of the shell, shown in the status bar.
    shell: String,
}

impl App {
    /// Create the terminal and spawn its shell.
    pub fn new(
        title: String,
        dynamic_title: bool,
        settings: Settings,
        chrome: Chrome,
    ) -> io::Result<Self> {
        let shell = shell_name(&settings.backend.program);
        let terminal = iced_term::Terminal::new(TERMINAL_ID, settings)?;
        Ok(Self { title, dynamic_title, terminal, chrome, shell })
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
        let background = self.chrome.background();
        let content = responsive(move |size| {
            let terminal = TerminalView::show(&self.terminal).map(Message::Terminal);
            self.chrome.view(size, terminal, &self.title, &self.shell)
        });

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(background.into()),
                ..Default::default()
            })
            .into()
    }
}

/// File name of the shell program, like `bash` for `/bin/bash`.
///
/// Both `/` and `\` are treated as separators, since the program comes from config files that
/// may be shared between platforms.
fn shell_name(program: &str) -> String {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    name.strip_suffix(".exe").unwrap_or(name).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_name_strips_path_and_extension() {
        assert_eq!(shell_name("/bin/bash"), "bash");
        assert_eq!(shell_name("fish"), "fish");
        assert_eq!(shell_name("C:\\Windows\\powershell.exe"), "powershell");
    }
}

/// Keyboard input through the real `App`, without a window.
#[cfg(all(test, unix))]
mod input_tests {
    use std::time::{Duration, Instant};

    use iced::keyboard::Key;
    use iced::keyboard::key::Named;
    use iced::{Point, Size};
    use iced_term::settings::BackendSettings;
    use iced_test::simulator::{self, Simulator};

    use super::*;
    use crate::cli::Options;
    use crate::config::UiConfig;
    use crate::term_settings;

    fn wakeup() -> Message {
        let event = iced_term::AlacrittyEvent::Wakeup;
        let command = iced_term::BackendCommand::ProcessAlacrittyEvent(event);
        Message::Terminal(iced_term::Event::BackendCall(TERMINAL_ID, command))
    }

    #[test]
    fn typed_text_reaches_the_program() {
        let config = UiConfig::default();
        let mut settings = term_settings::settings(&config, &Options::default());
        settings.backend = BackendSettings { program: "cat".into(), ..BackendSettings::default() };

        let primary = &config.colors.primary;
        let chrome = Chrome::new(
            config.style.resolve(),
            &settings.font,
            primary.foreground,
            primary.background,
        );
        let mut app = App::new("test".into(), true, settings, chrome).unwrap();

        let size = Size::new(800., 400.);
        let messages: Vec<Message> = {
            let mut ui = Simulator::with_size(Default::default(), size, app.view());

            // Focus the terminal by clicking inside the frame, then type a line.
            ui.point_at(Point::new(size.width / 2., size.height / 2.));
            ui.simulate(simulator::click());
            ui.typewrite("hello tank");
            ui.tap_key(Key::Named(Named::Enter));

            ui.into_messages().collect()
        };
        assert!(!messages.is_empty(), "typing produced no messages");
        for message in messages {
            let _ = app.update(message);
        }

        // The line shows up twice: the terminal's echo of the input, then `cat`'s output.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let _ = app.update(wakeup());
            let text = app.terminal.text();
            if text.iter().filter(|line| line.trim_end() == "hello tank").count() >= 2 {
                break;
            }

            assert!(Instant::now() < deadline, "typed text never reached `cat`: {text:?}");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}
