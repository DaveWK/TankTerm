//! How the cursor is drawn: its shape, whether it has focus and its blinking.

use crate::settings::{CursorBlinking, CursorSettings};
use iced::{Point, Rectangle, Size};
use std::time::Instant;
use tankterm_terminal::vte::ansi::{CursorShape, CursorStyle};

/// The cursor of a terminal, as far as the terminal itself doesn't track it.
#[derive(Debug, Clone)]
pub(crate) struct Cursor {
    pub(crate) settings: CursorSettings,
    /// Whether the terminal has the keyboard focus.
    focused: bool,
    /// Whether the cursor is in the dark half of a blink.
    hidden: bool,
    /// Since when the cursor has been blinking, for the blink timeout.
    blinking_since: Instant,
    /// When something was last typed.
    last_input: Option<Instant>,
    /// Whether the cursor blinked for so long that it stopped.
    timed_out: bool,
}

impl Cursor {
    pub(crate) fn new(settings: CursorSettings, now: Instant) -> Self {
        Self {
            settings,
            focused: true,
            hidden: false,
            blinking_since: now,
            last_input: None,
            timed_out: false,
        }
    }

    /// The style the cursor has until the application in the terminal asks
    /// for another one.
    pub(crate) fn default_style(&self) -> CursorStyle {
        CursorStyle {
            shape: self.settings.shape,
            blinking: matches!(
                self.settings.blinking,
                CursorBlinking::On | CursorBlinking::Always
            ),
        }
    }

    /// Whether the cursor blinks, given whether the terminal's cursor style
    /// asks for it: only with the keyboard focus, and until the timeout.
    pub(crate) fn blinks(&self, style: CursorStyle) -> bool {
        let blinking = match self.settings.blinking {
            CursorBlinking::Never => false,
            CursorBlinking::Always => true,
            CursorBlinking::Off | CursorBlinking::On => style.blinking,
        };

        blinking && self.focused && !self.timed_out
    }

    /// The shape to draw the cursor with, for the terminal's cursor style
    /// and whether the application in the terminal shows the cursor.
    pub(crate) fn shape(&self, style: CursorStyle, shown: bool) -> CursorShape {
        if !shown || (self.hidden && self.blinks(style)) {
            CursorShape::Hidden
        } else if !self.focused
            && self.settings.unfocused_hollow
            && style.shape != CursorShape::Hidden
        {
            CursorShape::HollowBlock
        } else {
            style.shape
        }
    }

    /// Start blinking from a lit cursor, with the whole timeout ahead.
    pub(crate) fn restart(&mut self, now: Instant) {
        self.hidden = false;
        self.timed_out = false;
        self.blinking_since = now;
    }

    /// Whether the terminal has the keyboard focus.
    pub(crate) fn set_focused(&mut self, focused: bool, now: Instant) {
        if self.focused != focused {
            self.focused = focused;
            self.restart(now);
        }
    }

    /// Something was typed: the cursor is lit, stays lit for a blink, and
    /// blinks again if it had stopped.
    pub(crate) fn typed(&mut self, now: Instant) {
        self.hidden = false;
        self.last_input = Some(now);
        if self.timed_out {
            self.restart(now);
        }
    }

    /// The blink interval passed: turn the cursor on or off, or stop
    /// blinking once the timeout is reached.
    pub(crate) fn blink(&mut self, style: CursorStyle, now: Instant) {
        if !self.blinks(style) {
            self.hidden = false;
            return;
        }

        let blinked_for = now.saturating_duration_since(self.blinking_since);
        if self
            .settings
            .blink_timeout
            .is_some_and(|timeout| blinked_for >= timeout)
        {
            self.timed_out = true;
            self.hidden = false;
            return;
        }

        let typing = self.last_input.is_some_and(|typed| {
            now.saturating_duration_since(typed) < self.settings.blink_interval
        });
        self.hidden = !typing && !self.hidden;
    }
}

