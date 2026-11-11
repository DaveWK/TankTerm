//! Application state: starting up, the session file, and the messages the application handles.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::time::Instant;

use iced::widget::pane_grid;
use iced::{Subscription, Task, window};
use iced_tterm::actions::Action;
use iced_tterm::settings::Settings;
use iced_tterm::{AlacrittyEvent, BackendCommand, TermMode, TerminalView};
use log::{error, warn};

use super::{App, AppOptions, Message, PROCESS_INTERVAL, PendingPaste, working_directory};
use crate::hotkeys::Hotkeys;
use crate::palette::{self, Palette};
use crate::session::{SavedNode, SavedTab, Session, ordered_panes};

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
            font_zoom: 0.,
            next_terminal: 0,
            next_tab: 0,
            window_focused: true,
            saved_session: None,
            session_error: None,
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
        app.sync_focus();

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
    pub(super) fn session(&self) -> Session {
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
    /// A write that failed is tried again when `retry` is set, which it is with every tick, and
    /// its error is logged once, not with every attempt.
    ///
    /// The last tab closing isn't recorded, so closing a window by exiting its last shell
    /// remembers that last tab.
    fn save_session(&mut self, retry: bool) {
        let Some(path) = &self.options.session_file else { return };
        if self.tabs.is_empty() {
            return;
        }

        let session = self.session();
        if self.saved_session.as_ref() == Some(&session) {
            // The file holds these tabs and panes; a write that failed has nothing left to do.
            self.session_error = None;
            return;
        }

        if self.session_error.is_some() && !retry {
            return;
        }

        match session.save(path) {
            Ok(()) => {
                self.saved_session = Some(session);
                self.session_error = None;
            },
            Err(err) => {
                let err = err.to_string();
                if self.session_error.as_ref() != Some(&err) {
                    warn!("Unable to save the session to {}: {err}", path.display());
                }
                self.session_error = Some(err);
            },
        }
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

    /// Tell the terminals which one of them has the keyboard focus: the focused pane of the
    /// tab that is shown, while the window has focus and nothing lies over the terminals.
    ///
    /// The others draw their cursor hollow and don't blink it.
    fn sync_focus(&mut self) {
        let typing = self.window_focused && self.palette.is_none() && self.pending_paste.is_none();
        let focused = self.focused_terminal().filter(|_| typing);

        for (id, shell) in &mut self.terminals {
            let has_focus = focused == Some(*id);
            if shell.focused != has_focus {
                shell.focused = has_focus;
                shell.terminal.handle(iced_tterm::Command::Focus(has_focus));
            }
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

        // Finished commands and working directories are only known on Unix. Elsewhere ticks are
        // only needed while the session file waits to be written again.
        let ticking = cfg!(unix) || self.session_error.is_some();
        let ticks = ticking.then(|| Subscription::run(ticks));

        // A flashing visual bell is drawn anew with every frame, until it has faded out.
        let frames = self.is_flashing().then(|| window::frames().map(Message::Frame));

        Subscription::batch(terminals.chain([window]).chain(ticks).chain(frames))
    }

    /// Whether the visual bell flashes for a terminal.
    pub(super) fn is_flashing(&self) -> bool {
        self.terminals.values().any(|shell| shell.rang.is_some())
    }

    /// The bell of a terminal rang: run the bell command and start the visual bell.
    ///
    /// The command runs in the directory of the terminal's shell, where that can be told. The
    /// visual bell flashes over the terminal's pane, if its tab is shown.
    fn ring(&mut self, terminal: u64) {
        let shown = self
            .tabs
            .get(self.active)
            .is_some_and(|tab| tab.panes.iter().any(|(_, id)| *id == terminal));
        let Some(shell) = self.terminals.get_mut(&terminal) else { return };

        if shown && self.options.bell.flash.is_enabled() {
            shell.rang = Some(Instant::now());
        }

        let directory = shell.terminal.child_pid().and_then(working_directory);
        self.options.bell.run_command(directory.as_deref());
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        let tick = matches!(message, Message::Tick);
        let task = self.handle(message);

        self.shown();
        self.sync_focus();
        self.save_session(tick);

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
                    Some(AlacrittyEvent::Bell) => {
                        self.ring(id);
                        self.notify(id)
                    },
                    _ => Task::none(),
                }
            },
            Message::Tick => {
                let terminals: Vec<u64> = self.terminals.keys().copied().collect();
                let finished = terminals.into_iter().filter(|id| self.poll_process(*id));
                let finished: Vec<u64> = finished.collect();
                self.poll_directories();

                Task::batch(finished.into_iter().map(|id| self.notify(id)))
            },
            Message::Frame(now) => {
                // Flashes that have faded out are over; the frames stop with the last one.
                let flash = self.options.bell.flash;
                for shell in self.terminals.values_mut() {
                    let over = |rang| flash.is_over(now.saturating_duration_since(rang));
                    shell.rang = shell.rang.filter(|rang| !over(*rang));
                }

                Task::none()
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
        // The warning shows the text it asks about; that text isn't replaced behind its back, and
        // nothing else gets into a terminal before it is answered.
        if self.pending_paste.is_some() {
            return;
        }

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

    /// Look at the shell of a terminal: whether it runs a command.
    ///
    /// Returns whether a command finished since the shell was last looked at. A command runs
    /// while another process group than the shell's has the terminal.
    ///
    /// This runs with every output of the terminal, so it only asks the terminal for its
    /// foreground process group.
    fn poll_process(&mut self, terminal: u64) -> bool {
        let Some(shell) = self.terminals.get_mut(&terminal) else { return false };
        let (Some(pid), Some(foreground)) =
            (shell.terminal.child_pid(), shell.terminal.foreground_process_group())
        else {
            return false;
        };

        let was_busy = shell.busy;
        shell.busy = foreground != pid;

        was_busy && !shell.busy
    }

    /// Look up the directories the shells are in, which the session file remembers.
    ///
    /// This takes the file system, so it runs with the ticks and not with every output of a
    /// terminal, and not at all if there is no session file to write the directories to.
    fn poll_directories(&mut self) {
        if self.options.session_file.is_none() {
            return;
        }

        for shell in self.terminals.values_mut() {
            if let Some(directory) = shell.terminal.child_pid().and_then(working_directory) {
                shell.working_directory = Some(directory);
            }
        }
    }
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
