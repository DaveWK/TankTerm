//! The iced application hosting the terminals.
//!
//! A window holds tabs, and every tab holds one or more terminals in split panes. Tabs and panes
//! are driven by the key bindings in [`crate::hotkeys`] and by the tab bar.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use iced::widget::pane_grid::{self, Direction, Node, PaneGrid};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, keyed_column, mouse_area, row, stack, text};
use iced::{Color, Element, Length, Padding, Subscription, Task, window};
use iced_tterm::actions::Action;
use iced_tterm::bindings::{Binding, BindingAction, InputKind};
use iced_tterm::settings::Settings;
use iced_tterm::{AlacrittyEvent, BackendCommand, TermMode, TerminalView};
use log::{error, warn};

use crate::config::tabs::{ShowTabs, Tabs, TabsPosition};
use crate::hotkeys::{AppAction, Hotkeys, KeyFilter};
use crate::palette::{self, Palette, blend, paste_warning};
use crate::session::{SavedNode, SavedTab, Session, ordered_panes};

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
    /// Directory the shell was in when it was last looked at, where that can be told.
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

impl App {
    /// Create the first terminal with `first` and spawn its shell.
    ///
    /// Terminals of further tabs and panes are created with `settings`. If the options name a
    /// session file with a usable session, its tabs and panes are opened instead.
    pub fn new(options: AppOptions, first: Settings, settings: Settings) -> io::Result<Self> {
        let mut app = Self {
            options,
            settings,
            tabs: Vec::new(),
            active: 0,
            terminals: HashMap::new(),
            next_terminal: 0,
            next_tab: 0,
            window_focused: true,
            saved_session: None,
            palette: None,
            palette_keys: Hotkeys::palette(),
            pending_paste: None,
            paste_keys: Hotkeys::paste_warning(),
        };

        let session = app.options.session_file.as_deref().and_then(Session::load);
        if let Some(session) = session {
            app.restore(&session);
            app.saved_session = Some(session);
        }

        if app.tabs.is_empty() {
            let terminal = app.spawn(first, app.options.layout.background)?;
            app.push_tab(terminal);
        }
        app.shown();

        Ok(app)
    }

    /// Open the tabs and panes of a session, as far as shells can be started for them.
    fn restore(&mut self, session: &Session) {
        for saved in &session.tabs {
            let mut spawn = |directory: Option<&Path>| {
                let mut settings = self.settings.clone();
                // A directory that is gone since would keep the shell from starting.
                if let Some(directory) = directory.filter(|directory| directory.is_dir()) {
                    settings.backend.working_directory = Some(directory.to_owned());
                }

                self.spawn(settings, self.options.layout.background)
            };
            let configuration = match saved.layout.configuration(&mut spawn) {
                Ok(configuration) => configuration,
                Err(err) => {
                    error!("Unable to restore a tab: {err}");
                    break;
                },
            };

            let panes = pane_grid::State::with_configuration(configuration);
            let order = ordered_panes(panes.layout());
            let focus = order[saved.focus.min(order.len() - 1)];
            self.add_tab(panes, focus);
        }

        // Shells started for a tab that couldn't be completed aren't shown anywhere.
        let shown: Vec<u64> =
            self.tabs.iter().flat_map(|tab| tab.panes.iter().map(|(_, id)| *id)).collect();
        self.terminals.retain(|id, _| shown.contains(id));

        self.active = session.active.min(self.tabs.len().saturating_sub(1));
    }

    /// The tabs and panes as they are stored in the session file.
    fn session(&self) -> Session {
        let tabs = self.tabs.iter().map(|tab| {
            let order = ordered_panes(tab.panes.layout());
            SavedTab {
                layout: SavedNode::from_layout(tab.panes.layout(), &|pane| {
                    let shell = tab.panes.get(pane).and_then(|id| self.terminals.get(id));
                    shell.and_then(|shell| shell.working_directory.clone())
                }),
                focus: order.iter().position(|pane| *pane == tab.focus).unwrap_or(0),
            }
        });

        Session { active: self.active, tabs: tabs.collect() }
    }

    /// Write the session file if the tabs or panes changed since it was last written.
    ///
    /// The last tab closing isn't recorded, so closing a window by exiting its last shell
    /// remembers that last tab.
    fn save_session(&mut self) {
        let Some(path) = &self.options.session_file else { return };
        if self.tabs.is_empty() {
            return;
        }

        let session = self.session();
        if self.saved_session.as_ref() == Some(&session) {
            return;
        }

        if let Err(err) = session.save(path) {
            warn!("Unable to save the session to {}: {err}", path.display());
        }
        self.saved_session = Some(session);
    }

    /// Mark the tab that is shown as seen, clearing its activity marker, and the pane that has
    /// focus as looked at, if the window has focus too.
    fn shown(&mut self) {
        let Some(tab) = self.tabs.get_mut(self.active) else { return };
        tab.seen = true;
        tab.activity = false;

        let focused = tab.panes.get(tab.focus).and_then(|id| self.terminals.get_mut(id));
        if let Some(shell) = focused.filter(|_| self.window_focused) {
            shell.attention = false;
        }
    }

    pub fn title(&self) -> String {
        let title = self.tabs.get(self.active).map(|tab| self.tab_title(tab));
        title.unwrap_or(&self.options.title).to_owned()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let terminals = self.terminals.values();
        let terminals = terminals.map(|shell| shell.terminal.subscription().map(Message::Terminal));

        let window = iced::event::listen_with(|event, _, _| match event {
            iced::Event::Window(window::Event::Opened { .. } | window::Event::Focused) => {
                Some(Message::WindowFocused(true))
            },
            iced::Event::Window(window::Event::Unfocused) => Some(Message::WindowFocused(false)),
            _ => None,
        });

        // Finished commands and working directories are only known on Unix.
        let ticks = cfg!(unix).then(|| Subscription::run(ticks));

        Subscription::batch(terminals.chain([window]).chain(ticks))
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle(message);

        self.shown();
        self.save_session();

        task
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Terminal(iced_tterm::Event::BackendCall(id, BackendCommand::Paste(text))) => {
                self.paste(id, text);
                Task::none()
            },
            Message::Terminal(iced_tterm::Event::SelectionFinished(id)) => self.copy_selection(id),
            Message::Terminal(iced_tterm::Event::BackendCall(id, command)) => {
                let Some(shell) = self.terminals.get_mut(&id) else { return Task::none() };

                let event = match &command {
                    BackendCommand::ProcessAlacrittyEvent(event) => Some(event.clone()),
                    _ => None,
                };

                match shell.terminal.handle(iced_tterm::Command::ProxyToBackend(command)) {
                    // The shell exited, close its pane.
                    Action::Shutdown => return self.close_terminal(id),
                    Action::ChangeTitle(title) => shell.title = Some(title),
                    Action::Ignore => (),
                }

                match event {
                    Some(AlacrittyEvent::Wakeup) => {
                        self.mark_activity(id);
                        // A prompt is often what gets printed when a command finishes.
                        if self.poll_process(id) { self.notify(id) } else { Task::none() }
                    },
                    Some(AlacrittyEvent::Bell) => self.notify(id),
                    _ => Task::none(),
                }
            },
            Message::Tick => {
                let terminals: Vec<u64> = self.terminals.keys().copied().collect();
                let finished = terminals.into_iter().filter(|id| self.poll_process(*id));
                let finished: Vec<u64> = finished.collect();

                Task::batch(finished.into_iter().map(|id| self.notify(id)))
            },
            Message::PaletteQuery(query) => {
                if let Some(palette) = &mut self.palette {
                    palette.set_query(query);
                }

                Task::none()
            },
            Message::PaletteSubmit => {
                let command = self.palette.as_ref().and_then(Palette::selected);
                self.run(command)
            },
            Message::PaletteChoose(position) => {
                let command = self.palette.as_ref().and_then(|palette| palette.command(position));
                self.run(command)
            },
            Message::WindowFocused(focused) => {
                self.window_focused = focused;
                if focused { self.focus() } else { Task::none() }
            },
            Message::Action(action) => self.perform(action),
            Message::CloseTab(index) => self.close_tab(index),
            Message::PaneClicked(pane) => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.focus = pane;
                }

