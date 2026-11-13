//! Tab and pane operations: opening, closing, moving between and resizing them.

use std::borrow::Cow;
use std::path::Path;
use std::time::Instant;
use std::{fs, io};

use iced::widget::pane_grid::{self, Direction, Node};
use iced::{Color, Task, window};
use iced_core::clipboard::Kind;
use iced_tterm::BackendCommand;
use iced_tterm::settings::Settings;
use log::{error, warn};

use super::actions::{FONT_SIZE_STEP, hide, minimize, toggle_fullscreen, toggle_maximized, zoomed};
use super::{
    App, Message, PANE_ID_ENV, PaneSelect, PendingPaste, RESIZE_LIMITS, RESIZE_STEP, Shell, Tab,
    status,
};
use crate::hotkeys::{ActiveTable, AppAction, PaneSelectMode};
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

        let shell = terminal.and_then(|id| self.terminals.get(&id));
        if let Some(directory) = shell.and_then(Shell::directory) {
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
            AppAction::ShowTabNavigator => {
                let titles = self.tabs.iter().map(|tab| self.tab_title(tab).to_owned());
                self.palette = Some(Palette::tabs(titles.collect::<Vec<_>>(), self.active));
            },
            AppAction::RenameTab => {
                let tab = &self.tabs[self.active];
                let shown = self.pane_title(tab).to_owned();
                self.palette = Some(Palette::rename(tab.title.as_deref(), &shown));
            },
            AppAction::OpenConfig => {
                let Some(path) = self.options.config_file.clone() else {
                    warn!("There is no place for a configuration file");
                    return Task::none();
                };
                if let Err(err) = create_config(&path) {
                    error!("Unable to create the configuration file {}: {err}", path.display());
                    return Task::none();
                }

                let mut settings = self.new_settings();
                (settings.backend.program, settings.backend.args) = editor(&path);
                settings.backend.working_directory = path.parent().map(Path::to_owned);
                match self.spawn(settings, self.options.layout.background) {
                    Ok(terminal) => self.push_tab(terminal),
                    Err(err) => error!("Unable to open the configuration file: {err}"),
                }
            },
            AppAction::PaneSelect(mode) => {
                let tab = &mut self.tabs[self.active];
                if tab.panes.len() < 2 {
                    return Task::none();
                }

                // Every pane is shown, to be labeled.
                tab.panes.restore();
                self.pane_select = Some(PaneSelect { mode, tab: tab.id });
            },
            AppAction::PaneSelectChoose(position) => {
                let tab = &mut self.tabs[self.active];
                // A letter that labels no pane is ignored.
                let Some(&chosen) = ordered_panes(tab.panes.layout()).get(position) else {
                    return Task::none();
                };
                let Some(PaneSelect { mode, .. }) = self.pane_select.take() else {
                    return Task::none();
                };

                match mode {
                    PaneSelectMode::Focus => tab.focus = chosen,
                    // The focused pane moves to the chosen one's place and keeps the focus.
                    PaneSelectMode::Swap if chosen != tab.focus => {
                        tab.panes.swap(tab.focus, chosen)
                    },
                    PaneSelectMode::Swap => (),
                }
            },
            AppAction::PaneSelectCancel => self.pane_select = None,
            AppAction::RotatePanes(clockwise) => {
                let tab = &mut self.tabs[self.active];
                rotate(&mut tab.panes, clockwise);
            },
            AppAction::ActivateKeyTable(name) => {
                let Some(timeout) = self
                    .options
                    .hotkeys
                    .has_table(&name)
                    .then(|| self.options.hotkeys.table_timeout(&name))
                else {
                    warn!("There is no key table named {name:?}");
                    return Task::none();
                };

                let deadline = timeout.map(|timeout| Instant::now() + timeout);
                self.key_tables.push(ActiveTable { name, deadline });
                return Task::none();
            },
            AppAction::PopKeyTable => {
                self.key_tables.pop();
                return Task::none();
            },
            AppAction::ClearKeyTables => {
                self.key_tables.clear();
                return Task::none();
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
            AppAction::SelectCommandOutput => return self.select_command_output(),
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
            marks_commands: false,
            reported_directory: None,
            process: None,
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
        match &tab.title {
            Some(title) => title,
            None => self.pane_title(tab),
        }
    }

    /// What the tab at `index` shows in the tab bar: the `tabs.title` format filled in.
    ///
    /// Placeholders without a value, like the directory where it can't be told, are left
    /// empty; placeholders the format shouldn't have stay as they are written.
    pub(super) fn tab_label(&self, index: usize, tab: &Tab) -> String {
        let focused = tab.panes.get(tab.focus).and_then(|id| self.terminals.get(id));
        let directory = focused.and_then(|shell| shell.working_directory.as_deref());
        let attention = tab
            .panes
            .iter()
            .any(|(_, id)| self.terminals.get(id).is_some_and(|shell| shell.attention));

        let label = self.options.tabs.title.template().render(|name| {
            let value: Cow<'_, str> = match name {
                "index" => (index + 1).to_string().into(),
                "title" => self.tab_title(tab).into(),
                "panes" => tab.panes.len().to_string().into(),
                "cwd" => directory.map(status::basename).unwrap_or_default().into(),
                "path" => directory.map(status::display_path).unwrap_or_default().into(),
                "process" => {
                    focused.and_then(|shell| shell.process.as_deref()).unwrap_or("").into()
                },
                "marker" => match (attention, tab.activity) {
                    (true, _) => " !",
                    (false, true) => " *",
                    (false, false) => "",
                }
                .into(),
                "zoom" => if tab.panes.maximized().is_some() { " Z" } else { "" }.into(),
                _ => format!("{{{name}}}").into(),
            };
            Some(value)
        });
        label.unwrap_or_default()
    }

    /// The title of a tab without one given to it: that of its focused pane.
    pub(super) fn pane_title<'a>(&'a self, tab: &'a Tab) -> &'a str {
        let title = tab
            .panes
            .get(tab.focus)
            .and_then(|id| self.terminals.get(id))
            .and_then(|shell| shell.title.as_deref());

        title.filter(|_| self.options.dynamic_title).unwrap_or(&self.options.title)
    }
}

