use crate::actions::Action;
use crate::settings::BackendSettings;
use crate::theme::Theme;
use iced::keyboard::Modifiers;
use iced_core::Size;
use std::borrow::Cow;
use std::cmp::min;
use std::io::Result;
use std::ops::{Index, RangeInclusive};
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
use tankterm_terminal::term::search::{Match, RegexIter, RegexSearch};
use tankterm_terminal::term::{
    self, cell::Cell, color::Colors, test::TermSize, viewport_to_point, Term,
    TermMode,
};
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

const URL_REGEX: &str = r#"(ipfs:|ipns:|magnet:|mailto:|gemini://|gopher://|https://|http://|news:|file://|git://|ssh:|ftp://)[^\u{0000}-\u{001F}\u{007F}-\u{009F}<>"\s{-}\^⟨⟩`]+"#;

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
    /// Move the view through the scrollback buffer.
    ScrollDisplay(Scroll),
    /// Drop the scrollback buffer, keeping what is on the screen.
    ClearHistory,
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
    ProcessLink(LinkAction, Point),
    MouseReport(MouseButton, Modifiers, Point, bool),
    ProcessAlacrittyEvent(Event),
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

#[derive(Debug, Clone)]
pub enum LinkAction {
    Clear,
    Hover,
    Open,
}

#[derive(Clone, Copy, Debug)]
pub struct TerminalSize {
    pub cell_width: u16,
    pub cell_height: u16,
    num_cols: u16,
    num_lines: u16,
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
            layout_width: 80.0,
            layout_height: 50.0,
        }
    }
}

impl TerminalSize {
    /// How far the grid is moved into `space` to centre it: half of what
    /// whole cells leave over on each axis, in whole pixels.
    ///
    /// A grid bigger than `space`, which it is for a moment while the
    /// terminal hasn't been resized yet, isn't moved.
    pub fn centring_offset(&self, space: Size<f32>) -> iced_core::Vector {
        let half_left_over = |space: f32, cell: u16, cells: u16| {
            ((space - f32::from(cell) * f32::from(cells)) / 2.)
                .floor()
                .max(0.)
        };

        iced_core::Vector::new(
            half_left_over(space.width, self.cell_width, self.num_cols),
            half_left_over(space.height, self.cell_height, self.num_lines),
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
    notifier: Notifier,
    /// The options the `Term` was last given.
    config: term::Config,
    last_content: RenderableContent,
    pub(crate) url_regex: RegexSearch,
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
            hovered_hyperlink: None,
            colors: *term.colors(),
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
            notifier,
            config,
            last_content: initial_content,
            url_regex: RegexSearch::new(URL_REGEX).expect("invalid url regexp"),
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
                term.scroll_display(scroll);
            },
            Command::ClearHistory => {
                term.clear_screen(ClearMode::Saved);
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
            Command::ProcessLink(link_action, point) => {
                self.process_link_action(&term, link_action, point);
            },
            Command::MouseReport(button, modifiers, point, pressed) => {
                self.process_mouse_report(button, modifiers, point, pressed);
            },
        };

        action
    }

    fn process_link_action(
        &mut self,
        terminal: &Term<EventProxy>,
        link_action: LinkAction,
        point: Point,
    ) {
        match link_action {
            LinkAction::Hover => {
                self.last_content.hovered_hyperlink = self.regex_match_at(
                    terminal,
                    point,
                    &mut self.url_regex.clone(),
                );
            },
            LinkAction::Clear => {
                self.last_content.hovered_hyperlink = None;
            },
            LinkAction::Open => {
                self.open_link();
            },
        };
    }

    fn open_link(&self) {
        if let Some(range) = &self.last_content.hovered_hyperlink {
            let start = range.start();
            let end = range.end();

            let mut url = String::from(self.last_content.grid.index(*start).c);
            for indexed in self.last_content.grid.iter_from(*start) {
                url.push(indexed.c);
                if indexed.point == *end {
                    break;
                }
            }

            open::that(url).unwrap_or_else(|_| {
                panic!("link opening is failed");
            })
        }
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
    }