                self.focus()
            },
            Message::PaneResized(pane_grid::ResizeEvent { split, ratio }) => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.panes.resize(split, ratio);
                }

                Task::none()
            },
        }
    }

    /// Paste into a terminal, after asking if the text could run commands by itself.
    ///
    /// Without bracketed paste a shell takes every line break in pasted text as the Enter key.
    fn paste(&mut self, terminal: u64, text: String) {
        let Some(shell) = self.terminals.get_mut(&terminal) else { return };

        let bracketed = shell.terminal.mode().contains(TermMode::BRACKETED_PASTE);
        if self.options.confirm_multiline_paste && !bracketed && text.contains(['\n', '\r']) {
            self.pending_paste = Some(PendingPaste { terminal, text });
        } else {
            let paste = BackendCommand::Paste(text);
            shell.terminal.handle(iced_tterm::Command::ProxyToBackend(paste));
        }
    }

    /// Put the text selected in a terminal on the selection clipboard, and on the regular
    /// clipboard too if the configuration asks for it.
    fn copy_selection(&self, terminal: u64) -> Task<Message> {
        let selection = self.terminals.get(&terminal).and_then(|shell| shell.terminal.selection());
        let Some(text) = selection else { return Task::none() };

        let primary = iced::clipboard::write_primary(text.clone());
        if self.options.copy_on_select {
            Task::batch([primary, iced::clipboard::write(text)])
        } else {
            primary
        }
    }

    /// Close the palette and run the command chosen in it, if one was.
    fn run(&mut self, command: Option<palette::Command>) -> Task<Message> {
        self.palette = None;

        match command {
            Some(palette::Command::Action(action)) => self.perform(action),
            Some(palette::Command::Profile(index)) => {
                let profile = self.options.profiles.get(index);
                let spawned = profile.map(|profile| (profile.settings.clone(), profile.background));

                match spawned.map(|(settings, background)| self.spawn(settings, background)) {
                    Some(Ok(terminal)) => self.push_tab(terminal),
                    Some(Err(err)) => error!("Unable to open the profile: {err}"),
                    None => (),
                }

                self.focus()
            },
            None => self.focus(),
        }
    }

    /// Give keyboard focus to the palette if it is shown, or else to the focused pane of the tab
    /// that is shown.
    pub fn focus(&self) -> Task<Message> {
        if let Some(palette) = &self.palette {
            return iced::widget::operation::focus(palette.input().clone());
        }

        let terminal = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.panes.get(tab.focus))
            .and_then(|id| self.terminals.get(id));

        match terminal {
            Some(shell) => TerminalView::focus(shell.terminal.widget_id().clone()),
            None => Task::none(),
        }
    }

    /// Note output of a terminal in the tab bar, if its tab isn't shown.
    fn mark_activity(&mut self, terminal: u64) {
        let tab = self
            .tabs
            .iter_mut()
            .enumerate()
            .find(|(_, tab)| tab.panes.iter().any(|(_, id)| *id == terminal));

        if let Some((index, tab)) = tab
            && index != self.active
        {
            // A shell's first prompt isn't worth pointing out.
            tab.activity |= tab.seen;
        }
    }

    /// Point out a terminal that rang the bell or finished a command, unless it is being looked
    /// at: it is the focused pane of the shown tab and the window has focus.
    ///
    /// The terminal's tab and pane are marked until it is looked at, and the desktop is asked to
    /// draw attention to the window if that doesn't have focus.
    fn notify(&mut self, terminal: u64) -> Task<Message> {
        let focused = self.tabs.get(self.active).and_then(|tab| tab.panes.get(tab.focus));
        if focused == Some(&terminal) && self.window_focused {
            return Task::none();
        }

        if let Some(shell) = self.terminals.get_mut(&terminal) {
            shell.attention = true;
        }

        if self.window_focused {
            return Task::none();
        }

        let attention = Some(window::UserAttention::Informational);
        window::latest().and_then(move |id| window::request_user_attention(id, attention))
    }

    /// Look at the shell of a terminal: whether it runs a command and which directory it is in.
    ///
    /// Returns whether a command finished since the shell was last looked at. A command runs
    /// while another process group than the shell's has the terminal.
    fn poll_process(&mut self, terminal: u64) -> bool {
        let Some(shell) = self.terminals.get_mut(&terminal) else { return false };
        let (Some(pid), Some(foreground)) =
            (shell.terminal.child_pid(), shell.terminal.foreground_process_group())
        else {
            return false;
        };

        let was_busy = shell.busy;
        shell.busy = foreground != pid;
        if let Some(directory) = working_directory(pid) {
            shell.working_directory = Some(directory);
        }

        was_busy && !shell.busy
    }

    /// Settings for the terminal of a new tab or pane: the shell starts in the directory of the
    /// focused pane's shell, if the configuration asks for that and the directory can be told.
    fn new_settings(&self) -> Settings {
        let mut settings = self.settings.clone();
        if !self.options.tabs.inherit_working_directory {
            return settings;
        }

        let focused = self.tabs.get(self.active).and_then(|tab| tab.panes.get(tab.focus));
        let pid = focused
            .and_then(|id| self.terminals.get(id))
            .and_then(|shell| shell.terminal.child_pid());
        if let Some(directory) = pid.and_then(working_directory) {
            settings.backend.working_directory = Some(directory);
        }

        settings
    }

    /// Run a tab or pane action.
    fn perform(&mut self, action: AppAction) -> Task<Message> {
        let tab_count = self.tabs.len();

        match action {
            AppAction::NewTab => {
                match self.spawn(self.new_settings(), self.options.layout.background) {
                    Ok(terminal) => self.push_tab(terminal),
                    Err(err) => error!("Unable to open a new tab: {err}"),
                }
            },
            AppAction::CloseTab => return self.close_tab(self.active),
            AppAction::NextTab => self.active = (self.active + 1) % tab_count,
            AppAction::PreviousTab => self.active = (self.active + tab_count - 1) % tab_count,
            AppAction::SelectTab(index) if index < tab_count => self.active = index,
            AppAction::SelectTab(_) => return Task::none(),
            AppAction::LastTab => self.active = tab_count - 1,
            AppAction::MoveTabLeft if self.active > 0 => {
                self.tabs.swap(self.active, self.active - 1);
                self.active -= 1;
            },
            AppAction::MoveTabRight if self.active + 1 < tab_count => {
                self.tabs.swap(self.active, self.active + 1);
                self.active += 1;
            },
            AppAction::MoveTabLeft | AppAction::MoveTabRight => return Task::none(),
            AppAction::Split(axis) => {
                match self.spawn(self.new_settings(), self.options.layout.background) {
                    Ok(terminal) => {
                        let tab = &mut self.tabs[self.active];
                        tab.panes.restore();

                        match tab.panes.split(axis, tab.focus, terminal) {
                            Some((pane, _)) => tab.focus = pane,
                            None => drop(self.terminals.remove(&terminal)),
                        }
                    },
                    Err(err) => error!("Unable to split the pane: {err}"),
                }
            },
            AppAction::ClosePane => {
                return self.close_pane(self.active, self.tabs[self.active].focus);
            },
            AppAction::FocusPane(direction) => {
                let tab = &mut self.tabs[self.active];
                tab.panes.restore();

                if let Some(pane) = tab.panes.adjacent(tab.focus, direction) {
                    tab.focus = pane;
                }
            },
            AppAction::NextPane | AppAction::PreviousPane => {
                let tab = &mut self.tabs[self.active];
                tab.panes.restore();

                let panes = ordered_panes(tab.panes.layout());
                let current = panes.iter().position(|pane| *pane == tab.focus).unwrap_or(0);
                let step = if action == AppAction::NextPane { 1 } else { panes.len() - 1 };
                tab.focus = panes[(current + step) % panes.len()];
            },
            AppAction::ToggleZoom => {
                let tab = &mut self.tabs[self.active];
                if tab.panes.maximized().is_some() {
                    tab.panes.restore();
                } else if tab.panes.len() > 1 {
                    tab.panes.maximize(tab.focus);
                }
            },
            AppAction::ResizePane(direction) => {
                let tab = &mut self.tabs[self.active];
                let (axis, step) = match direction {
                    Direction::Left => (pane_grid::Axis::Vertical, -RESIZE_STEP),
                    Direction::Right => (pane_grid::Axis::Vertical, RESIZE_STEP),
                    Direction::Up => (pane_grid::Axis::Horizontal, -RESIZE_STEP),
                    Direction::Down => (pane_grid::Axis::Horizontal, RESIZE_STEP),
                };

                if let Some((split, ratio)) = nearest_split(tab.panes.layout(), tab.focus, axis) {
                    let (min, max) = RESIZE_LIMITS;
                    tab.panes.resize(split, (ratio + step).clamp(min, max));
                }

                return Task::none();
            },
            AppAction::ShowCommands => {
                self.palette = Some(Palette::commands(&self.options.hotkeys));
            },
            AppAction::ShowProfiles => {
                let profiles = self.options.profiles.iter();
                self.palette = Some(Palette::profiles(
                    profiles.map(|profile| (profile.name.clone(), profile.detail.clone())),
                ));
            },
            AppAction::PaletteNext | AppAction::PalettePrevious => {
                if let Some(palette) = &mut self.palette {
                    palette.step(action == AppAction::PaletteNext);
                }

                return Task::none();
            },
            AppAction::PaletteClose => self.palette = None,
            AppAction::PasteConfirm => {
                if let Some(PendingPaste { terminal, text }) = self.pending_paste.take()
                    && let Some(shell) = self.terminals.get_mut(&terminal)
                {
                    let paste = BackendCommand::Paste(text);
                    shell.terminal.handle(iced_tterm::Command::ProxyToBackend(paste));
                }
            },
            AppAction::PasteCancel => self.pending_paste = None,
        }

        self.focus()
    }

    /// Create a terminal and spawn its shell, returning the terminal's ID.
    fn spawn(&mut self, settings: Settings, background: Color) -> io::Result<u64> {
        let id = self.next_terminal;
        let working_directory = settings.backend.working_directory.clone();
        let mut terminal = iced_tterm::Terminal::new(id, settings)?;
        terminal.handle(iced_tterm::Command::AddBindings(self.options.bindings.clone()));

        let shell = Shell {
            terminal,
            title: None,
            background,
            busy: false,
            working_directory,
            attention: false,
        };
        self.next_terminal += 1;
        self.terminals.insert(id, shell);

        Ok(id)
    }

    /// Add a tab holding `terminal` after the last tab and show it.
    fn push_tab(&mut self, terminal: u64) {
        let (panes, focus) = pane_grid::State::new(terminal);
        self.add_tab(panes, focus);
        self.active = self.tabs.len() - 1;
    }

    /// Add a tab after the last tab.
    fn add_tab(&mut self, panes: pane_grid::State<u64>, focus: pane_grid::Pane) {
        let id = self.next_tab;
        self.next_tab += 1;
        self.tabs.push(Tab { id, panes, focus, seen: false, activity: false });
    }

    /// Close the pane of a terminal whose shell exited.
    fn close_terminal(&mut self, id: u64) -> Task<Message> {
        let location = self.tabs.iter().enumerate().find_map(|(index, tab)| {
            let pane = tab.panes.iter().find(|(_, terminal)| **terminal == id)?;
            Some((index, *pane.0))
        });

        match location {
            Some((index, pane)) => self.close_pane(index, pane),
            None => {
                self.terminals.remove(&id);
                Task::none()
            },
        }
    }

    /// Close a pane, and its tab if it was the tab's last pane.
    fn close_pane(&mut self, index: usize, pane: pane_grid::Pane) -> Task<Message> {
        let tab = &mut self.tabs[index];

        match tab.panes.close(pane) {
            Some((terminal, sibling)) => {
                if tab.focus == pane {
                    tab.focus = sibling;
                }

                self.terminals.remove(&terminal);
                self.focus()
            },
            None => self.close_tab(index),
        }
    }

    /// Close a tab with all of its panes, and the window with its last tab.
    fn close_tab(&mut self, index: usize) -> Task<Message> {
        if index >= self.tabs.len() {
            return Task::none();
        }

        let tab = self.tabs.remove(index);
        for (_, terminal) in tab.panes.iter() {
            self.terminals.remove(terminal);
        }

        if self.tabs.is_empty() {
            return window::latest().and_then(window::close);
        }

        if index < self.active || self.active == self.tabs.len() {
            self.active -= 1;
        }

        self.focus()
    }

    /// Title of a tab: the one its focused terminal set, or the window title.
    fn tab_title(&self, tab: &Tab) -> &str {
        let title = tab
            .panes
            .get(tab.focus)
            .and_then(|id| self.terminals.get(id))
            .and_then(|shell| shell.title.as_deref());

        title.filter(|_| self.options.dynamic_title).unwrap_or(&self.options.title)
    }

    pub fn view(&self) -> Element<'_, Message> {
        let Some(tab) = self.tabs.get(self.active) else {
            // The last tab was closed and the window is about to close.
            return container("").into();
        };

        // Frame the panes of a split tab, to show which one has focus.
        let framed = tab.panes.len() > 1 && tab.panes.maximized().is_none();
        let panes = PaneGrid::new(&tab.panes, |pane, terminal, _| {
            pane_grid::Content::new(self.pane(*terminal, framed, pane == tab.focus))
        })
        .on_click(Message::PaneClicked)
        .on_resize(6, Message::PaneResized);

        // Keyed by tab, so the widget state of one tab isn't reused for another.
        let terminals =
            keyed_column([(tab.id, Element::from(panes))]).width(Length::Fill).height(Length::Fill);

        let show_tabs = match self.options.tabs.show {
            ShowTabs::Always => true,
            ShowTabs::Multiple => self.tabs.len() > 1,
            ShowTabs::Never => false,
        };

        let content: Element<'_, Message> = match (show_tabs, self.options.tabs.position) {
            (false, _) => terminals.into(),
            (true, TabsPosition::Top) => column![self.tab_bar(), terminals].into(),
            (true, TabsPosition::Bottom) => column![terminals, self.tab_bar()].into(),
        };

        // The palette is drawn over the terminals and takes the keyboard while it is shown.
        let appearance = palette::Appearance {
            font: self.settings.font.font_type,
            size: self.settings.font.size,
            background: self.options.layout.background,
            foreground: self.options.layout.foreground,
        };
        let layers = stack![content]
            .push(self.palette.as_ref().map(|palette| palette.view(appearance)))
            .push(self.pending_paste.as_ref().map(|paste| paste_warning(&paste.text, appearance)));

        // The paste warning is answered with a key; nothing typed meanwhile reaches a terminal.
        let hotkeys = match (&self.pending_paste, &self.palette) {
            (Some(_), _) => &self.paste_keys,
            (None, Some(_)) => &self.palette_keys,
            (None, None) => &self.options.hotkeys,
        };

        KeyFilter::new(layers, hotkeys, Message::Action)
            .exclusive(self.pending_paste.is_some())
            .into()
    }

    /// A terminal with its padding and, in a split tab, a frame showing whether it has focus.
    fn pane(&self, terminal: u64, framed: bool, focused: bool) -> Element<'_, Message> {
        let Layout { padding, foreground, .. } = self.options.layout;
        let Some(shell) = self.terminals.get(&terminal) else { return container("").into() };
        let background = shell.background;

        let terminal = container(TerminalView::show(&shell.terminal).map(Message::Terminal))
            .padding(padding)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(background.into()),
                ..Default::default()
            });

        // A pane that wants to be looked at stands out from the focused one.
        let frame = match (shell.attention, focused) {
            (true, _) => foreground,
            (false, true) => blend(self.options.layout.background, foreground, 0.6),
            (false, false) => blend(self.options.layout.background, foreground, 0.15),
        };
        container(terminal)
            .padding(if framed { 1 } else { 0 })
            .style(move |_| container::Style {
                background: Some(frame.into()),
                ..Default::default()
            })
            .into()
    }

    /// The tab bar: a button for every tab, each with a close button, and one to add a tab.
    fn tab_bar(&self) -> Element<'_, Message> {
        let Layout { background, foreground, .. } = self.options.layout;
        let bar = blend(background, foreground, 0.12);
        let font = self.settings.font.font_type;
        let size = self.settings.font.size;
        let label = move |content: String| text(content).font(font).size(size);

        let tabs = self.tabs.iter().enumerate().map(|(index, tab)| {
            let active = index == self.active;
            let color = if active { foreground } else { blend(foreground, background, 0.4) };
            let style = move |hovered: bool| button::Style {
                background: Some(if active { background } else { bar }.into()),
                text_color: if hovered { foreground } else { color },
                ..Default::default()
            };

            let attention = tab
                .panes
                .iter()
                .any(|(_, id)| self.terminals.get(id).is_some_and(|shell| shell.attention));
            let marker = match (attention, tab.activity) {
                (true, _) => " !",
                (false, true) => " *",
                (false, false) => "",
            };
            let title = label(format!("{}: {}{marker}", index + 1, self.tab_title(tab)));
            let select = button(container(title.wrapping(Wrapping::None)).clip(true))
                .padding([3, 8])
                .width(Length::Fill)
                .style(move |_, status| style(status == button::Status::Hovered))
                .on_press(Message::Action(AppAction::SelectTab(index)));
            let close = button(label("\u{d7}".into()))
                .padding([3, 6])
                .style(move |_, status| style(status == button::Status::Hovered))
                .on_press(Message::CloseTab(index));

            // Like in browsers, a middle click closes the tab.
            let tab = mouse_area(row![select, close]).on_middle_press(Message::CloseTab(index));
            container(tab).width(Length::FillPortion(1)).max_width(MAX_TAB_WIDTH).into()
        });

        let new_tab = button(label("+".into()))
            .padding([3, 8])
            .style(move |_, status| button::Style {
                background: Some(bar.into()),
                text_color: match status {
                    button::Status::Hovered => foreground,
                    _ => blend(foreground, background, 0.4),
                },
                ..Default::default()
            })
            .on_press(Message::Action(AppAction::NewTab));

        container(row(tabs).push(new_tab).spacing(1))
            .width(Length::Fill)
            .style(move |_| container::Style { background: Some(bar.into()), ..Default::default() })
            .into()
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

