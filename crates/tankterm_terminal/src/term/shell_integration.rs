//! Shell integration: the prompt marks (OSC 133) and working directories (OSC 7) shells send.
//!
//! The escape sequence parser doesn't know these sequences; [`crate::parser::Parser`] finds them
//! in the output and hands them to the terminal here.
//!
//! Prompt marks are kept in the cells: every cell records the [`Zone`] it was written in, so the
//! marks scroll, reflow and get cleared with the text they belong to.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::{mem, str};

use log::trace;

use crate::event::{Event, EventListener};
use crate::grid::{Dimensions, Grid, Scroll};
use crate::index::{Column, Direction, Line, Point, Side};
use crate::selection::{Selection, SelectionType};
use crate::term::cell::{Cell, CommandStatus, Zone};
use crate::term::{Term, TermMode};
use crate::vte::ansi::Handler;

/// A shell integration sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellSequence {
    /// OSC 7: the shell is in this directory, on this computer.
    WorkingDirectory(PathBuf),
    /// OSC 7 naming a directory on another computer, such as through SSH.
    RemoteDirectory,
    /// OSC 133: a mark around the shell's prompts and commands.
    Prompt(PromptMark),
}

impl ShellSequence {
    /// The sequence in the payload of an OSC, the bytes between `OSC` and its terminator, if it
    /// is one.
    pub fn parse(payload: &[u8]) -> Option<Self> {
        if let Some(url) = payload.strip_prefix(b"7;") {
            return Some(match local_directory(url) {
                Some(directory) => Self::WorkingDirectory(directory),
                None => Self::RemoteDirectory,
            });
        }

        let mut fields = payload.strip_prefix(b"133;")?.split(|byte| *byte == b';');
        let mark = match fields.next()? {
            // kitty marks secondary prompts with an `A` that has `k=s`, other prompt kinds too.
            b"A" if fields.any(|field| matches!(field, b"k=s" | b"k=c" | b"k=r")) => {
                PromptMark::Prompt
            },
            b"A" => PromptMark::PromptStart,
            b"P" => PromptMark::Prompt,
            b"B" => PromptMark::CommandStart,
            b"C" => PromptMark::OutputStart,
            b"D" => {
                let status = fields.next().and_then(|field| str::from_utf8(field).ok());
                PromptMark::CommandEnd(status.and_then(|status| status.parse().ok()))
            },
            _ => return None,
        };

        Some(Self::Prompt(mark))
    }
}

/// A mark a shell puts around its prompts and commands with OSC 133.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptMark {
    /// `A`: a prompt starts, on a line of its own.
    PromptStart,
    /// `P`: more of a prompt follows, such as a continuation or right prompt.
    Prompt,
    /// `B`: the prompt ends and the command line starts.
    CommandStart,
    /// `C`: the command runs and its output starts.
    OutputStart,
    /// `D`: the command finished, with this exit status if the shell gave one.
    CommandEnd(Option<i32>),
}

/// What a terminal keeps of the shell integration sequences.
#[derive(Debug, Default)]
pub(crate) struct ShellState {
    /// The zone cells are written in.
    pub(crate) zone: Zone,
    /// A command started (`C`) and didn't finish yet.
    running: bool,
    /// The directory the shell reported last.
    directory: Option<PathBuf>,
}

impl<T: EventListener> Term<T> {
    /// Handle a shell integration sequence.
    pub fn shell_sequence(&mut self, sequence: ShellSequence) {
        trace!("Shell integration: {sequence:?}");
        match sequence {
            ShellSequence::WorkingDirectory(directory) => {
                if self.shell.directory.as_ref() != Some(&directory) {
                    self.shell.directory = Some(directory.clone());
                    self.event_proxy.send_event(Event::WorkingDirectory(directory));
                }
            },
            // The shell runs elsewhere; the last local directory still holds for this computer.
            ShellSequence::RemoteDirectory => (),
            ShellSequence::Prompt(mark) => self.prompt_mark(mark),
        }
    }

