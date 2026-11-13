use crate::actions::Action;
use crate::backend;
use crate::bindings::{Binding, BindingAction, BindingsLayout, InputKind};
use crate::cursor::Cursor;
use crate::font::TermFont;
use crate::settings::{
    CursorSettings, FontSettings, MouseSettings, ScrollSettings,
    SelectionSettings, Settings, ThemeSettings,
};
use crate::theme::{ColorPalette, Theme};
use crate::{AlacrittyEvent, CursorShape, TermMode};
use iced::futures::stream::BoxStream;
use iced::futures::{SinkExt, StreamExt};
use iced::widget::canvas::Cache;
use iced::Subscription;
use std::hash::{Hash, Hasher};
use std::io::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tankterm_terminal::vte::ansi::NamedColor;
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
    /// Change the colors. The opacity of the background is kept.
    ChangeTheme(Box<ColorPalette>),
    /// Change how opaque the widget draws its background, from `0.0` to
    /// `1.0`.
    ChangeBackgroundOpacity(f32),
    ChangeFont(FontSettings),
    /// Change the size of the scrollback buffer and how far the mouse wheel
    /// scrolls. Lines the smaller buffer has no room for are dropped.
    ChangeScroll(ScrollSettings),
    /// Change how the cursor looks and blinks. A shape the application in
    /// the terminal asked for is kept.
    ChangeCursor(CursorSettings),
    /// Change how text is selected; a selection that is there is kept.
    ChangeSelection(SelectionSettings),
    /// Change how the mouse pointer behaves.
    ChangeMouse(MouseSettings),
    /// Change whether the grid is centred in the widget; see
    /// [`Settings::dynamic_padding`].
    ChangeDynamicPadding(bool),
    /// Tell the terminal whether it has the keyboard focus. Without it the
    /// cursor doesn't blink and is drawn hollow, if the settings say so. A
    /// new terminal has the focus.
    Focus(bool),
    AddBindings(Vec<(Binding<InputKind>, BindingAction)>),
    /// Take the widget's own keys for copying and pasting away, for
    /// applications that bind these actions themselves. The keys are sent
    /// to the terminal like any other key; the mouse bindings are kept.
    RemoveClipboardKeys,
    ProxyToBackend(backend::Command),
}

pub struct Terminal {
    pub id: u64,
    widget_id: iced::widget::Id,
    pub(crate) font: TermFont,
    pub(crate) theme: Theme,
    pub(crate) cache: Cache,
    pub(crate) bindings: BindingsLayout,
    /// Lines scrolled for every step of the mouse wheel.
    pub(crate) scroll_multiplier: f32,
    pub(crate) cursor: Cursor,
    pub(crate) mouse: MouseSettings,
    /// Centre the grid in the widget.
    pub(crate) dynamic_padding: bool,
    pub(crate) backend: backend::Backend,
    backend_event_rx: Arc<Mutex<backend::EventReceiver>>,
}