/// A message every [`PROCESS_INTERVAL`], from a thread of its own so no timer of an async
/// runtime is needed.
fn ticks() -> impl iced::futures::Stream<Item = Message> {
    let (mut sender, receiver) = iced::futures::channel::mpsc::channel(1);

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(PROCESS_INTERVAL);
            // A full channel means the application is behind; it doesn't need another tick.
            if sender.try_send(Message::Tick).is_err_and(|err| err.is_disconnected()) {
                break;
            }
        }
    });

    receiver
}

/// The innermost split around `pane` whose divider runs along `axis`, with its ratio.
fn nearest_split(
    node: &Node,
    pane: pane_grid::Pane,
    axis: pane_grid::Axis,
) -> Option<(pane_grid::Split, f32)> {
    let Node::Split { id, axis: split_axis, ratio, a, b } = node else { return None };

    let child = [a, b].into_iter().find(|child| ordered_panes(child).contains(&pane))?;
    nearest_split(child, pane, axis).or_else(|| (*split_axis == axis).then_some((*id, *ratio)))
}

/// Layout through the real `App` view, rendered headlessly.
#[cfg(all(test, unix))]
mod layout_tests {
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
            session_file: None,
            profiles: Vec::new(),
            confirm_multiline_paste: true,
            copy_on_select: false,
            bindings: Vec::new(),
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
}

