//! Hints: text in the view that matches a regex, and hyperlinks, to act on
//! without selecting them first. They are chosen by typing the labels drawn
//! over them, by clicking them while they are underlined under the mouse,
//! or with the vi cursor on them.
//!
//! Ported from Alacritty's `display/hint.rs`.

use std::cmp::Reverse;
use std::collections::HashSet;
use std::iter;

use iced_core::keyboard::Modifiers;
use tankterm_terminal::grid::{BidirectionalIterator, Dimensions};
use tankterm_terminal::index::{Boundary, Column, Direction, Line, Point};
use tankterm_terminal::term::cell::Hyperlink;
use tankterm_terminal::term::search::{Match, RegexIter, RegexSearch};
use tankterm_terminal::term::{Term, TermMode};

use crate::search::visible_regex_match_iter;

/// The share of the alphabet used for the last character of a label. The
/// other characters come from the rest, so a label can't be the start of
/// another one.
const HINT_SPLIT_PERCENTAGE: f32 = 0.5;

/// The characters of the labels unless the settings give others.
const DEFAULT_ALPHABET: &str = "jfkdls;ahgurieowpq";

/// What choosing a hint does with its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HintAction {
    /// Copy the text to the clipboard. The application does this, when the
    /// terminal returns [`crate::actions::Action::Hint`].
    Copy,
    /// Paste the text into the terminal, or into the search bar while it is
    /// shown.
    Paste,
    /// Select the text.
    Select,
    /// Move the vi cursor to the start of the text, entering vi mode.
    MoveViModeCursor,
    /// Run a program with the text as its last argument. The application
    /// does this, when the terminal returns [`crate::actions::Action::Hint`].
    Command { program: String, args: Vec<String> },
}

/// A kind of text the hints find, and what choosing it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    /// The regex the text matches.
    pub regex: Option<String>,
    /// Hyperlinks (OSC 8) are hints too, with their URI as their text.
    pub hyperlinks: bool,
    /// Leave unbalanced closing brackets and trailing punctuation out of a
    /// match, which URLs in prose are often followed by.
    pub post_processing: bool,
    /// Keep the labels after one is chosen, to choose more.
    pub persist: bool,
    pub action: HintAction,
    /// The modifiers that underline the hint under the mouse, for a click
    /// to choose it; without them the mouse doesn't. While the application
    /// asks for mouse reports, Shift has to be held as well.
    pub mouse: Option<Modifiers>,
}

/// The hints of a terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintSettings {
    /// The characters of the labels, at least two.
    pub alphabet: String,
    pub hints: Vec<Hint>,
}

impl Default for HintSettings {
    fn default() -> Self {
        Self {
            alphabet: DEFAULT_ALPHABET.to_owned(),
            hints: Vec::new(),
        }
    }
}

/// A command for a terminal's hints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HintCommand {
    /// Label the text in the view that the hint at this position in
    /// [`HintSettings::hints`] finds. Until a label is typed or the labels
    /// are taken away, keys type into the labels.
    Start(usize),
    /// Characters typed into the labels.
    Input(String),
    /// Take the last character typed into the labels back.
    Backspace,
    /// Take the labels away.
    Cancel,
    /// The mouse pointer is at this cell of the screen, counted from the
    /// top left corner of the view, with these modifiers held; or it left
    /// the terminal.
    Pointer(Option<(Point<usize>, Modifiers)>),
    /// Choose the hint underlined under the mouse, if there is one.
    OpenHovered,
}

/// A hint found in the terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintMatch {
    /// The position of its hint in the settings.
    hint: usize,
    bounds: Match,
    /// The hyperlink it is, if it is one.
    hyperlink: Option<Hyperlink>,
}

impl HintMatch {
    /// Whether the cell at `point`, which has the hyperlink `hyperlink`,
    /// belongs to the hint. All cells of a hyperlink do.
    pub fn contains(
        &self,
        point: Point,
        hyperlink: Option<&Hyperlink>,
    ) -> bool {
        self.hyperlink.as_ref() == hyperlink
            && (self.hyperlink.is_some() || self.bounds.contains(&point))
    }

