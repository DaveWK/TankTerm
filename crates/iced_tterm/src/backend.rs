use crate::actions::Action;
use crate::hint::{HintAction, HintCommand, HintMatch, HintSettings, Hints};
use crate::search::{Search, SearchCommand};
use crate::settings::BackendSettings;
use crate::theme::Theme;
use crate::vi::{Vi, ViCommand};
use iced::keyboard::Modifiers;
use iced_core::Size;
use std::borrow::Cow;
use std::cmp::min;
use std::io::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tankterm_terminal::event::{
    Event, EventListener, Notify, OnResize, WindowSize,
};
use tankterm_terminal::event_loop::{EventLoop, Msg, Notifier};
use tankterm_terminal::grid::{Dimensions, Scroll};
use tankterm_terminal::index::{Column, Direction, Line, Point, Side};
use tankterm_terminal::selection::{Selection, SelectionRange, SelectionType};
use tankterm_terminal::sync::FairMutex;
use tankterm_terminal::term::cell::{Cell, Flags};
use tankterm_terminal::term::search::Match;
use tankterm_terminal::term::{
    self, color::Colors, test::TermSize, viewport_to_point, Term, TermMode,
};
use tankterm_terminal::vi_mode::ViMotion;
use tankterm_terminal::vte::ansi::{ClearMode, CursorStyle, Handler, Rgb};
use tankterm_terminal::{tty, Grid};
use tokio::sync::mpsc;

/// The bytes to send for pasted text.
///
/// With bracketed paste the text goes between `\x1b[200~` and `\x1b[201~`,
/// which makes the application (readline, TUIs) take the paste as one block:
/// line breaks in it are not read as Enter. ESC (0x1b) and Ctrl-C (0x03) are
/// stripped first, mirroring Alacritty: otherwise a clipboard that itself
/// contains `\x1b[201~` would end the bracketed paste early, with the rest
/// read as keystrokes, and some shells end bracketed paste on 0x03.
///
/// Without bracketed paste the application sees the text as if it was typed,
/// so line breaks are sent as the carriage return the Enter key sends, like
/// Alacritty does.
pub(crate) fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    if !bracketed {
        return text.replace("\r\n", "\r").replace('\n', "\r").into_bytes();
    }

    let filtered = text.bytes().filter(|&b| b != 0x1b && b != 0x03);
    let mut out = Vec::with_capacity(text.len() + 12);
    out.extend_from_slice(b"\x1b[200~");
    out.extend(filtered);
    out.extend_from_slice(b"\x1b[201~");
    out
}

#[derive(Debug, Clone)]
pub enum Command {
    Write(Vec<u8>),
    /// Paste text, the way the application running in the terminal expects
    /// it: wrapped in bracketed paste markers if it enabled them, and with
    /// line breaks sent as carriage returns otherwise.
    ///
    /// The widget publishes this for its paste bindings. An application that
    /// wants a say in what gets pasted can hold the command back instead of
    /// passing it on to [`crate::Terminal::handle`]; [`crate::Terminal::mode`]
    /// tells whether bracketed paste is on.
    Paste(String),
    /// Scroll by this many lines, the way the mouse wheel does: on the
    /// alternate screen, applications that asked for it get arrow keys
    /// instead.
    Scroll(i32),
    /// Move the view through the scrollback buffer. In vi mode, scrolling
    /// by pages or to the top or bottom takes the vi cursor along.
    ScrollDisplay(Scroll),
    /// Scroll up half a screen, taking the vi cursor along in vi mode.
    ScrollHalfPageUp,
    /// Scroll down half a screen, taking the vi cursor along in vi mode.
    ScrollHalfPageDown,
    /// Drop the scrollback buffer, keeping what is on the screen.
    ClearHistory,
    /// Scroll the view to put the prompt before the top of the view at
    /// the top, where the shell marks its prompts (OSC 133).
    ScrollToPreviousPrompt,
    /// Scroll the view to put the prompt after the top of the view at the
    /// top, or to the bottom.
    ScrollToNextPrompt,
    /// Select the output of the command whose prompt is at the top of the
    /// view, or of the last command while the view is at the bottom, where
    /// the shell marks its commands (OSC 133).
    SelectCommandOutput,
    /// Select the output of the command at this point of the grid: the
    /// output the point is in, or the output of the command typed at the
    /// prompt it is in.
    SelectCommandOutputAt(Point),
    /// A click at this point of the grid, without a drag: a shell that takes
    /// clicks in its command line (OSC 133 `click_events` or `cl`) moves its
    /// cursor there.
    PromptClick(Point),
    /// The cursor's blink interval passed. The widget's subscription sends
    /// this while the cursor blinks.
    BlinkCursor,
    Resize(Option<Size<f32>>, Option<Size<f32>>),
    SelectStart(SelectionType, (f32, f32)),
    SelectUpdate((f32, f32)),
    /// Extend the selection to this point, making it of this type, or start
    /// one of this type there if there is none.
    ExpandSelection(SelectionType, (f32, f32)),
    /// Drop the selection, if there is one.
    ClearSelection,
    MouseReport(MouseButton, Modifiers, Point, bool),
    ProcessAlacrittyEvent(Event),
    /// Search the terminal's text. While the search bar is shown, it takes
    /// the terminal's last row.
    Search(SearchCommand),
    /// Enter or leave vi mode, or act in it.
    Vi(ViCommand),
    /// Label hints, choose one, or tell the hints where the mouse is.
    Hint(HintCommand),
}

#[derive(Debug, Clone)]
pub enum MouseMode {
    Sgr,
    Normal(bool),
}

impl From<TermMode> for MouseMode {
    fn from(term_mode: TermMode) -> Self {
        if term_mode.contains(TermMode::SGR_MOUSE) {
            MouseMode::Sgr
        } else if term_mode.contains(TermMode::UTF8_MOUSE) {
            MouseMode::Normal(true)
        } else {
            MouseMode::Normal(false)
        }
    }
}

#[derive(Debug, Clone)]
pub enum MouseButton {
    LeftButton = 0,
    MiddleButton = 1,
    RightButton = 2,
    LeftMove = 32,
    MiddleMove = 33,
    RightMove = 34,
    NoneMove = 35,
    ScrollUp = 64,
    ScrollDown = 65,
    Other = 99,
}

#[derive(Clone, Copy, Debug)]
pub struct TerminalSize {
    pub cell_width: u16,
    pub cell_height: u16,
    num_cols: u16,
    num_lines: u16,
    /// Rows below the grid that the search bar takes.
    bar_lines: u16,
    layout_width: f32,
    layout_height: f32,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            cell_width: 1,
            cell_height: 1,
            num_cols: 80,
            num_lines: 50,
            bar_lines: 0,
            layout_width: 80.0,
            layout_height: 50.0,
        }
    }
}

impl TerminalSize {
    /// How far the grid is moved into `space` to centre it: half of what
    /// whole cells leave over on each axis, in whole pixels.
    ///
    /// The search bar below the grid is centred along with it. A grid
    /// bigger than `space`, which it is for a moment while the terminal
    /// hasn't been resized yet, isn't moved.
    pub fn centring_offset(&self, space: Size<f32>) -> iced_core::Vector {
        let half_left_over = |space: f32, cell: u16, cells: u16| {
            ((space - f32::from(cell) * f32::from(cells)) / 2.)
                .floor()
                .max(0.)
        };

        iced_core::Vector::new(
            half_left_over(space.width, self.cell_width, self.num_cols),
            half_left_over(
                space.height,
                self.cell_height,
                self.num_lines + self.bar_lines,
            ),
        )
    }
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.screen_lines()
    }

    fn columns(&self) -> usize {
        self.num_cols as usize
    }

    fn last_column(&self) -> Column {
        Column(self.num_cols as usize - 1)
    }

    fn bottommost_line(&self) -> Line {
        Line(self.num_lines as i32 - 1)
    }

    fn screen_lines(&self) -> usize {
        self.num_lines as usize
    }
}

impl From<TerminalSize> for WindowSize {
    fn from(size: TerminalSize) -> Self {
        Self {
            num_lines: size.num_lines,
            num_cols: size.num_cols,
            cell_width: size.cell_width,
            cell_height: size.cell_height,
        }
    }
}

pub struct Backend {
    term: Arc<FairMutex<Term<EventProxy>>>,
    size: TerminalSize,
    /// Whether the widget was given the size of its layout yet. Until then
    /// `size` holds a placeholder layout, which isn't divided into cells.
    laid_out: bool,
    notifier: Notifier,
    /// The options the `Term` was last given.
    config: term::Config,
    last_content: RenderableContent,
    search: Search,
    vi: Vi,
    hints: Hints,
    /// The program started in the terminal and the PTY it talks to.
    #[cfg(unix)]
    process: Process,
}

/// The process started in a terminal, as far as it can be asked about.
#[cfg(unix)]
struct Process {
    pid: u32,
    /// A second handle to the PTY master; the PTY event loop owns the first.
    master: Option<std::fs::File>,
}

impl Backend {
    /// Tell the terminal whether the colors are dark or light, for applications that ask.
    pub(crate) fn set_color_scheme(
        &mut self,
        scheme: tankterm_terminal::term::ColorScheme,
    ) {
        self.term.lock().set_color_scheme(scheme);
    }