/// What a configuration file starts with when `OpenConfig` creates it.
const NEW_CONFIG: &str = "\
# TankTerm configuration. The options are in tankTerm(5): `man 5 tankTerm`.
# Changes take effect when the file is saved.
";

/// Create the configuration file at `path` if there is none.
fn create_config(path: &Path) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(directory) = path.parent() {
        fs::create_dir_all(directory)?;
    }

    fs::write(path, NEW_CONFIG)
}

/// The program and arguments that edit the file at `path` in a terminal: `$VISUAL`, `$EDITOR` or
/// `vi`. The shell splits the variables, which may hold arguments.
#[cfg(unix)]
fn editor(path: &Path) -> (String, Vec<String>) {
    let script = r#"exec ${VISUAL:-${EDITOR:-vi}} "$1""#;
    let args = ["-c", script, "sh"].map(str::to_owned);
    ("sh".into(), args.into_iter().chain([path.display().to_string()]).collect())
}

#[cfg(not(unix))]
fn editor(path: &Path) -> (String, Vec<String>) {
    ("notepad.exe".into(), vec![path.display().to_string()])
}

/// Move every pane to the place of the next one, counting from left to right and top to bottom,
/// and the last one to the place of the first; or the other way around.
///
/// Panes keep their identity as they move, so the focus moves along with the focused pane.
pub(super) fn rotate(panes: &mut pane_grid::State<u64>, clockwise: bool) {
    let order = ordered_panes(panes.layout());
    let Some((&first, &last)) = order.first().zip(order.last()) else { return };

    if clockwise {
        // The last pane is swapped into each place in turn, from the back.
        for &pane in order.iter().rev().skip(1) {
            panes.swap(pane, last);
        }
    } else {
        for &pane in order.iter().skip(1) {
            panes.swap(first, pane);
        }
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
