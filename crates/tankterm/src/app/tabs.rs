//! Tab and pane operations: opening, closing, moving between and resizing them.

use std::io;

use iced::widget::pane_grid::{self, Direction, Node};
use iced::{Color, Task, window};
use iced_core::clipboard::Kind;
use iced_tterm::BackendCommand;
use iced_tterm::settings::Settings;
use log::error;

use super::actions::{FONT_SIZE_STEP, hide, minimize, toggle_fullscreen, toggle_maximized, zoomed};
use super::{
    App, Message, PANE_ID_ENV, PendingPaste, RESIZE_LIMITS, RESIZE_STEP, Shell, Tab,
    working_directory,
};
use crate::hotkeys::AppAction;
use crate::palette::Palette;
use crate::session::ordered_panes;

impl App {
    /// Settings for the terminal of a new tab or pane: the shell starts in the directory of the
    /// focused pane's shell, if the configuration asks for that and the directory can be told.
    fn new_settings(&self) -> Settings {
        self.new_settings_from(self.focused_terminal())
    }

    /// Settings for the terminal of a new tab or pane opened from `terminal`: the shell starts
    /// in the directory of `terminal`'s shell, if the configuration asks for that and the
    /// directory can be told.
    pub(super) fn new_settings_from(&self, terminal: Option<u64>) -> Settings {
        let mut settings = self.settings.clone();
        if !self.options.tabs.inherit_working_directory {
            return settings;
        }

        let pid = terminal
            .and_then(|id| self.terminals.get(&id))
            .and_then(|shell| shell.terminal.child_pid());
        if let Some(directory) = pid.and_then(working_directory) {
            settings.backend.working_directory = Some(directory);
        }

        settings
    }

    /// Run an action.
    pub(super) fn perform(&mut self, action: AppAction) -> Task<Message> {
        // The last tab was closed and the window is about to close: nothing is left to act on.
        let tab_count = self.tabs.len();
        if tab_count == 0 {
            return Task::none();
        }

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
                let (index, pane) = (self.active, self.tabs[self.active].focus);
                if let Err(err) = self.split(index, pane, axis, self.new_settings()) {
                    error!("Unable to split the pane: {err}");
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
                tab.panes.restore();

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
            AppAction::Copy => return self.copy(),
            AppAction::Paste => return self.paste_clipboard(Kind::Standard),
            AppAction::PasteSelection => return self.paste_clipboard(Kind::Primary),
            AppAction::IncreaseFontSize => {
                self.change_font_size(Some(FONT_SIZE_STEP));
                return Task::none();
            },
            AppAction::DecreaseFontSize => {
                self.change_font_size(Some(-FONT_SIZE_STEP));
                return Task::none();
            },
            AppAction::ResetFontSize => {
                self.change_font_size(None);
                return Task::none();
            },
            AppAction::Scroll(to) => {
                self.scroll(to);
                return Task::none();
            },
            AppAction::ClearHistory => {
                self.clear_history();
                return Task::none();
            },
            AppAction::ToggleFullscreen => return toggle_fullscreen(),
            AppAction::ToggleMaximized => return toggle_maximized(),
            AppAction::CopySelection => return self.copy_to_selection(),
            AppAction::ClearSelection => {
                self.clear_selection();
                return Task::none();
            },
            AppAction::Chars(chars) => {
                self.write(chars.as_bytes());
                return Task::none();
            },
            AppAction::RunCommand(program) => {
                self.run_command(&program);
                return Task::none();
            },
            AppAction::Minimize => return minimize(),
            AppAction::Hide => return hide(),
            AppAction::Quit => return window::latest().and_then(window::close),
        }

        self.focus()
    }

    /// Split `pane` of the tab at `index` with a new terminal and focus it in that tab,
    /// returning the new terminal's ID.
    pub(super) fn split(
        &mut self,
        index: usize,
        pane: pane_grid::Pane,
        axis: pane_grid::Axis,
        settings: Settings,
    ) -> io::Result<u64> {
        let terminal = self.spawn(settings, self.options.layout.background)?;
        let tab = &mut self.tabs[index];
        tab.panes.restore();

        match tab.panes.split(axis, pane, terminal) {
            Some((pane, _)) => {
                tab.focus = pane;
                Ok(terminal)
            },
            None => {
                self.terminals.remove(&terminal);
                Err(io::Error::other("the pane is gone"))
            },
        }
    }

