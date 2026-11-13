//! The iced application hosting the terminals.
//!
//! A window holds tabs, and every tab holds one or more terminals in split panes. Tabs, panes
//! and the focused terminal are driven by the key bindings in [`crate::hotkeys`], tabs also by
//! the tab bar.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use iced::widget::pane_grid;
use iced::{Color, Padding};
use iced_tterm::bindings::{Binding, BindingAction, InputKind};
use iced_tterm::settings::Settings;

use crate::bell::Bell;
use crate::config::style::TabBarStyles;
use crate::config::tabs::Tabs;
use crate::config::window::WindowButtons;
use crate::hotkeys::{ActiveTable, AppAction, Hotkeys, PaneSelectMode};
use crate::palette::Palette;
use crate::session::Session;
use crate::setup::ConfigSource;

mod actions;
#[cfg(unix)]
mod ipc;
#[cfg(all(test, unix))]
mod ipc_tests;
#[cfg(all(test, unix))]
mod key_table_tests;
#[cfg(all(test, unix))]
mod layout_tests;
#[cfg(all(test, unix))]
mod option_tests;
#[cfg(all(test, unix))]
mod reload_tests;
#[cfg(all(test, unix))]
mod render_tests;
pub mod status;
mod tab_strip;
#[cfg(all(test, unix))]
mod tab_tests;
mod tabs;
mod update;
mod view;

/// Environment variable with the ID of the pane a shell runs in, for `tankTerm msg`.
pub const PANE_ID_ENV: &str = "TANKTERM_PANE_ID";

/// Share of the tab a divider moves by when a pane is resized with the keyboard.
const RESIZE_STEP: f32 = 0.05;

/// Smallest and largest share of a split its first pane can be resized to with the keyboard.
const RESIZE_LIMITS: (f32, f32) = (0.1, 0.9);

/// Widest a tab gets in the tab bar, in logical pixels.
const MAX_TAB_WIDTH: f32 = 240.;

/// How far the mouse moves with the button down on the tab bar before the window moves.
const DRAG_THRESHOLD: f32 = 4.;

/// How often the shells are looked at for finished commands and changed directories.
const PROCESS_INTERVAL: Duration = Duration::from_secs(1);

/// Messages handled by the application.
#[derive(Debug, Clone)]
pub enum Message {
    Terminal(iced_tterm::Event),
    /// An action, from a key binding, the tab bar or the command palette.
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
    /// The left button was pressed on empty space of the tab bar.
    TitleBarPressed,
    /// The mouse moved over the tab bar, to this position in it, with the button pressed.
    TitleBarMoved(iced::Point),
    /// The left button was released, or the mouse left the tab bar.
    TitleBarReleased,
    /// Empty space of the tab bar was double-clicked.
    TitleBarDoubleClicked,
    /// Time to look at the shells again.
    Tick,
    /// A frame is about to be drawn while a visual bell flashes.
    Frame(Instant),
    /// A configuration file changed.
    ConfigChanged,
    /// A message from `tankTerm msg`.
    #[cfg(unix)]
    Ipc(crate::ipc::Request),
}

/// Settings of the application, which only change when the configuration is read again.
pub struct AppOptions {
    /// Window title, used until a terminal sets its own.
    pub title: String,
    /// Whether terminals can change the window and tab titles.
    pub dynamic_title: bool,
    pub layout: Layout,
    pub tabs: Tabs,
    /// The segments at the end of the tab bar.
    pub status: Vec<status::Segment>,
    /// Whether the window was opened without the system's title bar and borders.
    pub frameless: bool,
    /// Where the tab bar has the window's buttons in a frameless window.
    pub window_buttons: WindowButtons,
    pub hotkeys: Hotkeys,
    /// The configuration file, or where a new one goes, for `OpenConfig`.
    pub config_file: Option<PathBuf>,
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
    /// What happens when a terminal rings the bell.
    pub bell: Bell,
    /// How opaque the background of the terminals is, from 0 to 1.
    pub opacity: f32,
}