    /// The URI of the hyperlink it is, if it is one.
    pub fn uri(&self) -> Option<&str> {
        self.hyperlink.as_ref().map(Hyperlink::uri)
    }

    pub fn bounds(&self) -> &Match {
        &self.bounds
    }

    /// The position of its hint in the settings.
    pub(crate) fn hint(&self) -> usize {
        self.hint
    }
}

/// A regex, compiled the first time it is used.
enum LazyRegex {
    Pattern(String),
    Compiled(Box<RegexSearch>),
    /// The regex doesn't compile; it finds nothing.
    Broken,
}

impl LazyRegex {
    fn compiled(&mut self) -> Option<&mut RegexSearch> {
        if let Self::Pattern(pattern) = self {
            *self = match RegexSearch::new(pattern) {
                Ok(regex) => Self::Compiled(Box::new(regex)),
                Err(err) => {
                    log::error!("could not compile hint regex: {err}");
                    Self::Broken
                },
            };
        }

        match self {
            Self::Compiled(regex) => Some(regex),
            _ => None,
        }
    }
}

/// A hint with its regex.
struct Compiled {
    hint: Hint,
    regex: Option<LazyRegex>,
}

/// The hints of a terminal: the labels while a hint is being chosen with
/// the keys, and where the mouse is.
#[derive(Default)]
pub(crate) struct Hints {
    settings: HintSettings,
    hints: Vec<Compiled>,
    /// The hint whose matches are labeled.
    active: Option<usize>,
    /// The labeled matches in the view, in order.
    matches: Vec<Match>,
    /// What is left of each match's label after the keys typed, or nothing
    /// for matches whose label doesn't start with them.
    labels: Vec<Vec<char>>,
    /// The characters typed into the labels.
    keys: Vec<char>,
    /// The cell of the screen the mouse pointer is on, and the modifiers
    /// held.
    pointer: Option<(Point<usize>, Modifiers)>,
}

impl Hints {
    /// Use these hints. Labels shown for others are taken away.
    pub fn set(&mut self, settings: HintSettings) {
        if self.settings == settings {
            return;
        }

        self.stop();
        self.hints = settings
            .hints
            .iter()
            .map(|hint| Compiled {
                hint: hint.clone(),
                regex: hint.regex.clone().map(LazyRegex::Pattern),
            })
            .collect();
        self.settings = settings;
    }

    /// Whether keys type into the labels.
    pub fn active(&self) -> bool {
        self.active.is_some()
    }

    /// Whether any hint is underlined under the mouse.
    pub fn for_mouse(&self) -> bool {
        self.hints
            .iter()
            .any(|compiled| compiled.hint.mouse.is_some())
    }

    /// What choosing a match of the hint at `index` does.
    pub fn action(&self, index: usize) -> &HintAction {
        &self.hints[index].hint.action
    }

    /// Label the matches of the hint at `index` in the view, unless it has
    /// none.
    pub fn start<T>(&mut self, term: &Term<T>, index: usize) {
        if index >= self.hints.len() {
            return;
        }

        self.stop();
        self.active = Some(index);
        self.update(term);
    }

    /// Take the labels away.
    pub fn stop(&mut self) {
        self.active = None;
        self.matches.clear();
        self.labels.clear();
        self.keys.clear();
    }