    /// Spawn the shell.
    ///
    /// `color_scheme` is set before the PTY reader starts, so the shell can query it right away.
    /// `scrollback` is the most lines the scrollback buffer keeps and
    /// `cursor_style` the cursor's style until the application sets one.
    pub fn new(
        id: u64,
        event_proxy: EventProxy,
        settings: BackendSettings,
        color_scheme: tankterm_terminal::term::ColorScheme,
        scrollback: usize,
        cursor_style: CursorStyle,
    ) -> Result<Self> {
        let pty_config = tty::Options {
            drain_on_exit: settings.hold,
            shell: Some(tty::Shell::new(settings.program, settings.args)),
            working_directory: settings.working_directory,
            env: settings.env,
            #[cfg(target_os = "windows")]
            escape_args: false,
        };

        let config = term_config(scrollback, cursor_style);
        let terminal_size = TerminalSize::default();
        let pty = tty::new(&pty_config, terminal_size.into(), id)?;
        #[cfg(unix)]
        let process = Process {
            pid: pty.child().id(),
            master: pty.file().try_clone().ok(),
        };

        let mut term =
            Term::new(config.clone(), &terminal_size, event_proxy.clone());
        term.set_color_scheme(color_scheme);

        let cursor = term.grid_mut().cursor_cell().clone();

        let initial_content = RenderableContent {
            grid: term.grid().clone(),
            selectable_range: None,
            terminal_mode: *term.mode(),
            terminal_size,
            cursor: cursor.clone(),
            cursor_style: term.cursor_style(),
            colors: *term.colors(),
            search_matches: Vec::new(),
            focused_match: None,
            search_bar: None,
            vi_cursor: None,
            line_indicator: None,
            hint_matches: Vec::new(),
            hint_labels: Vec::new(),
            hint_highlights: Vec::new(),
            hint_hovered: false,
            hyperlink_previews: Vec::new(),
        };

        let term = Arc::new(FairMutex::new(term));

        let pty_event_loop = EventLoop::new(
            term.clone(),
            event_proxy,
            pty,
            pty_config.drain_on_exit,
            false,
        )?;

        let notifier = Notifier(pty_event_loop.channel());

        let _ = pty_event_loop.spawn();

        Ok(Self {
            term: term.clone(),
            size: terminal_size,
            laid_out: false,
            notifier,
            config,
            last_content: initial_content,
            search: Search::default(),
            vi: Vi::default(),
            hints: Hints::default(),
            #[cfg(unix)]
            process,
        })
    }

    /// Change how many lines the scrollback buffer keeps. Lines that no
    /// longer fit are dropped, the oldest first.
    pub(crate) fn set_scrollback(&mut self, scrollback: usize) {
        if self.config.scrolling_history == scrollback {
            return;
        }

        self.config.scrolling_history = scrollback;
        self.term.lock().set_options(self.config.clone());
    }

    /// Change the characters that end a word for semantic selection,
    /// besides whitespace.
    pub(crate) fn set_semantic_escape_chars(&mut self, chars: &str) {
        if self.config.semantic_escape_chars == chars {
            return;
        }

        self.config.semantic_escape_chars = chars.to_owned();
        self.term.lock().set_options(self.config.clone());
    }

    /// Change the style the cursor has while the application in the
    /// terminal hasn't set one.
    pub(crate) fn set_default_cursor_style(&mut self, style: CursorStyle) {
        if self.config.default_cursor_style == style {
            return;
        }

        self.config.default_cursor_style = style;
        self.term.lock().set_options(self.config.clone());
    }

    /// Change the style the cursor has in vi mode; without one, it is the
    /// cursor's style outside vi mode.
    pub(crate) fn set_vi_cursor_style(&mut self, style: Option<CursorStyle>) {
        if self.config.vi_mode_cursor_style == style {
            return;
        }

        self.config.vi_mode_cursor_style = style;
        self.term.lock().set_options(self.config.clone());
    }

    /// Use these hints.
    pub(crate) fn set_hints(&mut self, settings: HintSettings) {
        self.hints.set(settings);
    }

    /// Whether hints are labeled, for keys to choose one.
    pub fn hint_active(&self) -> bool {
        self.hints.active()
    }

    /// Whether some hints are underlined under the mouse.
    pub fn mouse_hints(&self) -> bool {
        self.hints.for_mouse()
    }

    /// Whether the search bar is shown.
    pub fn search_active(&self) -> bool {
        self.search.active()
    }

    /// Whether a vi mode inline search waits for the character to move to.
    pub fn inline_search_pending(&self) -> bool {
        self.vi.inline_search_pending()
    }

    /// While the search while typing found no match in the lines it looked
    /// at, a number that changes with the regex. The whole buffer is
    /// searched on [`SearchCommand::Resume`].
    pub fn search_pending(&self) -> Option<u64> {
        self.search.pending()
    }

    /// The terminal's modes as they are right now, not as of the last sync.
    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    /// The selected text, if any text is selected.
    pub fn selection(&self) -> Option<String> {
        let text = self.term.lock().selection_to_string()?;
        (!text.is_empty()).then_some(text)
    }

    /// How many lines the screen has.
    pub fn screen_lines(&self) -> usize {
        self.size.num_lines as usize
    }

    /// Number of columns of the terminal.
    pub fn columns(&self) -> usize {
        self.size.num_cols as usize
    }

    /// ID of the process started in the terminal, where there is one to ask.
    pub fn child_pid(&self) -> Option<u32> {
        #[cfg(unix)]
        return Some(self.process.pid);
        #[cfg(not(unix))]
        None
    }

    /// The process group in the foreground of the terminal: the one that
    /// reads its input and gets its signals.
    ///
    /// This is the shell's own group (the same number as
    /// [`Self::child_pid`], since the shell leads its session) while it
    /// waits at its prompt, and the group of a command while one runs.
    pub fn foreground_process_group(&self) -> Option<u32> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;