    fn prompt_mark(&mut self, mark: PromptMark) {
        match mark {
            PromptMark::PromptStart => {
                // A command that didn't say it ended did when the next prompt starts.
                self.end_command(None);

                // The prompt starts on a line of its own, after output that didn't end one.
                if self.grid.cursor.point.column != 0 || self.grid.cursor.input_needs_wrap {
                    self.carriage_return();
                    self.linefeed();
                }

                self.shell.zone = Zone::PromptStart(CommandStatus::Unknown);
            },
            // A prompt that started and hasn't been written yet still has to mark its start.
            PromptMark::Prompt if matches!(self.shell.zone, Zone::PromptStart(_)) => (),
            PromptMark::Prompt => self.shell.zone = Zone::Prompt,
            PromptMark::CommandStart => self.shell.zone = Zone::Input,
            PromptMark::OutputStart => {
                self.shell.zone = Zone::Output;
                if !self.shell.running {
                    self.shell.running = true;
                    self.set_prompt_status(CommandStatus::Unknown, CommandStatus::Running);
                    self.event_proxy.send_event(Event::CommandStarted);
                }
            },
            PromptMark::CommandEnd(status) => {
                self.end_command(status);
                self.shell.zone = Zone::None;
            },
        }
    }

    /// The running command finished, with this exit status if it is known.
    fn end_command(&mut self, status: Option<i32>) {
        if !mem::take(&mut self.shell.running) {
            return;
        }

        let ended = match status {
            Some(0) => CommandStatus::Success,
            Some(_) => CommandStatus::Failure,
            None => CommandStatus::Unknown,
        };
        self.set_prompt_status(CommandStatus::Running, ended);
        self.event_proxy.send_event(Event::CommandFinished(status));
    }

    /// Change the status of the last prompt before the cursor, if it is `from`.
    ///
    /// A prompt that was cleared off the screen is left alone: then the last prompt is an older
    /// one, with another status.
    fn set_prompt_status(&mut self, from: CommandStatus, to: CommandStatus) {
        // A command started from the shell's prompt can run on the alternate screen.
        let grid = if self.mode.contains(TermMode::ALT_SCREEN) {
            &mut self.inactive_grid
        } else {
            &mut self.grid
        };

        let Some(point) = last_prompt_start(grid, grid.cursor.point) else { return };
        if grid[point].zone == Zone::PromptStart(from) {
            grid[point].zone = Zone::PromptStart(to);
        }
    }

    /// Scroll the view to put the start of the prompt before (`Direction::Left`) or after
    /// (`Direction::Right`) the top of the view at the top, as far as the scrollback reaches.
    ///
    /// Returns whether there was a prompt to scroll to.
    pub fn scroll_to_prompt(&mut self, direction: Direction) -> bool {
        if self.mode.contains(TermMode::ALT_SCREEN) {
            return false;
        }

        let top = -(self.grid.display_offset() as i32);
        let starts = |line: &i32| prompt_starts_on(&self.grid, Line(*line));
        let line = match direction {
            Direction::Left => (self.topmost_line().0..top).rev().find(starts),
            Direction::Right => (top + 1..=self.bottommost_line().0).find(starts),
        };
        let Some(line) = line else { return false };

        let offset = (-line).max(0);
        self.scroll_display(Scroll::Delta(offset - self.grid.display_offset() as i32));
        true
    }

    /// Select the output of the command whose prompt is at the top of the view, or of the last
    /// command while the view is at the bottom.
    ///
    /// Returns whether there was output to select.
    pub fn select_command_output(&mut self) -> bool {
        if self.mode.contains(TermMode::ALT_SCREEN) {
            return false;
        }

        let anchor = if self.grid.display_offset() == 0 {
            self.last_output()
        } else {
            self.output_after(Line(-(self.grid.display_offset() as i32)))
        };
        let Some((start, end)) = anchor.map(|point| output_around(&self.grid, point)) else {
            return false;
        };

        let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
        selection.update(end, Side::Right);
        self.selection = Some(selection);
        true
    }

    /// A cell of the last command's output: of the command running at the cursor, or of the
    /// command before the prompt the cursor is at.
    fn last_output(&self) -> Option<Point> {
        let mut prompts = 0;
        let mut point = Some(self.grid.cursor.point);
        while let Some(current) = point {
            match self.grid[current].zone {
                Zone::Output => return Some(current),
                // The command before the prompt before the cursor's printed nothing.
                Zone::PromptStart(_) if prompts == 1 => return None,
                Zone::PromptStart(_) => prompts += 1,
                _ => (),
            }
            point = before(&self.grid, current);
        }

        None
    }

