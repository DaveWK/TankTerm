//! The iced application hosting the terminals.
//!
//! A window holds tabs, and every tab holds one or more terminals in split panes. Tabs and panes
//! are driven by the key bindings in [`crate::hotkeys`] and by the tab bar.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use iced::widget::pane_grid;
use iced::{Color, Padding};
use iced_tterm::bindings::{Binding, BindingAction, InputKind};
use iced_tterm::settings::Settings;

use crate::config::tabs::Tabs;
use crate::hotkeys::{AppAction, Hotkeys};
use crate::palette::Palette;
use crate::session::Session;

#[cfg(all(test, unix))]
mod layout_tests;
#[cfg(all(test, unix))]
mod render_tests;
#[cfg(all(test, unix))]
mod tab_tests;
mod tabs;
mod update;
mod view;

/// Share of the tab a divider moves by when a pane is resized with the keyboard.
const RESIZE_STEP: f32 = 0.05;

/// Smallest and largest share of a split its first pane can be resized to with the keyboard.
const RESIZE_LIMITS: (f32, f32) = (0.1, 0.9);

/// Widest a tab gets in the tab bar, in logical pixels.
const MAX_TAB_WIDTH: f32 = 240.;

/// How often the shells are looked at for finished commands and changed directories.
const PROCESS_INTERVAL: Duration = Duration::from_secs(1);

/// Messages handled by the application.
#[derive(Debug, Clone)]
pub enum Message {
    Terminal(iced_tterm::Event),
    /// A tab or pane action, from a key binding or the tab bar.
    Action(AppAction),
    /// Close the tab at this position.
    CloseTab(usize),
    PaneClicked(pane_grid::Pane),
    PaneResized(pane_grid::ResizeEvent),
    /// The window was opened or gained focus (`true`), or lost focus (`false`).
    WindowFocused(bool),
    /// Text typed into the command palette or profile selector.
    PaletteQuery(String),
    /// Run the selected item of the command palette or profile selector.
    PaletteSubmit,
    /// Run the item at this position among the matching ones.
    PaletteChoose(usize),
    /// Time to look at the shells again.
    Tick,
}

/// Settings of the application that don't change while it runs.
pub struct AppOptions {
    /// Window title, used until a terminal sets its own.
    pub title: String,
    /// Whether terminals can change the window and tab titles.
    pub dynamic_title: bool,
    pub layout: Layout,
    pub tabs: Tabs,
    pub hotkeys: Hotkeys,
    /// File the tabs and panes are remembered in, if they should be.
    pub session_file: Option<PathBuf>,
    /// Profiles offered by the profile selector.
    pub profiles: Vec<Profile>,
    /// Ask before pasting text with line breaks into a program without bracketed paste.
    pub confirm_multiline_paste: bool,
    /// Copy selected text to the clipboard, not only to the selection clipboard.
    pub copy_on_select: bool,
    /// Bindings added to every terminal: what the middle and right mouse buttons do.
    pub bindings: Vec<(Binding<InputKind>, BindingAction)>,
}

/// A configured shell profile, ready to be opened in a tab.
pub struct Profile {
    pub name: String,
    /// The profile's command line, shown next to its name.
    pub detail: String,
    pub settings: Settings,
    /// Color shown in the padding around the profile's terminals.
    pub background: Color,
}

/// Application state: a window with tabs of terminals in split panes.
pub struct App {
    options: AppOptions,
    /// Settings for the terminals of new tabs and panes.
    settings: Settings,
    tabs: Vec<Tab>,
    /// Position of the tab that is shown.
    active: usize,
    terminals: HashMap<u64, Shell>,
    /// ID for the next terminal; `iced_tterm` uses it to route backend events.
    next_terminal: u64,
    next_tab: u64,
    /// Whether the window has focus; a bell in an unfocused window asks for attention.
    window_focused: bool,
    /// The session as it was last written to `session_file`.
    saved_session: Option<Session>,
    /// Why the session couldn't be written, while a write that failed waits to be tried again.
    session_error: Option<String>,
    /// The command palette or profile selector, while one is shown.
    palette: Option<Palette>,
    /// Keys that work while the palette is shown, in place of the configured ones.
    palette_keys: Hotkeys,
    /// A paste that is held back until the paste warning is answered.
    pending_paste: Option<PendingPaste>,
    /// Keys that answer the paste warning.
    paste_keys: Hotkeys,
}

/// Text that was pasted into a terminal but not sent to it yet.
struct PendingPaste {
    terminal: u64,
    text: String,
}

/// A tab: terminals arranged in split panes.
struct Tab {
    /// Identifies the tab's widgets, so switching tabs doesn't mix up their state.
    id: u64,
    /// The split layout; every pane holds the ID of its terminal.
    panes: pane_grid::State<u64>,
    /// Pane that receives keyboard input while the tab is shown.
    focus: pane_grid::Pane,
    /// Whether the tab was shown before; the first output of a shell isn't activity.
    seen: bool,
    /// A terminal of the tab printed something since the tab was last shown.
    activity: bool,
}

/// A terminal and what the application tracks about it.
struct Shell {
    terminal: iced_tterm::Terminal,
    /// Title set by the terminal's application, if any.
    title: Option<String>,
    /// Color shown in the padding around the terminal.
    background: Color,
    /// Whether a command was running in the shell when it was last looked at.
    busy: bool,
    /// Directory the shell was started in or was in at the last tick, where that can be told.
    /// Only kept up to date while there is a session file to remember it in.
    working_directory: Option<PathBuf>,
    /// The terminal rang the bell or finished a command while nobody was looking at it.
    attention: bool,
}

/// How terminals are placed in the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// Space around every terminal, in logical pixels.
    pub padding: Padding,
    /// Color shown in the padding.
    pub background: Color,
    /// Text color, used for the tab bar and pane frames.
    pub foreground: Color,
}

/// The directory a process is in, where the platform has a simple way to ask.
#[cfg(target_os = "linux")]
fn working_directory(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok().filter(|directory| directory.is_dir())
}

#[cfg(not(target_os = "linux"))]
fn working_directory(_pid: u32) -> Option<PathBuf> {
    None
}