            let master = self.process.master.as_ref()?;
            // SAFETY: the descriptor is open for as long as `master` lives.
            let group = unsafe { libc::tcgetpgrp(master.as_raw_fd()) };
            u32::try_from(group).ok().filter(|group| *group > 0)
        }
        #[cfg(not(unix))]
        None
    }

    pub fn handle(&mut self, cmd: Command) -> Action {
        let mut action = Action::default();
        let term = self.term.clone();
        let mut term = term.lock();
        let search_shown = self.search.active();
        match cmd {
            Command::ProcessAlacrittyEvent(event) => {
                match event {
                    Event::Exit => {
                        action = Action::Shutdown;
                    },
                    Event::Title(title) => {
                        action = Action::ChangeTitle(title);
                    },
                    Event::PtyWrite(pty) => {
                        self.notifier.notify(pty.into_bytes())
                    },
                    _ => {},
                };
            },
            Command::Write(input) => {
                self.write(input);
                term.scroll_display(Scroll::Bottom);
            },
            Command::Paste(text) => {
                let bracketed = term.mode().contains(TermMode::BRACKETED_PASTE);
                self.write(paste_bytes(&text, bracketed));
                term.scroll_display(Scroll::Bottom);
            },
            Command::Scroll(delta) => {
                self.scroll(&mut term, delta);
            },
            Command::ScrollDisplay(scroll) => {
                let screen = term.screen_lines() as i32;
                let cursor_lines = match scroll {
                    Scroll::PageUp => screen,
                    Scroll::PageDown => -screen,
                    _ => 0,
                };
                self.scroll_display(&mut term, scroll, cursor_lines);
            },
            Command::ScrollHalfPageUp => {
                let lines = term.screen_lines() as i32 / 2;
                self.scroll_display(&mut term, Scroll::Delta(lines), lines);
            },
            Command::ScrollHalfPageDown => {
                let lines = -(term.screen_lines() as i32 / 2);
                self.scroll_display(&mut term, Scroll::Delta(lines), lines);
            },
            Command::ClearHistory => {
                term.clear_screen(ClearMode::Saved);
            },
            Command::ScrollToPreviousPrompt => {
                let offset = term.grid().display_offset();
                term.scroll_to_prompt(Direction::Left);
                self.scrolled_from(&term, offset);
            },
            Command::ScrollToNextPrompt => {
                let offset = term.grid().display_offset();
                term.scroll_to_prompt(Direction::Right);
                self.scrolled_from(&term, offset);
            },
            Command::SelectCommandOutput => {
                term.select_command_output();
            },
            Command::SelectCommandOutputAt(point) => {
                term.select_command_output_at(point);
            },
            // A click on a hint chooses the hint instead.
            Command::PromptClick(point) => {
                let keys = term.prompt_click(point);
                if let Some(keys) =
                    keys.filter(|_| self.hints.under_pointer(&term).is_none())
                {
                    self.write(keys);
                }
            },
            // The cursor's blinking is the widget's, not the terminal's.
            Command::BlinkCursor => {},
            Command::Resize(layout_size, font_measure) => {
                self.resize(&mut term, layout_size, font_measure);
            },
            Command::SelectStart(selection_type, (x, y)) => {
                self.start_selection(&mut term, selection_type, x, y);
            },
            Command::SelectUpdate((x, y)) => {
                self.update_selection(&mut term, x, y);
            },
            Command::ExpandSelection(selection_type, (x, y)) => {
                match &mut term.selection {
                    Some(selection) => selection.ty = selection_type,
                    None => {
                        self.start_selection(&mut term, selection_type, x, y)
                    },
                }
                self.update_selection(&mut term, x, y);
            },
            Command::ClearSelection => term.selection = None,
            Command::MouseReport(button, modifiers, point, pressed) => {
                self.process_mouse_report(button, modifiers, point, pressed);
            },
            Command::Search(command) => {
                self.search.handle(&mut term, command);
            },
            Command::Vi(ViCommand::Open) => {
                if let Some(found) = self.hints.under_vi_cursor(&term) {
                    action = self.choose_hint(&mut term, found);
                }
            },
            Command::Vi(command) => {
                let offset = term.grid().display_offset();
                let center = command == ViCommand::CenterAroundCursor;
                self.vi.handle(&mut term, &mut self.search, command);
                if center {
                    self.scrolled_from(&term, offset);
                }
            },
            Command::Hint(command) => {
                action = self.hint(&mut term, command);
            },
        };

        // The search bar takes a row from the terminal or gives it back.
        if self.search.active() != search_shown {
            self.resize(&mut term, None, None);
        }

        action
    }

    /// Scroll the view. In vi mode, the vi cursor moves `cursor_lines` up
    /// with it, to the first character on its line, and scrolling to the
    /// top or bottom takes it to the first or last line, as in Alacritty.
    fn scroll_display(
        &mut self,
        terminal: &mut Term<EventProxy>,
        scroll: Scroll,
        cursor_lines: i32,
    ) {
        let offset = terminal.grid().display_offset();
        if terminal.mode().contains(TermMode::VI) && cursor_lines != 0 {
            let cursor = terminal.vi_mode_cursor;
            terminal.vi_mode_cursor = cursor.scroll(terminal, cursor_lines);
        }

        terminal.scroll_display(scroll);

        if terminal.mode().contains(TermMode::VI) {
            match scroll {
                Scroll::Top => {
                    terminal.vi_mode_cursor.point.line =
                        terminal.topmost_line();
                    terminal.vi_motion(ViMotion::FirstOccupied);
                },
                Scroll::Bottom => {
                    terminal.vi_mode_cursor.point.line =
                        terminal.bottommost_line();
                    // Twice, to get past a line that wraps.
                    terminal.vi_motion(ViMotion::FirstOccupied);
                    terminal.vi_motion(ViMotion::FirstOccupied);
                },
                _ => (),
            }
        }

        self.scrolled_from(terminal, offset);
    }

    /// Tell the search the view was scrolled from `offset` other than by
    /// the search.
    fn scrolled_from(&mut self, terminal: &Term<EventProxy>, offset: usize) {
        let lines = offset as i32 - terminal.grid().display_offset() as i32;
        self.search.scrolled(lines);
    }

    /// Act on a command for the hints.
    fn hint(
        &mut self,
        terminal: &mut Term<EventProxy>,
        command: HintCommand,
    ) -> Action {
        match command {
            HintCommand::Start(index) => self.hints.start(terminal, index),
            HintCommand::Input(text) => {
                for c in text.chars() {
                    if let Some(found) = self.hints.input(terminal, c) {
                        return self.choose_hint(terminal, found);
                    }
                }
            },
            HintCommand::Backspace => self.hints.backspace(terminal),
            HintCommand::Cancel => self.hints.stop(),
            HintCommand::Pointer(pointer) => self.hints.set_pointer(pointer),
            HintCommand::OpenHovered => {
                if let Some(found) = self.hints.under_pointer(terminal) {
                    return self.choose_hint(terminal, found);
                }
            },
        }

        Action::Ignore
    }

    /// Do what a hint does with its text, as far as the terminal does it,
    /// and tell the application the rest.
    fn choose_hint(
        &mut self,
        terminal: &mut Term<EventProxy>,
        found: HintMatch,
    ) -> Action {
        // The terminal may have changed since the hint was found.
        let Some(text) = self.hints.text(terminal, &found) else {
            return Action::Ignore;
        };
        let (start, end) = (*found.bounds().start(), *found.bounds().end());

        let hint_action = self.hints.action(found.hint()).clone();
        match &hint_action {
            HintAction::Paste if self.search.active() => {
                let text = text.replace(['\r', '\n'], "");
                self.search.handle(terminal, SearchCommand::Input(text));
            },
            HintAction::Paste => {
                let bracketed =
                    terminal.mode().contains(TermMode::BRACKETED_PASTE);
                self.write(paste_bytes(&text, bracketed));
                terminal.scroll_display(Scroll::Bottom);
            },
            HintAction::Select => {
                let mut selection =
                    Selection::new(SelectionType::Simple, start, Side::Left);
                selection.update(end, Side::Right);
                terminal.selection = Some(selection);
            },
            HintAction::MoveViModeCursor => {
                let offset = terminal.grid().display_offset();
                if !terminal.mode().contains(TermMode::VI) {
                    self.vi.handle(
                        terminal,
                        &mut self.search,
                        ViCommand::Toggle,
                    );
                }
                terminal.vi_goto_point(start);
                self.scrolled_from(terminal, offset);
            },
            HintAction::Copy | HintAction::Command { .. } => (),
        }

        Action::Hint(hint_action, text)
    }

    fn process_mouse_report(
        &self,
        button: MouseButton,
        modifiers: Modifiers,
        point: Point,
        pressed: bool,
    ) {
        let mut mods = 0;
        if modifiers.contains(Modifiers::SHIFT) {
            mods += 4;
        }
        if modifiers.contains(Modifiers::ALT) {
            mods += 8;
        }
        if modifiers.contains(Modifiers::COMMAND) {
            mods += 16;
        }

        match MouseMode::from(self.last_content.terminal_mode) {
            MouseMode::Sgr => {
                self.sgr_mouse_report(point, button as u8 + mods, pressed)
            },
            MouseMode::Normal(is_utf8) => {
                if pressed {
                    self.normal_mouse_report(
                        point,
                        button as u8 + mods,
                        is_utf8,
                    )
                } else {
                    self.normal_mouse_report(point, 3 + mods, is_utf8)
                }
            },
        }
    }

    fn sgr_mouse_report(&self, point: Point, button: u8, pressed: bool) {
        let c = if pressed { 'M' } else { 'm' };

        let msg = format!(
            "\x1b[<{};{};{}{}",
            button,
            point.column + 1,
            point.line + 1,
            c
        );

        self.notifier.notify(msg.as_bytes().to_vec());
    }

    fn normal_mouse_report(&self, point: Point, button: u8, is_utf8: bool) {
        let Point { line, column } = point;
        let max_point = if is_utf8 { 2015 } else { 223 };

        if line >= max_point || column >= max_point {
            return;
        }

        let mut msg = vec![b'\x1b', b'[', b'M', 32 + button];

        let mouse_pos_encode = |pos: usize| -> Vec<u8> {
            let pos = 32 + 1 + pos;
            let first = 0xC0 + pos / 64;
            let second = 0x80 + (pos & 63);
            vec![first as u8, second as u8]
        };

        if is_utf8 && column >= Column(95) {
            msg.append(&mut mouse_pos_encode(column.0));
        } else {
            msg.push(32 + 1 + column.0 as u8);
        }

        if is_utf8 && line >= 95 {
            msg.append(&mut mouse_pos_encode(line.0 as usize));
        } else {
            msg.push(32 + 1 + line.0 as u8);
        }

        self.notifier.notify(msg);
    }

    fn start_selection(
        &mut self,
        terminal: &mut Term<EventProxy>,
        selection_type: SelectionType,
        x: f32,
        y: f32,
    ) {
        let location = Self::selection_point(
            x,
            y,
            &self.size,
            terminal.grid().display_offset(),
        );
        terminal.selection = Some(Selection::new(
            selection_type,
            location,
            self.selection_side(x),
        ));

        // In vi mode, the vi cursor goes where the mouse is.
        if terminal.mode().contains(TermMode::VI) && !self.search.active() {
            terminal.vi_mode_cursor.point = location;
        }
    }

    fn update_selection(
        &mut self,
        terminal: &mut Term<EventProxy>,
        x: f32,
        y: f32,
    ) {
        let display_offset = terminal.grid().display_offset();
        let vi_mode =
            terminal.mode().contains(TermMode::VI) && !self.search.active();
        let location = Self::selection_point(x, y, &self.size, display_offset);
        let side = self.selection_side(x);
        let Some(selection) = &mut terminal.selection else {
            return;
        };
        selection.update(location, side);

        // In vi mode, the vi cursor goes where the mouse is, and the cell
        // under it is selected.
        if vi_mode {
            selection.include_all();
            terminal.vi_mode_cursor.point = location;
        }
    }

    pub fn selection_point(
        x: f32,
        y: f32,
        terminal_size: &TerminalSize,
        display_offset: usize,
    ) -> Point {
        let col = (x as usize) / (terminal_size.cell_width as usize);
        let col = min(Column(col), Column(terminal_size.num_cols as usize - 1));

        let line = (y as usize) / (terminal_size.cell_height as usize);
        let line = min(line, terminal_size.num_lines as usize - 1);

        viewport_to_point(display_offset, Point::new(line, col))
    }

    fn selection_side(&self, x: f32) -> Side {
        let cell_x = x as usize % self.size.cell_width as usize;
        let half_cell_width = (self.size.cell_width as f32 / 2.0) as usize;

        if cell_x > half_cell_width {
            Side::Right
        } else {
            Side::Left
        }
    }

    fn resize(
        &mut self,
        terminal: &mut Term<EventProxy>,
        layout_size: Option<Size<f32>>,
        font_measure: Option<Size<f32>>,
    ) {
        if let Some(size) = layout_size {
            self.size.layout_height = size.height;
            self.size.layout_width = size.width;
            self.laid_out = true;
        };

        if let Some(size) = font_measure {
            // A font too small to measure a pixel still has cells to divide
            // the layout by.
            self.size.cell_height = (size.height as u16).max(1);
            self.size.cell_width = (size.width as u16).max(1);
        }

        // The placeholder layout of a terminal that wasn't laid out yet is
        // a few cells of a real font, and the program in it would start in
        // a terminal of that size.
        if !self.laid_out {
            return;
        }

        // The search bar takes the last row.
        let lines = (self.size.layout_height / self.size.cell_height as f32)
            .floor() as u16;
        let bar_lines = u16::from(self.search.active());
        let lines = lines.saturating_sub(bar_lines);
        let cols = (self.size.layout_width / self.size.cell_width as f32)
            .floor() as u16;
        if lines > 0 && cols > 0 {
            if (lines, cols) != (self.size.num_lines, self.size.num_cols) {
                // The focused match isn't where it was in a resized grid.
                self.search.clear_focused_match();
            }
            self.size.num_lines = lines;
            self.size.bar_lines = bar_lines;
            self.size.num_cols = cols;
            self.notifier.on_resize(self.size.into());
            terminal.resize(TermSize::new(
                self.size.num_cols as usize,
                self.size.num_lines as usize,
            ));
        }
    }

    fn write<I: Into<Cow<'static, [u8]>>>(&self, input: I) {
        self.notifier.notify(input);
    }

    fn scroll(&mut self, terminal: &mut Term<EventProxy>, delta_value: i32) {
        if delta_value != 0 {
            let scroll = Scroll::Delta(delta_value);
            if terminal
                .mode()
                .contains(TermMode::ALTERNATE_SCROLL | TermMode::ALT_SCREEN)
            {
                let line_cmd = if delta_value > 0 { b'A' } else { b'B' };
                let mut content = vec![];

                for _ in 0..delta_value.abs() {
                    content.push(0x1b);
                    content.push(b'O');
                    content.push(line_cmd);
                }

                self.notifier.notify(content);
            } else {
                let offset = terminal.grid().display_offset();
                // The terminal keeps the vi cursor in the view.
                terminal.scroll_display(scroll);
                self.scrolled_from(terminal, offset);
            }
        }
    }

    /// Answer an application's color query (OSC 4/10/11/12) with the color
    /// the widget renders for `index`, see [`color_request_reply`].
    pub(crate) fn answer_color_request(
        &self,
        index: usize,
        formatter: &dyn Fn(Rgb) -> String,
        theme: &Theme,
    ) {
        // Read the live color table rather than the last synced snapshot, so
        // a set immediately followed by a query is answered with the new
        // value.
        let colors = *self.term.lock().colors();
        if let Some(reply) =
            color_request_reply(theme, &colors, index, formatter)
        {
            self.notifier.notify(reply.into_bytes());
        }
    }

    pub fn selectable_content(&self) -> String {
        self.term.lock().selection_to_string().unwrap_or_default()
    }

    /// The whole buffer as text, scrollback first and the visible screen last.
    ///
    /// [`Term::bounds_to_string`] does the work: it already trims each line at
    /// its last written cell, skips wide-character spacers, collapses tab runs
    /// and joins a row flagged `WRAPLINE` to the next, so what comes back is
    /// logical lines rather than however the window happened to split them.
    ///
    /// Unlike [`Self::renderable_content`] this reads the terminal itself, not
    /// the last synced snapshot of it.
    pub fn text(&self) -> Vec<String> {
        let term = self.term.lock();
        // The grid indexes scrollback with negative lines and the visible
        // screen from zero, so the oldest history row is the negated history
        // size and the newest is the bottommost line.
        let start = Point::new(Line(-(term.history_size() as i32)), Column(0));
        let end = Point::new(term.bottommost_line(), term.last_column());
        term.bounds_to_string(start, end)
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// The rows from `start` to `end` as text, both included.
    ///
    /// Row 0 is the top of the screen when it isn't scrolled, negative rows
    /// are scrollback, and rows outside the buffer are left out. Like
    /// [`Self::text`], rows a line wrapped across come back joined.
    pub fn lines(&self, start: i32, end: i32) -> Vec<String> {
        let term = self.term.lock();
        let start = start.max(-(term.history_size() as i32));
        let end = end.min(term.bottommost_line().0);
        if start > end {
            return Vec::new();
        }

        let start = Point::new(Line(start), Column(0));
        let end = Point::new(Line(end), term.last_column());
        term.bounds_to_string(start, end)
            .lines()
            .map(str::to_owned)
            .collect()
    }

    pub fn sync(&mut self) {
        let term = self.term.clone();
        let mut term = term.lock();
        self.internal_sync(&mut term);
    }

    fn internal_sync(&mut self, terminal: &mut Term<EventProxy>) {
        let selectable_range = match &terminal.selection {
            Some(s) => s.to_range(terminal),
            None => None,
        };

        // In vi mode, the vi cursor is the one shown.
        let vi_cursor = terminal.mode().contains(TermMode::VI).then(|| {
            let mut point = terminal.vi_mode_cursor.point;
            if terminal.grid()[point]
                .flags
                .contains(Flags::WIDE_CHAR_SPACER)
            {
                point.column -= 1;
            }
            point
        });
        let cursor = match vi_cursor {
            Some(point) => terminal.grid()[point].clone(),
            None => terminal.grid_mut().cursor_cell().clone(),
        };
        self.last_content.grid = terminal.grid().clone();
        self.last_content.selectable_range = selectable_range;
        self.last_content.cursor = cursor;
        self.last_content.vi_cursor = vi_cursor;
        self.last_content.line_indicator =
            line_indicator(terminal, vi_cursor, self.search.active());
        self.last_content.cursor_style = terminal.cursor_style();
        self.last_content.terminal_mode = *terminal.mode();
        self.last_content.terminal_size = self.size;
        self.last_content.colors = *terminal.colors();
        self.last_content.search_matches =
            self.search.visible_matches(terminal);
        self.last_content.focused_match = self.search.focused_match().cloned();
        self.last_content.search_bar =
            self.search.bar(self.size.num_cols as usize);

        // The labels follow what the terminal shows.
        self.hints.update(terminal);
        let (matches, labels) = self.hints.labels();
        self.last_content.hint_matches = matches.to_vec();
        self.last_content.hint_labels = labels.to_vec();

        let under_pointer = self.hints.under_pointer(terminal);
        self.last_content.hint_hovered = under_pointer.is_some();
        let under_vi_cursor = self.hints.under_vi_cursor(terminal);
        self.last_content.hint_highlights =
            under_pointer.into_iter().chain(under_vi_cursor).collect();
        self.last_content.hyperlink_previews = hyperlink_previews(
            terminal,
            &self.last_content.hint_highlights,
            self.hints.pointer_line(),
        );
    }

    pub fn renderable_content(&self) -> &RenderableContent {
        &self.last_content
    }
}