/// Tabs and panes through the real `App`, with `cat` as the shell.
#[cfg(all(test, unix))]
mod tab_tests {
    use iced::keyboard::{Key, Modifiers};
    use iced::widget::pane_grid::Axis;
    use iced::{Size, Theme};
    use iced_test::simulator::{self, Simulator};
    use iced_tterm::settings::BackendSettings;
    use iced_tterm::{AlacrittyEvent, BackendCommand};

    use super::*;
    use crate::cli::Options;
    use crate::config::UiConfig;
    use crate::term_settings;

    fn app_with_session(config: &str, hotkeys: Hotkeys, session_file: Option<PathBuf>) -> App {
        let config: UiConfig = toml::from_str(config).unwrap();
        let mut settings = term_settings::settings(&config, &Options::default());
        // Tests run `cat` unless their configuration names a shell.
        if config.terminal.shell.is_none() {
            settings.backend =
                BackendSettings { program: "cat".into(), ..BackendSettings::default() };
        }

        // Profiles of the test configuration, told apart by a white background.
        let profiles = config.profiles.iter().map(|profile| Profile {
            name: profile.name.clone(),
            detail: String::new(),
            settings: term_settings::profile_settings(profile, &config, &Options::default()),
            background: Color::WHITE,
        });
        let profiles = profiles.collect();

        let options = AppOptions {
            title: "test".into(),
            dynamic_title: true,
            layout: Layout {
                padding: Padding::ZERO,
                background: term_settings::background(&config),
                foreground: term_settings::foreground(&config),
            },
            tabs: config.tabs,
            hotkeys,
            session_file,
            profiles,
            confirm_multiline_paste: config.terminal.confirm_multiline_paste,
            copy_on_select: config.selection.save_to_clipboard,
            bindings: crate::hotkeys::mouse_bindings(config.mouse_bindings()),
        };

        App::new(options, settings.clone(), settings).unwrap()
    }

    fn app_with(config: &str, hotkeys: Hotkeys) -> App {
        app_with_session(config, hotkeys, None)
    }

    fn app() -> App {
        app_with("", Hotkeys::default())
    }

    fn perform(app: &mut App, actions: &[AppAction]) {
        for action in actions {
            let _ = app.update(Message::Action(*action));
        }
    }

    /// Terminal IDs of the panes of the tab at `index`, in layout order.
    fn terminals(app: &App, index: usize) -> Vec<u64> {
        let tab = &app.tabs[index];
        ordered_panes(tab.panes.layout())
            .iter()
            .map(|pane| *tab.panes.get(*pane).unwrap())
            .collect()
    }

    /// Terminal ID of the focused pane of the tab that is shown.
    fn focused(app: &App) -> u64 {
        let tab = &app.tabs[app.active];
        *tab.panes.get(tab.focus).unwrap()
    }

    /// Text drawn by the application's view.
    fn has_text(app: &App, text: &str) -> bool {
        let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
        ui.find(text).is_ok()
    }

    #[test]
    fn new_tabs_are_added_after_the_last_and_shown() {
        let mut app = app();
        perform(&mut app, &[AppAction::NewTab, AppAction::NewTab]);

        assert_eq!(app.tabs.len(), 3);
        assert_eq!(app.active, 2);
        assert_eq!(app.terminals.len(), 3);
        assert_eq!(focused(&app), 2);
    }

    #[test]
    fn tabs_are_selected_by_position_and_cycle() {
        let mut app = app();
        perform(&mut app, &[AppAction::NewTab, AppAction::NewTab]);

        perform(&mut app, &[AppAction::NextTab]);
        assert_eq!(app.active, 0);
        perform(&mut app, &[AppAction::PreviousTab]);
        assert_eq!(app.active, 2);
        perform(&mut app, &[AppAction::SelectTab(1)]);
        assert_eq!(app.active, 1);
        // There is no tenth tab.
        perform(&mut app, &[AppAction::SelectTab(9)]);
        assert_eq!(app.active, 1);
        perform(&mut app, &[AppAction::LastTab]);
        assert_eq!(app.active, 2);
    }

    #[test]
    fn tabs_move_without_wrapping_around() {
        let mut app = app();
        perform(&mut app, &[AppAction::NewTab, AppAction::NewTab, AppAction::MoveTabLeft]);

        assert_eq!(app.active, 1);
        assert_eq!([terminals(&app, 0), terminals(&app, 1), terminals(&app, 2)], [[0], [2], [1]]);

        perform(&mut app, &[AppAction::MoveTabRight, AppAction::MoveTabRight]);
        assert_eq!(app.active, 2);
        assert_eq!(terminals(&app, 2), [2]);
    }

    #[test]
    fn closing_a_tab_closes_its_terminals_and_shows_a_neighbor() {
        let mut app = app();
        perform(&mut app, &[
            AppAction::NewTab,
            AppAction::Split(Axis::Vertical),
            AppAction::NewTab,
            AppAction::SelectTab(1),
            AppAction::CloseTab,
        ]);

        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active, 1);
        assert_eq!(focused(&app), 3);
        let mut open: Vec<_> = app.terminals.keys().copied().collect();
        open.sort_unstable();
        assert_eq!(open, [0, 3]);

