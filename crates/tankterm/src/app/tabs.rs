//! Tab and pane operations: opening, closing, moving between and resizing them.

use std::io;

use iced::widget::pane_grid::{self, Direction, Node};
use iced::{Color, Task, window};
use iced_core::clipboard::Kind;
use iced_tterm::BackendCommand;
use iced_tterm::settings::Settings;
use log::error;

use super::actions::{FONT_SIZE_STEP, toggle_fullscreen, toggle_maximized};
use super::{
    App, Message, PendingPaste, RESIZE_LIMITS, RESIZE_STEP, Shell, Tab, working_directory,
};
use crate::hotkeys::AppAction;
use crate::palette::Palette;
use crate::session::ordered_panes;

impl App {
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
        }

        self.focus()
    }

    /// Create a terminal and spawn its shell, returning the terminal's ID.
    pub(super) fn spawn(&mut self, mut settings: Settings, background: Color) -> io::Result<u64> {
        let id = self.next_terminal;
        let working_directory = settings.backend.working_directory.clone();
        settings.font = self.zoomed(settings.font);
        let mut terminal = iced_tterm::Terminal::new(id, settings)?;
        terminal.handle(iced_tterm::Command::AddBindings(self.options.bindings.clone()));
        // Copying and pasting are bound in the configuration, like every other action.
        terminal.handle(iced_tterm::Command::RemoveClipboardKeys);

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
        self.tabs.push(Tab { id, panes, focus, seen: false, activity: false });
    }

    /// Close the pane of a terminal whose shell exited.
    pub(super) fn close_terminal(&mut self, id: u64) -> Task<Message> {
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

    /// Title of a tab: the one its focused terminal set, or the window title.
    pub(super) fn tab_title(&self, tab: &Tab) -> &str {
        let title = tab
            .panes
            .get(tab.focus)
            .and_then(|id| self.terminals.get(id))
            .and_then(|shell| shell.title.as_deref());

        title.filter(|_| self.options.dynamic_title).unwrap_or(&self.options.title)
    }
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