    fn update_selection(
        &mut self,
        terminal: &mut Term<EventProxy>,
        x: f32,
        y: f32,
    ) {
        let display_offset = terminal.grid().display_offset();
        if let Some(ref mut selection) = terminal.selection {
            let location =
                Self::selection_point(x, y, &self.size, display_offset);
            selection.update(location, self.selection_side(x));
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
        };

        if let Some(size) = font_measure {
            // A font too small to measure a pixel still has cells to divide
            // the layout by.
            self.size.cell_height = (size.height as u16).max(1);
            self.size.cell_width = (size.width as u16).max(1);
        }

        let lines = (self.size.layout_height / self.size.cell_height as f32)
            .floor() as u16;
        let cols = (self.size.layout_width / self.size.cell_width as f32)
            .floor() as u16;
        if lines > 0 && cols > 0 {
            self.size.num_lines = lines;
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
                terminal.grid_mut().scroll_display(scroll);
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

        let cursor = terminal.grid_mut().cursor_cell().clone();
        self.last_content.grid = terminal.grid().clone();
        self.last_content.selectable_range = selectable_range;
        self.last_content.cursor = cursor.clone();
        self.last_content.cursor_style = terminal.cursor_style();
        self.last_content.terminal_mode = *terminal.mode();
        self.last_content.terminal_size = self.size;
        self.last_content.colors = *terminal.colors();
    }

    pub fn renderable_content(&self) -> &RenderableContent {
        &self.last_content
    }

    /// Based on alacritty/src/display/hint.rs > regex_match_at
    /// Retrieve the match, if the specified point is inside the content matching the regex.
    fn regex_match_at(
        &self,
        terminal: &Term<EventProxy>,
        point: Point,
        regex: &mut RegexSearch,
    ) -> Option<Match> {
        let x = visible_regex_match_iter(terminal, regex)
            .find(|rm| rm.contains(&point));
        x
    }
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

/// Copied from alacritty/src/display/hint.rs:
/// Iterate over all visible regex matches.
fn visible_regex_match_iter<'a>(
    term: &'a Term<EventProxy>,
    regex: &'a mut RegexSearch,
) -> impl Iterator<Item = Match> + 'a {
    let viewport_start = Line(-(term.grid().display_offset() as i32));
    let viewport_end = viewport_start + term.bottommost_line();
    let mut start =
        term.line_search_left(Point::new(viewport_start, Column(0)));
    let mut end = term.line_search_right(Point::new(viewport_end, Column(0)));
    start.line = start.line.max(viewport_start - 100);
    end.line = end.line.min(viewport_end + 100);

    RegexIter::new(start, end, Direction::Right, term, regex)
        .skip_while(move |rm| rm.end().line < viewport_start)
        .take_while(move |rm| rm.start().line <= viewport_end)
}

pub struct RenderableContent {
    pub grid: Grid<Cell>,
    pub hovered_hyperlink: Option<RangeInclusive<Point>>,
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
}

