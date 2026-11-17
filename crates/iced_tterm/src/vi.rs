//! Vi mode: a cursor of its own, moved through the terminal's text and its
//! scrollback with keys, to select text and to search from.
//!
//! The motions are Alacritty's, in `tankterm_terminal`. This is what
//! Alacritty's front end does with them: selections from the vi cursor,
//! jumps to search matches and to characters on the cursor's line, and
//! searches for the word under the cursor.

use tankterm_terminal::event::EventListener;
use tankterm_terminal::grid::{BidirectionalIterator, Dimensions, Scroll};
use tankterm_terminal::index::{Boundary, Direction, Point, Side};
use tankterm_terminal::selection::{Selection, SelectionType};
use tankterm_terminal::term::cell::Flags;
use tankterm_terminal::term::{Term, TermMode};
pub use tankterm_terminal::vi_mode::ViMotion;

use crate::search::{Search, SearchCommand, SearchDirection};

/// What to do in a terminal's vi mode. Everything but [`Self::Toggle`] is
/// ignored outside vi mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViCommand {
    /// Enter vi mode, or leave it.
    Toggle,
    /// Move the vi cursor.
    Motion(ViMotion),
    /// Start a selection of this type at the vi cursor. A selection of
    /// another type becomes one of this type, and one of this type is
    /// dropped.
    ToggleSelection(SelectionType),
    /// Move the vi cursor to the next match of the last search, in the
    /// search's direction.
    SearchNext,
    /// Move the vi cursor to the next match of the last search, against the
    /// search's direction.
    SearchPrevious,
    /// Move the vi cursor to the start of the search match it is in, or of
    /// the one before it.
    SearchStart,
    /// Move the vi cursor to the end of the search match it is in, or of the
    /// one after it.
    SearchEnd,
    /// Scroll the view to put the vi cursor's line in the middle.
    CenterAroundCursor,
    /// Wait for a character, then move the vi cursor to it on its line. With
    /// `stop_short`, the cursor stops next to the character.
    InlineSearch {
        direction: SearchDirection,
        stop_short: bool,
    },
    /// The text typed for the character an inline search waits for.
    InlineSearchInput(String),
    /// Move to the inline search's character again, in its direction.
    InlineSearchNext,
    /// Move to the inline search's character again, against its direction.
    InlineSearchPrevious,
    /// Search for the selected text, or for the word under the vi cursor,
    /// and move to its next match in this direction.
    SemanticSearch(SearchDirection),
    /// Choose the hint under the vi cursor, of those for the mouse.
    Open,
}

/// The vi mode state the terminal doesn't keep itself.
#[derive(Debug)]
pub(crate) struct Vi {
    /// The character an inline search moves to, once it was typed.
    character: Option<char>,
    /// Whether an inline search waits for its character.
    pending: bool,
    direction: Direction,
    stop_short: bool,
}

impl Default for Vi {
    fn default() -> Self {
        Self {
            character: None,
            pending: false,
            direction: Direction::Right,
            stop_short: false,
        }
    }
}

impl Vi {
    /// Whether an inline search waits for the character to move to. The
    /// next key typed is that character, whatever it is bound to.
    pub(crate) fn inline_search_pending(&self) -> bool {
        self.pending
    }

    pub(crate) fn handle<T: EventListener>(
        &mut self,
        term: &mut Term<T>,
        search: &mut Search,
        command: ViCommand,
    ) {
        if command != ViCommand::Toggle && !term.mode().contains(TermMode::VI) {
            return;
        }

        match command {
            ViCommand::Toggle => self.toggle(term, search),
            ViCommand::Motion(motion) => term.vi_motion(motion),
            ViCommand::ToggleSelection(ty) => toggle_selection(term, ty),
            ViCommand::SearchNext => {
                goto_search_match(term, search, search.direction())
            },
            ViCommand::SearchPrevious => {
                let direction = search.direction().opposite();
                goto_search_match(term, search, direction);
            },
            ViCommand::SearchStart => {
                let point = term.vi_mode_cursor.point;
                let origin = point.sub(term, Boundary::None, 1);
                let found = search.next_match(
                    term,
                    origin,
                    Direction::Left,
                    Side::Left,
                );
                if let Some(found) = found {
                    term.vi_goto_point(*found.start());
                }
            },
            ViCommand::SearchEnd => {
                let point = term.vi_mode_cursor.point;
                let origin = point.add(term, Boundary::None, 1);
                let found = search.next_match(
                    term,
                    origin,
                    Direction::Right,
                    Side::Right,
                );
                if let Some(found) = found {
                    term.vi_goto_point(*found.end());
                }
            },
            ViCommand::CenterAroundCursor => {
                let offset = term.grid().display_offset() as i32;
                let middle = -offset + term.screen_lines() as i32 / 2 - 1;
                let line = term.vi_mode_cursor.point.line.0;
                term.scroll_display(Scroll::Delta(middle - line));
            },
            ViCommand::InlineSearch {
                direction,
                stop_short,
            } => {
                self.pending = true;
                self.character = None;
                self.direction = direction.into();
                self.stop_short = stop_short;
            },
            ViCommand::InlineSearchInput(text) => {
                // Keys without text, like modifiers, aren't the character.
                let Some(c) = text.chars().next() else { return };
                if !self.pending {
                    return;
                }

                self.pending = false;
                self.character = Some(c);
                self.inline_search(term, self.direction);
            },
            ViCommand::InlineSearchNext => {
                self.inline_search(term, self.direction)
            },
            ViCommand::InlineSearchPrevious => {
                self.inline_search(term, self.direction.opposite())
            },
            ViCommand::SemanticSearch(direction) => {
                seeded_search(term, search, direction.into())
            },
            // The hints are the backend's.
            ViCommand::Open => (),
        }
    }