    /// Find the matches in the view again, for what the terminal shows now.
    /// The labels are taken away when there are none.
    pub fn update<T>(&mut self, term: &Term<T>) {
        let Some(index) = self.active else { return };
        let Compiled { hint, regex } = &mut self.hints[index];

        self.matches.clear();
        if hint.hyperlinks {
            self.matches.extend(visible_unique_hyperlinks(term));
        }
        if let Some(regex) = regex.as_mut().and_then(LazyRegex::compiled) {
            let found: Vec<_> = visible_regex_match_iter(term, regex).collect();
            if hint.post_processing {
                for found in found {
                    self.matches.extend(PostProcessor::new(term, regex, found));
                }
            } else {
                self.matches.extend(found);
            }
        }

        if self.matches.is_empty() {
            self.stop();
            return;
        }

        // Matches that start at the same cell are one, the longest.
        self.matches
            .sort_by_key(|bounds| (*bounds.start(), Reverse(*bounds.end())));
        self.matches.dedup_by_key(|bounds| *bounds.start());

        // The labels are given out from the last match up, so the matches
        // at the bottom, where the prompt is, get the shortest.
        let mut labels = Labels::new(&self.settings.alphabet);
        let keys = self.keys.len();
        self.labels = vec![Vec::new(); self.matches.len()];
        for label in self.labels.iter_mut().rev() {
            let mut next = labels.next();
            if next.len() >= keys && next[..keys] == self.keys[..] {
                *label = next.split_off(keys);
            }
        }
    }

    /// Type `c` into the labels; the match whose label it completes is
    /// chosen.
    pub fn input<T>(&mut self, term: &Term<T>, c: char) -> Option<HintMatch> {
        let index = self.active?;
        self.update(term);

        // The last label that goes on with the character.
        let (position, label) = self
            .labels
            .iter()
            .enumerate()
            .rev()
            .find(|(_, label)| label.first() == Some(&c))?;

        if label.len() > 1 {
            self.keys.push(c);
            self.update(term);
            return None;
        }

        let bounds = self.matches[position].clone();
        if self.hints[index].hint.persist {
            self.keys.clear();
            self.update(term);
        } else {
            self.stop();
        }

        // A hyperlink there makes it one, whatever the regex found.
        let hyperlink = term.grid()[*bounds.start()].hyperlink();
        Some(HintMatch {
            hint: index,
            bounds,
            hyperlink,
        })
    }

    /// Take the last character typed into the labels back.
    pub fn backspace<T>(&mut self, term: &Term<T>) {
        self.keys.pop();
        self.update(term);
    }

    /// The labeled matches in the view, and what is left of their labels.
    pub fn labels(&self) -> (&[Match], &[Vec<char>]) {
        (&self.matches, &self.labels)
    }

    /// Remember where the mouse pointer is.
    pub fn set_pointer(&mut self, pointer: Option<(Point<usize>, Modifiers)>) {
        self.pointer = pointer;
    }

    /// The row of the view the mouse pointer is on.
    pub fn pointer_line(&self) -> Option<usize> {
        self.pointer.map(|(point, _)| point.line)
    }

    /// The hint underlined under the mouse pointer: one for the mouse whose
    /// modifiers are held, while no text is selected.
    pub fn under_pointer<T>(&mut self, term: &Term<T>) -> Option<HintMatch> {
        let (point, modifiers) = self.pointer?;
        if term
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.is_empty())
        {
            return None;
        }

        let offset = term.grid().display_offset();
        let line = Line(point.line as i32 - offset as i32);
        let point = Point::new(line, point.column);
        if point.line > term.bottommost_line()
            || point.column > term.last_column()
        {
            return None;
        }