        // Closing a tab before the shown one keeps the shown one.
        let _ = app.update(Message::CloseTab(0));
        assert_eq!(app.active, 0);
        assert_eq!(focused(&app), 3);
    }

    #[test]
    fn splits_focus_the_new_pane() {
        let mut app = app();
        perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::Split(Axis::Horizontal)]);

        assert_eq!(app.tabs.len(), 1);
        assert_eq!(terminals(&app, 0), [0, 1, 2]);
        assert_eq!(focused(&app), 2);
    }

    #[test]
    fn focus_moves_between_panes() {
        let mut app = app();
        // Terminal 0 on the left, 1 above 2 on the right.
        perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::Split(Axis::Horizontal)]);

        perform(&mut app, &[AppAction::FocusPane(Direction::Up)]);
        assert_eq!(focused(&app), 1);
        perform(&mut app, &[AppAction::FocusPane(Direction::Left)]);
        assert_eq!(focused(&app), 0);
        // There is no pane further left.
        perform(&mut app, &[AppAction::FocusPane(Direction::Left)]);
        assert_eq!(focused(&app), 0);

        perform(&mut app, &[AppAction::PreviousPane]);
        assert_eq!(focused(&app), 2);
        perform(&mut app, &[AppAction::NextPane, AppAction::NextPane]);
        assert_eq!(focused(&app), 1);
    }

    #[test]
    fn zoom_shows_only_the_focused_pane_until_focus_moves() {
        let mut app = app();
        perform(&mut app, &[AppAction::ToggleZoom]);
        assert_eq!(app.tabs[0].panes.maximized(), None);

        perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::ToggleZoom]);
        assert_eq!(app.tabs[0].panes.maximized(), Some(app.tabs[0].focus));

        perform(&mut app, &[AppAction::FocusPane(Direction::Left)]);
        assert_eq!(app.tabs[0].panes.maximized(), None);
        assert_eq!(focused(&app), 0);
    }

    #[test]
    fn panes_are_resized_at_the_nearest_divider() {
        let ratios = |app: &App| {
            let mut ratios = Vec::new();
            let mut nodes = vec![app.tabs[0].panes.layout()];
            while let Some(node) = nodes.pop() {
                if let Node::Split { axis, ratio, a, b, .. } = node {
                    ratios.push((*axis, *ratio));
                    nodes.extend([&**b, &**a]);
                }
            }
            ratios
        };

        let mut app = app();
        perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::Split(Axis::Horizontal)]);
        assert_eq!(ratios(&app), [(Axis::Vertical, 0.5), (Axis::Horizontal, 0.5)]);

        perform(&mut app, &[
            AppAction::ResizePane(Direction::Left),
            AppAction::ResizePane(Direction::Down),
        ]);
        assert_eq!(ratios(&app), [(Axis::Vertical, 0.45), (Axis::Horizontal, 0.55)]);

        // The left pane has no horizontal divider to move.
        perform(&mut app, &[
            AppAction::FocusPane(Direction::Left),
            AppAction::ResizePane(Direction::Up),
        ]);
        assert_eq!(ratios(&app), [(Axis::Vertical, 0.45), (Axis::Horizontal, 0.55)]);

        // Dividers stop before a pane disappears.
        perform(&mut app, &[AppAction::ResizePane(Direction::Left); 20]);
        assert_eq!(ratios(&app)[0], (Axis::Vertical, RESIZE_LIMITS.0));
    }

    #[test]
    fn closing_panes_focuses_a_sibling_and_closes_the_tab_with_the_last() {
        let mut app = app();
        perform(&mut app, &[AppAction::NewTab, AppAction::Split(Axis::Vertical)]);

        perform(&mut app, &[AppAction::ClosePane]);
        assert_eq!(terminals(&app, 1), [1]);
        assert_eq!(focused(&app), 1);
        assert_eq!(app.terminals.len(), 2);

        perform(&mut app, &[AppAction::ClosePane]);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(focused(&app), 0);
    }

    #[test]
    fn a_pane_closes_when_its_shell_exits() {
        let mut app = app();
        perform(&mut app, &[AppAction::Split(Axis::Vertical), AppAction::NewTab]);

        // The shell of an unfocused pane in a tab that isn't shown exits.
        let exit = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Exit);
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(0, exit.clone())));

        assert_eq!(terminals(&app, 0), [1]);
        assert_eq!(app.active, 1);
        assert_eq!(focused(&app), 2);

        // Its last pane takes the tab with it.
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(1, exit.clone())));
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(focused(&app), 2);

        // The last tab leaves nothing to show; the window is closed.
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(2, exit)));
        assert!(app.tabs.is_empty() && app.terminals.is_empty());
        let _ = app.view();
    }

    #[test]
    fn titles_follow_the_focused_terminal() {
        let mut app = app();
        assert_eq!(app.title(), "test");

        let title = |id, title: &str| {
            let event = AlacrittyEvent::Title(title.into());
            Message::Terminal(iced_tterm::Event::BackendCall(
                id,
                BackendCommand::ProcessAlacrittyEvent(event),
            ))
        };

        let _ = app.update(title(0, "first"));
        assert_eq!(app.title(), "first");

        perform(&mut app, &[AppAction::Split(Axis::Vertical)]);
        assert_eq!(app.title(), "test");
        let _ = app.update(title(1, "second"));
        assert_eq!(app.title(), "second");

        perform(&mut app, &[AppAction::FocusPane(Direction::Left)]);
        assert_eq!(app.title(), "first");
    }

    #[test]
    fn background_tabs_show_activity_and_bells() {
        let event = |id, event| {
            Message::Terminal(iced_tterm::Event::BackendCall(
                id,
                BackendCommand::ProcessAlacrittyEvent(event),
            ))
        };

        let mut app = app();
        perform(&mut app, &[AppAction::NewTab]);

        let _ = app.update(event(0, AlacrittyEvent::Wakeup));
        assert!(has_text(&app, "1: test *") && !has_text(&app, "1: test"));
        let _ = app.update(event(0, AlacrittyEvent::Bell));
        assert!(has_text(&app, "1: test !"));

        // Output of the tab that is shown isn't pointed out.
        let _ = app.update(event(1, AlacrittyEvent::Wakeup));
        let _ = app.update(event(1, AlacrittyEvent::Bell));
        assert!(has_text(&app, "2: test"));

        // Showing a tab clears its markers.
        perform(&mut app, &[AppAction::SelectTab(0)]);
        assert!(has_text(&app, "1: test") && has_text(&app, "2: test"));
    }

    #[test]
    fn tabs_and_panes_are_restored_from_the_session_file() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("session.json");

        let mut app = app_with_session("", Hotkeys::default(), Some(file.clone()));
        perform(&mut app, &[
            AppAction::Split(Axis::Vertical),
            AppAction::Split(Axis::Horizontal),
            AppAction::ResizePane(Direction::Left),
            AppAction::FocusPane(Direction::Up),
            AppAction::NewTab,
            AppAction::SelectTab(0),
        ]);
        let session = app.session();
        assert_eq!(session.tabs.len(), 2);
        drop(app);

        let mut app = app_with_session("", Hotkeys::default(), Some(file.clone()));
        assert_eq!(app.session(), session);
        assert_eq!((app.tabs.len(), app.active, app.terminals.len()), (2, 0, 4));
        // The pane above the last one on the right has focus again.
        assert_eq!(terminals(&app, 0), [0, 1, 2]);
        assert_eq!(focused(&app), 1);

        // The first prompt of a restored shell isn't activity.
        let wakeup = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup);
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(3, wakeup)));
        assert!(has_text(&app, "2: test"));

        // Closing the window's last tab leaves that tab to come back to.
        perform(&mut app, &[AppAction::CloseTab, AppAction::CloseTab]);
        assert!(app.tabs.is_empty());
        drop(app);

        let app = app_with_session("", Hotkeys::default(), Some(file));
        assert_eq!((app.tabs.len(), app.terminals.len()), (1, 1));
    }

    #[test]
    fn sessions_are_not_written_without_a_session_file() {
        let mut app = app();
        perform(&mut app, &[AppAction::NewTab]);

        assert_eq!(app.saved_session, None);
    }

    #[test]
    fn command_palette_filters_and_runs_commands() {
        let mut app = app();
        perform(&mut app, &[AppAction::ShowCommands]);
        assert!(has_text(&app, "Split Right") && has_text(&app, "New Tab"));

        let _ = app.update(Message::PaletteQuery("split".into()));
        assert!(has_text(&app, "Split Down") && !has_text(&app, "New Tab"));

        perform(&mut app, &[AppAction::PaletteNext]);
        let _ = app.update(Message::PaletteSubmit);
        assert!(app.palette.is_none());
        // "Split Down" put the new terminal below the first one.
        assert_eq!(terminals(&app, 0), [0, 1]);
        assert!(matches!(app.tabs[0].panes.layout(), Node::Split { axis: Axis::Horizontal, .. }));

        // Clicking an item runs it as well.
        perform(&mut app, &[AppAction::ShowCommands]);
        let _ = app.update(Message::PaletteChoose(0));
        assert_eq!((app.tabs.len(), app.palette.is_none()), (2, true));
    }

    #[test]
    fn palette_takes_the_keyboard_until_it_is_closed() {
        use iced::keyboard::key::Named;

        let hotkeys =
            Hotkeys::default().with(Key::Named(Named::F5), Modifiers::empty(), AppAction::NewTab);
        let mut app = app_with("", hotkeys);
        perform(&mut app, &[AppAction::ShowCommands]);

        let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
        ui.tap_key(Named::F5);
        ui.tap_key(Named::ArrowDown);
        ui.tap_key(Named::Escape);
        // Clicks don't reach the terminal underneath either.
        ui.point_at((400., 300.));
        ui.simulate(simulator::click());

        let actions: Vec<_> = ui
            .into_messages()
            .filter_map(|message| match message {
                Message::Action(action) => Some(action),
                Message::PaneClicked(_) => panic!("click went through the palette"),
                _ => None,
            })
            .collect();
        assert_eq!(actions, [AppAction::PaletteNext, AppAction::PaletteClose]);

        perform(&mut app, &[AppAction::PaletteClose]);
        assert!(app.palette.is_none());
        assert_eq!(app.tabs.len(), 1);
    }

    #[test]
    fn profiles_open_in_a_new_tab() {
        let config = "[[profiles]]\nname = \"Cat\"\nshell = \"cat\"\n";
        let mut app = app_with(config, Hotkeys::default());

        perform(&mut app, &[AppAction::ShowProfiles]);
        assert!(has_text(&app, "Default") && has_text(&app, "Cat"));

        let _ = app.update(Message::PaletteQuery("cat".into()));
        let _ = app.update(Message::PaletteSubmit);
        assert_eq!((app.tabs.len(), app.active, focused(&app)), (2, 1, 1));
        assert_eq!(app.terminals[&1].background, Color::WHITE);

        // The default entry opens the regular shell.
        perform(&mut app, &[AppAction::ShowProfiles]);
        let _ = app.update(Message::PaletteSubmit);
        assert_eq!(app.tabs.len(), 3);
        assert_ne!(app.terminals[&2].background, Color::WHITE);
    }

    /// Wait for a terminal to show `text`.
    fn wait_for_text(app: &App, terminal: u64, text: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !app.terminals[&terminal].terminal.text().concat().contains(text) {
            assert!(std::time::Instant::now() < deadline, "terminal never showed {text:?}");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn paste(terminal: u64, text: &str) -> Message {
        Message::Terminal(iced_tterm::Event::BackendCall(
            terminal,
            BackendCommand::Paste(text.into()),
        ))
    }

    #[test]
    fn pastes_with_line_breaks_wait_for_an_answer() {
        let mut app = app();

        // A single line is pasted right away.
        let _ = app.update(paste(0, "one"));
        assert!(app.pending_paste.is_none());
        wait_for_text(&app, 0, "one");

        let _ = app.update(paste(0, "two\nthree\n"));
        assert!(app.pending_paste.is_some());
        assert!(has_text(&app, "Paste 2 lines? Every line break runs a line."));
        assert!(has_text(&app, "two") && has_text(&app, "three"));

        // Cancelling pastes nothing.
        perform(&mut app, &[AppAction::PasteCancel]);
        assert!(app.pending_paste.is_none());

        let _ = app.update(paste(0, "four\nfive\n"));
        perform(&mut app, &[AppAction::PasteConfirm]);
        assert!(app.pending_paste.is_none());
        wait_for_text(&app, 0, "five");
        assert!(!app.terminals[&0].terminal.text().concat().contains("two"));
    }

    #[test]
    fn paste_warning_takes_the_keyboard() {
        use iced::keyboard::key::Named;

        let mut app = app();
        let _ = app.update(paste(0, "ls\n"));
        assert!(has_text(&app, "Paste this line? Its line break runs it."));

        let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
        ui.typewrite("y");
        ui.tap_key(Named::Enter);

        let mut actions = Vec::new();
        for message in ui.into_messages() {
            match message {
                Message::Action(action) => actions.push(action),
                Message::Terminal(iced_tterm::Event::BackendCall(_, BackendCommand::Write(_))) => {
                    panic!("a key reached the terminal")
                },
                _ => (),
            }
        }
        assert_eq!(actions, [AppAction::PasteConfirm]);
    }

    #[test]
    fn pastes_are_not_questioned_with_bracketed_paste_or_when_turned_off() {
        // The shell asks for bracketed paste, like interactive shells and editors do.
        let config = "[terminal]\nshell = { program = \"sh\", args = [\"-c\", \"printf \
                      '\\\\033[?2004h'; exec cat\"] }\n";
        let mut app = app_with(config, Hotkeys::default());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !app.terminals[&0].terminal.mode().contains(TermMode::BRACKETED_PASTE) {
            assert!(std::time::Instant::now() < deadline, "bracketed paste was never enabled");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let _ = app.update(paste(0, "one\ntwo\n"));
        assert!(app.pending_paste.is_none());
        wait_for_text(&app, 0, "^[[200~one");

        let mut app = app_with("[terminal]\nconfirm_multiline_paste = false\n", Hotkeys::default());
        let _ = app.update(paste(0, "one\ntwo\n"));
        assert!(app.pending_paste.is_none());
        wait_for_text(&app, 0, "two");
    }

    #[test]
    fn finished_selections_are_reported_with_their_text() {
        use iced::mouse;

        let config = "[terminal]\nshell = { program = \"sh\", args = [\"-c\", \"printf 'hello \
                      world'; exec cat\"] }\n";
        let mut app = app_with(config, Hotkeys::default());
        let window = Size::new(800., 400.);

        // Lay the terminal out so it knows its size, and let the shell print.
        let resizes: Vec<_> = {
            let mut ui = Simulator::with_size(Default::default(), window, app.view());
            ui.snapshot(&Theme::Dark).unwrap();
            ui.into_messages().collect()
        };
        for message in resizes {
            let _ = app.update(message);
        }
        wait_for_text(&app, 0, "hello world");
        let wakeup = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup);
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(0, wakeup)));
        assert_eq!(app.terminals[&0].terminal.selection(), None);

        // Drag over the start of the first line.
        let drag: Vec<_> = {
            let mut ui = Simulator::with_size(Default::default(), window, app.view());
            ui.point_at((2., 5.));
            ui.simulate([iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))]);
            ui.point_at((60., 5.));
            ui.simulate([
                iced::Event::Mouse(mouse::Event::CursorMoved { position: (60., 5.).into() }),
                iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ]);
            ui.into_messages().collect()
        };
        assert!(matches!(
            drag.last(),
            Some(Message::Terminal(iced_tterm::Event::SelectionFinished(0)))
        ));
        for message in drag {
            let _ = app.update(message);
        }

        let selection = app.terminals[&0].terminal.selection().expect("nothing was selected");
        assert!("hello world".starts_with(&selection), "{selection:?}");
    }

    #[test]
    fn mouse_paste_bindings_come_from_the_configuration() {
        use iced::mouse::Button;

        let bindings = |config: &str| {
            let config: UiConfig = toml::from_str(config).unwrap();
            crate::hotkeys::mouse_bindings(config.mouse_bindings())
                .into_iter()
                .filter_map(|(binding, action)| match binding.target {
                    InputKind::Mouse(button) => Some((button, binding.modifiers, action)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };

        // By default only the middle button pastes.
        let default = bindings("");
        assert!(default.contains(&(
            Button::Middle,
            Modifiers::empty(),
            BindingAction::PasteSelection
        )));
        assert!(default.contains(&(Button::Right, Modifiers::empty(), BindingAction::Ignore)));

        let right_click = bindings(
            "[mouse]\nbindings = [{ mouse = \"Right\", action = \"Paste\" }, { mouse = \
             \"Middle\", action = \"None\" }]\n",
        );
        assert!(right_click.contains(&(Button::Right, Modifiers::empty(), BindingAction::Paste)));
        assert!(right_click.contains(&(Button::Middle, Modifiers::empty(), BindingAction::Ignore)));
        assert!(!right_click.iter().any(|(.., action)| *action == BindingAction::PasteSelection));
    }

    /// Configuration running an interactive shell, which gives every command the terminal.
    const INTERACTIVE_SHELL: &str = "[terminal]\nshell = { program = \"sh\", args = [\"-i\"] }\n";

    /// Type a line into the shell of a terminal.
    fn run(app: &mut App, terminal: u64, line: &str) {
        let write = BackendCommand::Write(format!("{line}\n").into_bytes());
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(terminal, write)));
    }

    /// Look at the shells until `condition` holds.
    fn tick_until(app: &mut App, what: &str, condition: impl Fn(&App) -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let _ = app.update(Message::Tick);
            if condition(app) {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "{what}");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// Run a short command in a terminal and wait for it to finish.
    #[cfg(unix)]
    fn run_to_the_end(app: &mut App, terminal: u64) {
        run(app, terminal, "sleep 0.3");
        tick_until(app, "the command never started", |app| app.terminals[&terminal].busy);
        tick_until(app, "the command never finished", |app| !app.terminals[&terminal].busy);
    }

    #[cfg(unix)]
    #[test]
    fn finished_commands_are_pointed_out_unless_looked_at() {
        let mut app = app_with(INTERACTIVE_SHELL, Hotkeys::default());
        perform(&mut app, &[AppAction::NewTab]);

        // A command finishes in the tab that isn't shown.
        run_to_the_end(&mut app, 0);
        assert!(app.terminals[&0].attention);
        assert!(has_text(&app, "1: test !"));

        // Looking at it is enough.
        perform(&mut app, &[AppAction::SelectTab(0)]);
        assert!(!app.terminals[&0].attention);
        assert!(has_text(&app, "1: test"));

        // What finishes in front of the user isn't pointed out.
        run_to_the_end(&mut app, 0);
        assert!(!app.terminals[&0].attention);

        // An unfocused pane of the shown tab is.
        perform(&mut app, &[AppAction::Split(Axis::Vertical)]);
        run_to_the_end(&mut app, 0);
        assert!(app.terminals[&0].attention && !app.terminals[&2].attention);
        assert!(has_text(&app, "1: test !"));
        perform(&mut app, &[AppAction::FocusPane(Direction::Left)]);
        assert!(!app.terminals[&0].attention);

        // So is the focused pane while the window isn't looked at.
        let _ = app.update(Message::WindowFocused(false));
        run_to_the_end(&mut app, 0);
        assert!(app.terminals[&0].attention);
        let _ = app.update(Message::WindowFocused(true));
        assert!(!app.terminals[&0].attention);
    }

    /// The directory the shell of a terminal is in right now.
    #[cfg(target_os = "linux")]
    fn directory(app: &App, terminal: u64) -> Option<PathBuf> {
        working_directory(app.terminals[&terminal].terminal.child_pid().unwrap())
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn new_tabs_and_panes_start_in_the_focused_shells_directory() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap();

        let mut app = app_with(INTERACTIVE_SHELL, Hotkeys::default());
        run(&mut app, 0, &format!("cd {}", path.display()));
        tick_until(&mut app, "the shell never changed directory", |app| {
            app.terminals[&0].working_directory.as_ref() == Some(&path)
        });

        perform(&mut app, &[AppAction::NewTab, AppAction::Split(Axis::Vertical)]);
        assert_eq!(self::directory(&app, 1), Some(path.clone()));
        assert_eq!(self::directory(&app, 2), Some(path.clone()));

        // Without the option, new shells start where the first one did.
        let config = format!("{INTERACTIVE_SHELL}\n[tabs]\ninherit_working_directory = false\n");
        let mut app = app_with(&config, Hotkeys::default());
        let start = self::directory(&app, 0);
        run(&mut app, 0, &format!("cd {}", path.display()));
        tick_until(&mut app, "the shell never changed directory", |app| {
            app.terminals[&0].working_directory.as_ref() == Some(&path)
        });

        perform(&mut app, &[AppAction::NewTab]);
        assert_eq!(self::directory(&app, 1), start);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn restored_panes_start_in_their_directories() {
        let state = tempfile::tempdir().unwrap();
        let file = state.path().join("session.json");
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().canonicalize().unwrap();
        let config = format!("{INTERACTIVE_SHELL}\n[tabs]\ninherit_working_directory = false\n");

        let mut app = app_with_session(&config, Hotkeys::default(), Some(file.clone()));
        perform(&mut app, &[AppAction::Split(Axis::Vertical)]);
        run(&mut app, 1, &format!("cd {}", path.display()));
        tick_until(&mut app, "the shell never changed directory", |app| {
            app.terminals[&1].working_directory.as_ref() == Some(&path)
        });
        let start = self::directory(&app, 0);
        drop(app);

        let app = app_with_session(&config, Hotkeys::default(), Some(file.clone()));
        assert_eq!(terminals(&app, 0), [0, 1]);
        assert_eq!(self::directory(&app, 0), start);
        assert_eq!(self::directory(&app, 1), Some(path));
        drop(app);

        // A directory that is gone doesn't keep its pane from coming back.
        drop(directory);
        let app = app_with_session(&config, Hotkeys::default(), Some(file));
        assert_eq!(terminals(&app, 0), [0, 1]);
        assert_eq!(self::directory(&app, 1), start);
    }

    #[test]
    fn tab_bar_is_shown_with_more_than_one_tab() {
        let mut app = app();
        assert!(!has_text(&app, "1: test"));

        perform(&mut app, &[AppAction::NewTab]);
        assert!(has_text(&app, "1: test") && has_text(&app, "2: test"));

        perform(&mut app, &[AppAction::CloseTab]);
        assert!(!has_text(&app, "1: test"));
    }

    #[test]
    fn tab_bar_visibility_is_configurable() {
        let mut always =
            app_with("[tabs]\nshow = \"Always\"\nposition = \"Bottom\"\n", Hotkeys::default());
        assert!(has_text(&always, "1: test"));
        perform(&mut always, &[AppAction::NewTab]);
        assert!(has_text(&always, "2: test"));

        let mut never = app_with("[tabs]\nshow = \"Never\"\n", Hotkeys::default());
        perform(&mut never, &[AppAction::NewTab]);
        assert!(!has_text(&never, "1: test"));
    }

    #[test]
    fn tab_bar_buttons_select_close_and_add_tabs() {
        let mut app = app();
        perform(&mut app, &[AppAction::NewTab]);

        let click = |app: &App, text: &str| {
            let mut ui =
                Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
            ui.click(text).unwrap();
            // The terminal also publishes its size when it is laid out.
            ui.into_messages()
                .filter(|message| !matches!(message, Message::Terminal(_)))
                .collect::<Vec<_>>()
        };

        let messages = click(&app, "1: test");
        assert!(matches!(messages[..], [Message::Action(AppAction::SelectTab(0))]));
        let messages = click(&app, "+");
        assert!(matches!(messages[..], [Message::Action(AppAction::NewTab)]));
        // Both tabs have a close button; the first one found belongs to the first tab.
        let messages = click(&app, "\u{d7}");
        assert!(matches!(messages[..], [Message::CloseTab(0)]));
    }

    #[test]
    fn bound_keys_are_handled_and_kept_from_the_shell() {
        let hotkeys = Hotkeys::default().with(
            Key::Character("a".into()),
            Modifiers::empty(),
            AppAction::NewTab,
        );
        let app = app_with("", hotkeys);

        let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
        // Click the terminal to give it keyboard focus.
        ui.point_at((400., 200.));
        ui.simulate(simulator::click());
        ui.typewrite("ab");

        let mut actions = Vec::new();
        let mut written = Vec::new();
        for message in ui.into_messages() {
            match message {
                Message::Action(action) => actions.push(action),
                Message::Terminal(iced_tterm::Event::BackendCall(
                    _,
                    BackendCommand::Write(bytes),
                )) => {
                    written.extend(bytes);
                },
                _ => (),
            }
        }

        assert_eq!(actions, [AppAction::NewTab]);
        assert_eq!(written, b"b");
    }

    /// Press and release keys in a focused terminal of `app`, pausing where a key is `None`.
    ///
    /// Returns the actions the keys ran and the bytes they sent to the shell.
    fn type_keys(app: &App, keys: &[Option<(Key, Modifiers)>]) -> (Vec<AppAction>, Vec<u8>) {
        use iced::keyboard::key::{NativeCode, Physical};
        use iced::keyboard::{Event, Location};

        let mut ui = Simulator::with_size(Default::default(), Size::new(800., 400.), app.view());
        // Click the terminal to give it keyboard focus.
        ui.point_at((400., 200.));
        ui.simulate(simulator::click());

        for key in keys {
            let Some((key, modifiers)) = key.clone() else {
                std::thread::sleep(std::time::Duration::from_millis(80));
                continue;
            };

            let physical_key = Physical::Unidentified(NativeCode::Unidentified);
            let text = match &key {
                Key::Character(text) if modifiers.is_empty() => Some(text.clone()),
                _ => None,
            };
            ui.simulate([
                iced::Event::Keyboard(Event::ModifiersChanged(modifiers)),
                iced::Event::Keyboard(Event::KeyPressed {
                    key: key.clone(),
                    modified_key: key.clone(),
                    physical_key,
                    location: Location::Standard,
                    modifiers,
                    text,
                    repeat: false,
                }),
                iced::Event::Keyboard(Event::KeyReleased {
                    key: key.clone(),
                    modified_key: key,
                    physical_key,
                    location: Location::Standard,
                    modifiers,
                }),
                iced::Event::Keyboard(Event::ModifiersChanged(Modifiers::empty())),
            ]);
        }

        let mut actions = Vec::new();
        let mut written = Vec::new();
        for message in ui.into_messages() {
            match message {
                Message::Action(action) => actions.push(action),
                Message::Terminal(iced_tterm::Event::BackendCall(
                    _,
                    BackendCommand::Write(bytes),
                )) => written.extend(bytes),
                _ => (),
            }
        }

        (actions, written)
    }

    fn key(character: &str) -> Option<(Key, Modifiers)> {
        Some((Key::Character(character.into()), Modifiers::empty()))
    }

    fn ctrl(character: &str) -> Option<(Key, Modifiers)> {
        Some((Key::Character(character.into()), Modifiers::CTRL))
    }

    /// Hotkeys with `Ctrl+A` as the leader key and `c` after it bound to a new tab.
    fn leader_hotkeys(timeout: u64) -> Hotkeys {
        Hotkeys::default()
            .with_leader(
                Key::Character("a".into()),
                Modifiers::CTRL,
                std::time::Duration::from_millis(timeout),
            )
            .with_sequence(Key::Character("c".into()), Modifiers::empty(), AppAction::NewTab)
    }

    #[test]
    fn leader_key_and_the_key_after_it_are_kept_from_the_shell() {
        let app = app_with("", leader_hotkeys(10_000));

        let (actions, written) = type_keys(&app, &[ctrl("a"), key("c"), key("c")]);
        assert_eq!(actions, [AppAction::NewTab]);
        // Only the second `c` is typed into the shell.
        assert_eq!(written, b"c");
    }

    #[test]
    fn unbound_key_after_the_leader_key_cancels_the_sequence() {
        use iced::keyboard::key::Named;

        let app = app_with("", leader_hotkeys(10_000));

        let (actions, written) = type_keys(&app, &[ctrl("a"), key("q"), key("c")]);
        assert_eq!(actions, []);
        assert_eq!(written, b"c");

        // Reaching for a modifier on the way to the second key doesn't cancel.
        let shift = Some((Key::Named(Named::Shift), Modifiers::SHIFT));
        let (actions, written) = type_keys(&app, &[ctrl("a"), shift, key("c")]);
        assert_eq!(actions, [AppAction::NewTab]);
        assert_eq!(written, b"");
    }

    #[test]
    fn leader_key_twice_sends_it_to_the_shell() {
        let app = app_with("", leader_hotkeys(10_000));

        let (actions, written) = type_keys(&app, &[ctrl("a"), ctrl("a"), key("c")]);
        assert_eq!(actions, []);
        assert_eq!(written, b"\x01c");
    }

    #[test]
    fn leader_key_gives_up_after_its_timeout() {
        let app = app_with("", leader_hotkeys(20));

        let (actions, written) = type_keys(&app, &[ctrl("a"), None, key("c")]);
        assert_eq!(actions, []);
        assert_eq!(written, b"c");
    }

    #[test]
    fn leader_key_is_left_to_the_shell_without_leader_bindings() {
        let hotkeys = Hotkeys::default().with_leader(
            Key::Character("a".into()),
            Modifiers::CTRL,
            std::time::Duration::from_secs(10),
        );
        let app = app_with("", hotkeys);

        let (actions, written) = type_keys(&app, &[ctrl("a"), key("c")]);
        assert_eq!(actions, []);
        assert_eq!(written, b"\x01c");
    }
}

/// Writes screenshots of tabs and split panes, rendered headlessly, for looking at by hand.
///
/// Run with `TANKTERM_SNAPSHOT_DIR=<dir> cargo test -p tankterm render_tests -- --ignored`.
#[cfg(all(test, unix))]
mod render_tests {
    use std::path::PathBuf;
    use std::time::Duration;
    use std::{env, thread};

    use iced::widget::pane_grid::Axis;
    use iced::{Size, Theme};
    use iced_test::simulator::Simulator;
    use iced_tterm::settings::BackendSettings;
    use iced_tterm::{AlacrittyEvent, BackendCommand};

    use super::*;
    use crate::cli::Options;
    use crate::config::UiConfig;
    use crate::term_settings;

    #[test]
    #[ignore = "writes screenshots for manual inspection"]
    fn tabs_and_panes() {
        let Some(dir) = env::var_os("TANKTERM_SNAPSHOT_DIR").map(PathBuf::from) else { return };
        let window = Size::new(900., 420.);

        let config = UiConfig::default();
        let mut settings = term_settings::settings(&config, &Options::default());
        settings.backend = BackendSettings {
            program: "sh".into(),
            args: vec!["-c".into(), "echo \"pane $$\"; exec cat".into()],
            ..BackendSettings::default()
        };

        let options = AppOptions {
            title: "TankTerm".into(),
            dynamic_title: true,
            layout: Layout {
                padding: Padding::new(4.),
                background: term_settings::background(&config),
                foreground: term_settings::foreground(&config),
            },
            tabs: config.tabs,
            hotkeys: Hotkeys::from_config(&config),
            session_file: None,
            profiles: Vec::new(),
            confirm_multiline_paste: true,
            copy_on_select: false,
            bindings: Vec::new(),
        };
        let mut app = App::new(options, settings.clone(), settings).unwrap();

        for action in [
            AppAction::NewTab,
            AppAction::Split(Axis::Vertical),
            AppAction::Split(Axis::Horizontal),
            AppAction::NewTab,
            AppAction::SelectTab(1),
        ] {
            let _ = app.update(Message::Action(action));
        }

        // Lay the terminals out once so they get their size, then let the shells print.
        let resizes: Vec<_> = {
            let mut ui = Simulator::with_size(Default::default(), window, app.view());
            ui.snapshot(&Theme::Dark).unwrap();
            ui.into_messages().collect()
        };
        for message in resizes {
            let _ = app.update(message);
        }
        thread::sleep(Duration::from_millis(500));
        for id in 0..app.next_terminal {
            let wakeup = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup);
            let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(id, wakeup)));
        }

        let mut ui = Simulator::with_size(Default::default(), window, app.view());
        let snapshot = ui.snapshot(&Theme::Dark).unwrap();
        assert!(snapshot.matches_image(dir.join("tabs-and-panes")).unwrap());
        drop(ui);

        // The command palette over the terminals, with a background tab showing activity.
        let wakeup = BackendCommand::ProcessAlacrittyEvent(AlacrittyEvent::Wakeup);
        let _ = app.update(Message::Terminal(iced_tterm::Event::BackendCall(0, wakeup)));
        let _ = app.update(Message::Action(AppAction::ShowCommands));
        let _ = app.update(Message::PaletteQuery("pane".into()));
        let _ = app.update(Message::Action(AppAction::PaletteNext));

        let mut ui = Simulator::with_size(Default::default(), window, app.view());
        let snapshot = ui.snapshot(&Theme::Dark).unwrap();
        assert!(snapshot.matches_image(dir.join("command-palette")).unwrap());
    }
}
