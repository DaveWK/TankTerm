//! Searching the terminal's text, scrollback included, for a regex typed
//! into a bar on the terminal's last row.
//!
//! This is Alacritty's search: matches are found from where the search
//! started while the regex is typed, the view scrolls to the focused match,
//! and the regexes searched for are kept in a history.

use std::collections::VecDeque;
use std::mem;

use tankterm_terminal::event::EventListener;
use tankterm_terminal::grid::{Dimensions, Scroll};
use tankterm_terminal::index::{
    Boundary, Column, Direction, Line, Point, Side,
};
use tankterm_terminal::selection::{Selection, SelectionType};
use tankterm_terminal::term::search::{Match, RegexIter, RegexSearch};
use tankterm_terminal::term::{Term, TermMode};
use unicode_width::UnicodeWidthChar;

/// Lines searched for a match while the regex is typed. When there is none
/// in them, the rest of the buffer is searched once typing pauses.
const MAX_SEARCH_WHILE_TYPING: usize = 1000;

/// Most regexes the search history keeps.
const MAX_HISTORY: usize = 255;

/// Lines searched before and after the view for matches that reach into it.
const MAX_SEARCH_LINES: usize = 100;

/// The bar's label while searching forward.
const FORWARD_LABEL: &str = "Search: ";

/// The bar's label while searching backward.
const BACKWARD_LABEL: &str = "Backward Search: ";

/// Where a search goes from where it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchDirection {
    /// Down the buffer, towards the newest line.
    Forward,
    /// Up the buffer, towards the oldest line.
    Backward,
}

impl From<SearchDirection> for Direction {
    fn from(direction: SearchDirection) -> Self {
        match direction {
            SearchDirection::Forward => Direction::Right,
            SearchDirection::Backward => Direction::Left,
        }
    }
}

/// What to do with a terminal's search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchCommand {
    /// Show the search bar with an empty regex, searching from the view in
    /// this direction.
    Start(SearchDirection),
    /// Add text to the regex. Backspace (`\x08` or `\x7f`) deletes the last
    /// character, and other control characters are left out.
    Input(String),
    /// Focus the next match in the search's direction.
    FocusNext,
    /// Focus the next match against the search's direction.
    FocusPrevious,
    /// Close the search bar. In vi mode the vi cursor stays on the focused
    /// match, and the matches stay shown for the vi mode search commands.
    /// Outside vi mode this is [`Self::Cancel`].
    Confirm,
    /// Close the search bar. Outside vi mode the focused match is left
    /// selected; in vi mode the vi cursor goes back to where it was.
    Cancel,
    /// Empty the regex.
    Clear,
    /// Delete the regex's last word.
    DeleteWord,
    /// Show the regex searched for before the shown one.
    HistoryPrevious,
    /// Show the regex searched for after the shown one.
    HistoryNext,
    /// Search the whole buffer, after the search while typing found no match
    /// in the lines it looks at.
    Resume,
}

/// The state of a terminal's search.
pub(crate) struct Search {
    direction: Direction,

    /// The regex shown in the bar, while the bar is shown.
    history_index: Option<usize>,

    /// The regexes searched for, the newest first. While the bar is shown,
    /// the first one is the regex being typed.
    history: VecDeque<String>,

    /// Where the search starts. Typing more of the regex searches from here
    /// again.
    origin: Point,

    /// How far the view scrolled since the search started, to go back in
    /// vi mode.
    display_offset_delta: i32,

    /// The match the view was scrolled to.
    focused_match: Option<Match>,

    /// The compiled regex, while there is a valid one.
    dfas: Option<RegexSearch>,

    /// Whether the search while typing found no match in the lines it looked
    /// at, and the rest of the buffer is still to be searched.
    pending: bool,

    /// Counts the changes to the regex, which tell pending searches apart.
    generation: u64,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            direction: Direction::Right,
            history_index: None,
            history: VecDeque::new(),
            origin: Point::default(),
            display_offset_delta: 0,
            focused_match: None,
            dfas: None,
            pending: false,
            generation: 0,
        }
    }
}

impl Search {
    /// Whether the search bar is shown.
    pub(crate) fn active(&self) -> bool {
        self.history_index.is_some()
    }