        self.at(term, point, modifiers)
    }

    /// The hint under the vi cursor, of those for the mouse.
    pub fn under_vi_cursor<T>(&mut self, term: &Term<T>) -> Option<HintMatch> {
        if !term.mode().contains(TermMode::VI) {
            return None;
        }

        self.at(term, term.vi_mode_cursor.point, Modifiers::all())
    }

    /// The first hint for the mouse at `point` whose modifiers are among
    /// `modifiers`.
    fn at<T>(
        &mut self,
        term: &Term<T>,
        point: Point,
        modifiers: Modifiers,
    ) -> Option<HintMatch> {
        // The application gets the clicks unless Shift is held, outside vi
        // mode.
        let mode = term.mode();
        let reported = mode.intersects(TermMode::MOUSE_MODE)
            && !mode.contains(TermMode::VI);

        self.hints
            .iter_mut()
            .enumerate()
            .find_map(|(index, compiled)| {
                let Compiled { hint, regex } = compiled;
                let wanted = hint.mouse?;
                if !modifiers.contains(wanted)
                    || (reported && !modifiers.contains(Modifiers::SHIFT))
                {
                    return None;
                }

                if hint.hyperlinks {
                    if let Some((hyperlink, bounds)) = hyperlink_at(term, point)
                    {
                        return Some(HintMatch {
                            hint: index,
                            bounds,
                            hyperlink: Some(hyperlink),
                        });
                    }
                }

                let regex = regex.as_mut()?.compiled()?;
                let bounds =
                    regex_match_at(term, point, regex, hint.post_processing)?;
                Some(HintMatch {
                    hint: index,
                    bounds,
                    hyperlink: None,
                })
            })
    }

    /// The text of a hint found before, if the terminal still has it there.
    pub fn text<T>(
        &mut self,
        term: &Term<T>,
        found: &HintMatch,
    ) -> Option<String> {
        if let Some(hyperlink) = &found.hyperlink {
            let (now, bounds) = hyperlink_at(term, *found.bounds.start())?;
            return (&now == hyperlink && bounds == found.bounds)
                .then(|| hyperlink.uri().to_owned());
        }

        let Compiled { hint, regex } = self.hints.get_mut(found.hint)?;
        let regex = regex.as_mut()?.compiled()?;
        let bounds = regex_match_at(
            term,
            *found.bounds.start(),
            regex,
            hint.post_processing,
        )?;
        (bounds == found.bounds)
            .then(|| term.bounds_to_string(*bounds.start(), *bounds.end()))
    }
}

/// The labels of the matches, shortest first.
struct Labels {
    alphabet: Vec<char>,
    /// Positions in the alphabet of the next label's characters, the last
    /// character first.
    indices: Vec<usize>,
    /// The characters up to here are for the last character of a label, the
    /// others for the characters before it.
    split_point: usize,
}

impl Labels {
    fn new(alphabet: &str) -> Self {
        let alphabet: Vec<char> = alphabet.chars().collect();
        let last = alphabet.len().saturating_sub(1);
        let split_point = (last as f32 * HINT_SPLIT_PERCENTAGE) as usize;

        Self {
            alphabet,
            indices: vec![0],
            split_point,
        }
    }

    fn next(&mut self) -> Vec<char> {
        let label = self
            .indices
            .iter()
            .rev()
            .map(|index| self.alphabet[*index])
            .collect();
        self.increment();
        label
    }

    fn increment(&mut self) {
        let tail = &mut self.indices[0];
        if *tail < self.split_point {
            *tail += 1;
            return;
        }
        *tail = 0;

        let len = self.alphabet.len();
        for index in self.indices.iter_mut().skip(1) {
            if *index + 1 == len {
                *index = self.split_point + 1;
            } else {
                *index += 1;
                return;
            }
        }

        self.indices.push(self.split_point + 1);
    }
}

/// The hyperlinks in the view, each once, where it is first.
fn visible_unique_hyperlinks<T>(
    term: &Term<T>,
) -> impl Iterator<Item = Match> + '_ {
    let mut cells = term.grid().display_iter().peekable();
    let mut seen = HashSet::<Hyperlink>::new();

    iter::from_fn(move || {
        let (start, hyperlink) = cells.find_map(|cell| {
            let hyperlink = cell.hyperlink()?;
            seen.insert(hyperlink.clone())
                .then_some((cell.point, hyperlink))
        })?;

        let mut end = start;
        while let Some(next) = cells.peek() {
            if next.hyperlink().as_ref() != Some(&hyperlink) {
                break;
            }
            end = next.point;
            cells.next();
        }

        Some(start..=end)
    })
}

/// The hyperlink at `point`, with the cells next to it that have it too.
fn hyperlink_at<T>(term: &Term<T>, point: Point) -> Option<(Hyperlink, Match)> {
    let grid = term.grid();
    let hyperlink = grid[point].hyperlink()?;

    let mut end = point;
    for cell in grid.iter_from(point) {
        if cell.hyperlink().as_ref() != Some(&hyperlink) {
            break;
        }
        end = cell.point;
    }

    let mut start = point;
    let mut cells = grid.iter_from(point);
    while let Some(cell) = cells.prev() {
        if cell.hyperlink().as_ref() != Some(&hyperlink) {
            break;
        }
        start = cell.point;
    }

    Some((hyperlink, start..=end))
}

