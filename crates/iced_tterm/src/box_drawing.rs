//! Box drawing characters, block elements, sextants and powerline symbols,
//! drawn as shapes that fit their cell instead of taken from the font.
//!
//! Lines drawn this way meet the lines of the cells around them without gaps,
//! and blocks cover their cells exactly, whatever the font's glyphs look like.
//!
//! The shapes are ported from Alacritty's built-in font,
//! `alacritty/src/renderer/text/builtin_font.rs` (Copyright 2020 The Alacritty
//! Project, Apache-2.0), as carried in TankTerm's
//! `tankterm/src/renderer/text/builtin_font.rs` until the OpenGL front end was
//! removed. That one drew glyphs pixel by pixel into bitmaps; here the
//! rectangles it filled are outlined for iced's canvas, on the same whole
//! pixels, and the arcs, diagonals and powerline symbols are outlined as
//! curves and polygons.

use iced::widget::canvas::path::Builder;
use iced::{Point, Size};

/// The powerline symbols: a solid and a hollow arrow pointing right, and the
/// same two pointing left.
const POWERLINE_TRIANGLE_LTR: char = '\u{e0b0}';
const POWERLINE_ARROW_LTR: char = '\u{e0b1}';
const POWERLINE_TRIANGLE_RTL: char = '\u{e0b2}';
const POWERLINE_ARROW_RTL: char = '\u{e0b3}';

/// Where the outlines of a character go: the part of a path builder they
/// need.
pub(crate) trait Pen {
    /// Start a new outline at `point`.
    fn move_to(&mut self, point: Point);

    /// A straight line to `point`.
    fn line_to(&mut self, point: Point);

    /// A straight line towards `corner` that turns at it towards `to` in a
    /// circular arc of `radius`, ending where the arc does.
    fn arc_to(&mut self, corner: Point, to: Point, radius: f32);

    /// Close the outline with a straight line back to where it started.
    fn close(&mut self);
}

impl Pen for Builder {
    fn move_to(&mut self, point: Point) {
        Builder::move_to(self, point);
    }

    fn line_to(&mut self, point: Point) {
        Builder::line_to(self, point);
    }

    fn arc_to(&mut self, corner: Point, to: Point, radius: f32) {
        Builder::arc_to(self, corner, to, radius);
    }

    fn close(&mut self) {
        Builder::close(self);
    }
}

/// Whether [`draw`] draws `c` itself.
#[inline]
pub(crate) fn is_builtin(c: char) -> bool {
    matches!(
        c,
        '\u{2500}'..='\u{259f}'
            | '\u{1fb00}'..='\u{1fb3b}'
            | '\u{1fb82}'..='\u{1fb8b}'
            | POWERLINE_TRIANGLE_LTR..=POWERLINE_ARROW_RTL
    )
}

/// Outline `c` for a cell of size `cell` with its top left corner at
/// `origin`, and return how opaque to fill the outlines with the cell's text
/// color: less than fully for the shades. Nothing is drawn, and `None`
/// returned, for a character that isn't built in.
///
/// The outlines are meant to be filled together with the non-zero rule;
/// where they overlap they don't cancel out. Straight lines and blocks lie on
/// whole pixels of the cell. Everything stays inside the cell but the
/// diagonals, which go on past its top and bottom to meet the diagonals of the
/// cells diagonally next to it.
pub(crate) fn draw(
    c: char,
    cell: Size,
    origin: Point,
    pen: &mut impl Pen,
) -> Option<f32> {
    let mut canvas = Canvas::new(cell, origin, pen);
    let stroke = canvas.stroke_size();
    let heavy = stroke * 2.;

    match c {
        // Horizontal dashes: '┄', '┅', '┈', '┉', '╌', '╍'.
        '\u{2504}' | '\u{2505}' | '\u{2508}' | '\u{2509}' | '\u{254c}'
        | '\u{254d}' => {
            let (gaps, stroke) = match c {
                '\u{2504}' => (2, stroke),
                '\u{2505}' => (2, heavy),
                '\u{2508}' => (3, stroke),
                '\u{2509}' => (3, heavy),
                '\u{254c}' => (1, stroke),
                _ => (1, heavy),
            };
            canvas.h_dashes(gaps, stroke);
        },
        // Vertical dashes: '┆', '┇', '┊', '┋', '╎', '╏'.
        '\u{2506}' | '\u{2507}' | '\u{250a}' | '\u{250b}' | '\u{254e}'
        | '\u{254f}' => {
            let (gaps, stroke) = match c {
                '\u{2506}' => (2, stroke),
                '\u{2507}' => (2, heavy),
                '\u{250a}' => (3, stroke),
                '\u{250b}' => (3, heavy),
                '\u{254e}' => (1, stroke),
                _ => (1, heavy),
            };
            canvas.v_dashes(gaps, stroke);
        },
        // Horizontal lines: '─', '━', '╴', '╶', '╸', '╺'.
        // Vertical lines: '│', '┃', '╵', '╷', '╹', '╻'.
        // Light and heavy line box components:
        // '┌','┍','┎','┏','┐','┑','┒','┓','└','┕','┖','┗','┘','┙','┚','┛',├','┝','┞','┟','┠','┡',
        // '┢','┣','┤','┥','┦','┧','┨','┩','┪','┫','┬','┭','┮','┯','┰','┱','┲','┳','┴','┵','┶','┷',
        // '┸','┹','┺','┻','┼','┽','┾','┿','╀','╁','╂','╃','╄','╅','╆','╇','╈','╉','╊','╋'.
        // Mixed light and heavy lines: '╼', '╽', '╾', '╿'.
        '\u{2500}'..='\u{2503}'
        | '\u{250c}'..='\u{254b}'
        | '\u{2574}'..='\u{257f}' => canvas.lines(c, stroke, heavy),
        // Light and double line box components:
        // '═','║','╒','╓','╔','╕','╖','╗','╘','╙','╚','╛','╜','╝','╞','╟','╠','╡','╢','╣','╤','╥',
        // '╦','╧','╨','╩','╪','╫','╬'.
        '\u{2550}'..='\u{256c}' => canvas.double_lines(c, stroke),
        // Arcs: '╭', '╮', '╯', '╰'.
        '\u{256d}'..='\u{2570}' => canvas.arc(c, stroke),
        // Diagonals: '╱', '╲', '╳'.
        '\u{2571}'..='\u{2573}' => canvas.diagonals(c, stroke),
        // Parts of full block: '▀', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '▔', '▉', '▊', '▋', '▌',
        // '▍', '▎', '▏', '▐', '▕', '🮂', '🮃', '🮄', '🮅', '🮆', '🮇', '🮈', '🮉', '🮊', '🮋'.
        '\u{2580}'..='\u{2587}'
        | '\u{2589}'..='\u{2590}'
        | '\u{2594}'
        | '\u{2595}'
        | '\u{1fb82}'..='\u{1fb8b}' => canvas.block(c),
        // Full block and shades: '█', '░', '▒', '▓'.
        '\u{2588}' | '\u{2591}'..='\u{2593}' => {
            canvas.rect(0., 0., canvas.width, canvas.height);
            return Some(match c {
                '\u{2591}' => 0.25,
                '\u{2592}' => 0.5,
                '\u{2593}' => 0.75,
                _ => 1.,
            });
        },
        // Quadrants: '▖', '▗', '▘', '▙', '▚', '▛', '▜', '▝', '▞', '▟'.
        '\u{2596}'..='\u{259f}' => canvas.quadrant(c),
        // Sextants: '🬀' to '🬻'.
        '\u{1fb00}'..='\u{1fb3b}' => canvas.sextant(c),
        // Powerline symbols: '\u{e0b0}', '\u{e0b1}', '\u{e0b2}', '\u{e0b3}'.
        POWERLINE_TRIANGLE_LTR..=POWERLINE_ARROW_RTL => canvas.powerline(c),
        _ => return None,
    }

    Some(1.)
}