/// A configured shell profile, ready to be opened in a tab.
pub struct Profile {
    pub name: String,
    /// The profile's command line, shown next to its name.
    pub detail: String,
    pub settings: Settings,
    /// Color drawn behind the profile's terminals and their padding.
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
    /// How far the font size actions moved the font size of the terminals from the configured
    /// one, in logical pixels.
    font_zoom: f32,
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
    /// The key tables that were activated and not left yet, the last one activated at the end.
    key_tables: Vec<ActiveTable>,
    /// The panes of the shown tab are labeled, while one is chosen by typing its label.
    pane_select: Option<PaneSelect>,
    /// The keys of the pane labels, and `Escape`.
    pane_keys: Hotkeys,
    /// Where the configuration comes from, while its files are watched for changes.
    config: Option<ConfigSource>,
    /// Whether the window was opened transparent, which it is for the rest of its life.
    transparent: bool,
    /// Whether the window was opened without the system's title bar, which it is for the rest
    /// of its life.
    frameless: bool,
    /// The left button was pressed on empty space of the tab bar, while it is the title bar.
    title_drag: TitleDrag,
    /// The socket `tankTerm msg` talks to, while there is one.
    #[cfg(unix)]
    socket: Option<crate::ipc::Listener>,
    /// The time at the last tick, while a clock segment shows it.
    now: Option<jiff::Zoned>,
    /// The batteries at the last tick, while a battery segment shows them.
    battery: Option<status::Battery>,
}

/// Dragging the window by the tab bar. The window moves once the mouse has moved a few
/// pixels with the button down, so a double click doesn't start a move.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum TitleDrag {
    #[default]
    Released,
    /// Pressed, before the mouse moved.
    Pressed,
    /// Pressed, and the mouse was here when it first moved.
    Moved(iced::Point),
}

/// Choosing a pane of a tab by typing its label.
struct PaneSelect {
    mode: PaneSelectMode,
    /// ID of the tab whose panes are labeled; choosing ends when another tab is shown.
    tab: u64,
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
    /// Title given to the tab with `tankTerm msg set-tab-title`, shown in place of the title
    /// of its focused pane.
    title: Option<String>,
}

/// A terminal and what the application tracks about it.
struct Shell {
    terminal: iced_tterm::Terminal,
    /// Title set by the terminal's application, if any.
    title: Option<String>,
    /// Color drawn behind the terminal and its padding, until its application sets another.
    background: Color,
    /// Whether a command was running in the shell when it was last looked at.
    busy: bool,
    /// The shell marks where its commands start and end (OSC 133), so its process isn't
    /// watched for that.
    marks_commands: bool,
    /// Directory the shell reported last (OSC 7).
    reported_directory: Option<PathBuf>,
    /// Directory the shell was started in, was in at the last tick or reported last, where that
    /// can be told. Only kept up to date from ticks while there is a session file to remember
    /// it in or a status segment to show it.
    working_directory: Option<PathBuf>,
    /// Name of the program that had the terminal at the last tick, where that can be told.
    /// Only kept up to date while the tab title shows it.
    process: Option<String>,
    /// The terminal rang the bell or finished a command while nobody was looking at it.
    attention: bool,
    /// Whether the terminal was last told that it has the keyboard focus.
    focused: bool,
    /// When the terminal rang the bell, while the visual bell flashes for it.
    rang: Option<Instant>,
    /// Name of the profile the terminal was opened from, if it was.
    profile: Option<String>,
    /// Keep the pane open when the program exits (`--hold`).
    hold: bool,
}

/// How terminals are placed in the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// Space around every terminal, in logical pixels.
    pub padding: Padding,
    /// Color drawn behind the terminals and their padding.
    pub background: Color,
    /// Text color, used for the pane frames.
    pub foreground: Color,
    /// Styles of the tab bar, in a focused and an unfocused window.
    pub tab_bar: [TabBarStyles; 2],
}

impl Shell {
    /// The directory the shell is in: the one it reported, if it still exists, or else the one
    /// its process is in, where the platform has a simple way to ask.
    fn directory(&self) -> Option<PathBuf> {
        let reported = self.reported_directory.as_ref().filter(|directory| directory.is_dir());
        reported.cloned().or_else(|| self.terminal.child_pid().and_then(working_directory))
    }
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

/// The name of a process, where the platform has a simple way to ask.
#[cfg(target_os = "linux")]
fn process_name(pid: u32) -> Option<String> {
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    Some(name.trim_end().to_owned()).filter(|name| !name.is_empty())
}

#[cfg(not(target_os = "linux"))]
fn process_name(_pid: u32) -> Option<String> {
    None
}