/// The match of `regex` that `point` is in, looking only at its line and the
/// lines it wraps into.
fn regex_match_at<T>(
    term: &Term<T>,
    point: Point,
    regex: &mut RegexSearch,
    post_processing: bool,
) -> Option<Match> {
    let start = term.line_search_left(point);
    let end = term.line_search_right(point);
    let found = RegexIter::new(start, end, Direction::Right, term, regex)
        .find(|found| found.contains(&point))?;

    if post_processing {
        PostProcessor::new(term, regex, found)
            .find(|found| found.contains(&point))
    } else {
        Some(found)
    }
}

/// The matches left in a match after brackets without their opening one and
/// trailing punctuation are cut off it, which is mostly for URLs.
struct PostProcessor<'a, T> {
    regex: &'a mut RegexSearch,
    term: &'a Term<T>,
    next_match: Option<Match>,
    /// Where the next match is looked for.
    start: Point,
    /// The end of the match being processed.
    end: Point,
}

impl<'a, T> PostProcessor<'a, T> {
    fn new(
        term: &'a Term<T>,
        regex: &'a mut RegexSearch,
        found: Match,
    ) -> Self {
        let mut processor = Self {
            next_match: None,
            start: *found.start(),
            end: *found.end(),
            term,
            regex,
        };
        processor.next_processed_match(found);
        processor
    }

    /// `found` with closing brackets without their opening one, and what
    /// follows them, and trailing punctuation cut off; nothing if that
    /// leaves nothing.
    fn process(&self, found: &Match) -> Option<Match> {
        let mut cells = self.term.grid().iter_from(*found.start());
        let mut c = cells.cell().c;

        let end = *found.end();
        let mut open_parens = 0;
        let mut open_brackets = 0;
        loop {
            match c {
                '(' => open_parens += 1,
                '[' => open_brackets += 1,
                ')' if open_parens == 0 => {
                    cells.prev();
                    break;
                },
                ')' => open_parens -= 1,
                ']' if open_brackets == 0 => {
                    cells.prev();
                    break;
                },
                ']' => open_brackets -= 1,
                _ => (),
            }

            if cells.point() == end {
                break;
            }

            match cells.next() {
                Some(cell) => c = cell.cell.c,
                None => break,
            }
        }

        let start = *found.start();
        while cells.point() != start {
            if !matches!(
                c,
                '.' | ',' | ':' | ';' | '?' | '!' | '(' | '[' | '\''
            ) {
                break;
            }

            match cells.prev() {
                Some(cell) => c = cell.cell.c,
                None => break,
            }
        }

        (start <= cells.point()).then(|| start..=cells.point())
    }

    /// Process matches from `found` on until one leaves something.
    fn next_processed_match(&mut self, mut found: Match) {
        self.next_match = loop {
            if let Some(processed) = self.process(&found) {
                self.start = processed.end().add(self.term, Boundary::Grid, 1);
                break Some(processed);
            }

            self.start = found.start().add(self.term, Boundary::Grid, 1);
            if self.start > self.end {
                return;
            }

            match self
                .term
                .regex_search_right(self.regex, self.start, self.end)
            {
                Some(next) => found = next,
                None => return,
            }
        };
    }
}

impl<T> Iterator for PostProcessor<'_, T> {
    type Item = Match;

    fn next(&mut self) -> Option<Self::Item> {
        let next_match = self.next_match.take()?;

        if self.start <= self.end {
            if let Some(found) = self
                .term
                .regex_search_right(self.regex, self.start, self.end)
            {
                self.next_processed_match(found);
            }
        }

        Some(next_match)
    }
}

/// Where the labels of the matches go: over the first cells of each match
/// in the view, and the rest of the match is underlined.
pub(crate) struct LabelTracker<'a> {
    matches: &'a [Match],
    labels: &'a [Vec<char>],
    /// The first match that doesn't end before the last cell asked about.
    index: usize,
    /// The first cell of the view.
    view_start: Point,
    columns: usize,
}

