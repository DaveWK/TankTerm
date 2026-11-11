use crate::actions::Action;
use crate::backend;
use crate::bindings::{Binding, BindingAction, BindingsLayout, InputKind};
use crate::font::TermFont;
use crate::settings::{FontSettings, Settings, ThemeSettings};
use crate::theme::{ColorPalette, Theme};
use crate::{AlacrittyEvent, TermMode};
use iced::futures::stream::BoxStream;
use iced::futures::{SinkExt, StreamExt};
use iced::widget::canvas::Cache;
use iced::Subscription;
use std::hash::{Hash, Hasher};
use std::io::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub enum Event {
    BackendCall(u64, backend::Command),
    /// The mouse button was released after selecting text in the terminal
    /// with this ID; [`Terminal::selection`] has the text.
    SelectionFinished(u64),
}

#[derive(Debug, Clone)]
pub enum Command {
    ChangeTheme(Box<ColorPalette>),
    ChangeFont(FontSettings),
    AddBindings(Vec<(Binding<InputKind>, BindingAction)>),
    ProxyToBackend(backend::Command),
}

pub struct Terminal {
    pub id: u64,
    widget_id: iced::widget::Id,
    pub(crate) font: TermFont,
    pub(crate) theme: Theme,
    pub(crate) cache: Cache,
    pub(crate) bindings: BindingsLayout,
    pub(crate) backend: backend::Backend,
    backend_event_rx: Arc<Mutex<backend::EventReceiver>>,
}

impl Terminal {
    pub fn new(id: u64, settings: Settings) -> Result<Self> {
        let (backend_event_tx, backend_event_rx) = backend::event_channel(100);
        let theme = Theme::new(settings.theme);
        let font = TermFont::new(settings.font);
        let backend = backend::Backend::new(
            id,
            backend_event_tx,
            settings.backend,
            theme.color_scheme(),
        )?;

        Ok(Self {
            id,
            widget_id: iced::widget::Id::unique(),
            font,
            theme,
            bindings: BindingsLayout::default(),
            cache: Cache::default(),
            backend,
            backend_event_rx: Arc::new(Mutex::new(backend_event_rx)),
        })
    }

    pub fn widget_id(&self) -> &iced::widget::Id {
        &self.widget_id
    }

    pub fn subscription(&self) -> Subscription<Event> {
        let data = TerminalSubscriptionData {
            id: self.id,
            event_receiver: self.backend_event_rx.clone(),
        };

        Subscription::run_with(data, terminal_subscription_stream)
    }

    /// The terminal's text, scrollback first and the visible screen last, one
    /// `String` per line.
    ///
    /// For callers that need to read what the terminal shows rather than only
    /// draw it: scraping the output of a command an application wrote into the
    /// PTY itself, asserting on a terminal's contents in a test, or handing the
    /// screen to an accessibility layer. [`Self::handle`] can write into the
    /// PTY already; this is the other half of that conversation.
    ///
    /// A line is a *logical* line, not a grid row: output that wrapped across
    /// the terminal's width comes back joined, so a reader is not looking at
    /// where the window happened to be split. Trailing blanks are trimmed.
    pub fn text(&self) -> Vec<String> {
        self.backend.text()
    }

    /// The terminal's current modes, like whether the application in it asked
    /// for bracketed paste or mouse reports.
    pub fn mode(&self) -> TermMode {
        self.backend.mode()
    }

    /// The selected text, if any text is selected.
    pub fn selection(&self) -> Option<String> {
        self.backend.selection()
    }

    /// ID of the process started in the terminal.
    ///
    /// Only known on Unix.
    pub fn child_pid(&self) -> Option<u32> {
        self.backend.child_pid()
    }

    /// The process group in the foreground of the terminal.
    ///
    /// While the shell waits at its prompt this is the shell's own group,
    /// which has the same number as [`Self::child_pid`]; while a command runs
    /// it is the command's. Only known on Unix.
    pub fn foreground_process_group(&self) -> Option<u32> {
        self.backend.foreground_process_group()
    }

    pub fn handle(&mut self, cmd: Command) -> Action {
        let mut action = Action::default();

        match cmd {
            Command::ChangeTheme(color_pallete) => {
                self.theme = Theme::new(ThemeSettings::new(color_pallete));
                self.backend.set_color_scheme(self.theme.color_scheme());
            },
            Command::ChangeFont(font_settings) => {
                self.font = TermFont::new(font_settings);
            },
            Command::AddBindings(bindings) => {
                self.bindings.add_bindings(bindings);
            },
            Command::ProxyToBackend(
                backend::Command::ProcessAlacrittyEvent(
                    AlacrittyEvent::ColorRequest(index, formatter),
                ),
            ) => {
                // The reply depends on the theme, which the backend doesn't
                // own, so it is answered here.
                self.backend.answer_color_request(
                    index,
                    &*formatter,
                    &self.theme,
                );
            },
            Command::ProxyToBackend(cmd) => {
                action = self.backend.handle(cmd);
            },
        };

        self.sync_and_redraw();
        action
    }

    fn sync_and_redraw(&mut self) {
        self.sync_font();
        self.backend.sync();
        self.redraw();
    }

    fn sync_font(&mut self) {
        self.font.sync();
        self.backend
            .handle(backend::Command::Resize(None, Some(self.font.measure)));
    }

    fn redraw(&mut self) {
        self.cache.clear();
    }
}

#[derive(Clone)]
struct TerminalSubscriptionData {
    id: u64,
    event_receiver: Arc<Mutex<backend::EventReceiver>>,
}

impl Hash for TerminalSubscriptionData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

fn terminal_subscription_stream(
    data: &TerminalSubscriptionData,
) -> BoxStream<'static, Event> {
    let id = data.id;
    let event_receiver = data.event_receiver.clone();
    iced::stream::channel(1000, async move |mut output| {
        let mut shutdown = false;
        loop {
            let mut event_receiver = event_receiver.lock().await;
            match event_receiver.recv().await {
                Some(event) => {
                    if let AlacrittyEvent::Exit = event {
                        shutdown = true
                    };

                    output
                        .send(Event::BackendCall(id, backend::Command::ProcessAlacrittyEvent(event)))
                        .await
                        .unwrap_or_else(|_| {
                            panic!("iced_tterm stream {}: sending BackendEventReceived event is failed", id)
                        });
                },
                None => {
                    if !shutdown {
                        panic!("iced_tterm stream {}: terminal event channel closed unexpected", id);
                    }
                },
            }
        }
    })
    .boxed()
}