    fn toggle<T: EventListener>(
        &mut self,
        term: &mut Term<T>,
        search: &mut Search,
    ) {
        if term.mode().contains(TermMode::VI) {
            // The matches of the last search stop being shown.
            search.drop_regex();
        } else {
            term.selection = None;
        }

        if search.active() {
            search.handle(term, SearchCommand::Cancel);
        }

        self.pending = false;
        term.toggle_vi_mode();
    }

    /// Move to the inline search's character on the vi cursor's line, in
    /// `direction`.
    fn inline_search<T: EventListener>(
        &self,
        term: &mut Term<T>,
        direction: Direction,
    ) {
        let Some(c) = self.character else { return };
        let mut buf = [0; 4];
        let needle = c.encode_utf8(&mut buf);

        let point = term.vi_mode_cursor.point;
        let found = match direction {
            Direction::Right => term.inline_search_right(point, needle),
            Direction::Left => term.inline_search_left(point, needle),
        };
        let Ok(mut point) = found else { return };

        if self.stop_short {
            let grid = term.grid();
            point = match direction {
                Direction::Right => grid
                    .iter_from(point)
                    .prev()
                    .map_or(point, |cell| cell.point),
                Direction::Left => grid
                    .iter_from(point)
                    .next()
                    .map_or(point, |cell| cell.point),
            };
        }

        term.vi_goto_point(point);
    }
}

/// Start a selection of `ty` at the vi cursor, change the selection's type
/// to `ty`, or drop a selection of `ty`.
fn toggle_selection<T: EventListener>(term: &mut Term<T>, ty: SelectionType) {
    match &mut term.selection {
        Some(selection) if !selection.is_empty() && selection.ty == ty => {
            term.selection = None;
        },
        Some(selection) if !selection.is_empty() => selection.ty = ty,
        _ => {
            let point = term.vi_mode_cursor.point;
            let mut selection = Selection::new(ty, point, Side::Left);
            // The cell under the cursor is selected from the start.
            selection.include_all();
            term.selection = Some(selection);
        },
    }
}

/// Move the vi cursor to the start of the next search match in `direction`.
fn goto_search_match<T: EventListener>(
    term: &mut Term<T>,
    search: &mut Search,
    direction: Direction,
) {
    let point = term.vi_mode_cursor.point;
    let origin = match direction {
        Direction::Right => point.add(term, Boundary::None, 1),
        Direction::Left => point.sub(term, Boundary::None, 1),
    };

    if let Some(found) = search.next_match(term, origin, direction, Side::Left)
    {
        term.vi_goto_point(*found.start());
    }
}

/// Search for the selected text, or for the word under the vi cursor, and
/// move the vi cursor to its next match in `direction`.
fn seeded_search<T: EventListener>(
    term: &mut Term<T>,
    search: &mut Search,
    direction: Direction,
) {
    let origin = term.vi_mode_cursor.point;
    let text = match term.selection_to_string() {
        Some(text) if !text.is_empty() => text,
        _ => semantic_word(term, origin),
    };
    if text.is_empty() {
        return;
    }

    // The text is searched for as it is, not as a regex.
    let mut regex = String::with_capacity(text.len());
    for c in text.chars() {
        if let '$' | '('..='+' | '?' | '['..='^' | '{'..='}' = c {
            regex.push('\\');
        }
        regex.push(c);
    }

    term.selection = None;
    let start = match direction {
        Direction::Right => SearchDirection::Forward,
        Direction::Left => SearchDirection::Backward,
    };
    search.handle(term, SearchCommand::Start(start));
    search.handle(term, SearchCommand::Input(regex));
    search.handle(term, SearchCommand::Confirm);

    // The match the cursor is on is passed over: from the end of the match
    // at or after the cursor, the next one in `direction` is the target.
    let Some(current) =
        search.next_match(term, origin, Direction::Right, Side::Right)
    else {
        return;
    };
    let target = match direction {
        Direction::Right => {
            let origin = current.end().add(term, Boundary::None, 1);
            search.next_match(term, origin, Direction::Right, Side::Left)
        },
        Direction::Left => {
            let origin = current.start().sub(term, Boundary::None, 1);
            search.next_match(term, origin, Direction::Left, Side::Left)
        },
    };

    if let Some(target) = target {
        term.vi_goto_point(*target.start());
    }
}

