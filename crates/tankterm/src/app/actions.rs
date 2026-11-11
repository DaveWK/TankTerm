//! Actions on the focused terminal and the window: copying and pasting, the font size, scrolling
//! and fullscreen.

use iced::{Task, window};
use iced_core::clipboard::Kind;
use iced_tterm::settings::FontSettings;
use iced_tterm::{BackendCommand, Scroll};

use super::{App, Message, Shell};
use crate::hotkeys::ScrollTo;

/// How much a font size action changes the font size by, in logical pixels.
pub(super) const FONT_SIZE_STEP: f32 = 1.;

/// Smallest font size the font size actions go down to, in logical pixels.
///
/// The terminal widget draws every cell by itself, so a window full of cells of a pixel or two
/// would take it seconds to draw.
const MIN_FONT_SIZE: f32 = 4.;

impl App {
    /// ID of the terminal that gets the keyboard input: the one in the focused pane of the tab
    /// that is shown.
    pub(super) fn focused_terminal(&self) -> Option<u64> {
        self.tabs.get(self.active).and_then(|tab| tab.panes.get(tab.focus)).copied()
    }

    /// The terminal that gets the keyboard input.
    pub(super) fn focused_shell(&self) -> Option<&Shell> {
        self.focused_terminal().and_then(|id| self.terminals.get(&id))
    }

    /// Put the text selected in the focused terminal on the clipboard.
    pub(super) fn copy(&self) -> Task<Message> {
        match self.focused_shell().and_then(|shell| shell.terminal.selection()) {
            Some(text) => iced::clipboard::write(text),
            None => Task::none(),
        }
    }

    /// Paste a clipboard into the focused terminal.
    ///
    /// Where there is no selection clipboard, the regular one stands in for it. The text takes
    /// the way of every paste, so the paste warning gets to ask about it.
    pub(super) fn paste_clipboard(&self, kind: Kind) -> Task<Message> {
        let Some(terminal) = self.focused_terminal() else { return Task::none() };

        let text = match kind {
            Kind::Standard => iced::clipboard::read(),
            Kind::Primary => iced::clipboard::read_primary().then(|text| match text {
                Some(text) => Task::done(Some(text)),
                None => iced::clipboard::read(),
            }),
        };

        text.and_then(move |text| {
            if text.is_empty() {
                return Task::none();
            }

            let paste = BackendCommand::Paste(text);
            Task::done(Message::Terminal(iced_tterm::Event::BackendCall(terminal, paste)))
        })
    }

    /// Change the font size of every terminal by `step` pixels, or back to the configured size
    /// without a step.
    pub(super) fn change_font_size(&mut self, step: Option<f32>) {
        let smallest = MIN_FONT_SIZE - self.settings.font.size.round();
        self.font_zoom = step.map_or(0., |step| (self.font_zoom + step).max(smallest.min(0.)));

        let font = zoomed(self.settings.font.clone(), self.font_zoom);
        for shell in self.terminals.values_mut() {
            shell.terminal.handle(iced_tterm::Command::ChangeFont(font.clone()));
        }
    }

    /// Scroll the focused terminal through its scrollback buffer.
    pub(super) fn scroll(&mut self, to: ScrollTo) {
        let Some(shell) = self.focused_terminal().and_then(|id| self.terminals.get_mut(&id)) else {
            return;
        };

        let half_page = (shell.terminal.screen_lines() / 2) as i32;
        let scroll = match to {
            ScrollTo::PageUp => Scroll::PageUp,
            ScrollTo::PageDown => Scroll::PageDown,
            ScrollTo::HalfPageUp => Scroll::Delta(half_page),
            ScrollTo::HalfPageDown => Scroll::Delta(-half_page),
            ScrollTo::LineUp => Scroll::Delta(1),
            ScrollTo::LineDown => Scroll::Delta(-1),
            ScrollTo::Top => Scroll::Top,
            ScrollTo::Bottom => Scroll::Bottom,
        };
        let scroll = BackendCommand::ScrollDisplay(scroll);
        shell.terminal.handle(iced_tterm::Command::ProxyToBackend(scroll));
    }

    /// Drop the scrollback buffer of the focused terminal.
    pub(super) fn clear_history(&mut self) {
        if let Some(shell) = self.focused_terminal().and_then(|id| self.terminals.get_mut(&id)) {
            let clear = BackendCommand::ClearHistory;
            shell.terminal.handle(iced_tterm::Command::ProxyToBackend(clear));
        }
    }
}

/// A font at the size the font size actions left it at, `zoom` pixels from the configured one.
///
/// The size changes in whole pixels from the rounded size, which fonts look best at.
pub(super) fn zoomed(mut font: FontSettings, zoom: f32) -> FontSettings {
    if zoom != 0. {
        font.size = (font.size.round() + zoom).max(MIN_FONT_SIZE);
    }

    font
}

/// Make the window fullscreen, or a regular window again if it is.
pub(super) fn toggle_fullscreen() -> Task<Message> {
    window::latest().and_then(|id| {
        window::mode(id).then(move |mode| {
            let mode = match mode {
                window::Mode::Fullscreen => window::Mode::Windowed,
                _ => window::Mode::Fullscreen,
            };
            window::set_mode(id, mode)
        })
    })
}

/// Maximize the window, or give it its size back if it is.
pub(super) fn toggle_maximized() -> Task<Message> {
    window::latest().and_then(window::toggle_maximize)
}