    /// A cell of the output of the command whose prompt starts at or above `top`, or of the
    /// output after `top` if no prompt starts there.
    fn output_after(&self, top: Line) -> Option<Point> {
        let prompt = (self.topmost_line().0..=top.0)
            .rev()
            .map(Line)
            .find(|line| prompt_starts_on(&self.grid, *line))
            .and_then(|line| {
                let row = &self.grid[line];
                let column =
                    row[..].iter().position(|cell| matches!(cell.zone, Zone::PromptStart(_)));
                column.map(|column| Point::new(line, Column(column)))
            });

        let mut point = match prompt {
            Some(prompt) => after(&self.grid, prompt),
            None => Some(Point::new(top, Column(0))),
        };
        while let Some(current) = point {
            match self.grid[current].zone {
                Zone::Output => return Some(current),
                // The command printed nothing before the next prompt.
                Zone::PromptStart(_) => return None,
                _ => (),
            }
            point = after(&self.grid, current);
        }

        None
    }
}

/// The first and last cell of the output `point` is in.
///
/// Output reaches over cells that weren't written, such as empty lines, up to a prompt or
/// command line.
fn output_around(grid: &Grid<Cell>, point: Point) -> (Point, Point) {
    let in_output = |point: Point| matches!(grid[point].zone, Zone::Output | Zone::None);

    let mut start = point;
    let mut current = before(grid, point);
    while let Some(point) = current.filter(|point| in_output(*point)) {
        if grid[point].zone == Zone::Output {
            start = point;
        }
        current = before(grid, point);
    }

    let mut end = point;
    let mut current = after(grid, point);
    while let Some(point) = current.filter(|point| in_output(*point)) {
        if grid[point].zone == Zone::Output {
            end = point;
        }
        current = after(grid, point);
    }

    (start, end)
}

/// Where the last prompt at or before `point` starts.
fn last_prompt_start(grid: &Grid<Cell>, point: Point) -> Option<Point> {
    let mut current = Some(point);
    while let Some(point) = current {
        if let Zone::PromptStart(_) = grid[point].zone {
            return Some(point);
        }
        current = before(grid, point);
    }

    None
}

/// Whether a prompt starts on a line.
pub(crate) fn prompt_starts_on(grid: &Grid<Cell>, line: Line) -> bool {
    grid[line][..].iter().any(|cell| matches!(cell.zone, Zone::PromptStart(_)))
}

/// The cell before `point`, reading left to right and top to bottom.
fn before(grid: &Grid<Cell>, point: Point) -> Option<Point> {
    if point.column > 0 {
        Some(Point::new(point.line, point.column - 1))
    } else if point.line > grid.topmost_line() {
        Some(Point::new(point.line - 1, grid.last_column()))
    } else {
        None
    }
}

/// The cell after `point`, reading left to right and top to bottom.
fn after(grid: &Grid<Cell>, point: Point) -> Option<Point> {
    if point.column < grid.last_column() {
        Some(Point::new(point.line, point.column + 1))
    } else if point.line < grid.bottommost_line() {
        Some(Point::new(point.line + 1, Column(0)))
    } else {
        None
    }
}

/// The directory in an OSC 7 URL, if it is on this computer.
///
/// The URL is `file://host/path`, percent-encoded, or kitty's `kitty-shell-cwd://host/path`,
/// which isn't encoded.
fn local_directory(url: &[u8]) -> Option<PathBuf> {
    let (rest, encoded) = if let Some(rest) = url.strip_prefix(b"file://") {
        (rest, true)
    } else {
        (url.strip_prefix(b"kitty-shell-cwd://")?, false)
    };

    let slash = rest.iter().position(|byte| *byte == b'/')?;
    let (host, path) = rest.split_at(slash);
    if !is_local_host(str::from_utf8(host).ok()?) {
        return None;
    }

    let path = if encoded { percent_decode(path) } else { path.to_vec() };
    path_from_bytes(path).filter(|path| path.is_absolute())
}

/// Whether a host name in an OSC 7 URL names this computer.
///
/// Shells give the host name with or without its domain, so only the part before the first dot
/// is compared.
fn is_local_host(host: &str) -> bool {
    if host.is_empty() || host.eq_ignore_ascii_case("localhost") {
        return true;
    }

    let short = |name: &str| name.split('.').next().unwrap_or_default().to_ascii_lowercase();
    local_host_name().is_some_and(|local| short(local) == short(host))
}

/// This computer's host name.
fn local_host_name() -> Option<&'static str> {
    static NAME: OnceLock<Option<String>> = OnceLock::new();
    NAME.get_or_init(host_name).as_deref()
}

#[cfg(unix)]
fn host_name() -> Option<String> {
    let mut buffer = [0u8; 256];
    // SAFETY: The buffer is valid for writes of its length.
    let result = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if result != 0 {
        return None;
    }

    let length = buffer.iter().position(|byte| *byte == 0).unwrap_or(buffer.len());
    String::from_utf8(buffer[..length].to_vec()).ok()
}

