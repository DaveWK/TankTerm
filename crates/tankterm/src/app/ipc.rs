//! Answers to the messages `tankTerm msg` sends, which follow `wezterm cli`.

use iced::Task;
use iced::widget::pane_grid::{self, Axis};
use iced_tterm::BackendCommand;
use iced_tterm::settings::Settings;

use super::tabs::nearest_split;
use super::{App, Message, working_directory};
use crate::cli::{
    ActivateTabOptions, GetTextOptions, ProgramOptions, SetTabTitleOptions, SocketMessage,
    SplitOptions, ZoomOptions,
};
use crate::ipc::{PaneInfo, PaneSize, SocketReply};
use crate::session::ordered_panes;

/// The answer to a message and what the application does next, or why the message can't be
/// carried out.
type Answer = Result<(SocketReply, Task<Message>), String>;

impl App {
    /// Carry out a message from `tankTerm msg`, returning the answer to it.
    pub(super) fn answer(&mut self, message: SocketMessage) -> (SocketReply, Task<Message>) {
        let result = match message {
            SocketMessage::List(_) => Ok((SocketReply::Panes(self.panes()), Task::none())),
            SocketMessage::Spawn(options) => {
                self.open_tab(options.profile.as_deref(), options.program)
            },
            SocketMessage::SplitPane(options) => self.split_pane(options),
            SocketMessage::SendText(options) => self.target(options.pane_id).map(|target| {
                let text = options.text.unwrap_or_default();
                let command = if options.no_paste {
                    BackendCommand::Write(text.into_bytes())
                } else {
                    BackendCommand::Paste(text)
                };

                let shell = self.terminals.get_mut(&target.2).expect("located terminal");
                shell.terminal.handle(iced_tterm::Command::ProxyToBackend(command));
                (SocketReply::Done, Task::none())
            }),
            SocketMessage::GetText(options) => self.text(options),
            SocketMessage::ActivatePane(options) => self.target(options.pane_id).map(|target| {
                let (index, pane, _) = target;
                let tab = &mut self.tabs[index];
                // A zoomed tab shows only its zoomed pane.
                if tab.panes.maximized().is_some_and(|zoomed| zoomed != pane) {
                    tab.panes.restore();
                }
                tab.focus = pane;
                self.active = index;

                (SocketReply::Done, self.focus())
            }),
            SocketMessage::ActivateTab(options) => self.activate_tab(options),
            SocketMessage::KillPane(options) => self.target(options.pane_id).map(|target| {
                let (index, pane, _) = target;
                (SocketReply::Done, self.close_pane(index, pane))
            }),
            SocketMessage::ZoomPane(options) => self.zoom(options),
            SocketMessage::SetTabTitle(options) => self.set_tab_title(options),
            SocketMessage::CreateWindow(_) => Err(unsupported("create-window")),
            SocketMessage::Config(_) => Err(unsupported("config")),
            SocketMessage::GetConfig(_) => Err(unsupported("get-config")),
        };

        result.unwrap_or_else(|err| (SocketReply::Error(err), Task::none()))
    }

    /// The panes of all tabs.
    fn panes(&self) -> Vec<PaneInfo> {
        let mut panes = Vec::new();
        for (index, tab) in self.tabs.iter().enumerate() {
            let tab_title = self.tab_title(tab);

            for pane in ordered_panes(tab.panes.layout()) {
                let Some(id) = tab.panes.get(pane) else { continue };
                let Some(shell) = self.terminals.get(id) else { continue };

                let title = shell.title.as_deref().filter(|_| self.options.dynamic_title);
                let cwd = shell.terminal.child_pid().and_then(working_directory);
                let cwd = cwd.or_else(|| shell.working_directory.clone());
                panes.push(PaneInfo {
                    window_id: 0,
                    tab_id: tab.id,
                    tab_index: index,
                    pane_id: *id,
                    size: PaneSize {
                        rows: shell.terminal.screen_lines(),
                        cols: shell.terminal.columns(),
                    },
                    title: title.unwrap_or(&self.options.title).to_owned(),
                    cwd: cwd.map(|cwd| cwd.display().to_string()).unwrap_or_default(),
                    tab_title: tab_title.to_owned(),
                    is_active: pane == tab.focus,
                    is_zoomed: tab.panes.maximized() == Some(pane),
                    is_active_tab: index == self.active,
                });
            }
        }

        panes
    }

    /// Open a tab running `program`, from the profile named `profile` if there is one.
    fn open_tab(&mut self, profile: Option<&str>, program: ProgramOptions) -> Answer {
        let (settings, background) = match profile {
            Some(name) => {
                let profiles = &self.options.profiles;
                let profile = profiles.iter().find(|profile| profile.name == name);
                let profile = profile.ok_or_else(|| format!("no profile named {name:?}"))?;
                (profile.settings.clone(), profile.background)
            },
            None => {
                (self.new_settings_from(self.focused_terminal()), self.options.layout.background)
            },
        };

        let id = self.spawn(with_program(settings, program), background);
        let id = id.map_err(|err| format!("unable to open a tab: {err}"))?;
        if let Some(shell) = self.terminals.get_mut(&id) {
            // Remembered for when the configuration changes the profile.
            shell.profile = profile.map(str::to_owned);
        }
        self.push_tab(id);

        Ok((SocketReply::Pane(id), self.focus()))
    }