impl Terminal {
    pub fn new(id: u64, settings: Settings) -> Result<Self> {
        let (backend_event_tx, backend_event_rx) = backend::event_channel(100);
        let theme = Theme::new(settings.theme);
        let font = TermFont::new(settings.font);
        let cursor = Cursor::new(settings.cursor, Instant::now());
        let mut backend = backend::Backend::new(
            id,
            backend_event_tx,
            settings.backend,
            theme.color_scheme(),
            settings.scroll.history,
            cursor.default_style(),
        )?;
        backend.set_semantic_escape_chars(
            &settings.selection.semantic_escape_chars,
        );

        Ok(Self {
            id,
            widget_id: iced::widget::Id::unique(),
            font,
            theme,
            bindings: BindingsLayout::default(),
            scroll_multiplier: settings.scroll.multiplier,
            cursor,
            mouse: settings.mouse,
            dynamic_padding: settings.dynamic_padding,
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
        let events = Subscription::run_with(data, terminal_subscription_stream);

        if !self.cursor_blinks() {
            return events;
        }

        // The timer only runs while the cursor blinks, so a terminal with a
        // steady cursor is never woken up for it.
        let blink = BlinkSubscriptionData {
            id: self.id,
            interval: self.cursor.settings.blink_interval,
        };
        Subscription::batch([
            events,
            Subscription::run_with(blink, blink_subscription_stream),
        ])
    }

    /// The shape the cursor is drawn with right now:
    /// [`CursorShape::Hidden`] while the application in the terminal hides
    /// it and in the dark half of a blink, [`CursorShape::HollowBlock`]
    /// without the keyboard focus, and otherwise the shape the application
    /// asked for or the one from the settings.
    pub fn cursor_shape(&self) -> CursorShape {
        let content = self.backend.renderable_content();
        let shown = content.terminal_mode.contains(TermMode::SHOW_CURSOR);
        self.cursor.shape(content.cursor_style, shown)
    }

    /// Whether the cursor is blinking: it is set to, by the settings or the
    /// application in the terminal, the terminal has the keyboard focus and
    /// the blink timeout hasn't passed since something was typed.
    pub fn cursor_blinks(&self) -> bool {
        self.cursor
            .blinks(self.backend.renderable_content().cursor_style)
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

    /// The background color the application in the terminal set (OSC 11),
    /// if it set one. Without one the background is the palette's.
    pub fn background_override(&self) -> Option<iced::Color> {
        let colors = &self.backend.renderable_content().colors;
        colors[NamedColor::Background]
            .map(|rgb| iced::Color::from_rgb8(rgb.r, rgb.g, rgb.b))
    }

    /// How many lines the terminal's screen has, which is a page for
    /// scrolling.
    pub fn screen_lines(&self) -> usize {
        self.backend.screen_lines()
    }

    /// How many lines the view is scrolled back into the scrollback buffer;
    /// `0` while it shows the screen.
    pub fn display_offset(&self) -> usize {
        self.backend.renderable_content().grid.display_offset()
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
                self.theme = Theme::new(ThemeSettings {
                    color_pallete,
                    background_opacity: self.theme.background_opacity(),
                });
                self.backend.set_color_scheme(self.theme.color_scheme());
            },
            Command::ChangeBackgroundOpacity(opacity) => {
                self.theme.set_background_opacity(opacity);
            },
            Command::ChangeFont(font_settings) => {
                self.font = TermFont::new(font_settings);
            },
            Command::ChangeScroll(scroll) => {
                self.scroll_multiplier = scroll.multiplier;
                self.backend.set_scrollback(scroll.history);
            },
            Command::ChangeCursor(settings) => {
                self.cursor.settings = settings;
                self.cursor.restart(Instant::now());
                self.backend
                    .set_default_cursor_style(self.cursor.default_style());
            },
            Command::ChangeSelection(selection) => {
                self.backend.set_semantic_escape_chars(
                    &selection.semantic_escape_chars,
                );
            },
            Command::ChangeMouse(mouse) => self.mouse = mouse,
            Command::ChangeDynamicPadding(dynamic) => {
                self.dynamic_padding = dynamic;
            },
            Command::Focus(focused) => {
                self.cursor.set_focused(focused, Instant::now());
            },
            Command::ProxyToBackend(backend::Command::BlinkCursor) => {
                let shape = self.cursor_shape();
                let style = self.backend.renderable_content().cursor_style;
                self.cursor.blink(style, Instant::now());

                // A blink of a cursor the application hides, or one that
                // stays lit while something is typed, changes nothing.
                if self.cursor_shape() == shape {
                    return action;
                }
            },
            Command::AddBindings(bindings) => {
                self.bindings.add_bindings(bindings);
            },
            Command::RemoveClipboardKeys => {
                self.bindings.remove_clipboard_keys();
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
                match &cmd {
                    backend::Command::Write(_) | backend::Command::Paste(_) => {
                        self.cursor.typed(Instant::now());
                    },
                    // The application changed whether the cursor blinks.
                    backend::Command::ProcessAlacrittyEvent(
                        AlacrittyEvent::CursorBlinkingChange,
                    ) => self.cursor.restart(Instant::now()),
                    _ => {},
                }

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

    /// Tell the backend the cell size of the font, which `ChangeFont`
    /// measured.
    fn sync_font(&mut self) {
        self.backend
            .handle(backend::Command::Resize(None, Some(self.font.measure)));
    }

    fn redraw(&mut self) {
        self.cache.clear();
    }
}

/// Identifies the timer of a terminal's blinking cursor.
#[derive(Clone, Hash)]
struct BlinkSubscriptionData {
    id: u64,
    interval: Duration,
}

/// An event every blink interval, from a thread of its own so that no timer
/// of an async runtime is needed.
fn blink_subscription_stream(
    data: &BlinkSubscriptionData,
) -> BoxStream<'static, Event> {
    let (id, interval) = (data.id, data.interval);
    let (mut sender, receiver) = iced::futures::channel::mpsc::channel(1);

    std::thread::spawn(move || loop {
        std::thread::sleep(interval);
        let blink = Event::BackendCall(id, backend::Command::BlinkCursor);
        // A full channel means the application is behind and doesn't need
        // another blink; a closed one that the cursor stopped blinking.
        if sender
            .try_send(blink)
            .is_err_and(|err| err.is_disconnected())
        {
            break;
        }
    });

    receiver.boxed()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blink_timer_sends_a_blink_every_interval() {
        let data = BlinkSubscriptionData {
            id: 7,
            interval: Duration::from_millis(5),
        };
        let mut blinks = blink_subscription_stream(&data);

        for _ in 0..3 {
            let blink = iced::futures::executor::block_on(blinks.next());
            assert!(matches!(
                blink,
                Some(Event::BackendCall(7, backend::Command::BlinkCursor))
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn cursor_settings_and_focus_change_for_a_running_terminal() {
        use crate::settings::{BackendSettings, CursorBlinking};

        let settings = Settings {
            backend: BackendSettings {
                program: "cat".into(),
                ..BackendSettings::default()
            },
            ..Settings::default()
        };
        let mut terminal = Terminal::new(0, settings).unwrap();
        assert_eq!(terminal.cursor_shape(), CursorShape::Block);
        assert!(!terminal.cursor_blinks());

        terminal.handle(Command::ChangeCursor(CursorSettings {
            shape: CursorShape::Underline,
            blinking: CursorBlinking::On,
            ..CursorSettings::default()
        }));
        assert_eq!(terminal.cursor_shape(), CursorShape::Underline);
        assert!(terminal.cursor_blinks());

        let blink = Command::ProxyToBackend(backend::Command::BlinkCursor);
        terminal.handle(blink.clone());
        assert_eq!(terminal.cursor_shape(), CursorShape::Hidden);
        terminal.handle(blink);
        assert_eq!(terminal.cursor_shape(), CursorShape::Underline);

        terminal.handle(Command::Focus(false));
        assert_eq!(terminal.cursor_shape(), CursorShape::HollowBlock);
        assert!(!terminal.cursor_blinks());
        terminal.handle(Command::Focus(true));
        assert_eq!(terminal.cursor_shape(), CursorShape::Underline);
        assert!(terminal.cursor_blinks());
    }

    #[cfg(unix)]
    #[test]
    fn background_opacity_outlives_a_new_palette() {
        use crate::settings::BackendSettings;
        use crate::theme::TerminalStyle;
        use tankterm_terminal::term::color::Colors;

        let settings = Settings {
            backend: BackendSettings {
                program: "cat".into(),
                ..BackendSettings::default()
            },
            ..Settings::default()
        };
        let mut terminal = Terminal::new(0, settings).unwrap();
        let alpha = |terminal: &Terminal| {
            let style = terminal.theme.container_style(&Colors::default());
            style.background.map(|background| match background {
                iced::Background::Color(color) => (color.r, color.a),
                background => panic!("unexpected {background:?}"),
            })
        };
        assert_eq!(alpha(&terminal).map(|(_, alpha)| alpha), Some(1.0));
        assert_eq!(terminal.background_override(), None);

        terminal.handle(Command::ChangeBackgroundOpacity(0.25));
        assert_eq!(alpha(&terminal).map(|(_, alpha)| alpha), Some(0.25));

        let white = ColorPalette {
            background: "#ffffff".into(),
            ..ColorPalette::default()
        };
        terminal.handle(Command::ChangeTheme(Box::new(white)));
        assert_eq!(alpha(&terminal), Some((1.0, 0.25)));
    }
}