impl<'a> LabelTracker<'a> {
    pub fn new(
        matches: &'a [Match],
        labels: &'a [Vec<char>],
        display_offset: usize,
        columns: usize,
    ) -> Self {
        Self {
            matches,
            labels,
            index: 0,
            view_start: Point::new(Line(-(display_offset as i32)), Column(0)),
            columns,
        }
    }

    /// For `point`, which comes after the points asked about before: `None`
    /// outside the matches, and otherwise the label's character drawn there,
    /// if one is, and whether it is the label's first.
    pub fn at(&mut self, point: Point) -> Option<(Option<char>, bool)> {
        while let Some(bounds) = self.matches.get(self.index) {
            if *bounds.start() > point {
                return None;
            }
            if *bounds.end() >= point {
                break;
            }
            self.index += 1;
        }
        let bounds = self.matches.get(self.index)?;

        // A label of a match that starts above the view starts in the view.
        let start = (*bounds.start()).max(self.view_start);
        let lines = point.line.0 - start.line.0;
        let columns = point.column.0 as i32 - start.column.0 as i32;
        let position =
            usize::try_from(lines * self.columns as i32 + columns).unwrap_or(0);

        Some(match self.labels[self.index].get(position) {
            Some(c) => (Some(*c), position == 0),
            None => (None, false),
        })
    }
}

#[cfg(test)]
mod tests {
    use tankterm_terminal::term::test::mock_term;
    use tankterm_terminal::vte::ansi::{
        Handler, NamedPrivateMode, PrivateMode,
    };

    use super::*;

    fn url_hint(action: HintAction) -> Hint {
        Hint {
            regex: Some("https?://[^ ]+".into()),
            hyperlinks: true,
            post_processing: true,
            persist: false,
            action,
            mouse: Some(Modifiers::empty()),
        }
    }

    fn hints(hints: Vec<Hint>) -> Hints {
        let mut state = Hints::default();
        state.set(HintSettings {
            alphabet: "abcd".into(),
            hints,
        });
        state
    }

    #[test]
    fn labels_are_shortest_first() {
        let mut labels = Labels::new("0123");
        let labels: Vec<String> = (0..8)
            .map(|_| labels.next().into_iter().collect())
            .collect();
        assert_eq!(labels, ["0", "1", "20", "21", "30", "31", "220", "221"]);
    }

    #[test]
    fn a_closing_bracket_alone_leaves_nothing() {
        let term = mock_term(" ) ");
        let mut regex = RegexSearch::new("[^/ ]").unwrap();
        let point = Point::new(Line(0), Column(1));

        assert_eq!(
            PostProcessor::new(&term, &mut regex, point..=point).count(),
            0
        );
    }

    #[test]
    fn hyperlinks_are_found_once() {
        let mut term = mock_term("000\r\n111");
        term.goto(0, 0);
        let foo = Hyperlink::new(Some("1"), String::from("foo"));
        let bar = Hyperlink::new(Some("2"), String::from("bar"));
        for line in 0..2 {
            term.goto(line, 0);
            term.set_hyperlink(Some(foo.clone().into()));
            term.input('b');
            term.input('a');
            term.set_hyperlink(Some(bar.clone().into()));
            term.input('r');
        }
        term.set_hyperlink(None);

        let found: Vec<_> = visible_unique_hyperlinks(&term).collect();
        let at = |column| Point::new(Line(0), Column(column));
        assert_eq!(found, [at(0)..=at(1), at(2)..=at(2)]);
    }