    /// The regex in the search bar, while it is shown.
    pub(crate) fn regex(&self) -> Option<&str> {
        self.history_index
            .and_then(|index| self.history.get(index))
            .map(String::as_str)
    }

    /// The match the view was scrolled to.
    pub(crate) fn focused_match(&self) -> Option<&Match> {
        self.focused_match.as_ref()
    }

    /// Forget the focused match, whose place in the grid a resize changes.
    pub(crate) fn clear_focused_match(&mut self) {
        self.focused_match = None;
    }

    /// The direction the search goes in.
    pub(crate) fn direction(&self) -> Direction {
        self.direction
    }

    /// The next match of the last regex searched for, from `origin` in
    /// `direction`: the first one whose start, or end with `Side::Right`,
    /// is at or past `origin`. The search goes round at the ends of the
    /// buffer.
    pub(crate) fn next_match<T>(
        &mut self,
        term: &Term<T>,
        origin: Point,
        direction: Direction,
        side: Side,
    ) -> Option<Match> {
        let dfas = self.dfas.as_mut()?;
        term.search_next(dfas, origin, direction, side, None)
    }

    /// Forget the last regex searched for, so its matches are no longer
    /// shown.
    pub(crate) fn drop_regex(&mut self) {
        self.dfas = None;
    }

    /// The view was scrolled by `lines`, up being positive, other than by
    /// the search. Leaving the search in vi mode scrolls back by them too.
    pub(crate) fn scrolled(&mut self, lines: i32) {
        if self.active() {
            self.display_offset_delta += lines;
        }
    }

    /// The regex the whole buffer is still to be searched for, as a number
    /// that changes with it.
    pub(crate) fn pending(&self) -> Option<u64> {
        self.pending.then_some(self.generation)
    }

    /// The search bar's text, cut to fit `columns` cells, with a cell left
    /// over at the end for the bar's cursor.
    pub(crate) fn bar(&self, columns: usize) -> Option<String> {
        let regex = self.regex()?;
        let label = match self.direction {
            Direction::Right => FORWARD_LABEL,
            Direction::Left => BACKWARD_LABEL,
        };

        if label.len() >= columns {
            return Some(label.chars().take(columns).collect());
        }

        let mut text = String::from(label);
        text.push_str(&keep_end(regex, columns - label.len() - 1));
        Some(text)
    }

    /// The matches in the view, and the lines next to it for those that
    /// reach into it, in order.
    pub(crate) fn visible_matches<T>(&mut self, term: &Term<T>) -> Vec<Match> {
        match &mut self.dfas {
            Some(dfas) => visible_regex_match_iter(term, dfas).collect(),
            None => Vec::new(),
        }
    }

    pub(crate) fn handle<T: EventListener>(
        &mut self,
        term: &mut Term<T>,
        command: SearchCommand,
    ) {
        if !self.active()
            && !matches!(
                command,
                SearchCommand::Start(_) | SearchCommand::Resume
            )
        {
            return;
        }

        match command {
            SearchCommand::Start(direction) => {
                self.start(term, direction.into())
            },
            SearchCommand::Input(text) => self.input(term, &text),
            SearchCommand::FocusNext => self.advance(term, self.direction),
            SearchCommand::FocusPrevious => {
                self.advance(term, self.direction.opposite())
            },
            SearchCommand::Confirm => self.confirm(term),
            SearchCommand::Cancel => self.cancel(term),
            SearchCommand::Clear => {
                if let Some(regex) = self.editable() {
                    regex.clear();
                    self.update(term);
                }
            },
            SearchCommand::DeleteWord => {
                if let Some(regex) = self.editable() {
                    *regex = regex.trim_end().to_owned();
                    regex.truncate(regex.rfind(' ').map_or(0, |i| i + 1));
                    self.update(term);
                }
            },
            SearchCommand::HistoryPrevious => {
                match &mut self.history_index {
                    Some(index) if *index + 1 < self.history.len() => {
                        *index += 1
                    },
                    _ => return,
                }
                self.update(term);
            },
            SearchCommand::HistoryNext => {
                match &mut self.history_index {
                    Some(index) if *index > 0 => *index -= 1,
                    _ => return,
                }
                self.update(term);
            },
            SearchCommand::Resume => {
                if self.pending {
                    self.goto_match(term, None);
                }
            },
        }
    }