/// The URIs of the hyperlinks underlined under the mouse and the vi
/// cursor, with the rows of the view they are shown on: from the bottom up,
/// leaving out the rows of the mouse pointer and the vi cursor while there
/// are rows enough. They are cut short to fit on a row.
fn hyperlink_previews<T>(
    terminal: &Term<T>,
    highlights: &[HintMatch],
    pointer_line: Option<usize>,
) -> Vec<(usize, String)> {
    let columns = terminal.columns();
    let uris: Vec<String> = highlights
        .iter()
        .filter_map(HintMatch::uri)
        .map(|uri| shorten(uri, columns))
        .collect();
    if uris.is_empty() {
        return Vec::new();
    }

    let screen_lines = terminal.screen_lines();
    let mut taken = Vec::new();
    if screen_lines > uris.len() * 2 {
        let offset = terminal.grid().display_offset() as i32;
        let vi_line = terminal
            .mode()
            .contains(TermMode::VI)
            .then(|| terminal.vi_mode_cursor.point.line.0 + offset)
            .and_then(|line| usize::try_from(line).ok());
        taken.extend(pointer_line);
        taken.extend(vi_line);
    }

    let rows = (0..screen_lines).rev().filter(|row| !taken.contains(row));
    rows.zip(uris).collect()
}

/// `text`, cut short with an ellipsis to fit in `columns` cells.
fn shorten(text: &str, columns: usize) -> String {
    use unicode_width::UnicodeWidthChar;

    if text.chars().map(|c| c.width().unwrap_or(0)).sum::<usize>() <= columns {
        return text.to_owned();
    }

    let mut width = 0;
    let mut short: String = text
        .chars()
        .take_while(|c| {
            width += c.width().unwrap_or(0);
            width < columns
        })
        .collect();
    short.push('…');
    short
}

/// The text of the line indicator: in vi mode, how many lines the vi cursor
/// is above the last line, and otherwise while searching, how far the view
/// is scrolled; out of the buffer's lines.
///
/// In vi mode it is left out while it would cover the vi cursor.
fn line_indicator<T>(
    terminal: &Term<T>,
    vi_cursor: Option<Point>,
    searching: bool,
) -> Option<String> {
    let offset = terminal.grid().display_offset();
    let line = match vi_cursor {
        Some(point) => (terminal.bottommost_line() - point.line).0 as usize,
        None if searching => offset,
        None => return None,
    };

    let text = format!("[{}/{}]", line, terminal.total_lines() - 1);
    let column = terminal.columns().saturating_sub(text.len());
    let covers_cursor = vi_cursor.is_some_and(|point| {
        point.line.0 == -(offset as i32) && point.column.0 >= column
    });

    (!covers_cursor).then_some(text)
}

/// The configuration of the widget's `Term`, keeping at most `scrollback`
/// lines of history and showing the cursor with `cursor_style` by default.
fn term_config(scrollback: usize, cursor_style: CursorStyle) -> term::Config {
    term::Config {
        scrolling_history: scrollback,
        default_cursor_style: cursor_style,
        // Let applications opt into the kitty keyboard protocol; keys are
        // encoded for whatever flags they enable (see `crate::keyboard`), and
        // nothing changes for applications that don't.
        kitty_keyboard: true,
        ..term::Config::default()
    }
}

/// The bytes to write back to the PTY for a
/// [`Event::ColorRequest`]`(index, formatter)`.
///
/// The color is the one the widget renders for that color table entry: the
/// value the application set at runtime (`overrides`, i.e. `Term::colors`)
/// if any, otherwise the theme's palette color. See [`Theme::rgb_for_index`].
pub(crate) fn color_request_reply(
    theme: &Theme,
    overrides: &Colors,
    index: usize,
    formatter: &dyn Fn(Rgb) -> String,
) -> Option<String> {
    theme.rgb_for_index(index, overrides).map(formatter)
}

pub struct RenderableContent {
    pub grid: Grid<Cell>,
    pub selectable_range: Option<SelectionRange>,
    pub cursor: Cell,
    /// The cursor's shape and whether it blinks, as the application in the
    /// terminal set them or else as configured.
    pub cursor_style: CursorStyle,
    pub terminal_mode: TermMode,
    pub terminal_size: TerminalSize,
    /// Colors the application changed at runtime (OSC 4/10/11/12); `None`
    /// entries use the theme's palette.
    pub colors: Colors,
    /// The search's matches in the view, and in the lines next to it for
    /// those that reach into it, in order.
    pub search_matches: Vec<Match>,
    /// The search match the view was scrolled to.
    pub focused_match: Option<Match>,
    /// The search bar's text, while it is shown on the row below the grid.
    pub search_bar: Option<String>,
    /// Where the vi cursor is, in vi mode. It is drawn instead of the
    /// terminal's cursor, with the vi cursor's colors.
    pub vi_cursor: Option<Point>,
    /// Where the vi cursor or the view is in the buffer, for the top right
    /// corner of the view: in vi mode, and while the search bar is shown.
    pub line_indicator: Option<String>,
    /// The labeled hints in the view, in order, while keys choose one.
    pub hint_matches: Vec<Match>,
    /// What is left of the label of each match in
    /// [`Self::hint_matches`] after the keys typed.
    pub hint_labels: Vec<Vec<char>>,
    /// The hints underlined under the mouse and the vi cursor.
    pub hint_highlights: Vec<HintMatch>,
    /// Whether a hint is underlined under the mouse, for a click to choose.
    pub hint_hovered: bool,
    /// The URIs of the hyperlinks in [`Self::hint_highlights`], with the
    /// rows of the view they are shown on.
    pub hyperlink_previews: Vec<(usize, String)>,
}