    #[test]
    fn typing_a_label_chooses_its_match() {
        let term = mock_term(
            "see https://one.example. and http://two.example/(x)\r\n\
             ftp://no https://three.example",
        );
        let mut state = hints(vec![url_hint(HintAction::Copy)]);

        state.start(&term, 0);
        assert!(state.active());
        let (matches, labels) = state.labels();
        let texts: Vec<_> = matches
            .iter()
            .map(|found| term.bounds_to_string(*found.start(), *found.end()))
            .collect();
        // The full stop is cut off the first, and the brackets are kept.
        assert_eq!(
            texts,
            [
                "https://one.example",
                "http://two.example/(x)",
                "https://three.example"
            ]
        );
        // The last match gets the first label.
        assert_eq!(labels, [vec!['c', 'a'], vec!['b'], vec!['a']]);

        // A character that starts no label does nothing; one that starts a
        // longer label narrows the labels down to those.
        assert_eq!(state.input(&term, 'x'), None);
        assert_eq!(state.input(&term, 'c'), None);
        assert_eq!(state.labels().1, [vec!['a'], vec![], vec![]]);
        state.backspace(&term);
        assert_eq!(state.labels().1, [vec!['c', 'a'], vec!['b'], vec!['a']]);

        let chosen = state.input(&term, 'b').expect("a match");
        assert_eq!(
            state.text(&term, &chosen).as_deref(),
            Some("http://two.example/(x)")
        );
        assert!(!state.active());
    }

    #[test]
    fn persistent_hints_keep_their_labels() {
        let term = mock_term("https://one.example https://two.example");
        let mut hint = url_hint(HintAction::Copy);
        hint.persist = true;
        let mut state = hints(vec![hint]);

        state.start(&term, 0);
        assert!(state.input(&term, 'a').is_some());
        assert!(state.active());
        assert_eq!(state.labels().1, [vec!['b'], vec!['a']]);
    }

    #[test]
    fn hints_without_matches_take_no_keys() {
        let term = mock_term("nothing to see");
        let mut state = hints(vec![url_hint(HintAction::Copy)]);

        state.start(&term, 0);
        assert!(!state.active());
    }

    #[test]
    fn the_mouse_needs_the_hints_modifiers() {
        let mut term = mock_term("go to https://example.org now");
        let mut hint = url_hint(HintAction::Copy);
        hint.mouse = Some(Modifiers::CTRL);
        let mut state = hints(vec![hint]);
        let on_url = Point::new(0, Column(10));

        state.set_pointer(Some((on_url, Modifiers::empty())));
        assert_eq!(state.under_pointer(&term), None);

        state.set_pointer(Some((on_url, Modifiers::CTRL | Modifiers::ALT)));
        let found = state.under_pointer(&term).expect("the URL");
        assert_eq!(
            state.text(&term, &found).as_deref(),
            Some("https://example.org")
        );

        // While the application takes the clicks, Shift is needed too.
        term.set_private_mode(PrivateMode::Named(
            NamedPrivateMode::ReportMouseClicks,
        ));
        assert_eq!(state.under_pointer(&term), None);
        state.set_pointer(Some((on_url, Modifiers::CTRL | Modifiers::SHIFT)));
        assert!(state.under_pointer(&term).is_some());

        // Off the URL there is none.
        state.set_pointer(Some((Point::new(0, Column(2)), Modifiers::all())));
        assert_eq!(state.under_pointer(&term), None);
    }

    #[test]
    fn labels_go_over_the_start_of_their_match() {
        let matches = [
            Point::new(Line(-1), Column(3))..=Point::new(Line(0), Column(1)),
            Point::new(Line(0), Column(4))..=Point::new(Line(0), Column(6)),
        ];
        let labels = [vec!['a', 'b'], vec!['c']];
        // The view starts at line 0, after the first match started.
        let mut tracker = LabelTracker::new(&matches, &labels, 0, 5);
        let at = |tracker: &mut LabelTracker<'_>, line, column| {
            tracker.at(Point::new(Line(line), Column(column)))
        };

        assert_eq!(at(&mut tracker, 0, 0), Some((Some('a'), true)));
        assert_eq!(at(&mut tracker, 0, 1), Some((Some('b'), false)));
        assert_eq!(at(&mut tracker, 0, 2), None);
        assert_eq!(at(&mut tracker, 0, 4), Some((Some('c'), true)));
        assert_eq!(at(&mut tracker, 0, 5), Some((None, false)));
        assert_eq!(at(&mut tracker, 1, 0), None);
    }
}