/// The rectangles a cursor of `shape` fills in the cell at `origin`.
///
/// `thickness` is the width of the beam, the height of the underline and the
/// width of the hollow block's outline, relative to the width of the cell;
/// they are at least a pixel and at most the cell.
pub(crate) fn cursor_rectangles(
    shape: CursorShape,
    origin: Point,
    cell: Size,
    thickness: f32,
) -> Vec<Rectangle> {
    let line = (thickness * cell.width).round().max(1.0);
    let (across, down) = (line.min(cell.width), line.min(cell.height));
    let bottom = Point::new(origin.x, origin.y + cell.height - down);
    let right = Point::new(origin.x + cell.width - across, origin.y);

    match shape {
        CursorShape::Block => vec![Rectangle::new(origin, cell)],
        CursorShape::Underline => {
            vec![Rectangle::new(bottom, Size::new(cell.width, down))]
        },
        CursorShape::Beam => {
            vec![Rectangle::new(origin, Size::new(across, cell.height))]
        },
        CursorShape::HollowBlock => vec![
            Rectangle::new(origin, Size::new(cell.width, down)),
            Rectangle::new(bottom, Size::new(cell.width, down)),
            Rectangle::new(origin, Size::new(across, cell.height)),
            Rectangle::new(right, Size::new(across, cell.height)),
        ],
        CursorShape::Hidden => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const BLOCK: CursorStyle = CursorStyle {
        shape: CursorShape::Block,
        blinking: false,
    };
    const BLINKING_BEAM: CursorStyle = CursorStyle {
        shape: CursorShape::Beam,
        blinking: true,
    };

    fn cursor(blinking: CursorBlinking) -> (Cursor, Instant) {
        let now = Instant::now();
        let settings = CursorSettings {
            blinking,
            blink_interval: Duration::from_millis(500),
            blink_timeout: Some(Duration::from_secs(5)),
            ..CursorSettings::default()
        };
        (Cursor::new(settings, now), now)
    }

    fn ms(millis: u64) -> Duration {
        Duration::from_millis(millis)
    }

    #[test]
    fn default_style_follows_the_settings() {
        let style = |shape, blinking| {
            let settings = CursorSettings {
                shape,
                blinking,
                ..CursorSettings::default()
            };
            Cursor::new(settings, Instant::now()).default_style()
        };

        assert_eq!(CursorSettings::default().shape, CursorShape::Block);
        assert_eq!(style(CursorShape::Block, CursorBlinking::Off), BLOCK);
        assert_eq!(style(CursorShape::Block, CursorBlinking::Never), BLOCK);
        assert_eq!(style(CursorShape::Beam, CursorBlinking::On), BLINKING_BEAM);
        assert_eq!(
            style(CursorShape::Beam, CursorBlinking::Always),
            BLINKING_BEAM
        );
    }

    #[test]
    fn blinking_follows_the_application_unless_forced() {
        let blinks = |blinking, style| cursor(blinking).0.blinks(style);

        assert!(!blinks(CursorBlinking::Off, BLOCK));
        assert!(blinks(CursorBlinking::Off, BLINKING_BEAM));
        assert!(!blinks(CursorBlinking::On, BLOCK));
        assert!(blinks(CursorBlinking::On, BLINKING_BEAM));
        // Neither of these leaves it to the application.
        assert!(!blinks(CursorBlinking::Never, BLINKING_BEAM));
        assert!(blinks(CursorBlinking::Always, BLOCK));
    }

    #[test]
    fn cursor_turns_off_and_on_with_every_interval() {
        let (mut cursor, start) = cursor(CursorBlinking::Off);
        let shape = |cursor: &Cursor| cursor.shape(BLINKING_BEAM, true);
        assert_eq!(shape(&cursor), CursorShape::Beam);

        cursor.blink(BLINKING_BEAM, start + ms(500));
        assert_eq!(shape(&cursor), CursorShape::Hidden);
        cursor.blink(BLINKING_BEAM, start + ms(1000));
        assert_eq!(shape(&cursor), CursorShape::Beam);

        // A cursor that doesn't blink stays lit.
        cursor.blink(BLOCK, start + ms(1500));
        cursor.blink(BLOCK, start + ms(2000));
        assert_eq!(cursor.shape(BLOCK, true), CursorShape::Block);

        // A cursor the application hides isn't drawn in either half.
        assert_eq!(cursor.shape(BLINKING_BEAM, false), CursorShape::Hidden);
    }

    #[test]
    fn typing_keeps_the_cursor_lit() {
        let (mut cursor, start) = cursor(CursorBlinking::Always);
        cursor.blink(BLOCK, start + ms(500));
        assert_eq!(cursor.shape(BLOCK, true), CursorShape::Hidden);

        cursor.typed(start + ms(600));
        assert_eq!(cursor.shape(BLOCK, true), CursorShape::Block);
        // Less than an interval after the key the cursor doesn't turn off.
        cursor.blink(BLOCK, start + ms(1000));
        assert_eq!(cursor.shape(BLOCK, true), CursorShape::Block);
        cursor.blink(BLOCK, start + ms(1500));
        assert_eq!(cursor.shape(BLOCK, true), CursorShape::Hidden);
    }

    #[test]
    fn blinking_stops_after_the_timeout_until_something_is_typed() {
        let (mut cursor, start) = cursor(CursorBlinking::Always);

        cursor.blink(BLOCK, start + ms(4500));
        assert!(cursor.blinks(BLOCK));
        assert_eq!(cursor.shape(BLOCK, true), CursorShape::Hidden);

        // The cursor stays lit once the timeout is over.
        cursor.blink(BLOCK, start + ms(5000));
        assert!(!cursor.blinks(BLOCK));
        assert_eq!(cursor.shape(BLOCK, true), CursorShape::Block);

        // Typing starts the blinking and its timeout over.
        cursor.typed(start + ms(9000));
        assert!(cursor.blinks(BLOCK));
        cursor.blink(BLOCK, start + ms(13_900));
        assert!(cursor.blinks(BLOCK));
        cursor.blink(BLOCK, start + ms(14_000));
        assert!(!cursor.blinks(BLOCK));
    }

    #[test]
    fn blinking_never_stops_without_a_timeout() {
        let (mut cursor, start) = cursor(CursorBlinking::Always);
        cursor.settings.blink_timeout = None;

        cursor.blink(BLOCK, start + Duration::from_secs(3600));
        assert!(cursor.blinks(BLOCK));
        assert_eq!(cursor.shape(BLOCK, true), CursorShape::Hidden);
    }

    #[test]
    fn unfocused_cursor_is_hollow_and_does_not_blink() {
        let (mut cursor, start) = cursor(CursorBlinking::Always);
        cursor.blink(BLINKING_BEAM, start + ms(500));
        assert_eq!(cursor.shape(BLINKING_BEAM, true), CursorShape::Hidden);

        // Losing the focus in the dark half of a blink still shows it.
        cursor.set_focused(false, start + ms(600));
        assert!(!cursor.blinks(BLINKING_BEAM));
        assert_eq!(cursor.shape(BLINKING_BEAM, true), CursorShape::HollowBlock);
        cursor.blink(BLINKING_BEAM, start + ms(1000));
        assert_eq!(cursor.shape(BLINKING_BEAM, true), CursorShape::HollowBlock);
        assert_eq!(cursor.shape(BLINKING_BEAM, false), CursorShape::Hidden);

        // Without the option the cursor keeps its shape.
        cursor.settings.unfocused_hollow = false;
        assert_eq!(cursor.shape(BLINKING_BEAM, true), CursorShape::Beam);

        // Focus starts the blinking over, with a lit cursor.
        cursor.set_focused(true, start + ms(10_000));
        assert!(cursor.blinks(BLINKING_BEAM));
        assert_eq!(cursor.shape(BLINKING_BEAM, true), CursorShape::Beam);
    }

    #[test]
    fn cursor_shapes_fill_their_part_of_the_cell() {
        let origin = Point::new(20.0, 40.0);
        let cell = Size::new(10.0, 20.0);
        let rectangles = |shape, thickness| {
            cursor_rectangles(shape, origin, cell, thickness)
        };
        let rectangle = |x, y, width, height| {
            Rectangle::new(Point::new(x, y), Size::new(width, height))
        };

        assert_eq!(
            rectangles(CursorShape::Block, 0.15),
            [rectangle(20.0, 40.0, 10.0, 20.0)]
        );
        // 0.15 of a cell 10 pixels wide rounds to 2 pixels.
        assert_eq!(
            rectangles(CursorShape::Underline, 0.15),
            [rectangle(20.0, 58.0, 10.0, 2.0)]
        );
        assert_eq!(
            rectangles(CursorShape::Beam, 0.15),
            [rectangle(20.0, 40.0, 2.0, 20.0)]
        );
        assert_eq!(
            rectangles(CursorShape::HollowBlock, 0.15),
            [
                rectangle(20.0, 40.0, 10.0, 2.0),
                rectangle(20.0, 58.0, 10.0, 2.0),
                rectangle(20.0, 40.0, 2.0, 20.0),
                rectangle(28.0, 40.0, 2.0, 20.0),
            ]
        );
        assert!(rectangles(CursorShape::Hidden, 0.15).is_empty());

        // The thinnest cursor is a pixel, the thickest the cell.
        assert_eq!(
            rectangles(CursorShape::Beam, 0.0),
            [rectangle(20.0, 40.0, 1.0, 20.0)]
        );
        assert_eq!(
            rectangles(CursorShape::Beam, 1.0),
            [rectangle(20.0, 40.0, 10.0, 20.0)]
        );
        assert_eq!(
            rectangles(CursorShape::Underline, 1.0),
            [rectangle(20.0, 50.0, 10.0, 10.0)]
        );
        assert_eq!(
            rectangles(CursorShape::Underline, 5.0),
            [rectangle(20.0, 40.0, 10.0, 20.0)]
        );
    }
}