/// The cell a character is drawn in, with its origin at the top left corner
/// and `y` growing downwards.
struct Canvas<'a, P> {
    pen: &'a mut P,
    /// Where the cell's top left corner is.
    origin: Point,
    /// Width of the cell, in whole pixels.
    width: f32,
    /// Height of the cell, in whole pixels.
    height: f32,
}

impl<'a, P: Pen> Canvas<'a, P> {
    fn new(cell: Size, origin: Point, pen: &'a mut P) -> Self {
        Self {
            pen,
            origin,
            width: cell.width.floor().max(1.),
            height: cell.height.floor().max(1.),
        }
    }

    /// Width of light lines: an eighth of the cell's width, like the steps
    /// of the block elements, and at least a pixel. Heavy lines are twice as
    /// wide.
    fn stroke_size(&self) -> f32 {
        (self.width / 8.).round().max(1.)
    }

    fn x_center(&self) -> f32 {
        self.width / 2.
    }

    fn y_center(&self) -> f32 {
        self.height / 2.
    }

    /// The rows a horizontal line around `y` of `stroke` pixels covers, from
    /// the first to past the last.
    fn h_line_bounds(&self, y: f32, stroke: f32) -> (f32, f32) {
        let top = (y - stroke / 2.).trunc().max(0.);
        let bottom = (y + stroke / 2.).trunc().min(self.height);
        (top, bottom)
    }

    /// The columns a vertical line around `x` of `stroke` pixels covers, from
    /// the first to past the last.
    fn v_line_bounds(&self, x: f32, stroke: f32) -> (f32, f32) {
        let left = (x - stroke / 2.).trunc().max(0.);
        let right = (x + stroke / 2.).trunc().min(self.width);
        (left, right)
    }

    /// A horizontal line around `y` from `x` on, `length` long.
    fn h_line(&mut self, x: f32, y: f32, length: f32, stroke: f32) {
        let (top, bottom) = self.h_line_bounds(y, stroke);
        self.rect(x, top, length, bottom - top);
    }

    /// A vertical line around `x` from `y` on, `length` long.
    fn v_line(&mut self, x: f32, y: f32, length: f32, stroke: f32) {
        let (left, right) = self.v_line_bounds(x, stroke);
        self.rect(left, y, right - left, length);
    }

    /// The whole pixels of the cell from (`x`, `y`) on that a rectangle
    /// `width` by `height` covers, cut off at the cell's edges.
    fn rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
        let (left, right) = (x.trunc().max(0.), (x + width).trunc());
        let (top, bottom) = (y.trunc().max(0.), (y + height).trunc());
        let (right, bottom) = (right.min(self.width), bottom.min(self.height));