impl Default for RenderableContent {
    fn default() -> Self {
        Self {
            grid: Grid::new(0, 0, 0),
            selectable_range: None,
            cursor: Cell::default(),
            cursor_style: CursorStyle::default(),
            terminal_mode: TermMode::empty(),
            terminal_size: TerminalSize::default(),
            colors: Colors::default(),
            search_matches: Vec::new(),
            focused_match: None,
            search_bar: None,
            vi_cursor: None,
            line_indicator: None,
            hint_matches: Vec::new(),
            hint_labels: Vec::new(),
            hint_highlights: Vec::new(),
            hint_hovered: false,
            hyperlink_previews: Vec::new(),
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        let _ = self.notifier.0.send(Msg::Shutdown);
    }
}

/// Create the channel that carries terminal events from the PTY event loop to
/// the widget's subscription.
pub(crate) fn event_channel(capacity: usize) -> (EventProxy, EventReceiver) {
    let (sender, receiver) = mpsc::channel(capacity);
    let wakeup_pending = Arc::new(AtomicBool::new(false));
    (
        EventProxy {
            sender,
            wakeup_pending: wakeup_pending.clone(),
        },
        EventReceiver {
            receiver,
            wakeup_pending,
        },
    )
}

/// Forwards the terminal's events to the widget.
///
/// [`Event::Wakeup`]s are coalesced: while one is queued and not yet received,
/// further ones are dropped, as the queued one already makes the widget sync
/// with the terminal's latest state. Without this a busy application could
/// fill the bounded channel with redundant wakeups, and events that must not
/// be lost (`PtyWrite` and `ColorRequest` replies to the application's
/// queries) would be dropped instead.
///
/// The PTY event loop only sends a wakeup when there is visible output: bytes
/// inside a synchronized update (DEC mode 2026) are held back by the parser
/// and not applied to the terminal until the update ends or times out.
#[derive(Clone)]
pub struct EventProxy {
    sender: mpsc::Sender<Event>,
    wakeup_pending: Arc<AtomicBool>,
}

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        if let Event::Wakeup = event {
            if self.wakeup_pending.swap(true, Ordering::AcqRel) {
                return;
            }

            if self.sender.try_send(event).is_err() {
                // Not queued: let the next wakeup try again.
                self.wakeup_pending.store(false, Ordering::Release);
            }
            return;
        }

        let _ = self.sender.try_send(event);
    }
}

/// The receiving end of [`event_channel`].
pub(crate) struct EventReceiver {
    receiver: mpsc::Receiver<Event>,
    wakeup_pending: Arc<AtomicBool>,
}

impl EventReceiver {
    pub(crate) async fn recv(&mut self) -> Option<Event> {
        let event = self.receiver.recv().await;
        if let Some(Event::Wakeup) = event {
            // Cleared before the widget syncs for this wakeup, so output
            // arriving after that sync sends a new one.
            self.wakeup_pending.store(false, Ordering::Release);
        }
        event
    }