#[cfg(windows)]
fn host_name() -> Option<String> {
    std::env::var("COMPUTERNAME").ok()
}

#[cfg(not(any(unix, windows)))]
fn host_name() -> Option<String> {
    None
}

/// Decode `%XX` escapes; a `%` that doesn't start one stays as it is.
fn percent_decode(bytes: &[u8]) -> Vec<u8> {
    let hex = |byte: u8| (byte as char).to_digit(16);
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escape = bytes.get(index + 1..index + 3);
        match escape.and_then(|pair| Some(hex(pair[0])? * 16 + hex(pair[1])?)) {
            Some(byte) if bytes[index] == b'%' => {
                decoded.push(byte as u8);
                index += 3;
            },
            _ => {
                decoded.push(bytes[index]);
                index += 1;
            },
        }
    }

    decoded
}

#[cfg(unix)]
fn path_from_bytes(bytes: Vec<u8>) -> Option<PathBuf> {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    Some(PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: Vec<u8>) -> Option<PathBuf> {
    let path = String::from_utf8(bytes).ok()?;
    // `/C:/Users` names the drive `C:`.
    let drive = path.as_bytes().get(1..3).is_some_and(|drive| drive[1] == b':');
    let path = if path.starts_with('/') && drive { &path[1..] } else { &path[..] };
    Some(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    use crate::parser::Parser;
    use crate::term::Config;
    use crate::term::test::TermSize;

    /// Collects the events a terminal sends.
    #[derive(Clone, Default)]
    struct Events(Rc<RefCell<Vec<String>>>);

    impl EventListener for Events {
        fn send_event(&self, event: Event) {
            if !matches!(event, Event::MouseCursorDirty) {
                self.0.borrow_mut().push(format!("{event:?}"));
            }
        }
    }

    struct Shell {
        term: Term<Events>,
        parser: Parser,
        events: Events,
    }

    impl Shell {
        fn new(columns: usize, lines: usize) -> Self {
            let events = Events::default();
            let size = TermSize::new(columns, lines);
            let term = Term::new(Config::default(), &size, events.clone());
            Self { term, parser: Parser::new(), events }
        }

        fn feed(&mut self, output: &str) -> &mut Self {
            self.parser.advance(&mut self.term, output.as_bytes());
            self
        }

        /// A prompt, the command typed at it, and the command's output.
        fn command(&mut self, command: &str, output: &str, status: i32) -> &mut Self {
            self.feed(&format!("\x1b]133;A\x07$ \x1b]133;B\x07{command}\r\n\x1b]133;C\x07"));
            self.feed(output);
            self.feed(&format!("\x1b]133;D;{status}\x07"))
        }

        fn prompt(&mut self) -> &mut Self {
            self.feed("\x1b]133;A\x07$ \x1b]133;B\x07")
        }

        fn zone(&self, line: i32, column: usize) -> Zone {
            self.term.grid[Line(line)][Column(column)].zone
        }

        /// The lines prompts start on, top to bottom.
        fn prompts(&self) -> Vec<i32> {
            let lines = self.term.topmost_line().0..=self.term.bottommost_line().0;
            lines.filter(|line| prompt_starts_on(&self.term.grid, Line(*line))).collect()
        }

        fn events(&self) -> Vec<String> {
            mem::take(&mut *self.events.0.borrow_mut())
        }

        fn top(&self) -> i32 {
            -(self.term.grid.display_offset() as i32)
        }
    }

    #[test]
    fn marks_zones_and_reports_commands() {
        let mut shell = Shell::new(20, 10);
        shell.command("ls", "a\r\nb\r\n", 0).command("false", "", 1).prompt();

        assert_eq!(shell.zone(0, 0), Zone::PromptStart(CommandStatus::Success));
        assert_eq!(shell.zone(0, 1), Zone::Prompt);
        assert_eq!(shell.zone(0, 2), Zone::Input);
        assert_eq!(shell.zone(1, 0), Zone::Output);
        assert_eq!(shell.zone(2, 0), Zone::Output);
        assert_eq!(shell.zone(3, 0), Zone::PromptStart(CommandStatus::Failure));
        assert_eq!(shell.zone(4, 0), Zone::PromptStart(CommandStatus::Unknown));
        assert_eq!(shell.events(), [
            "CommandStarted",
            "CommandFinished(Some(0))",
            "CommandStarted",
            "CommandFinished(Some(1))",
        ]);
    }

    #[test]
    fn a_prompt_ends_a_command_that_did_not_say_it_ended() {
        let mut shell = Shell::new(20, 10);
        shell.feed("\x1b]133;A\x07$ \x1b]133;B\x07sleep 1\r\n\x1b]133;C\x07");
        assert_eq!(shell.zone(0, 0), Zone::PromptStart(CommandStatus::Running));

        // A mark that ends no command is left out.
        shell.prompt().feed("\x1b]133;D;0\x07");
        assert_eq!(shell.zone(0, 0), Zone::PromptStart(CommandStatus::Unknown));
        assert_eq!(shell.events(), ["CommandStarted", "CommandFinished(None)"]);
    }

    #[test]
    fn prompts_drawn_again_keep_their_start() {
        let mut shell = Shell::new(20, 10);
        // A shell that marks the start of its prompt once, and draws it over itself again.
        shell.feed("\x1b]133;A\x07$ ls\r$ ls -l");
        assert_eq!(shell.zone(0, 0), Zone::PromptStart(CommandStatus::Unknown));
        assert_eq!(shell.zone(0, 1), Zone::Prompt);

        // Erasing the prompt erases its start, as clearing the screen does.
        shell.feed("\r\x1b[K$ ls");
        assert_eq!(shell.prompts(), Vec::<i32>::new());
        shell.feed("\r\x1b]133;A\x07$ ls");

        // Output written over a prompt takes its place.
        shell.feed("\x1b]133;C\x07\rout");
        assert_eq!(shell.zone(0, 0), Zone::Output);
        assert_eq!(shell.prompts(), Vec::<i32>::new());
    }

    #[test]
    fn prompts_start_on_a_line_of_their_own() {
        let mut shell = Shell::new(20, 10);
        shell.feed("no newline").prompt();
        assert_eq!(shell.prompts(), [1]);
        assert_eq!(shell.term.grid.cursor.point, Point::new(Line(1), Column(2)));
    }

    #[test]
    fn marks_land_where_they_are_in_a_synchronized_update() {
        let mut shell = Shell::new(20, 10);
        shell.feed("\x1b[?2026hno newline").prompt().feed("ls\x1b[?2026l");
        assert_eq!(shell.prompts(), [1]);
        assert_eq!(shell.zone(1, 2), Zone::Input);
        assert_eq!(shell.parser.sync_bytes_count(), 0);
    }

    #[test]
    fn scrolls_to_prompts_in_the_scrollback() {
        let mut shell = Shell::new(20, 5);
        for _ in 0..3 {
            shell.command("seq 6", "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n", 0);
        }
        shell.prompt();
        assert_eq!(shell.prompts(), [-17, -10, -3, 4]);

        assert!(shell.term.scroll_to_prompt(Direction::Left));
        assert_eq!(shell.top(), -3);
        assert!(shell.term.scroll_to_prompt(Direction::Left));
        assert_eq!(shell.top(), -10);
        assert!(shell.term.scroll_to_prompt(Direction::Left));
        assert!(!shell.term.scroll_to_prompt(Direction::Left));
        assert_eq!(shell.top(), -17);

        assert!(shell.term.scroll_to_prompt(Direction::Right));
        assert_eq!(shell.top(), -10);
        assert!(shell.term.scroll_to_prompt(Direction::Right));
        assert!(shell.term.scroll_to_prompt(Direction::Right));
        assert_eq!(shell.top(), 0);
    }

    #[test]
    fn marks_survive_reflow() {
        let mut shell = Shell::new(20, 10);
        shell.command("ls -l", "a\r\n", 0).prompt();
        assert_eq!(shell.prompts(), [0, 2]);

        // The first command line wraps onto two lines, which pushes it into the scrollback.
        shell.term.resize(TermSize::new(4, 10));
        assert_eq!(shell.prompts(), [-1, 2]);
        assert_eq!(shell.zone(-1, 0), Zone::PromptStart(CommandStatus::Success));
        assert_eq!(shell.zone(0, 0), Zone::Input);

        shell.term.resize(TermSize::new(20, 10));
        assert_eq!(shell.prompts(), [0, 2]);
    }

    #[test]
    fn selects_the_output_of_the_last_command() {
        let mut shell = Shell::new(20, 10);
        shell.command("ls", "a\r\n\r\nb\r\n", 0).command("true", "", 0).prompt();
        // The last command printed nothing.
        assert!(!shell.term.select_command_output());

        shell.command("ls", "c\r\nd\r\n", 0).prompt().feed("ech");
        assert!(shell.term.select_command_output());
        assert_eq!(shell.term.selection_to_string().as_deref(), Some("c\nd"));

        // While a command runs, its output so far.
        shell.feed("o\r\n\x1b]133;C\x07e\r\nf");
        assert!(shell.term.select_command_output());
        assert_eq!(shell.term.selection_to_string().as_deref(), Some("e\nf"));
    }

    #[test]
    fn selects_the_output_of_the_command_at_the_top_of_the_view() {
        let mut shell = Shell::new(20, 5);
        shell.command("ls", "a\r\n\r\nb\r\n", 0);
        for _ in 0..2 {
            shell.command("seq 6", "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n", 0);
        }
        shell.prompt();

        shell.term.scroll_display(Scroll::Top);
        assert!(shell.term.select_command_output());
        assert_eq!(shell.term.selection_to_string().as_deref(), Some("a\n\nb"));

        // Scrolled into the output of the second command.
        shell.term.scroll_display(Scroll::Delta(-6));
        assert!(shell.term.select_command_output());
        assert_eq!(shell.term.selection_to_string().as_deref(), Some("1\n2\n3\n4\n5\n6"));
    }

    #[test]
    fn reports_working_directories_when_they_change() {
        let mut shell = Shell::new(20, 10);
        shell.feed("\x1b]7;file:///tmp\x07\x1b]7;file:///tmp\x07");
        shell.feed("\x1b]7;file://elsewhere-not-this-host/srv\x07\x1b]7;file:///\x07");
        assert_eq!(shell.events(), ["WorkingDirectory(/tmp)", "WorkingDirectory(/)"]);
    }

    #[test]
    fn parses_prompt_marks() {
        let parse = |payload: &[u8]| ShellSequence::parse(payload);
        let mark = |mark| Some(ShellSequence::Prompt(mark));

        assert_eq!(parse(b"133;A"), mark(PromptMark::PromptStart));
        assert_eq!(parse(b"133;A;aid=12;cl=m"), mark(PromptMark::PromptStart));
        assert_eq!(parse(b"133;A;k=s"), mark(PromptMark::Prompt));
        assert_eq!(parse(b"133;P;k=r"), mark(PromptMark::Prompt));
        assert_eq!(parse(b"133;B"), mark(PromptMark::CommandStart));
        assert_eq!(parse(b"133;C"), mark(PromptMark::OutputStart));
        assert_eq!(parse(b"133;D"), mark(PromptMark::CommandEnd(None)));
        assert_eq!(parse(b"133;D;0"), mark(PromptMark::CommandEnd(Some(0))));
        assert_eq!(parse(b"133;D;130;aid=12"), mark(PromptMark::CommandEnd(Some(130))));
        assert_eq!(parse(b"133;D;nope"), mark(PromptMark::CommandEnd(None)));
        assert_eq!(parse(b"133;Z"), None);
        assert_eq!(parse(b"133"), None);
        assert_eq!(parse(b"8;;https://example.org"), None);
    }

    #[cfg(unix)]
    #[test]
    fn parses_local_directories() {
        let directory = |url: &[u8]| match ShellSequence::parse(&[b"7;", url].concat()) {
            Some(ShellSequence::WorkingDirectory(path)) => Some(path),
            _ => None,
        };

        assert_eq!(directory(b"file:///home/me"), Some(PathBuf::from("/home/me")));
        assert_eq!(directory(b"file://localhost/tmp/a%20b"), Some(PathBuf::from("/tmp/a b")));
        assert_eq!(directory(b"file:///tmp/100%"), Some(PathBuf::from("/tmp/100%")));
        assert_eq!(
            directory(b"kitty-shell-cwd://localhost/tmp/a%20b"),
            Some(PathBuf::from("/tmp/a%20b"))
        );

        let host = host_name().unwrap();
        let url = format!("file://{}/srv", host.to_uppercase());
        assert_eq!(directory(url.as_bytes()), Some(PathBuf::from("/srv")));
        let url = format!("file://{host}.example.org/srv");
        assert_eq!(directory(url.as_bytes()), Some(PathBuf::from("/srv")));

        assert_eq!(
            ShellSequence::parse(b"7;file://elsewhere-not-this-host/srv"),
            Some(ShellSequence::RemoteDirectory)
        );
        assert_eq!(directory(b"file://localhost"), None);
        assert_eq!(directory(b"https://localhost/srv"), None);
    }
}