        if left < right && top < bottom {
            self.polygon(&[
                Point::new(left, top),
                Point::new(right, top),
                Point::new(right, bottom),
                Point::new(left, bottom),
            ]);
        }
    }

    /// Outline a polygon clockwise on the screen, whichever way its points
    /// go, so that where it overlaps other outlines the non-zero rule fills
    /// both.
    fn polygon(&mut self, points: &[Point]) {
        let twice_area: f32 = points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .map(|(a, b)| a.x * b.y - b.x * a.y)
            .sum();

        if twice_area >= 0. {
            self.outline(points.iter().copied());
        } else {
            self.outline(points.iter().rev().copied());
        }
    }

    fn outline(&mut self, mut points: impl Iterator<Item = Point>) {
        let Some(first) = points.next() else { return };
        self.move_to(first);
        for point in points {
            self.line_to(point);
        }
        self.close();
    }

    /// A point of the cell where the pen is.
    fn at(&self, point: Point) -> Point {
        Point::new(self.origin.x + point.x, self.origin.y + point.y)
    }

    fn move_to(&mut self, point: Point) {
        let point = self.at(point);
        self.pen.move_to(point);
    }

    fn line_to(&mut self, point: Point) {
        let point = self.at(point);
        self.pen.line_to(point);
    }

    fn arc_to(&mut self, corner: Point, to: Point, radius: f32) {
        let (corner, to) = (self.at(corner), self.at(to));
        self.pen.arc_to(corner, to, radius);
    }

    fn close(&mut self) {
        self.pen.close();
    }

    /// `gaps + 1` dashes across the middle of the cell.
    fn h_dashes(&mut self, gaps: u32, stroke: f32) {
        let width = self.width as u32;
        let gap = (width / 8).max(1);
        let dash = (width.saturating_sub(gap * gaps) / (gaps + 1)).max(1);
        let y = self.y_center();
        for index in 0..=gaps {
            let x = (index * (dash + gap)).min(width);
            self.h_line(x as f32, y, dash as f32, stroke);
        }
    }

    /// `gaps + 1` dashes down the middle of the cell.
    fn v_dashes(&mut self, gaps: u32, stroke: f32) {
        let height = self.height as u32;
        let gap = (height / 8).max(1);
        let dash = (height.saturating_sub(gap * gaps) / (gaps + 1)).max(1);
        let x = self.x_center();
        for index in 0..=gaps {
            let y = (index * (dash + gap)).min(height);
            self.v_line(x, y as f32, dash as f32, stroke);
        }
    }

    /// Light and heavy lines from the middle of the cell to its edges.
    fn lines(&mut self, c: char, light: f32, heavy: f32) {
        // Left horizontal line.
        let stroke_h1 = match c {
            '\u{2500}' | '\u{2510}' | '\u{2512}' | '\u{2518}' | '\u{251a}'
            | '\u{2524}' | '\u{2526}' | '\u{2527}' | '\u{2528}'
            | '\u{252c}' | '\u{252e}' | '\u{2530}' | '\u{2532}'
            | '\u{2534}' | '\u{2536}' | '\u{2538}' | '\u{253a}'
            | '\u{253c}' | '\u{253e}' | '\u{2540}' | '\u{2541}'
            | '\u{2542}' | '\u{2544}' | '\u{2546}' | '\u{254a}'
            | '\u{2574}' | '\u{257c}' => light,
            '\u{2501}' | '\u{2511}' | '\u{2513}' | '\u{2519}' | '\u{251b}'
            | '\u{2525}' | '\u{2529}' | '\u{252a}' | '\u{252b}'
            | '\u{252d}' | '\u{252f}' | '\u{2531}' | '\u{2533}'
            | '\u{2535}' | '\u{2537}' | '\u{2539}' | '\u{253b}'
            | '\u{253d}' | '\u{253f}' | '\u{2543}' | '\u{2545}'
            | '\u{2547}' | '\u{2548}' | '\u{2549}' | '\u{254b}'
            | '\u{2578}' | '\u{257e}' => heavy,
            _ => 0.,
        };
        // Right horizontal line.
        let stroke_h2 = match c {
            '\u{2500}' | '\u{250c}' | '\u{250e}' | '\u{2514}' | '\u{2516}'
            | '\u{251c}' | '\u{251e}' | '\u{251f}' | '\u{2520}'
            | '\u{252c}' | '\u{252d}' | '\u{2530}' | '\u{2531}'
            | '\u{2534}' | '\u{2535}' | '\u{2538}' | '\u{2539}'
            | '\u{253c}' | '\u{253d}' | '\u{2540}' | '\u{2541}'
            | '\u{2542}' | '\u{2543}' | '\u{2545}' | '\u{2549}'
            | '\u{2576}' | '\u{257e}' => light,
            '\u{2501}' | '\u{250d}' | '\u{250f}' | '\u{2515}' | '\u{2517}'
            | '\u{251d}' | '\u{2521}' | '\u{2522}' | '\u{2523}'
            | '\u{252e}' | '\u{252f}' | '\u{2532}' | '\u{2533}'
            | '\u{2536}' | '\u{2537}' | '\u{253a}' | '\u{253b}'
            | '\u{253e}' | '\u{253f}' | '\u{2544}' | '\u{2546}'
            | '\u{2547}' | '\u{2548}' | '\u{254a}' | '\u{254b}'
            | '\u{257a}' | '\u{257c}' => heavy,
            _ => 0.,
        };
        // Top vertical line.
        let stroke_v1 = match c {
            '\u{2502}' | '\u{2514}' | '\u{2515}' | '\u{2518}' | '\u{2519}'
            | '\u{251c}' | '\u{251d}' | '\u{251f}' | '\u{2522}'
            | '\u{2524}' | '\u{2525}' | '\u{2527}' | '\u{252a}'
            | '\u{2534}' | '\u{2535}' | '\u{2536}' | '\u{2537}'
            | '\u{253c}' | '\u{253d}' | '\u{253e}' | '\u{253f}'
            | '\u{2541}' | '\u{2545}' | '\u{2546}' | '\u{2548}'
            | '\u{2575}' | '\u{257d}' => light,
            '\u{2503}' | '\u{2516}' | '\u{2517}' | '\u{251a}' | '\u{251b}'
            | '\u{251e}' | '\u{2520}' | '\u{2521}' | '\u{2523}'
            | '\u{2526}' | '\u{2528}' | '\u{2529}' | '\u{252b}'
            | '\u{2538}' | '\u{2539}' | '\u{253a}' | '\u{253b}'
            | '\u{2540}' | '\u{2542}' | '\u{2543}' | '\u{2544}'
            | '\u{2547}' | '\u{2549}' | '\u{254a}' | '\u{254b}'
            | '\u{2579}' | '\u{257f}' => heavy,
            _ => 0.,
        };
        // Bottom vertical line.
        let stroke_v2 = match c {
            '\u{2502}' | '\u{250c}' | '\u{250d}' | '\u{2510}' | '\u{2511}'
            | '\u{251c}' | '\u{251d}' | '\u{251e}' | '\u{2521}'
            | '\u{2524}' | '\u{2525}' | '\u{2526}' | '\u{2529}'
            | '\u{252c}' | '\u{252d}' | '\u{252e}' | '\u{252f}'
            | '\u{253c}' | '\u{253d}' | '\u{253e}' | '\u{253f}'
            | '\u{2540}' | '\u{2543}' | '\u{2544}' | '\u{2547}'
            | '\u{2577}' | '\u{257f}' => light,
            '\u{2503}' | '\u{250e}' | '\u{250f}' | '\u{2512}' | '\u{2513}'
            | '\u{251f}' | '\u{2520}' | '\u{2522}' | '\u{2523}'
            | '\u{2527}' | '\u{2528}' | '\u{252a}' | '\u{252b}'
            | '\u{2530}' | '\u{2531}' | '\u{2532}' | '\u{2533}'
            | '\u{2541}' | '\u{2542}' | '\u{2545}' | '\u{2546}'
            | '\u{2548}' | '\u{2549}' | '\u{254a}' | '\u{254b}'
            | '\u{257b}' | '\u{257d}' => heavy,
            _ => 0.,
        };

        let x_v = self.x_center();
        let y_h = self.y_center();

        let v_top = self.v_line_bounds(x_v, stroke_v1);
        let v_bottom = self.v_line_bounds(x_v, stroke_v2);
        let h_left = self.h_line_bounds(y_h, stroke_h1);
        let h_right = self.h_line_bounds(y_h, stroke_h2);

        // The horizontal lines reach across the vertical ones, and the other
        // way round, so the junction is filled.
        let size_h1 = v_top.1.max(v_bottom.1);
        let x_h = v_top.0.min(v_bottom.0);
        let size_h2 = self.width - x_h;

        let size_v1 = h_left.1.max(h_right.1);
        let y_v = h_left.0.min(h_right.0);
        let size_v2 = self.height - y_v;

        self.h_line(0., y_h, size_h1, stroke_h1);
        self.h_line(x_h, y_h, size_h2, stroke_h2);
        self.v_line(x_v, 0., size_v1, stroke_v1);
        self.v_line(x_v, y_v, size_v2, stroke_v2);
    }

    /// Double lines, and light ones meeting double ones.
    fn double_lines(&mut self, c: char, stroke: f32) {
        let width = self.width;
        let height = self.height;
        let x_center = self.x_center();
        let y_center = self.y_center();

        let v_lines = match c {
            '\u{2552}' | '\u{2555}' | '\u{2558}' | '\u{255b}' | '\u{255e}'
            | '\u{2561}' | '\u{2564}' | '\u{2567}' | '\u{256a}' => {
                (x_center, x_center)
            },
            _ => {
                let bounds = self.v_line_bounds(x_center, stroke);
                ((bounds.0 - 1.).max(0.), (bounds.1 + 1.).min(width))
            },
        };
        let h_lines = match c {
            '\u{2553}' | '\u{2556}' | '\u{2559}' | '\u{255c}' | '\u{255f}'
            | '\u{2562}' | '\u{2565}' | '\u{2568}' | '\u{256b}' => {
                (y_center, y_center)
            },
            _ => {
                let bounds = self.h_line_bounds(y_center, stroke);
                ((bounds.0 - 1.).max(0.), (bounds.1 + 1.).min(height))
            },
        };

        // Bounds of each line of the double lines.
        let v_left_bounds = self.v_line_bounds(v_lines.0, stroke);
        let v_right_bounds = self.v_line_bounds(v_lines.1, stroke);
        let h_top_bounds = self.h_line_bounds(h_lines.0, stroke);
        let h_bot_bounds = self.h_line_bounds(h_lines.1, stroke);

        // Left horizontal part.
        let (top_left_size, bot_left_size) = match c {
            '\u{2550}' | '\u{256b}' => (x_center, x_center),
            '\u{2555}'..='\u{2557}' => (v_right_bounds.1, v_left_bounds.1),
            '\u{255b}'..='\u{255d}' => (v_left_bounds.1, v_right_bounds.1),
            '\u{2561}'..='\u{2563}' | '\u{256a}' | '\u{256c}' => {
                (v_left_bounds.1, v_left_bounds.1)
            },
            '\u{2564}'..='\u{2568}' => (x_center, v_left_bounds.1),
            '\u{2569}' => (v_left_bounds.1, x_center),
            _ => (0., 0.),
        };

        // Right horizontal part.
        let (top_right_x, bot_right_x, right_size) = match c {
            '\u{2550}' | '\u{2565}' | '\u{256b}' => (x_center, x_center, width),
            '\u{2552}'..='\u{2554}' | '\u{2568}' => {
                (v_left_bounds.0, v_right_bounds.0, width)
            },
            '\u{2558}'..='\u{255a}' => {
                (v_right_bounds.0, v_left_bounds.0, width)
            },
            '\u{255e}'..='\u{2560}' | '\u{256a}' | '\u{256c}' => {
                (v_right_bounds.0, v_right_bounds.0, width)
            },
            '\u{2564}' | '\u{2566}' => (x_center, v_right_bounds.0, width),
            '\u{2567}' | '\u{2569}' => (v_right_bounds.0, x_center, width),
            _ => (0., 0., 0.),
        };

        // Top vertical part.
        let (left_top_size, right_top_size) = match c {
            '\u{2551}' | '\u{256a}' => (y_center, y_center),
            '\u{2558}'..='\u{255c}' | '\u{2568}' => {
                (h_bot_bounds.1, h_top_bounds.1)
            },
            '\u{255d}' => (h_top_bounds.1, h_bot_bounds.1),
            '\u{255e}'..='\u{2560}' => (y_center, h_top_bounds.1),
            '\u{2561}'..='\u{2563}' => (h_top_bounds.1, y_center),
            '\u{2567}' | '\u{2569}' | '\u{256b}' | '\u{256c}' => {
                (h_top_bounds.1, h_top_bounds.1)
            },
            _ => (0., 0.),
        };

        // Bottom vertical part.
        let (left_bot_y, right_bot_y, bottom_size) = match c {
            '\u{2551}' | '\u{256a}' => (y_center, y_center, height),
            '\u{2552}'..='\u{2554}' => (h_top_bounds.0, h_bot_bounds.0, height),
            '\u{2555}'..='\u{2557}' => (h_bot_bounds.0, h_top_bounds.0, height),
            '\u{255e}'..='\u{2560}' => (y_center, h_bot_bounds.0, height),
            '\u{2561}'..='\u{2563}' => (h_bot_bounds.0, y_center, height),
            '\u{2564}'..='\u{2566}' | '\u{256b}' | '\u{256c}' => {
                (h_bot_bounds.0, h_bot_bounds.0, height)
            },
            _ => (0., 0., 0.),
        };

        // Left horizontal line.
        self.h_line(0., h_lines.0, top_left_size, stroke);
        self.h_line(0., h_lines.1, bot_left_size, stroke);

        // Right horizontal line.
        self.h_line(top_right_x, h_lines.0, right_size, stroke);
        self.h_line(bot_right_x, h_lines.1, right_size, stroke);

        // Top vertical line.
        self.v_line(v_lines.0, 0., left_top_size, stroke);
        self.v_line(v_lines.1, 0., right_top_size, stroke);

        // Bottom vertical line.
        self.v_line(v_lines.0, left_bot_y, bottom_size, stroke);
        self.v_line(v_lines.1, right_bot_y, bottom_size, stroke);
    }

    /// A light line from one edge of the cell to another that turns in a
    /// quarter circle: '╭', '╮', '╯' or '╰'. Where it leaves the cell it lines
    /// up with the straight light lines.
    fn arc(&mut self, c: char, stroke: f32) {
        let (left, right) = self.v_line_bounds(self.x_center(), stroke);
        let (top, bottom) = self.h_line_bounds(self.y_center(), stroke);
        // The middle of the vertical and horizontal lines, and half their
        // width.
        let (x, y) = ((left + right) / 2., (top + bottom) / 2.);
        let (half_v, half_h) = ((right - left) / 2., (bottom - top) / 2.);

        // The edges the vertical and the horizontal line go to.
        let (to_y, to_x) = match c {
            '\u{256d}' => (self.height, self.width),
            '\u{256e}' => (self.height, 0.),
            '\u{256f}' => (0., 0.),
            _ => (0., self.width),
        };
        let toward_x = if to_x > x { 1. } else { -1. };
        let toward_y = if to_y > y { 1. } else { -1. };

        // As round as the cell allows, the same for all four arcs.
        let radius = x.min(self.width - x).min(y).min(self.height - y).max(0.);

        // The outer edge of the line, then back along the inner one.
        let outer = Point::new(x - toward_x * half_v, y - toward_y * half_h);
        let inner = Point::new(x + toward_x * half_v, y + toward_y * half_h);
        self.move_to(Point::new(outer.x, to_y));
        self.arc_to(outer, Point::new(to_x, outer.y), radius + half_v);
        self.line_to(Point::new(to_x, outer.y));
        self.line_to(Point::new(to_x, inner.y));
        self.arc_to(
            inner,
            Point::new(inner.x, to_y),
            (radius - half_v).max(0.),
        );
        self.line_to(Point::new(inner.x, to_y));
        self.close();
    }

    /// '╱', '╲' or both crossed, '╳', from corner to corner of the cell, as
    /// wide as light lines.
    ///
    /// The lines go on past the top and bottom of the cell, cut off at its
    /// sides, so they meet the same lines in the cells diagonally next to it.
    fn diagonals(&mut self, c: char, stroke: f32) {
        let (width, height) = (self.width, self.height);
        // Half the height of the line at every `x`.
        let half = stroke / 2. * (1. + (height / width).powi(2)).sqrt();

        if c != '\u{2572}' {
            self.polygon(&[
                Point::new(0., height - half),
                Point::new(width, -half),
                Point::new(width, half),
                Point::new(0., height + half),
            ]);
        }
        if c != '\u{2571}' {
            self.polygon(&[
                Point::new(0., -half),
                Point::new(width, height - half),
                Point::new(width, height + half),
                Point::new(0., half),
            ]);
        }
    }

    /// Eighths of a block, from the left, right, top or bottom.
    fn block(&mut self, c: char) {
        let (width, height) = (self.width, self.height);
        let rect_width = match c {
            '\u{2589}' | '\u{1fb8b}' => width * 7. / 8.,
            '\u{258a}' | '\u{1fb8a}' => width * 6. / 8.,
            '\u{258b}' | '\u{1fb89}' => width * 5. / 8.,
            '\u{258c}' => width * 4. / 8.,
            '\u{258d}' | '\u{1fb88}' => width * 3. / 8.,
            '\u{258e}' | '\u{1fb87}' => width * 2. / 8.,
            '\u{258f}' => width * 1. / 8.,
            '\u{2590}' => width * 4. / 8.,
            '\u{2595}' => width * 1. / 8.,
            _ => width,
        };

        // The height, and how far from the bottom the top is.
        let (rect_height, y) = match c {
            '\u{2580}' => (height * 4. / 8., height * 8. / 8.),
            '\u{2581}' => (height * 1. / 8., height * 1. / 8.),
            '\u{2582}' => (height * 2. / 8., height * 2. / 8.),
            '\u{2583}' => (height * 3. / 8., height * 3. / 8.),
            '\u{2584}' => (height * 4. / 8., height * 4. / 8.),
            '\u{2585}' => (height * 5. / 8., height * 5. / 8.),
            '\u{2586}' => (height * 6. / 8., height * 6. / 8.),
            '\u{2587}' => (height * 7. / 8., height * 7. / 8.),
            '\u{2594}' => (height * 1. / 8., height * 8. / 8.),
            '\u{1fb82}' => (height * 2. / 8., height * 8. / 8.),
            '\u{1fb83}' => (height * 3. / 8., height * 8. / 8.),
            '\u{1fb84}' => (height * 5. / 8., height * 8. / 8.),
            '\u{1fb85}' => (height * 6. / 8., height * 8. / 8.),
            '\u{1fb86}' => (height * 7. / 8., height * 8. / 8.),
            _ => (height, height),
        };
        let y = (height - y).round();

        // Round the sizes rather than cut them, and keep them visible.
        let rect_width = rect_width.round().max(1.);
        let rect_height = rect_height.round().max(1.);

        let x = match c {
            '\u{2590}' => self.x_center(),
            '\u{2595}' | '\u{1fb87}'..='\u{1fb8b}' => width - rect_width,
            _ => 0.,
        };

        self.rect(x, y, rect_width, rect_height);
    }

    /// Quarters of a block.
    fn quadrant(&mut self, c: char) {
        let x_center = self.x_center().round().max(1.);
        let y_center = self.y_center().round().max(1.);

        let top_left = matches!(c, '\u{2598}' | '\u{2599}'..='\u{259c}');
        let top_right = matches!(c, '\u{259b}'..='\u{259f}');
        let bottom_left = matches!(
            c,
            '\u{2596}' | '\u{2599}' | '\u{259b}' | '\u{259e}' | '\u{259f}'
        );
        let bottom_right = matches!(
            c,
            '\u{2597}' | '\u{2599}' | '\u{259a}' | '\u{259c}' | '\u{259f}'
        );

        for (filled, x, y) in [
            (top_left, 0., 0.),
            (top_right, x_center, 0.),
            (bottom_left, 0., y_center),
            (bottom_right, x_center, y_center),
        ] {
            if filled {
                self.rect(x, y, x_center, y_center);
            }
        }
    }

    /// Sixths of a block, in two columns and three rows.
    fn sextant(&mut self, c: char) {
        let x_center = self.x_center().round().max(1.);
        let y_third = (self.height / 3.).round().max(1.);
        let y_last_third = self.height - 2. * y_third;

        // The sextants count up in binary through which sixths they fill, a
        // bit each from the top left to the bottom right, row by row, leaving
        // out the left and right halves, '▌' and '▐'.
        let mut sixths = c as u32 - 0x1fb00 + 1;
        if sixths >= 0b010101 {
            sixths += 1;
        }
        if sixths >= 0b101010 {
            sixths += 1;
        }

        for sixth in 0..6 {
            if sixths & (1 << sixth) != 0 {
                let x = if sixth % 2 == 0 { 0. } else { x_center };
                let row = sixth / 2;
                let height = if row == 2 { y_last_third } else { y_third };
                self.rect(x, row as f32 * y_third, x_center, height);
            }
        }
    }

    /// A solid triangle or a hollow arrow, as high as the cell and pointing
    /// at the middle of its right or left edge.
    fn powerline(&mut self, c: char) {
        let (width, height) = (self.width, self.height);
        let middle = height / 2.;
        let flip = |point: Point| match c {
            POWERLINE_TRIANGLE_LTR | POWERLINE_ARROW_LTR => point,
            _ => Point::new(width - point.x, point.y),
        };
        let triangle = [
            Point::new(0., 0.),
            Point::new(width, middle),
            Point::new(0., height),
        ];

        // The arrow is as wide as light lines, measured across its sides.
        let slope = middle / width;
        let thickness = self.stroke_size() * (1. + slope * slope).sqrt();
        let inner_tip = width - thickness / slope;

        if matches!(c, POWERLINE_TRIANGLE_LTR | POWERLINE_TRIANGLE_RTL)
            || thickness >= middle
            || inner_tip <= 0.
        {
            self.polygon(&triangle.map(flip));
        } else {
            self.polygon(
                &[
                    triangle[0],
                    triangle[1],
                    triangle[2],
                    Point::new(0., height - thickness),
                    Point::new(inner_tip, middle),
                    Point::new(0., thickness),
                ]
                .map(flip),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::f32::consts::FRAC_PI_2;

    /// What a character's outlines are made of, relative to its cell.
    #[derive(Debug, Default)]
    struct Outlines {
        /// Every outline as the points it goes through, with quarter circles
        /// flattened.
        paths: Vec<Vec<Point>>,
        /// The arcs: their corner, where they turn to and their radius.
        arcs: Vec<(Point, Point, f32)>,
    }

    impl Pen for Outlines {
        fn move_to(&mut self, point: Point) {
            self.paths.push(vec![point]);
        }

        fn line_to(&mut self, point: Point) {
            self.paths.last_mut().unwrap().push(point);
        }

        fn arc_to(&mut self, corner: Point, to: Point, radius: f32) {
            self.arcs.push((corner, to, radius));
            let path = self.paths.last_mut().unwrap();
            let from = *path.last().unwrap();

            // Like iced's path builder, a corner that isn't one is a line.
            if radius == 0. || from == corner || corner == to {
                path.push(corner);
                return;
            }

            // The arcs here turn by right angles: from `start` on the way to
            // the corner to `end` on the way from it, around the corner of
            // the square they make with it.
            let toward = |point: Point| {
                let distance = point.distance(corner);
                Point::new(
                    corner.x + (point.x - corner.x) / distance * radius,
                    corner.y + (point.y - corner.y) / distance * radius,
                )
            };
            let (start, end) = (toward(from), toward(to));
            let center = Point::new(
                start.x + end.x - corner.x,
                start.y + end.y - corner.y,
            );
            for step in 0..=8 {
                let angle = step as f32 / 8. * FRAC_PI_2;
                let (sin, cos) = angle.sin_cos();
                path.push(Point::new(
                    center.x
                        + (start.x - center.x) * cos
                        + (end.x - center.x) * sin,
                    center.y
                        + (start.y - center.y) * cos
                        + (end.y - center.y) * sin,
                ));
            }
        }

        fn close(&mut self) {}
    }

    impl Outlines {
        /// The pixels of a cell of `width` by `height` whose middle the
        /// outlines cover, under the non-zero rule: a `#` for each, a `.`
        /// for the others.
        fn pixels(&self, width: usize, height: usize) -> Vec<String> {
            (0..height)
                .map(|row| {
                    (0..width)
                        .map(|column| {
                            let middle = Point::new(
                                column as f32 + 0.5,
                                row as f32 + 0.5,
                            );
                            if self.winding(middle) != 0 {
                                '#'
                            } else {
                                '.'
                            }
                        })
                        .collect()
                })
                .collect()
        }

        /// How many times the outlines go around `point`, clockwise on the
        /// screen.
        fn winding(&self, point: Point) -> i32 {
            let mut winding = 0;
            for path in &self.paths {
                for (a, b) in path.iter().zip(path.iter().cycle().skip(1)) {
                    let side = (b.x - a.x) * (point.y - a.y)
                        - (point.x - a.x) * (b.y - a.y);
                    if a.y <= point.y && b.y > point.y && side > 0. {
                        winding += 1;
                    } else if a.y > point.y && b.y <= point.y && side < 0. {
                        winding -= 1;
                    }
                }
            }
            winding
        }

        /// The outlines as rectangles, all of them being some.
        fn rectangles(&self) -> Vec<[f32; 4]> {
            self.paths
                .iter()
                .map(|path| {
                    let [a, b, c, d] = path[..] else {
                        panic!("not a rectangle: {path:?}")
                    };
                    assert!(
                        a.y == b.y && b.x == c.x && c.y == d.y && d.x == a.x,
                        "not a rectangle: {path:?}"
                    );
                    [a.x, a.y, c.x - a.x, c.y - a.y]
                })
                .collect()
        }
    }

    fn outlines(c: char, width: f32, height: f32) -> (Option<f32>, Outlines) {
        let mut outlines = Outlines::default();
        let opacity =
            draw(c, Size::new(width, height), Point::ORIGIN, &mut outlines);
        (opacity, outlines)
    }

    fn pixels(c: char, width: usize, height: usize) -> Vec<String> {
        let (opacity, outlines) = outlines(c, width as f32, height as f32);
        assert_eq!(opacity, Some(1.), "{c}");
        outlines.pixels(width, height)
    }

    /// Every character that is built in, in Unicode's order.
    fn builtin() -> impl Iterator<Item = char> {
        ('\u{2500}'..='\u{259f}')
            .chain('\u{1fb00}'..='\u{1fb3b}')
            .chain('\u{1fb82}'..='\u{1fb8b}')
            .chain('\u{e0b0}'..='\u{e0b3}')
    }

    #[test]
    fn box_drawing_blocks_sextants_and_powerline_are_built_in() {
        for c in builtin() {
            assert!(is_builtin(c), "{c}");
            let (opacity, outlines) = outlines(c, 9., 19.);
            assert!(opacity.is_some(), "{c} isn't drawn");
            assert!(!outlines.paths.is_empty(), "{c} has no outlines");
        }

        let neighbours = ('\u{2450}'..'\u{2500}')
            .chain('\u{25a0}'..'\u{2600}')
            .chain('\u{1fb3c}'..'\u{1fb82}')
            .chain('\u{1fb8c}'..'\u{1fc00}')
            .chain('\u{e0a0}'..'\u{e0b0}')
            .chain('\u{e0b4}'..'\u{e0c0}')
            .chain(['a', ' ', '|', '-', '+', '=', '#']);
        for c in neighbours {
            assert!(!is_builtin(c), "{c}");
            let (opacity, outlines) = outlines(c, 9., 19.);
            assert_eq!(opacity, None, "{c}");
            assert!(outlines.paths.is_empty(), "{c}");
        }

        // `is_builtin` says which characters `draw` draws.
        for c in (0..0x20000).filter_map(char::from_u32) {
            let drawn = outlines(c, 8., 16.).0.is_some();
            assert_eq!(is_builtin(c), drawn, "{c:?}");
        }
    }

    #[test]
    fn outlines_stay_in_the_cell_and_go_clockwise() {
        for (width, height) in [
            (8., 16.),
            (9., 19.),
            (7., 15.),
            (20., 10.),
            (2., 3.),
            (1., 1.),
        ] {
            for c in builtin() {
                let (_, outlines) = outlines(c, width, height);
                // Flattening the arcs here rounds a little.
                let inside = |value: f32, end: f32| {
                    (-1e-4..=end + 1e-4).contains(&value)
                };
                for path in &outlines.paths {
                    for point in path {
                        assert!(point.x.is_finite() && point.y.is_finite());
                        assert!(inside(point.x, width), "{c} {point:?}");
                        // Only the diagonals go past the top and bottom.
                        if !('\u{2571}'..='\u{2573}').contains(&c) {
                            assert!(
                                inside(point.y, height),
                                "{c} {point:?} in {width}x{height}"
                            );
                        }
                    }

                    // Straight outlines go clockwise, so overlapping ones
                    // add up under the non-zero rule.
                    if outlines.arcs.is_empty() {
                        let twice_area: f32 = path
                            .iter()
                            .zip(path.iter().cycle().skip(1))
                            .map(|(a, b)| a.x * b.y - b.x * a.y)
                            .sum();
                        assert!(twice_area > 0., "{c} {path:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn outlines_are_drawn_at_the_origin() {
        let mut outlines = Outlines::default();
        let origin = Point::new(80., 32.);
        draw('\u{2588}', Size::new(8., 16.), origin, &mut outlines);

        assert_eq!(outlines.rectangles(), [[80., 32., 8., 16.]]);
    }

    #[test]
    fn a_light_line_crosses_the_middle_of_the_cell() {
        let horizontal = [
            "........", "........", "........", "........", "########",
            "........", "........", "........", "........", "........",
        ];
        assert_eq!(pixels('─', 8, 10), horizontal);

        let vertical = [
            "...#....", "...#....", "...#....", "...#....", "...#....",
            "...#....", "...#....", "...#....", "...#....", "...#....",
        ];
        assert_eq!(pixels('│', 8, 10), vertical);
        // As rectangles: the halves of the line on either side of where a
        // vertical one would cross it, a pixel high up to cells 12 pixels
        // wide.
        assert_eq!(
            outlines('─', 11., 10.).1.rectangles(),
            [[0., 4., 5., 1.], [5., 4., 6., 1.]]
        );
    }

    #[test]
    fn heavy_and_double_lines_meet_in_junctions() {
        // Heavy lines are twice as wide as light ones.
        let heavy_cross = [
            "...##...", "...##...", "...##...", "...##...", "########",
            "########", "...##...", "...##...", "...##...", "...##...",
        ];
        assert_eq!(pixels('╋', 8, 10), heavy_cross);

        // A light line up, heavy ones down and to the right.
        let mixed = [
            "...#....", "...#....", "...#....", "...#....", "...#####",
            "...#####", "...##...", "...##...", "...##...", "...##...",
        ];
        assert_eq!(pixels('┢', 8, 10), mixed);

        let double_cross = [
            ".#..#...", ".#..#...", "##..####", "........", "........",
            "##..####", ".#..#...", ".#..#...", ".#..#...", ".#..#...",
        ];
        assert_eq!(pixels('╬', 8, 10), double_cross);

        let double_corner = [
            "........", "........", ".#######", ".#......", ".#......",
            ".#..####", ".#..#...", ".#..#...", ".#..#...", ".#..#...",
        ];
        assert_eq!(pixels('╔', 8, 10), double_corner);
    }

    #[test]
    fn dashes_leave_gaps() {
        assert_eq!(
            pixels('┄', 8, 4),
            ["........", "##.##.##", "........", "........"]
        );
        assert_eq!(
            pixels('╎', 3, 8),
            [".#.", ".#.", ".#.", "...", ".#.", ".#.", ".#.", "...",]
        );
    }

    #[test]
    fn arcs_turn_where_the_straight_lines_go() {
        let rounded = [
            "........", "........", "........", "........", ".....###",
            "....#...", "...#....", "...#....", "...#....", "...#....",
        ];
        assert_eq!(pixels('╭', 8, 10), rounded);

        // Where the arcs leave the cell, they cover the same pixels as the
        // straight lines.
        let (width, height) = (9, 19);
        let row = |c, y: usize| pixels(c, width, height)[y].clone();
        let column = |c, x: usize| -> String {
            pixels(c, width, height)
                .iter()
                .map(|row| row.as_bytes()[x] as char)
                .collect()
        };
        for arc in ['╭', '╮', '╯', '╰'] {
            let (y, x) = match arc {
                '╭' => (height - 1, width - 1),
                '╮' => (height - 1, 0),
                '╯' => (0, 0),
                _ => (0, width - 1),
            };
            assert_eq!(row(arc, y), row('│', y), "{arc}");
            assert_eq!(column(arc, x), column('─', x), "{arc}");
        }

        // The outer and inner edges of the line turn around the same point,
        // as round as the cell allows and the same for all four arcs.
        for arc in ['╭', '╮', '╯', '╰'] {
            let (_, outlines) = outlines(arc, 8., 16.);
            let radii: Vec<f32> =
                outlines.arcs.iter().map(|&(_, _, radius)| radius).collect();
            assert_eq!(radii, [4., 3.], "{arc}");
        }
    }

    #[test]
    fn diagonals_meet_the_cells_diagonally_next_to_them() {
        let (width, height) = (8., 16.);
        let (_, rising) = outlines('╱', width, height);

        // As wide across as a light line, a pixel.
        let slope = height / width;
        let tall = rising.paths[0][3].y - rising.paths[0][0].y;
        let across = tall / (1. + slope * slope).sqrt();
        assert!((across - 1.).abs() < 1e-5, "{across}");
        let half = 1. / 2. * (1. + slope * slope).sqrt();

        // From the bottom left corner to the top right one and on past
        // them, so the line overlaps the same one in the cell below on the
        // left, which ends where this one starts.
        assert_eq!(
            rising.paths,
            [[
                Point::new(0., height - half),
                Point::new(width, -half),
                Point::new(width, half),
                Point::new(0., height + half),
            ]]
        );

        let (_, falling) = outlines('╲', width, height);
        assert_eq!(
            falling.paths,
            [[
                Point::new(0., -half),
                Point::new(width, height - half),
                Point::new(width, height + half),
                Point::new(0., half),
            ]]
        );

        // The cross is both lines.
        let (_, cross) = outlines('╳', width, height);
        assert_eq!(
            cross.paths,
            [rising.paths[0].clone(), falling.paths[0].clone()]
        );
    }

    #[test]
    fn block_eighths_fill_whole_rows_and_columns() {
        // A cell 16 pixels high has rows of two pixels to the eighth.
        assert_eq!(outlines('▁', 8., 16.).1.rectangles(), [[0., 14., 8., 2.]]);
        assert_eq!(outlines('▆', 8., 16.).1.rectangles(), [[0., 4., 8., 12.]]);
        assert_eq!(outlines('▔', 8., 16.).1.rectangles(), [[0., 0., 8., 2.]]);
        assert_eq!(outlines('🮄', 8., 16.).1.rectangles(), [[0., 0., 8., 10.]]);
        assert_eq!(outlines('▏', 8., 16.).1.rectangles(), [[0., 0., 1., 16.]]);
        assert_eq!(outlines('▊', 8., 16.).1.rectangles(), [[0., 0., 6., 16.]]);
        assert_eq!(outlines('▕', 8., 16.).1.rectangles(), [[7., 0., 1., 16.]]);
        assert_eq!(outlines('🮋', 8., 16.).1.rectangles(), [[1., 0., 7., 16.]]);

        // Halves meet without overlapping.
        assert_eq!(outlines('▀', 8., 17.).1.rectangles(), [[0., 0., 8., 9.]]);
        assert_eq!(outlines('▄', 8., 17.).1.rectangles(), [[0., 9., 8., 8.]]);

        assert_eq!(pixels('▚', 4, 4), ["##..", "##..", "..##", "..##"]);
    }

    #[test]
    fn shades_fill_the_cell_partly_opaque() {
        for (c, opacity) in [('░', 0.25), ('▒', 0.5), ('▓', 0.75), ('█', 1.)]
        {
            let (drawn, outlines) = outlines(c, 8., 16.);
            assert_eq!(drawn, Some(opacity), "{c}");
            assert_eq!(outlines.rectangles(), [[0., 0., 8., 16.]], "{c}");
        }
    }

    #[test]
    fn sextants_fill_sixths_of_the_cell() {
        // The first fills the top left sixth, the last all but that one.
        assert_eq!(outlines('🬀', 8., 15.).1.rectangles(), [[0., 0., 4., 5.]]);
        assert_eq!(
            pixels('🬻', 4, 6),
            ["..##", "..##", "####", "####", "####", "####",]
        );
        // On either side of the left half, '▌', which the range leaves out.
        assert_eq!(
            pixels('🬓', 4, 6),
            ["....", "....", "##..", "##..", "##..", "##..",]
        );
        assert_eq!(
            pixels('🬔', 4, 6),
            ["..##", "..##", "##..", "##..", "##..", "##..",]
        );
        // And of the right half, '▐'.
        assert_eq!(
            pixels('🬧', 4, 6),
            ["##..", "##..", "..##", "..##", "..##", "..##",]
        );
        assert_eq!(
            pixels('🬨', 4, 6),
            ["####", "####", "..##", "..##", "..##", "..##",]
        );
        // The rows are thirds of the height, the last one taking what is
        // left.
        assert_eq!(
            outlines('🬭', 8., 16.).1.rectangles(),
            [[0., 10., 4., 6.], [4., 10., 4., 6.]]
        );
    }

    #[test]
    fn powerline_triangles_point_at_the_middle_of_the_edge() {
        let (_, right) = outlines('\u{e0b0}', 8., 16.);
        assert_eq!(
            right.paths,
            [[Point::new(0., 0.), Point::new(8., 8.), Point::new(0., 16.)]]
        );
        let (_, left) = outlines('\u{e0b2}', 8., 16.);
        assert_eq!(
            left.paths,
            [[Point::new(8., 16.), Point::new(0., 8.), Point::new(8., 0.)]]
        );

        // The arrows are hollow, as wide across as light lines.
        assert_eq!(
            pixels('\u{e0b1}', 8, 8),
            [
                "#.......", ".##.....", "...##...", ".....##.", ".....##.",
                "...##...", ".##.....", "#.......",
            ]
        );
        assert_eq!(
            pixels('\u{e0b3}', 8, 8),
            [
                ".......#", ".....##.", "...##...", ".##.....", ".##.....",
                "...##...", ".....##.", ".......#",
            ]
        );
    }
}