    /// The next event, waiting for at most `timeout`. Returns `None` once
    /// the time is up.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn recv_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> Option<Event> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("a runtime for waiting on terminal events");
        runtime.block_on(async {
            tokio::time::timeout(timeout, self.recv())
                .await
                .ok()
                .flatten()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use tankterm_terminal::vte::ansi::Processor;

    /// Collects the events a `Term` emits.
    #[derive(Clone, Default)]
    struct EventCollector(Arc<Mutex<Vec<Event>>>);

    impl EventListener for EventCollector {
        fn send_event(&self, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    struct TestTerm {
        term: Term<EventCollector>,
        parser: Processor,
        events: EventCollector,
    }

    impl TestTerm {
        fn new() -> Self {
            let events = EventCollector::default();
            let term = Term::new(
                term_config(10_000, CursorStyle::default()),
                &TermSize::new(80, 24),
                events.clone(),
            );
            Self {
                term,
                parser: Processor::new(),
                events,
            }
        }

        fn feed(&mut self, bytes: &[u8]) {
            self.parser.advance(&mut self.term, bytes);
        }

        /// Feed `query` and answer the resulting `ColorRequest` like the
        /// widget does.
        fn query(&mut self, theme: &Theme, query: &[u8]) -> Option<String> {
            self.events.0.lock().unwrap().clear();
            self.feed(query);
            let events = std::mem::take(&mut *self.events.0.lock().unwrap());
            let mut replies = events.into_iter().filter_map(|e| match e {
                Event::ColorRequest(index, formatter) => color_request_reply(
                    theme,
                    self.term.colors(),
                    index,
                    &*formatter,
                ),
                _ => None,
            });
            let reply = replies.next();
            assert!(replies.next().is_none());
            reply
        }
    }

    #[test]
    fn color_queries_are_answered_from_the_theme() {
        let theme = Theme::default();
        let mut t = TestTerm::new();

        // OSC 11 (background), BEL and ST terminated: the reply keeps the
        // query's terminator.
        assert_eq!(
            t.query(&theme, b"\x1b]11;?\x07").as_deref(),
            Some("\x1b]11;rgb:1818/1818/1818\x07")
        );
        assert_eq!(
            t.query(&theme, b"\x1b]11;?\x1b\\").as_deref(),
            Some("\x1b]11;rgb:1818/1818/1818\x1b\\")
        );
        // OSC 10 (foreground) and OSC 12 (cursor, painted in the foreground).
        assert_eq!(
            t.query(&theme, b"\x1b]10;?\x1b\\").as_deref(),
            Some("\x1b]10;rgb:d8d8/d8d8/d8d8\x1b\\")
        );
        assert_eq!(
            t.query(&theme, b"\x1b]12;?\x1b\\").as_deref(),
            Some("\x1b]12;rgb:d8d8/d8d8/d8d8\x1b\\")
        );
        // OSC 4 (indexed): a palette color and a color cube entry.
        assert_eq!(
            t.query(&theme, b"\x1b]4;1;?\x1b\\").as_deref(),
            Some("\x1b]4;1;rgb:acac/4242/4242\x1b\\")
        );
        assert_eq!(
            t.query(&theme, b"\x1b]4;196;?\x1b\\").as_deref(),
            Some("\x1b]4;196;rgb:ffff/0000/0000\x1b\\")
        );
    }

    #[test]
    fn color_queries_report_colors_set_by_the_application() {
        let theme = Theme::default();
        let mut t = TestTerm::new();

        t.feed(b"\x1b]11;#102030\x1b\\");
        assert_eq!(
            t.query(&theme, b"\x1b]11;?\x1b\\").as_deref(),
            Some("\x1b]11;rgb:1010/2020/3030\x1b\\")
        );

        t.feed(b"\x1b]4;1;rgb:aa/bb/cc\x1b\\");
        assert_eq!(
            t.query(&theme, b"\x1b]4;1;?\x1b\\").as_deref(),
            Some("\x1b]4;1;rgb:aaaa/bbbb/cccc\x1b\\")
        );

        t.feed(b"\x1b]12;#010203\x1b\\");
        assert_eq!(
            t.query(&theme, b"\x1b]12;?\x1b\\").as_deref(),
            Some("\x1b]12;rgb:0101/0202/0303\x1b\\")
        );

        // Resetting (OSC 111 / OSC 104) goes back to the theme.
        t.feed(b"\x1b]111\x1b\\\x1b]104;1\x1b\\");
        assert_eq!(
            t.query(&theme, b"\x1b]11;?\x1b\\").as_deref(),
            Some("\x1b]11;rgb:1818/1818/1818\x1b\\")
        );
        assert_eq!(
            t.query(&theme, b"\x1b]4;1;?\x1b\\").as_deref(),
            Some("\x1b]4;1;rgb:acac/4242/4242\x1b\\")
        );
    }

    #[test]
    fn color_queries_follow_a_custom_palette() {
        let palette = crate::ColorPalette {
            background: String::from("#ffffff"),
            foreground: String::from("#000000"),
            ..Default::default()
        };
        let theme =
            Theme::new(crate::settings::ThemeSettings::new(Box::new(palette)));
        let mut t = TestTerm::new();

        assert_eq!(
            t.query(&theme, b"\x1b]11;?\x07").as_deref(),
            Some("\x1b]11;rgb:ffff/ffff/ffff\x07")
        );
        assert_eq!(
            t.query(&theme, b"\x1b]10;?\x07").as_deref(),
            Some("\x1b]10;rgb:0000/0000/0000\x07")
        );
    }

    fn pty_writes(t: &TestTerm) -> Vec<String> {
        t.events
            .0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                Event::PtyWrite(text) => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn applications_can_enable_the_kitty_keyboard_protocol() {
        let mut t = TestTerm::new();
        assert!(!t.term.mode().intersects(TermMode::KITTY_KEYBOARD_PROTOCOL));

        // Query, then push "disambiguate | report event types".
        t.feed(b"\x1b[?u\x1b[>3u\x1b[?u");
        assert!(t.term.mode().contains(
            TermMode::DISAMBIGUATE_ESC_CODES | TermMode::REPORT_EVENT_TYPES
        ));
        assert_eq!(pty_writes(&t), ["\x1b[?0u", "\x1b[?3u"]);

        // Popping restores legacy encoding.
        t.feed(b"\x1b[<u");
        assert!(!t.term.mode().intersects(TermMode::KITTY_KEYBOARD_PROTOCOL));
    }

    fn screen_text(term: &Term<EventCollector>) -> String {
        let start = Point::new(Line(0), Column(0));
        let end = Point::new(term.bottommost_line(), term.last_column());
        term.bounds_to_string(start, end).trim_end().to_owned()
    }

    #[test]
    fn synchronized_update_is_not_applied_until_it_ends() {
        // The widget draws from snapshots of the `Term` (`Backend::sync`),
        // taken whenever any event reaches it. That is only free of partial
        // frames because the parser holds back everything inside a
        // synchronized update (DEC mode 2026) instead of applying it, and
        // the PTY event loop doesn't send a wakeup for held-back bytes
        // (`sync_bytes_count`).
        let mut t = TestTerm::new();
        t.feed(b"before");
        assert_eq!(screen_text(&t.term), "before");

        t.feed(b"\x1b[?2026h\x1b[2J\x1b[Hhalf a fr");
        assert!(t.parser.sync_bytes_count() > 0);
        assert_eq!(screen_text(&t.term), "before");

        t.feed(b"ame\x1b[?2026l");
        assert_eq!(t.parser.sync_bytes_count(), 0);
        assert_eq!(screen_text(&t.term), "half a frame");
    }

    #[test]
    fn synchronized_update_mode_is_reported_as_supported() {
        // DECRQM for mode 2026: Bubble Tea and others only use synchronized
        // output when the terminal reports it (`2` = reset, i.e. known).
        let mut t = TestTerm::new();
        t.feed(b"\x1b[?2026$p");
        assert_eq!(pty_writes(&t), ["\x1b[?2026;2$y"]);
    }

    /// The shell can query the color scheme as soon as it starts, before `Backend::new` returns.
    #[cfg(unix)]
    #[tokio::test]
    async fn color_scheme_is_set_before_shell_starts() {
        use tankterm_terminal::term::ColorScheme;

        let (proxy, mut receiver) = event_channel(100);
        let settings = BackendSettings {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "printf '\\033[?996n'; sleep 2".into()],
            ..BackendSettings::default()
        };
        let _backend = Backend::new(
            0,
            proxy,
            settings,
            ColorScheme::Light,
            10_000,
            CursorStyle::default(),
        )
        .unwrap();

        let reply =
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    match receiver.recv().await {
                        Some(Event::PtyWrite(text)) => return text,
                        Some(_) => continue,
                        None => panic!("event channel closed"),
                    }
                }
            })
            .await
            .expect("no reply to the color scheme query");

        assert_eq!(reply, "\x1b[?997;2n");
    }

    #[test]
    fn bracketed_paste_is_wrapped_and_cannot_be_ended_by_the_text() {
        let wrapped = paste_bytes("line 1\nline 2", true);
        assert_eq!(wrapped, b"\x1b[200~line 1\nline 2\x1b[201~");

        // A clipboard containing the end marker or Ctrl-C must not be able
        // to close the bracketed paste early.
        let wrapped = paste_bytes("a\x1b[201~b\x03c", true);
        assert_eq!(wrapped, b"\x1b[200~a[201~bc\x1b[201~");
    }

    #[test]
    fn plain_paste_sends_line_breaks_like_the_enter_key() {
        assert_eq!(
            paste_bytes("one\ntwo\r\nthree\r", false),
            b"one\rtwo\rthree\r"
        );
        // Without the markers there is nothing for an escape to break out of.
        assert_eq!(paste_bytes("a\x1bb", false), b"a\x1bb");
    }

    /// How long a test waits for a program before it gives up. Only a test
    /// that is about to fail waits this long.
    #[cfg(unix)]
    const PATIENCE: std::time::Duration = std::time::Duration::from_secs(10);

    /// Poll `condition` until it holds. Only for state nothing sends an
    /// event about, like which process group has the terminal; wait for a
    /// program's output with [`Running::wait_until`].
    #[cfg(unix)]
    fn poll_until(what: &str, mut condition: impl FnMut() -> bool) {
        let deadline = std::time::Instant::now() + PATIENCE;
        while !condition() {
            assert!(std::time::Instant::now() < deadline, "{what}");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    /// A terminal with a running program, and the events it sends.
    #[cfg(unix)]
    struct Running {
        backend: Backend,
        events: EventReceiver,
    }

    #[cfg(unix)]
    impl Running {
        fn new(settings: BackendSettings, scrollback: usize) -> Self {
            let (proxy, events) = event_channel(100);
            let backend = Backend::new(
                0,
                proxy,
                settings,
                tankterm_terminal::term::ColorScheme::Dark,
                scrollback,
                CursorStyle::default(),
            )
            .unwrap();
            Self { backend, events }
        }

        /// Wait until `condition` holds, checking it again after each event
        /// the terminal sends: the PTY event loop sends a wakeup once it
        /// parsed the program's output.
        fn wait_until(
            &mut self,
            what: &str,
            mut condition: impl FnMut(&mut Backend) -> bool,
        ) {
            let deadline = std::time::Instant::now() + PATIENCE;
            while !condition(&mut self.backend) {
                let left = deadline
                    .saturating_duration_since(std::time::Instant::now());
                assert!(!left.is_zero(), "{what}");
                self.events.recv_timeout(left);
            }
        }
    }

    #[cfg(unix)]
    impl std::ops::Deref for Running {
        type Target = Backend;

        fn deref(&self) -> &Backend {
            &self.backend
        }
    }

    #[cfg(unix)]
    impl std::ops::DerefMut for Running {
        fn deref_mut(&mut self) -> &mut Backend {
            &mut self.backend
        }
    }

    #[cfg(unix)]
    fn backend_running(script: &str) -> Running {
        backend_with_scrollback(script, 10_000)
    }

    /// A terminal running `script` that keeps `scrollback` lines of history.
    #[cfg(unix)]
    fn backend_with_scrollback(script: &str, scrollback: usize) -> Running {
        let settings = BackendSettings {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
            ..BackendSettings::default()
        };
        Running::new(settings, scrollback)
    }

    /// What a program printed right before it exited is kept when the
    /// terminal holds: the PTY is read once more after the exit.
    #[cfg(unix)]
    #[test]
    fn held_terminal_keeps_the_last_output() {
        let settings = BackendSettings {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "printf held; exit 3".into()],
            hold: true,
            ..BackendSettings::default()
        };
        let mut backend = Running::new(settings, 100);

        backend.wait_until("the output was lost", |backend| {
            backend.text().concat().contains("held")
        });
    }

    #[cfg(unix)]
    #[test]
    fn lines_count_from_the_top_of_the_screen() {
        // 250 lines and the cursor's line fill the 50 rows of the screen
        // from "202" on, with 201 lines of history above them.
        let mut backend =
            backend_with_scrollback("seq 1 250; exec cat", 10_000);
        backend.wait_until("the output never ended", |backend| {
            backend.text().iter().any(|line| line == "250")
        });

        assert_eq!(backend.lines(0, 1), ["202", "203"]);
        assert_eq!(backend.lines(-1, -1), ["201"]);
        assert_eq!(backend.lines(47, 48), ["249", "250"]);
        // Rows outside the buffer are left out.
        assert_eq!(backend.lines(-1000, -200), ["1", "2"]);
        assert_eq!(
            backend.lines(48, 1000).first().map(String::as_str),
            Some("250")
        );
        assert!(backend.lines(1000, 2000).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn scrollback_keeps_the_configured_number_of_lines() {
        // The default size has 50 lines, so 250 lines of output leave 201
        // lines of history behind, counting the line of the cursor.
        let script = "seq 1 250; exec cat";

        let first = |backend: &Backend| backend.text().first().cloned();

        let mut backend = backend_with_scrollback(script, 10_000);
        backend.wait_until("the output never ended", |backend| {
            backend.text().iter().any(|line| line == "250")
        });
        assert_eq!(first(&backend).as_deref(), Some("1"));

        // 20 lines of history above the 49 lines of output on the screen.
        let mut backend = backend_with_scrollback(script, 20);
        backend.wait_until("the history never filled up", |backend| {
            first(backend).as_deref() == Some("182")
        });

        // A smaller buffer drops the oldest lines, and no buffer leaves
        // only the screen.
        backend.set_scrollback(5);
        assert_eq!(first(&backend).as_deref(), Some("197"));
        backend.set_scrollback(0);
        assert_eq!(first(&backend).as_deref(), Some("202"));

        // Scrolling has nowhere to go without history.
        backend.handle(Command::Scroll(3));
        backend.sync();
        assert_eq!(backend.renderable_content().grid.display_offset(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn pastes_follow_the_bracketed_paste_mode() {
        let mut backend = backend_running("printf '\\033[?2004h'; exec cat");
        backend.wait_until("bracketed paste was not enabled", |backend| {
            backend.mode().contains(TermMode::BRACKETED_PASTE)
        });

        // The terminal echoes the markers of the paste in caret notation.
        backend.handle(Command::Paste("one\ntwo".into()));
        backend.wait_until("the paste was not wrapped", |backend| {
            backend.text().concat().contains("^[[200~one")
        });
    }

    #[cfg(unix)]
    #[test]
    fn selection_is_the_selected_text() {
        let mut backend = backend_running("printf 'hello world'; exec cat");
        backend.wait_until("nothing was printed", |backend| {
            backend.text().concat().contains("hello world")
        });
        assert_eq!(backend.selection(), None);

        // Cells of the default size are a pixel wide: drag over five of them.
        backend.handle(Command::SelectStart(SelectionType::Simple, (0., 0.)));
        assert_eq!(backend.selection(), None);
        backend.handle(Command::SelectUpdate((5.6, 0.)));
        assert_eq!(backend.selection().as_deref(), Some("hello"));

        backend.handle(Command::ClearSelection);
        assert_eq!(backend.selection(), None);
    }

    #[cfg(unix)]
    #[test]
    fn terminal_keeps_its_size_until_it_is_laid_out() {
        use tankterm_terminal::grid::Dimensions;

        let mut backend = backend_running("exec cat");
        let size = |backend: &Backend| {
            let term = backend.term.lock();
            (term.columns(), term.screen_lines())
        };
        assert_eq!(size(&backend), (80, 50));

        // The font is known before the layout: the size stays.
        let cell = Size::new(8., 16.);
        backend.handle(Command::Resize(None, Some(cell)));
        assert_eq!(size(&backend), (80, 50));

        backend.handle(Command::Resize(Some(Size::new(800., 320.)), None));
        assert_eq!(size(&backend), (100, 20));
        backend.handle(Command::Resize(None, Some(Size::new(10., 32.))));
        assert_eq!(size(&backend), (80, 10));
    }

    #[cfg(unix)]
    #[test]
    fn search_bar_takes_a_row_and_shows_the_matches() {
        use crate::search::SearchDirection;
        use tankterm_terminal::grid::Dimensions;

        let mut backend = backend_running("printf needle; exec cat");
        let lines = |backend: &Backend| backend.term.lock().screen_lines();
        backend.handle(Command::Resize(
            Some(Size::new(800., 320.)),
            Some(Size::new(8., 16.)),
        ));
        assert_eq!(lines(&backend), 20);

        let forward = SearchCommand::Start(SearchDirection::Forward);
        backend.handle(Command::Search(forward));
        assert!(backend.search_active());
        assert_eq!(lines(&backend), 19);

        backend.wait_until("the program prints its needle", |backend| {
            backend.sync();
            backend.text().concat().contains("needle")
        });
        backend.handle(Command::Search(SearchCommand::Input("needle".into())));
        backend.sync();
        let content = backend.renderable_content();
        assert_eq!(content.search_bar.as_deref(), Some("Search: needle"));
        assert_eq!(content.search_matches.len(), 1);
        assert_eq!(
            content.focused_match,
            Some(content.search_matches[0].clone())
        );

        backend.handle(Command::Search(SearchCommand::Cancel));
        assert!(!backend.search_active());
        assert_eq!(lines(&backend), 20);
        assert_eq!(backend.selection().as_deref(), Some("needle"));
    }

    #[cfg(unix)]
    #[test]
    fn hints_are_chosen_by_label_mouse_and_vi_cursor() {
        use crate::hint::{Hint, HintAction, HintCommand, HintSettings};
        use crate::vi::ViCommand;

        let mut backend = backend_running(
            "printf 'go https://a.example and https://b.example\\n'; exec cat",
        );
        backend.wait_until("the program prints", |backend| {
            backend.text().concat().contains("b.example")
        });
        let hint = |action| Hint {
            regex: Some("https://[a-z.]+".into()),
            hyperlinks: false,
            post_processing: false,
            persist: false,
            action,
            mouse: Some(Modifiers::empty()),
        };
        backend.set_hints(HintSettings {
            alphabet: "jf".into(),
            hints: vec![
                hint(HintAction::Copy),
                hint(HintAction::Select),
                hint(HintAction::MoveViModeCursor),
            ],
        });
        let a = || Action::Hint(HintAction::Copy, "https://a.example".into());
        let label = |backend: &mut Backend, label: &str| {
            backend.handle(Command::Hint(HintCommand::Input(label.into())))
        };

        // The last match gets the shortest label.
        backend.handle(Command::Hint(HintCommand::Start(0)));
        backend.sync();
        let labels = &backend.renderable_content().hint_labels;
        assert_eq!(labels, &[vec!['f', 'j'], vec!['j']]);
        assert_eq!(label(&mut backend, "f"), Action::Ignore);
        assert_eq!(label(&mut backend, "j"), a());
        assert!(!backend.hint_active());

        backend.handle(Command::Hint(HintCommand::Start(1)));
        let b = "https://b.example".to_owned();
        let selected = Action::Hint(HintAction::Select, b.clone());
        assert_eq!(label(&mut backend, "j"), selected);
        assert_eq!(backend.selection(), Some(b.clone()));

        // The mouse underlines the first hint for it, with nothing selected.
        let pointer = (Point::new(0, Column(5)), Modifiers::empty());
        backend.handle(Command::Hint(HintCommand::Pointer(Some(pointer))));
        backend.sync();
        assert!(!backend.renderable_content().hint_hovered);
        backend.handle(Command::ClearSelection);
        backend.sync();
        assert!(backend.renderable_content().hint_hovered);
        assert_eq!(
            backend.handle(Command::Hint(HintCommand::OpenHovered)),
            a()
        );

        backend.handle(Command::Hint(HintCommand::Pointer(None)));
        backend.handle(Command::Hint(HintCommand::Start(2)));
        let moved = Action::Hint(HintAction::MoveViModeCursor, b.clone());
        assert_eq!(label(&mut backend, "j"), moved);
        backend.sync();
        let start = Point::new(Line(0), Column(25));
        assert_eq!(backend.renderable_content().vi_cursor, Some(start));
        assert_eq!(backend.renderable_content().hint_highlights.len(), 1);

        let open = backend.handle(Command::Vi(ViCommand::Open));
        assert_eq!(open, Action::Hint(HintAction::Copy, b));
    }

    #[cfg(unix)]
    #[test]
    fn vi_mode_shows_its_cursor_and_where_it_is() {
        use crate::search::SearchDirection;
        use crate::vi::ViCommand;
        use tankterm_terminal::grid::Dimensions;

        // 30 lines and the cursor's on a screen of 10 lines, printed once it
        // has its size: 21 lines of scrollback.
        let mut backend = backend_running(
            "stty -echo; printf ready; read x; printf '\\r\\033[K'; \
             seq 1 30; exec cat",
        );
        backend.wait_until("the program waits", |backend| {
            backend.text().concat().contains("ready")
        });
        backend.handle(Command::Resize(
            Some(Size::new(800., 160.)),
            Some(Size::new(8., 16.)),
        ));
        backend.handle(Command::Write(b"\n".to_vec()));
        backend.wait_until("the program prints its lines", |backend| {
            backend.term.lock().total_lines() == 31
        });
        let last = 30;
        let content = |backend: &mut Backend| {
            backend.sync();
            let content = backend.renderable_content();
            (content.vi_cursor, content.line_indicator.clone())
        };
        assert_eq!(content(&mut backend), (None, None));

        // The vi cursor starts at the terminal's, on the last line.
        backend.handle(Command::Vi(ViCommand::Toggle));
        let cursor = backend.term.lock().grid().cursor.point;
        let indicator = |line: usize| Some(format!("[{line}/{last}]"));
        assert_eq!(content(&mut backend), (Some(cursor), indicator(0)));

        // Paging takes the vi cursor along, to the start of its line.
        backend.handle(Command::ScrollDisplay(Scroll::PageUp));
        let (vi_cursor, line) = content(&mut backend);
        assert_eq!(vi_cursor, Some(Point::new(cursor.line - 10, Column(0))));
        assert_eq!(line, indicator(10));
        assert_eq!(backend.renderable_content().grid.display_offset(), 10);

        backend.handle(Command::ScrollHalfPageDown);
        assert_eq!(content(&mut backend).1, indicator(5));
        backend.handle(Command::ScrollDisplay(Scroll::Top));
        assert_eq!(content(&mut backend).1, indicator(last));

        // A click moves the vi cursor too; cells are 8 by 16 pixels.
        backend.handle(Command::SelectStart(SelectionType::Simple, (20., 40.)));
        let top = Line(-(backend.term.lock().grid().display_offset() as i32));
        let (vi_cursor, _) = content(&mut backend);
        assert_eq!(vi_cursor, Some(Point::new(top + 2, Column(2))));

        // Leaving vi mode takes the cursor and the indicator away; a search
        // outside vi mode shows how far the view is scrolled.
        backend.handle(Command::Vi(ViCommand::Toggle));
        assert_eq!(content(&mut backend), (None, None));
        backend.handle(Command::ScrollDisplay(Scroll::Bottom));
        let backward = SearchCommand::Start(SearchDirection::Backward);
        backend.handle(Command::Search(backward));
        assert_eq!(content(&mut backend), (None, indicator(0)));
    }

    #[test]
    fn line_indicator_makes_way_for_the_vi_cursor() {
        use tankterm_terminal::event::VoidListener;
        use tankterm_terminal::term::Config;

        let size = TermSize::new(20, 5);
        let term = Term::new(Config::default(), &size, VoidListener);

        let corner = Point::new(Line(0), Column(19));
        assert_eq!(line_indicator(&term, Some(corner), false), None);
        let next_line = Point::new(Line(1), Column(19));
        let indicator = line_indicator(&term, Some(next_line), false);
        assert_eq!(indicator.as_deref(), Some("[3/4]"));
        let left = Point::new(Line(0), Column(14));
        let indicator = line_indicator(&term, Some(left), false);
        assert_eq!(indicator.as_deref(), Some("[4/4]"));
    }

    #[cfg(unix)]
    #[test]
    fn expanding_starts_or_extends_the_selection() {
        let mut backend = backend_running("printf 'hello world'; exec cat");
        backend.wait_until("nothing was printed", |backend| {
            backend.text().concat().contains("hello world")
        });

        // Without a selection, expanding starts one where the pointer is.
        backend.handle(Command::ExpandSelection(
            SelectionType::Semantic,
            (7.5, 0.),
        ));
        assert_eq!(backend.selection().as_deref(), Some("world"));

        // A selection is extended, and takes the new type.
        backend.handle(Command::SelectStart(SelectionType::Simple, (0., 0.)));
        backend
            .handle(Command::ExpandSelection(SelectionType::Simple, (3.6, 0.)));
        assert_eq!(backend.selection().as_deref(), Some("hel"));
        backend.handle(Command::ExpandSelection(
            SelectionType::Semantic,
            (3.6, 0.),
        ));
        assert_eq!(backend.selection().as_deref(), Some("hello"));
    }

    #[cfg(unix)]
    #[test]
    fn words_end_at_the_semantic_escape_chars() {
        let mut backend = backend_running("printf 'one:two three'; exec cat");
        backend.wait_until("nothing was printed", |backend| {
            backend.text().concat().contains("three")
        });
        let word_at_start = |backend: &mut Backend| {
            backend.handle(Command::SelectStart(
                SelectionType::Semantic,
                (0., 0.),
            ));
            backend.selection()
        };

        assert_eq!(word_at_start(&mut backend).as_deref(), Some("one"));
        backend.set_semantic_escape_chars(" ");
        assert_eq!(word_at_start(&mut backend).as_deref(), Some("one:two"));
    }

    #[cfg(unix)]
    #[test]
    fn cursor_style_is_the_applications_or_the_default() {
        use tankterm_terminal::vte::ansi::CursorShape;

        let style = |shape, blinking| CursorStyle { shape, blinking };
        let synced = |backend: &mut Backend| {
            backend.sync();
            backend.renderable_content().cursor_style
        };

        // DECSCUSR 5 is a blinking beam; the shell then waits to be told
        // to go back to the default with DECSCUSR 0.
        let script =
            "printf '\\033[5 q'; read line; printf '\\033[0 q'; exec cat";
        let mut backend = backend_running(script);
        backend.wait_until("the cursor never became a beam", |backend| {
            synced(backend) == style(CursorShape::Beam, true)
        });

        // A new default doesn't replace what the application asked for.
        backend.set_default_cursor_style(style(CursorShape::Underline, false));
        assert_eq!(synced(&mut backend), style(CursorShape::Beam, true));

        backend.handle(Command::Write(b"\n".to_vec()));
        backend.wait_until(
            "the cursor never went back to the default",
            |backend| synced(backend) == style(CursorShape::Underline, false),
        );
        backend.set_default_cursor_style(style(CursorShape::Block, true));
        assert_eq!(synced(&mut backend), style(CursorShape::Block, true));
    }

    #[test]
    fn centring_offset_shares_what_whole_cells_leave_over() {
        let size = TerminalSize {
            cell_width: 8,
            cell_height: 17,
            num_cols: 10,
            num_lines: 4,
            ..TerminalSize::default()
        };

        // 85 - 80 = 5 pixels across, 75 - 68 = 7 down, halved and rounded
        // down to whole pixels.
        let offset = size.centring_offset(Size::new(85., 75.));
        assert_eq!((offset.x, offset.y), (2., 3.));

        // Space that whole cells fill leaves nothing to share.
        let offset = size.centring_offset(Size::new(80., 68.));
        assert_eq!((offset.x, offset.y), (0., 0.));

        // A grid that is bigger than its space until it is resized stays at
        // the top left.
        let offset = size.centring_offset(Size::new(40., 10.));
        assert_eq!((offset.x, offset.y), (0., 0.));
    }

    #[cfg(unix)]
    #[test]
    fn cells_of_a_tiny_font_are_a_pixel() {
        let mut backend = backend_running("exec cat");
        backend.handle(Command::Resize(
            Some(Size::new(120., 40.)),
            Some(Size::new(0.4, 0.9)),
        ));

        assert_eq!((backend.size.cell_width, backend.size.cell_height), (1, 1));
        assert_eq!((backend.size.num_cols, backend.size.num_lines), (120, 40));
        // Pointing at a cell doesn't divide by zero.
        let point = Backend::selection_point(5., 5., &backend.size, 0);
        assert_eq!((point.line.0, point.column.0), (5, 5));
    }

    #[cfg(unix)]
    #[test]
    fn scrollback_is_paged_through_and_cleared() {
        let mut backend = backend_running("seq 1 250; printf done; exec cat");
        backend.wait_until("the output never ended", |backend| {
            backend.text().concat().contains("done")
        });
        let offset = |backend: &mut Backend, scroll| {
            backend.handle(Command::ScrollDisplay(scroll));
            backend.sync();
            backend.renderable_content().grid.display_offset()
        };

        // 250 lines above the line of the cursor, 49 of them on the screen.
        assert_eq!(backend.screen_lines(), 50);
        assert_eq!(offset(&mut backend, Scroll::PageUp), 50);
        assert_eq!(offset(&mut backend, Scroll::Delta(-25)), 25);
        assert_eq!(offset(&mut backend, Scroll::Delta(1)), 26);
        assert_eq!(offset(&mut backend, Scroll::Top), 201);
        assert_eq!(offset(&mut backend, Scroll::PageDown), 151);
        assert_eq!(offset(&mut backend, Scroll::Bottom), 0);

        // Clearing the history leaves the screen as it is.
        offset(&mut backend, Scroll::PageUp);
        backend.handle(Command::ClearHistory);
        backend.sync();
        assert_eq!(backend.renderable_content().grid.display_offset(), 0);
        assert_eq!(backend.text().first().map(String::as_str), Some("202"));
        assert_eq!(offset(&mut backend, Scroll::Top), 0);
    }

    #[cfg(unix)]
    #[test]
    fn foreground_group_is_the_shell_until_a_command_runs() {
        // An interactive shell puts every command in a process group of its
        // own and hands it the terminal.
        let settings = BackendSettings {
            program: "/bin/sh".into(),
            args: vec!["-i".into()],
            ..BackendSettings::default()
        };
        let mut backend = Running::new(settings, 10_000);

        let shell = backend.child_pid().unwrap();
        poll_until("the shell never took the terminal", || {
            backend.foreground_process_group() == Some(shell)
        });

        backend.handle(Command::Write(b"sleep 1\n".to_vec()));
        poll_until("the command never took the terminal", || {
            backend
                .foreground_process_group()
                .is_some_and(|group| group != shell)
        });
        poll_until("the shell never got the terminal back", || {
            backend.foreground_process_group() == Some(shell)
        });
    }

    #[tokio::test]
    async fn wakeups_are_coalesced_until_received() {
        let (proxy, mut receiver) = event_channel(8);

        for _ in 0..20 {
            proxy.send_event(Event::Wakeup);
        }
        proxy.send_event(Event::PtyWrite(String::from("reply")));
        proxy.send_event(Event::Wakeup);

        // One wakeup for the burst; other events are never dropped.
        assert!(matches!(receiver.recv().await, Some(Event::Wakeup)));
        assert!(matches!(
            receiver.recv().await,
            Some(Event::PtyWrite(text)) if text == "reply"
        ));
        assert!(receiver.receiver.try_recv().is_err());

        // Once received, the next output wakes the widget again.
        proxy.send_event(Event::Wakeup);
        proxy.send_event(Event::Wakeup);
        assert!(matches!(receiver.recv().await, Some(Event::Wakeup)));
        assert!(receiver.receiver.try_recv().is_err());
    }

    #[tokio::test]
    async fn wakeup_is_retried_after_a_full_channel() {
        let (proxy, mut receiver) = event_channel(1);

        proxy.send_event(Event::Bell);
        // The channel is full: this wakeup is lost...
        proxy.send_event(Event::Wakeup);
        assert!(matches!(receiver.recv().await, Some(Event::Bell)));
        // ...but doesn't block the next one.
        proxy.send_event(Event::Wakeup);
        assert!(matches!(receiver.recv().await, Some(Event::Wakeup)));
    }

    /// A terminal running `program` with `args`, and what it sends.
    #[cfg(unix)]
    fn backend_with_events(
        program: &str,
        args: &[&str],
    ) -> (Backend, EventReceiver) {
        let (proxy, receiver) = event_channel(100);
        // TankTerm sets `TERM` for its shells when it starts. Tests run with
        // the test runner's, which on CI is unset or `dumb`, where the shell
        // integration scripts stay out of the way.
        let env = [("TERM".to_owned(), "xterm-256color".to_owned())];
        let settings = BackendSettings {
            program: program.into(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            env: env.into(),
            ..BackendSettings::default()
        };
        let backend = Backend::new(
            0,
            proxy,
            settings,
            tankterm_terminal::term::ColorScheme::Dark,
            10_000,
            CursorStyle::default(),
        )
        .unwrap();
        (backend, receiver)
    }

    /// Wait until the terminal sent `event`; returns what it sent, but
    /// for its wakeups.
    #[cfg(unix)]
    fn events_until(receiver: &mut EventReceiver, event: &str) -> Vec<String> {
        let deadline = std::time::Instant::now() + PATIENCE;
        let mut events = Vec::new();
        while !events.iter().any(|sent| sent == event) {
            let left =
                deadline.saturating_duration_since(std::time::Instant::now());
            assert!(!left.is_zero(), "no {event}");
            match receiver.recv_timeout(left) {
                Some(Event::Wakeup | Event::MouseCursorDirty) | None => (),
                Some(sent) => events.push(format!("{sent:?}")),
            }
        }
        events
    }

    #[cfg(unix)]
    #[test]
    fn shell_integration_sequences_from_a_script() {
        use tankterm_terminal::term::cell::{CommandStatus, Zone};

        // Two commands, the first failing, and a prompt waiting for the
        // next one.
        let script = r#"
            prompt='\033]133;A\007$ \033]133;B\007'
            printf '\033]7;file://localhost/tmp\007'
            printf "${prompt}false\r\n\033]133;C\007\033]133;D;1\007"
            printf "${prompt}seq 60\r\n\033]133;C\007"
            seq 60
            printf '\033]133;D;0\007'
            printf "${prompt}"
            exec cat
        "#;
        let (mut backend, mut receiver) =
            backend_with_events("/bin/sh", &["-c", script]);

        let events = events_until(&mut receiver, "CommandFinished(Some(0))");
        assert_eq!(
            events,
            [
                "WorkingDirectory(/tmp)",
                "CommandStarted",
                "CommandFinished(Some(1))",
                "CommandStarted",
                "CommandFinished(Some(0))",
            ]
        );

        // The output of the last command is selected.
        backend.handle(Command::SelectCommandOutput);
        let numbers: Vec<String> = (1..=60).map(|n| n.to_string()).collect();
        assert_eq!(backend.selection(), Some(numbers.join("\n")));

        // 63 lines on a screen of 50: the prompts of both commands are in
        // the scrollback.
        let offset = |backend: &mut Backend, command| {
            backend.handle(command);
            backend.sync();
            backend.renderable_content().grid.display_offset()
        };
        assert_eq!(offset(&mut backend, Command::ScrollToPreviousPrompt), 12);
        assert_eq!(offset(&mut backend, Command::ScrollToPreviousPrompt), 13);
        assert_eq!(offset(&mut backend, Command::ScrollToPreviousPrompt), 13);
        assert_eq!(offset(&mut backend, Command::ScrollToNextPrompt), 12);
        assert_eq!(offset(&mut backend, Command::ScrollToNextPrompt), 0);

        let grid = &backend.renderable_content().grid;
        let zone = |line| grid[Line(line)][Column(0)].zone;
        assert_eq!(zone(-13), Zone::PromptStart(CommandStatus::Failure));
        assert_eq!(zone(-12), Zone::PromptStart(CommandStatus::Success));
        assert_eq!(zone(49), Zone::PromptStart(CommandStatus::Unknown));
    }

    /// The bash integration in `extra/`, in an interactive bash.
    #[cfg(target_os = "linux")]
    #[test]
    fn bash_integration_marks_commands_and_directories() {
        let snippet = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../extra/shell-integration/tankterm.bash"
        );
        let (mut backend, mut receiver) =
            backend_with_events("bash", &["--norc", "--noprofile", "-i"]);

        backend.handle(Command::Write(
            format!("HISTFILE= PS1='$ '; source {snippet}\n").into_bytes(),
        ));
        backend.handle(Command::Write(b"false\n".to_vec()));
        events_until(&mut receiver, "CommandFinished(Some(1))");

        // An empty command line runs nothing.
        backend.handle(Command::Write(b"\ncd /\n".to_vec()));
        let events = events_until(&mut receiver, "WorkingDirectory(/)");
        assert_eq!(
            events,
            [
                "CommandStarted",
                "CommandFinished(Some(0))",
                "WorkingDirectory(/)"
            ]
        );
    }
}