    /// Create a terminal and spawn its shell, returning the terminal's ID.
    ///
    /// The shell finds the ID in `$TANKTERM_PANE_ID`.
    pub(super) fn spawn(&mut self, mut settings: Settings, background: Color) -> io::Result<u64> {
        let id = self.next_terminal;
        settings.backend.env.insert(PANE_ID_ENV.into(), id.to_string());
        let working_directory = settings.backend.working_directory.clone();
        let hold = settings.backend.hold;
        settings.font = zoomed(settings.font, self.font_zoom);
        let mut terminal = iced_tterm::Terminal::new(id, settings)?;
        terminal.handle(iced_tterm::Command::AddBindings(self.options.bindings.clone()));
        // Copying and pasting are bound in the configuration, like every other action.
        terminal.handle(iced_tterm::Command::RemoveClipboardKeys);
        if !self.window_focused {
            terminal.handle(iced_tterm::Command::WindowFocus(false));
        }

        let shell = Shell {
            terminal,
            title: None,
            background,
            busy: false,
            working_directory,
            attention: false,
            // A new terminal takes itself to have the keyboard focus.
            focused: true,
            rang: None,
            profile: None,
            hold,
        };
        self.next_terminal += 1;
        self.terminals.insert(id, shell);

        Ok(id)
    }

    /// Add a tab holding `terminal` after the last tab and show it.
    pub(super) fn push_tab(&mut self, terminal: u64) {
        let (panes, focus) = pane_grid::State::new(terminal);
        self.add_tab(panes, focus);
        self.active = self.tabs.len() - 1;
    }

    /// Add a tab after the last tab.
    pub(super) fn add_tab(&mut self, panes: pane_grid::State<u64>, focus: pane_grid::Pane) {
        let id = self.next_tab;
        self.next_tab += 1;
        self.tabs.push(Tab { id, panes, focus, seen: false, activity: false, title: None });
    }

    /// The position of the tab holding terminal `id`, and its pane.
    pub(super) fn locate(&self, id: u64) -> Option<(usize, pane_grid::Pane)> {
        self.tabs.iter().enumerate().find_map(|(index, tab)| {
            let pane = tab.panes.iter().find(|(_, terminal)| **terminal == id)?;
            Some((index, *pane.0))
        })
    }

    /// Close the pane of a terminal whose shell exited.
    pub(super) fn close_terminal(&mut self, id: u64) -> Task<Message> {
        match self.locate(id) {
            Some((index, pane)) => self.close_pane(index, pane),
            None => {
                self.terminals.remove(&id);
                Task::none()
            },
        }
    }

    /// Close a pane, and its tab if it was the tab's last pane.
    pub(super) fn close_pane(&mut self, index: usize, pane: pane_grid::Pane) -> Task<Message> {
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
    pub(super) fn close_tab(&mut self, index: usize) -> Task<Message> {
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

    /// Title of a tab: the one it was given, the one its focused terminal set, or the window
    /// title.
    pub(super) fn tab_title<'a>(&'a self, tab: &'a Tab) -> &'a str {
        if let Some(title) = &tab.title {
            return title;
        }

        let title = tab
            .panes
            .get(tab.focus)
            .and_then(|id| self.terminals.get(id))
            .and_then(|shell| shell.title.as_deref());

        title.filter(|_| self.options.dynamic_title).unwrap_or(&self.options.title)
    }
}

/// The innermost split around `pane` whose divider runs along `axis`, with its ratio.
pub(super) fn nearest_split(
    node: &Node,
    pane: pane_grid::Pane,
    axis: pane_grid::Axis,
) -> Option<(pane_grid::Split, f32)> {
    let Node::Split { id, axis: split_axis, ratio, a, b } = node else { return None };

    let child = [a, b].into_iter().find(|child| ordered_panes(child).contains(&pane))?;
    nearest_split(child, pane, axis).or_else(|| (*split_axis == axis).then_some((*id, *ratio)))
}