    /// The regex in the bar, to change. A regex from the history is copied
    /// to the front first, so the history keeps it as it was.
    fn editable(&mut self) -> Option<&mut String> {
        match self.history_index? {
            0 => (),
            index => {
                self.history[0] = self.history[index].clone();
                self.history_index = Some(0);
            },
        }
        self.history.front_mut()
    }

    fn start<T>(&mut self, term: &Term<T>, direction: Direction) {
        // An empty regex left in the history is used again.
        if self.history.front().is_none_or(|regex| !regex.is_empty()) {
            self.history.push_front(String::new());
            self.history.truncate(MAX_HISTORY);
        }

        self.history_index = Some(0);
        self.direction = direction;
        self.focused_match = None;
        self.pending = false;
        self.generation += 1;

        if term.mode().contains(TermMode::VI) {
            self.origin = term.vi_mode_cursor.point;
            self.display_offset_delta = 0;

            // The bar takes the last row, which moves the content up a line
            // when the cursor is on it.
            if term.grid().cursor.point.line + 1 == term.screen_lines() {
                self.origin.line -= 1;
            }
        } else {
            // Just outside the view, so a match at its edge is found first.
            let top = Line(-(term.grid().display_offset() as i32)) - 1;
            let bottom = top + term.bottommost_line();
            self.origin = match direction {
                Direction::Right => Point::new(top, Column(0)),
                Direction::Left => Point::new(bottom, term.last_column()),
            };
        }
    }

    fn input<T: EventListener>(&mut self, term: &mut Term<T>, text: &str) {
        let Some(regex) = self.editable() else { return };
        let before = regex.len();
        let mut deleted = false;
        for c in text.chars() {
            match c {
                '\x08' | '\x7f' => deleted |= regex.pop().is_some(),
                ' '..='~' | '\u{a0}'..='\u{10ffff}' => regex.push(c),
                _ => (),
            }
        }
        if !deleted && regex.len() == before {
            return;
        }

        // A selection would hide the matches under it.
        if !term.mode().contains(TermMode::VI) {
            term.selection = None;
        }

        self.update(term);
    }

    /// Search for the regex as it is now.
    fn update<T: EventListener>(&mut self, term: &mut Term<T>) {
        self.generation += 1;
        self.pending = false;

        let Some(regex) = self.regex() else { return };
        if regex.is_empty() {
            // Nothing to search for.
            self.reset(term);
            self.dfas = None;
        } else {
            self.dfas = RegexSearch::new(regex).ok();
            self.goto_match(term, Some(MAX_SEARCH_WHILE_TYPING));
        }
    }

    /// Focus the first match from the origin, searching at most `limit`
    /// lines.
    fn goto_match<T: EventListener>(
        &mut self,
        term: &mut Term<T>,
        limit: Option<usize>,
    ) {
        let Some(dfas) = &mut self.dfas else { return };

        // A limit is only worth having in a buffer that is longer.
        let limit = limit.filter(|&limit| limit <= term.total_lines());
        let origin = self.origin.grid_clamp(term, Boundary::Grid);
        let found =
            term.search_next(dfas, origin, self.direction, Side::Left, limit);

        match found {
            Some(regex_match) => {
                let old_offset = term.grid().display_offset() as i32;
                if term.mode().contains(TermMode::VI) {
                    term.vi_goto_point(*regex_match.start());
                } else {
                    term.scroll_to_point(*regex_match.start());
                }
                let new_offset = term.grid().display_offset() as i32;

                self.focused_match = Some(regex_match);
                self.display_offset_delta += old_offset - new_offset;
                self.pending = false;
            },
            // Only with the whole buffer searched is there no match, and the
            // view goes back to where it was.
            None if limit.is_none() => self.reset(term),
            None => {
                self.focused_match = None;
                self.pending = true;
            },
        }
    }