    /// Split a pane, below it unless the options put the new pane elsewhere.
    fn split_pane(&mut self, options: SplitOptions) -> Answer {
        let (index, pane, terminal) = self.target(options.pane_id)?;
        let (axis, first) = if options.left {
            (Axis::Vertical, true)
        } else if options.right || options.horizontal {
            (Axis::Vertical, false)
        } else {
            (Axis::Horizontal, options.top)
        };

        let settings = with_program(self.new_settings_from(Some(terminal)), options.program);
        let id = self.split(index, pane, axis, settings);
        let id = id.map_err(|err| format!("unable to split the pane: {err}"))?;

        // The new pane is focused, and comes second in its split until it is moved first.
        let tab = &mut self.tabs[index];
        if first {
            tab.panes.swap(pane, tab.focus);
        }
        if let Some(percent) = options.percent {
            let share = f32::from(percent) / 100.;
            let ratio = if first { share } else { 1. - share };
            if let Some((split, _)) = nearest_split(tab.panes.layout(), tab.focus, axis) {
                tab.panes.resize(split, ratio);
            }
        }

        Ok((SocketReply::Pane(id), self.focus()))
    }

    /// The lines of a pane: those on the screen unless others are asked for.
    fn text(&self, options: GetTextOptions) -> Answer {
        let (_, _, id) = self.target(options.pane_id)?;
        let terminal = &self.terminals[&id].terminal;

        let start = options.start_line.unwrap_or(0);
        let end = options.end_line.unwrap_or(terminal.screen_lines() as i32 - 1);
        Ok((SocketReply::Text(terminal.lines(start, end)), Task::none()))
    }

    /// Show a tab, by ID, position or distance from the shown tab.
    fn activate_tab(&mut self, options: ActivateTabOptions) -> Answer {
        let count = self.tabs.len() as isize;
        let index = match options {
            ActivateTabOptions { tab_id: Some(id), .. } => self.tab_index(id)?,
            ActivateTabOptions { tab_index: Some(index), .. } => {
                let position = if index < 0 { count + index } else { index };
                if !(0..count).contains(&position) {
                    return Err(format!("no tab at position {index}"));
                }
                position as usize
            },
            ActivateTabOptions { tab_relative: Some(offset), no_wrap, .. } => {
                let position = self.active as isize + offset;
                let position =
                    if no_wrap { position.clamp(0, count - 1) } else { position.rem_euclid(count) };
                position as usize
            },
            _ => return Err("no tab to activate".into()),
        };

        self.active = index;
        Ok((SocketReply::Done, self.focus()))
    }

    /// Zoom a pane or unzoom its tab.
    fn zoom(&mut self, options: ZoomOptions) -> Answer {
        let (index, pane, _) = self.target(options.pane_id)?;
        let tab = &mut self.tabs[index];

        let zoomed = tab.panes.maximized() == Some(pane);
        let unzoom = options.unzoom || (options.toggle && zoomed);
        if unzoom {
            tab.panes.restore();
        } else {
            // A tab with one pane has nothing to zoom it over.
            if tab.panes.len() > 1 {
                tab.panes.maximize(pane);
            }
            tab.focus = pane;
        }

        Ok((SocketReply::Done, self.focus()))
    }

    /// Give a tab a title, or take it away again with an empty one.
    fn set_tab_title(&mut self, options: SetTabTitleOptions) -> Answer {
        let index = match options.tab_id {
            Some(id) => self.tab_index(id)?,
            None => self.target(options.pane_id)?.0,
        };

        let title = options.title;
        self.tabs[index].title = (!title.is_empty()).then_some(title);
        Ok((SocketReply::Done, Task::none()))
    }

    /// Position of the tab with ID `id`.
    fn tab_index(&self, id: u64) -> Result<usize, String> {
        let index = self.tabs.iter().position(|tab| tab.id == id);
        index.ok_or_else(|| format!("no tab with ID {id}"))
    }

    /// The tab position, pane and terminal ID of the pane with ID `pane_id`, or of the focused
    /// pane without one.
    fn target(&self, pane_id: Option<u64>) -> Result<(usize, pane_grid::Pane, u64), String> {
        let id = match pane_id {
            Some(id) => id,
            None => self.focused_terminal().ok_or("there is no pane")?,
        };

        let (index, pane) = self.locate(id).ok_or_else(|| format!("no pane with ID {id}"))?;
        Ok((index, pane, id))
    }
}

/// `settings` running the program of `options`, in its directory.
fn with_program(mut settings: Settings, options: ProgramOptions) -> Settings {
    if let Some(cwd) = options.cwd {
        settings.backend.working_directory = Some(cwd);
    }
    if let Some((program, args)) = options.command.split_first() {
        settings.backend.program = program.clone();
        settings.backend.args = args.to_vec();
    }

    settings
}

fn unsupported(message: &str) -> String {
    format!("`tankTerm msg {message}` is not supported by the iced front end yet")
}