/// The word at `point`, as words are told apart by the semantic escape
/// characters. On an escape character, the word after it.
fn semantic_word<T>(term: &Term<T>, point: Point) -> String {
    let grid = term.grid();

    // The end of the word.
    let mut end = term.semantic_search_right(point);

    // Where going on past escape characters gets back to where it started.
    let start_cell = &grid[point];
    let search_end =
        if start_cell.flags.intersects(Flags::LEADING_WIDE_CHAR_SPACER) {
            point.add(term, Boundary::None, 2)
        } else if start_cell.flags.intersects(Flags::WIDE_CHAR) {
            point.add(term, Boundary::None, 1)
        } else {
            point
        };

    // Go on until the end is not on an escape character.
    let escape_chars = term.semantic_escape_chars();
    loop {
        let cell = &grid[end];
        let c = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            grid[end.sub(term, Boundary::None, 1)].c
        } else {
            cell.c
        };

        if !escape_chars.contains(c) {
            break;
        }

        end = term.semantic_search_right(end.add(term, Boundary::None, 1));

        // The whole grid is escape characters.
        if end == search_end {
            return String::new();
        }
    }

    let start = term.semantic_search_left(end);
    term.bounds_to_string(start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    use tankterm_terminal::event::VoidListener;
    use tankterm_terminal::index::{Column, Line};
    use tankterm_terminal::term::test::TermSize;
    use tankterm_terminal::term::Config;
    use tankterm_terminal::vte::ansi;

    /// A terminal of `screen` lines of 20 columns with `lines` written to
    /// it, and room for all of them in its scrollback, in vi mode.
    fn term(screen: usize, lines: &[&str]) -> Term<VoidListener> {
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

    fn cursor(term: &Term<VoidListener>) -> (i32, usize) {
        let point = term.vi_mode_cursor.point;
        (point.line.0, point.column.0)
    }

    fn go_to(term: &mut Term<VoidListener>, line: i32, column: usize) {
        term.vi_mode_cursor.point = Point::new(Line(line), Column(column));
    }

    #[test]
    fn commands_need_vi_mode_and_toggle_starts_it_at_the_cursor() {
        let mut term = term(3, &["one", "two", "three"]);
        let (mut vi, mut search) = (Vi::default(), Search::default());

        vi.handle(&mut term, &mut search, ViCommand::Motion(ViMotion::Up));
        assert!(!term.mode().contains(TermMode::VI));

        vi.handle(&mut term, &mut search, ViCommand::Toggle);
        assert!(term.mode().contains(TermMode::VI));
        assert_eq!(cursor(&term), (2, 5));

        vi.handle(&mut term, &mut search, ViCommand::Motion(ViMotion::Up));
        assert_eq!(cursor(&term), (1, 5));

        vi.handle(&mut term, &mut search, ViCommand::Toggle);
        assert!(!term.mode().contains(TermMode::VI));
    }

    #[test]
    fn selections_follow_the_cursor_and_toggle_off() {
        let mut term = term(3, &["one two three"]);
        let (mut vi, mut search) = (Vi::default(), Search::default());
        vi.handle(&mut term, &mut search, ViCommand::Toggle);
        go_to(&mut term, 0, 0);

        let simple = ViCommand::ToggleSelection(SelectionType::Simple);
        vi.handle(&mut term, &mut search, simple.clone());
        assert_eq!(term.selection_to_string().as_deref(), Some("o"));

        let word = ViCommand::Motion(ViMotion::SemanticRight);
        vi.handle(&mut term, &mut search, word);
        assert_eq!(term.selection_to_string().as_deref(), Some("one t"));

        let lines = ViCommand::ToggleSelection(SelectionType::Lines);
        vi.handle(&mut term, &mut search, lines);
        assert_eq!(
            term.selection_to_string().as_deref(),
            Some("one two three\n")
        );

        vi.handle(
            &mut term,
            &mut search,
            ViCommand::ToggleSelection(SelectionType::Lines),
        );
        assert_eq!(term.selection, None);
    }

    #[test]
    fn inline_search_waits_for_its_character_and_repeats() {
        let mut term = term(3, &["a.b.c.d"]);
        let (mut vi, mut search) = (Vi::default(), Search::default());
        vi.handle(&mut term, &mut search, ViCommand::Toggle);
        go_to(&mut term, 0, 0);

        let forward = ViCommand::InlineSearch {
            direction: SearchDirection::Forward,
            stop_short: false,
        };
        vi.handle(&mut term, &mut search, forward);
        assert!(vi.inline_search_pending());
        // A key without text, like Shift, is not the character.
        let input = |text: &str| ViCommand::InlineSearchInput(text.into());
        vi.handle(&mut term, &mut search, input(""));
        assert!(vi.inline_search_pending());

        vi.handle(&mut term, &mut search, input("."));
        assert!(!vi.inline_search_pending());
        assert_eq!(cursor(&term), (0, 1));

        vi.handle(&mut term, &mut search, ViCommand::InlineSearchNext);
        assert_eq!(cursor(&term), (0, 3));
        vi.handle(&mut term, &mut search, ViCommand::InlineSearchPrevious);
        assert_eq!(cursor(&term), (0, 1));

        // Stopping short ends next to the character.
        let short = ViCommand::InlineSearch {
            direction: SearchDirection::Forward,
            stop_short: true,
        };
        vi.handle(&mut term, &mut search, short);
        vi.handle(&mut term, &mut search, input("d"));
        assert_eq!(cursor(&term), (0, 5));
    }

    #[test]
    fn search_moves_the_cursor_between_matches() {
        let mut term = term(5, &["x", "needle", "x", "needle", "x"]);
        let (mut vi, mut search) = (Vi::default(), Search::default());
        vi.handle(&mut term, &mut search, ViCommand::Toggle);
        go_to(&mut term, 0, 0);

        let forward = SearchCommand::Start(SearchDirection::Forward);
        search.handle(&mut term, forward);
        search.handle(&mut term, SearchCommand::Input("needle".into()));
        assert_eq!(cursor(&term), (1, 0));
        // Confirming in vi mode keeps the cursor on the match, and the
        // regex for the next jumps.
        search.handle(&mut term, SearchCommand::Confirm);
        assert!(!search.active());
        assert_eq!(cursor(&term), (1, 0));

        vi.handle(&mut term, &mut search, ViCommand::SearchNext);
        assert_eq!(cursor(&term), (3, 0));
        vi.handle(&mut term, &mut search, ViCommand::SearchEnd);
        assert_eq!(cursor(&term), (3, 5));
        vi.handle(&mut term, &mut search, ViCommand::SearchStart);
        assert_eq!(cursor(&term), (3, 0));
        vi.handle(&mut term, &mut search, ViCommand::SearchPrevious);
        assert_eq!(cursor(&term), (1, 0));
    }

    #[test]
    fn semantic_search_finds_the_word_under_the_cursor() {
        let mut term = term(4, &["a foo(bar) b", "foo(bar)", "c foo(bar)"]);
        let (mut vi, mut search) = (Vi::default(), Search::default());
        vi.handle(&mut term, &mut search, ViCommand::Toggle);
        // On the "f" of the first "foo(bar)". The parentheses are escape
        // characters, so the word is "foo".
        go_to(&mut term, 0, 2);

        let forward = ViCommand::SemanticSearch(SearchDirection::Forward);
        vi.handle(&mut term, &mut search, forward.clone());
        assert_eq!(cursor(&term), (1, 0));
        assert_eq!(search.regex(), None);
        vi.handle(&mut term, &mut search, forward);
        assert_eq!(cursor(&term), (2, 2));

        let backward = ViCommand::SemanticSearch(SearchDirection::Backward);
        vi.handle(&mut term, &mut search, backward);
        assert_eq!(cursor(&term), (1, 0));

        // Selected text is searched for as it is, regex characters and all.
        go_to(&mut term, 1, 0);
        vi.handle(
            &mut term,
            &mut search,
            ViCommand::ToggleSelection(SelectionType::Simple),
        );
        for _ in 0..7 {
            vi.handle(
                &mut term,
                &mut search,
                ViCommand::Motion(ViMotion::Right),
            );
        }
        let forward = ViCommand::SemanticSearch(SearchDirection::Forward);
        vi.handle(&mut term, &mut search, forward);
        assert_eq!(term.selection, None);
        assert_eq!(cursor(&term), (2, 2));
    }

    #[test]
    fn center_puts_the_cursor_line_in_the_middle_of_the_view() {
        let lines: Vec<String> = (0..20).map(|n| format!("line {n}")).collect();
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        let mut term = term(5, &lines);
        let (mut vi, mut search) = (Vi::default(), Search::default());
        vi.handle(&mut term, &mut search, ViCommand::Toggle);
        go_to(&mut term, -10, 0);

        vi.handle(&mut term, &mut search, ViCommand::CenterAroundCursor);
        // The middle of five lines is the second from the top here, as in
        // Alacritty: line -10 is shown second.
        assert_eq!(term.grid().display_offset(), 11);
    }
}