impl Default for RenderableContent {
    fn default() -> Self {
        Self {
            grid: Grid::new(0, 0, 0),
            hovered_hyperlink: None,
            selectable_range: None,
            cursor: Cell::default(),
            cursor_style: CursorStyle::default(),
            terminal_mode: TermMode::empty(),
            terminal_size: TerminalSize::default(),
            colors: Colors::default(),
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

    /// Wait for `condition` to hold for a terminal with a running program.
    #[cfg(unix)]
    fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
        let deadline =
            std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !condition() {
            assert!(std::time::Instant::now() < deadline, "{what}");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    #[cfg(unix)]
    fn backend_running(script: &str) -> Backend {
        backend_with_scrollback(script, 10_000)
    }

    /// A terminal running `script` that keeps `scrollback` lines of history.
    #[cfg(unix)]
    fn backend_with_scrollback(script: &str, scrollback: usize) -> Backend {
        use tankterm_terminal::term::ColorScheme;

        let (proxy, _receiver) = event_channel(100);
        let settings = BackendSettings {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
            ..BackendSettings::default()
        };
        Backend::new(
            0,
            proxy,
            settings,
            ColorScheme::Dark,
            scrollback,
            CursorStyle::default(),
        )
        .unwrap()
    }

    /// What a program printed right before it exited is kept when the
    /// terminal holds: the PTY is read once more after the exit.
    #[cfg(unix)]
    #[test]
    fn held_terminal_keeps_the_last_output() {
        use tankterm_terminal::term::ColorScheme;

        let (proxy, _receiver) = event_channel(100);
        let settings = BackendSettings {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "printf held; exit 3".into()],
            hold: true,
            ..BackendSettings::default()
        };
        let backend = Backend::new(
            0,
            proxy,
            settings,
            ColorScheme::Dark,
            100,
            CursorStyle::default(),
        )
        .unwrap();

        eventually("the output was lost", || {
            backend.text().concat().contains("held")
        });
    }

    #[cfg(unix)]
    #[test]
    fn scrollback_keeps_the_configured_number_of_lines() {
        // The default size has 50 lines, so 250 lines of output leave 201
        // lines of history behind, counting the line of the cursor.
        let script = "seq 1 250; exec cat";

        let first = |backend: &Backend| backend.text().first().cloned();

        let backend = backend_with_scrollback(script, 10_000);
        eventually("the output never ended", || {
            backend.text().iter().any(|line| line == "250")
        });
        assert_eq!(first(&backend).as_deref(), Some("1"));

        // 20 lines of history above the 49 lines of output on the screen.
        let mut backend = backend_with_scrollback(script, 20);
        eventually("the history never filled up", || {
            first(&backend).as_deref() == Some("182")
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
        eventually("bracketed paste was not enabled", || {
            backend.mode().contains(TermMode::BRACKETED_PASTE)
        });

        // The terminal echoes the markers of the paste in caret notation.
        backend.handle(Command::Paste("one\ntwo".into()));
        eventually("the paste was not wrapped", || {
            backend.text().concat().contains("^[[200~one")
        });
    }

    #[cfg(unix)]
    #[test]
    fn selection_is_the_selected_text() {
        let mut backend = backend_running("printf 'hello world'; exec cat");
        eventually("nothing was printed", || {
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
    fn expanding_starts_or_extends_the_selection() {
        let mut backend = backend_running("printf 'hello world'; exec cat");
        eventually("nothing was printed", || {
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
        eventually("nothing was printed", || {
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
        eventually("the cursor never became a beam", || {
            synced(&mut backend) == style(CursorShape::Beam, true)
        });

        // A new default doesn't replace what the application asked for.
        backend.set_default_cursor_style(style(CursorShape::Underline, false));
        assert_eq!(synced(&mut backend), style(CursorShape::Beam, true));

        backend.handle(Command::Write(b"\n".to_vec()));
        eventually("the cursor never went back to the default", || {
            synced(&mut backend) == style(CursorShape::Underline, false)
        });
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
        eventually("the output never ended", || {
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
        let (proxy, _receiver) = event_channel(100);
        let settings = BackendSettings {
            program: "/bin/sh".into(),
            args: vec!["-i".into()],
            ..BackendSettings::default()
        };
        let mut backend = Backend::new(
            0,
            proxy,
            settings,
            tankterm_terminal::term::ColorScheme::Dark,
            10_000,
            CursorStyle::default(),
        )
        .unwrap();

        let shell = backend.child_pid().unwrap();
        eventually("the shell never took the terminal", || {
            backend.foreground_process_group() == Some(shell)
        });

        backend.handle(Command::Write(b"sleep 1\n".to_vec()));
        eventually("the command never took the terminal", || {
            backend
                .foreground_process_group()
                .is_some_and(|group| group != shell)
        });
        eventually("the shell never got the terminal back", || {
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
}