    /// Focus the next match in `direction` from the focused one.
    fn advance<T: EventListener>(
        &mut self,
        term: &mut Term<T>,
        direction: Direction,
    ) {
        // The search goes on from just past the focused match.
        if let Some(focused) = &self.focused_match {
            let origin = match direction {
                Direction::Right => focused.end().add(term, Boundary::None, 1),
                Direction::Left => focused.start().sub(term, Boundary::None, 1),
            };

            term.scroll_to_point(origin);
            self.display_offset_delta = 0;
            self.origin = origin;
        }

        let search_direction = mem::replace(&mut self.direction, direction);
        self.goto_match(term, None);
        self.direction = search_direction;

        // Typing more of the regex searches from the start of the new match,
        // so it stays focused while it still matches.
        let Some(focused) = &self.focused_match else {
            return;
        };
        let origin = match self.direction {
            Direction::Right => *focused.start(),
            Direction::Left => *focused.end(),
        };

        // Remember how far the view has to scroll to the origin.
        let old_offset = term.grid().display_offset() as i32;
        term.scroll_to_point(origin);
        let new_offset = term.grid().display_offset() as i32;
        self.display_offset_delta = new_offset - old_offset;

        term.scroll_display(Scroll::Delta(-self.display_offset_delta));
        self.origin = origin;
    }

    /// Lose the focused match, and in vi mode go back to where the search
    /// started.
    fn reset<T: EventListener>(&mut self, term: &mut Term<T>) {
        self.focused_match = None;
        self.pending = false;

        // Outside vi mode the search starts from the view, which hasn't
        // moved.
        if !term.mode().contains(TermMode::VI) {
            return;
        }

        term.vi_mode_cursor.point = self.origin;
        term.scroll_display(Scroll::Delta(self.display_offset_delta));
        self.display_offset_delta = 0;
    }

    fn confirm<T: EventListener>(&mut self, term: &mut Term<T>) {
        if !term.mode().contains(TermMode::VI) {
            self.cancel(term);
            return;
        }

        // A search cut short while typing finishes first.
        if self.pending {
            self.goto_match(term, None);
        }

        self.exit();
    }

    fn cancel<T: EventListener>(&mut self, term: &mut Term<T>) {
        if term.mode().contains(TermMode::VI) {
            self.reset(term);
        } else if let Some(focused) = &self.focused_match {
            let mut selection = Selection::new(
                SelectionType::Simple,
                *focused.start(),
                Side::Left,
            );
            selection.update(*focused.end(), Side::Right);
            term.selection = Some(selection);
        }

        self.dfas = None;
        self.exit();
    }

    /// Close the search bar.
    fn exit(&mut self) {
        self.history_index = None;
        self.focused_match = None;
        self.pending = false;
    }
}

/// The end of `text` that fits `width` cells, after an ellipsis if some of it
/// was cut off.
fn keep_end(text: &str, width: usize) -> String {
    let cells = |c: char| c.width().unwrap_or(0);
    if text.chars().map(cells).sum::<usize>() <= width {
        return text.to_owned();
    }

    let mut kept = Vec::new();
    let mut used = 0;
    for c in text.chars().rev() {
        if used + cells(c) + 1 > width {
            break;
        }
        used += cells(c);
        kept.push(c);
    }

    let ellipsis = (width > 0).then_some('…');
    ellipsis.into_iter().chain(kept.into_iter().rev()).collect()
}

/// The regex's matches in the view, and in the lines next to it for those
/// that reach into it, in order.
pub(crate) fn visible_regex_match_iter<'a, T>(
    term: &'a Term<T>,
    regex: &'a mut RegexSearch,
) -> impl Iterator<Item = Match> + 'a {
    let viewport_start = Line(-(term.grid().display_offset() as i32));
    let viewport_end = viewport_start + term.bottommost_line();
    let mut start =
        term.line_search_left(Point::new(viewport_start, Column(0)));
    let mut end = term.line_search_right(Point::new(viewport_end, Column(0)));
    start.line = start.line.max(viewport_start - MAX_SEARCH_LINES);
    end.line = end.line.min(viewport_end + MAX_SEARCH_LINES);

    RegexIter::new(start, end, Direction::Right, term, regex)
        .skip_while(move |rm| rm.end().line < viewport_start)
        .take_while(move |rm| rm.start().line <= viewport_end)
}

#[cfg(test)]
mod tests {
    use super::*;

    use tankterm_terminal::event::VoidListener;
    use tankterm_terminal::term::test::TermSize;
    use tankterm_terminal::term::Config;
    use tankterm_terminal::vte::ansi;

    /// A terminal of `screen` lines of 20 columns with `lines` written to
    /// it, and room for all of them in its scrollback.
    fn term(screen: usize, lines: &[String]) -> Term<VoidListener> {
        let config = Config {
            scrolling_history: lines.len(),
            ..Config::default()
        };
        let mut term =
            Term::new(config, &TermSize::new(20, screen), VoidListener);
        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, lines.join("\r\n").as_bytes());
        term
    }

    /// Lines numbered from 0, with a needle on the lines in `needles`.
    fn numbered(count: usize, needles: &[usize]) -> Vec<String> {
        (0..count)
            .map(|line| match needles.contains(&line) {
                true => format!("needle {line}"),
                false => format!("line {line}"),
            })
            .collect()
    }

    fn input(text: &str) -> SearchCommand {
        SearchCommand::Input(text.to_owned())
    }

    /// The line the focused match starts on.
    fn focused_line(search: &Search) -> Option<i32> {
        search.focused_match().map(|found| found.start().line.0)
    }

    #[test]
    fn typing_focuses_matches_and_scrolls_to_them() {
        // 30 lines on a screen of 5: line 25 is at the top of the screen.
        let mut term = term(5, &numbered(30, &[3, 20]));
        let mut search = Search::default();

        search
            .handle(&mut term, SearchCommand::Start(SearchDirection::Backward));
        assert!(search.active());
        search.handle(&mut term, input("needle"));
        assert_eq!(focused_line(&search), Some(20 - 25));
        let offset = term.grid().display_offset();
        assert!(
            (5..=9).contains(&offset),
            "line 20 is out of view at {offset}"
        );

        search.handle(&mut term, SearchCommand::FocusNext);
        assert_eq!(focused_line(&search), Some(3 - 25));
        assert!(term.grid().display_offset() >= 22);

        search.handle(&mut term, SearchCommand::FocusPrevious);
        assert_eq!(focused_line(&search), Some(20 - 25));

        // Both matches are found in the view and next to it.
        let matches = search.visible_matches(&term);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start().line.0, 20 - 25);
    }

    #[test]
    fn a_forward_search_starts_at_the_top_of_the_view() {
        let mut term = term(5, &numbered(30, &[3, 20, 27]));
        let mut search = Search::default();

        search
            .handle(&mut term, SearchCommand::Start(SearchDirection::Forward));
        search.handle(&mut term, input("needle"));
        assert_eq!(focused_line(&search), Some(27 - 25));
        assert_eq!(term.grid().display_offset(), 0);

        // Past the last match the search goes round to the first.
        search.handle(&mut term, SearchCommand::FocusNext);
        assert_eq!(focused_line(&search), Some(3 - 25));
    }

    #[test]
    fn cancelling_selects_the_focused_match() {
        let mut term = term(5, &numbered(30, &[20]));
        let mut search = Search::default();

        search
            .handle(&mut term, SearchCommand::Start(SearchDirection::Backward));
        search.handle(&mut term, input("needle 2"));
        search.handle(&mut term, SearchCommand::Cancel);

        assert!(!search.active());
        assert_eq!(term.selection_to_string().as_deref(), Some("needle 2"));
        assert!(search.visible_matches(&term).is_empty());
        // The view stays at the match.
        assert!(term.grid().display_offset() >= 5);
    }

    #[test]
    fn without_a_match_nothing_moves() {
        let mut term = term(5, &numbered(30, &[]));
        let mut search = Search::default();

        search
            .handle(&mut term, SearchCommand::Start(SearchDirection::Backward));
        search.handle(&mut term, input("needle"));
        assert_eq!(focused_line(&search), None);
        assert_eq!(search.pending(), None);
        assert_eq!(term.grid().display_offset(), 0);

        search.handle(&mut term, SearchCommand::Cancel);
        assert_eq!(term.selection_to_string(), None);
    }

    #[test]
    fn typing_leaves_the_rest_of_a_long_buffer_for_a_pause() {
        let mut term = term(5, &numbered(2001, &[0]));
        let mut search = Search::default();

        search
            .handle(&mut term, SearchCommand::Start(SearchDirection::Backward));
        search.handle(&mut term, input("needle"));
        assert_eq!(focused_line(&search), None);
        let pending = search.pending().expect("no search left for a pause");

        // Typing more changes which search is pending.
        search.handle(&mut term, input(" "));
        assert!(search.pending().is_some_and(|newer| newer != pending));

        search.handle(&mut term, SearchCommand::Resume);
        assert_eq!(focused_line(&search), Some(-1996));
        assert_eq!(search.pending(), None);
    }

    #[test]
    fn the_regex_is_typed_deleted_and_cleared() {
        let mut term = term(5, &numbered(5, &[]));
        let mut search = Search::default();
        let regex = |search: &Search| search.regex().map(str::to_owned);

        // Typing does nothing without the bar.
        search.handle(&mut term, input("x"));
        assert_eq!(regex(&search), None);

        search
            .handle(&mut term, SearchCommand::Start(SearchDirection::Forward));
        search.handle(&mut term, input("foo bar\x01\t"));
        assert_eq!(regex(&search).as_deref(), Some("foo bar"));
        search.handle(&mut term, input("\x08"));
        assert_eq!(regex(&search).as_deref(), Some("foo ba"));
        search.handle(&mut term, SearchCommand::DeleteWord);
        assert_eq!(regex(&search).as_deref(), Some("foo "));
        search.handle(&mut term, SearchCommand::DeleteWord);
        assert_eq!(regex(&search).as_deref(), Some(""));

        search.handle(&mut term, input("xyz"));
        search.handle(&mut term, SearchCommand::Clear);
        assert_eq!(regex(&search).as_deref(), Some(""));
    }

    #[test]
    fn regexes_are_kept_in_the_history() {
        let mut term = term(5, &numbered(5, &[]));
        let mut search = Search::default();
        let regex = |search: &Search| search.regex().map(str::to_owned);
        let forward = SearchCommand::Start(SearchDirection::Forward);

        search.handle(&mut term, forward.clone());
        search.handle(&mut term, input("abc"));
        search.handle(&mut term, SearchCommand::Cancel);

        // Changing a regex from the history leaves it there.
        search.handle(&mut term, forward.clone());
        search.handle(&mut term, SearchCommand::HistoryPrevious);
        assert_eq!(regex(&search).as_deref(), Some("abc"));
        search.handle(&mut term, input("d"));
        assert_eq!(regex(&search).as_deref(), Some("abcd"));
        search.handle(&mut term, SearchCommand::Cancel);

        // A search closed with nothing typed adds nothing.
        search.handle(&mut term, forward.clone());
        search.handle(&mut term, SearchCommand::Cancel);

        search.handle(&mut term, forward);
        assert_eq!(regex(&search).as_deref(), Some(""));
        search.handle(&mut term, SearchCommand::HistoryPrevious);
        assert_eq!(regex(&search).as_deref(), Some("abcd"));
        search.handle(&mut term, SearchCommand::HistoryPrevious);
        assert_eq!(regex(&search).as_deref(), Some("abc"));
        search.handle(&mut term, SearchCommand::HistoryPrevious);
        assert_eq!(regex(&search).as_deref(), Some("abc"));
        search.handle(&mut term, SearchCommand::HistoryNext);
        assert_eq!(regex(&search).as_deref(), Some("abcd"));
    }

    #[test]
    fn the_bar_shows_the_direction_and_what_fits_of_the_regex() {
        let mut term = term(5, &numbered(5, &[]));
        let mut search = Search::default();
        assert_eq!(search.bar(40), None);

        search
            .handle(&mut term, SearchCommand::Start(SearchDirection::Forward));
        search.handle(&mut term, input("needle"));
        assert_eq!(search.bar(40).as_deref(), Some("Search: needle"));
        assert_eq!(search.bar(12).as_deref(), Some("Search: …le"));
        assert_eq!(search.bar(5).as_deref(), Some("Searc"));

        search.handle(&mut term, SearchCommand::Cancel);
        search
            .handle(&mut term, SearchCommand::Start(SearchDirection::Backward));
        assert_eq!(search.bar(40).as_deref(), Some("Backward Search: "));
    }

    #[test]
    fn long_regexes_keep_their_end() {
        assert_eq!(keep_end("abc", 3), "abc");
        assert_eq!(keep_end("abcdef", 4), "…def");
        assert_eq!(keep_end("ab全", 3), "…全");
        assert_eq!(keep_end("abc", 0), "");
    }
}
