//! Borders of the parts of the tab bar, drawn like Lip Gloss draws them: on any of the four
//! sides, each side in a color of its own, in every one of Lip Gloss's kinds.
//!
//! iced's `Border` has one width and one color for all four sides, so [`Bordered`] wraps a part
//! and draws its border over the edge of the part's padding, where it takes no space.
//!
//! Lip Gloss draws borders with box drawing characters, a cell thick. In pixels, a `Normal`
//! border is a 1 pixel line, `Rounded` the same with rounded corners and `Thick` a 2 pixel
//! line, as before. `Double` is two 1 pixel lines with a pixel between them, like `═`. `Block`
//! is a solid 4 pixel band, the heaviest of them, like `█`. `Ascii` is a dashed 1 pixel line:
//! in a terminal, rows of `-` and `|` show gaps between the characters, joined by `+` corners
//! where the dashes meet.

use iced_core::border::Radius;
use iced_core::layout::{Layout, Limits, Node};
use iced_core::renderer::Quad;
use iced_core::widget::{Operation, Tree, tree};
use iced_core::{
    Border as Stroke, Clipboard, Color, Element, Event, Length, Rectangle, Shell, Size, Vector,
    Widget, mouse, overlay, renderer,
};

use crate::color::Rgb;
use crate::config::style::{Border, Sides, TabStyle};

/// Radius of the corners of a `Rounded` border.
const RADIUS: f32 = 6.;

/// Width of an `Ascii` border's dashes, and of the gaps between them, in logical pixels.
const DASH: f32 = 4.;
const GAP: f32 = 2.;

/// How a border is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxBorder {
    kind: Border,
    sides: Sides<bool>,
    colors: Sides<Color>,
}

impl BoxBorder {
    pub fn new(style: &TabStyle) -> Self {
        let colors = style.border_colors();
        Self {
            kind: style.border,
            sides: style.border_sides(),
            colors: Sides {
                top: color(colors.top),
                right: color(colors.right),
                bottom: color(colors.bottom),
                left: color(colors.left),
            },
        }
    }

    /// The corners of the box: rounded where a `Rounded` border draws both sides that meet
    /// there. A background is drawn with the same corners.
    pub fn radius(&self) -> Radius {
        let sides = self.sides;
        let corner =
            |a: bool, b: bool| if self.kind == Border::Rounded && a && b { RADIUS } else { 0. };
        Radius {
            top_left: corner(sides.top, sides.left),
            top_right: corner(sides.top, sides.right),
            bottom_right: corner(sides.bottom, sides.right),
            bottom_left: corner(sides.bottom, sides.left),
        }
    }

    fn drawn(&self) -> bool {
        let sides = self.sides;
        self.kind != Border::Hidden && (sides.top || sides.right || sides.bottom || sides.left)
    }

    /// Draw the border inside `bounds`.
    fn draw<Renderer: renderer::Renderer>(&self, renderer: &mut Renderer, bounds: Rectangle) {
        if !self.drawn() {
            return;
        }

        match self.kind {
            Border::Hidden => (),
            Border::Normal | Border::Rounded => self.lines(renderer, bounds, &[(0., 1.)]),
            Border::Thick => self.lines(renderer, bounds, &[(0., 2.)]),
            Border::Block => self.lines(renderer, bounds, &[(0., 4.)]),
            Border::Double => self.lines(renderer, bounds, &[(0., 1.), (2., 1.)]),
            Border::Ascii => self.dashes(renderer, bounds),
        }
    }

    /// Draw a line along every side for each `(inset, width)`, as quads with iced's border.
    ///
    /// A side that isn't drawn is moved out of `bounds` and clipped. Sides of different colors
    /// are drawn one region at a time: the top and bottom lines with the corners, which are
    /// theirs as in Lip Gloss, and the left and right lines between them.
    fn lines<Renderer: renderer::Renderer>(
        &self,
        renderer: &mut Renderer,
        bounds: Rectangle,
        lines: &[(f32, f32)],
    ) {
        let (sides, colors) = (self.sides, self.colors);
        let radius = self.radius();
        let (inset, width) = lines.last().copied().unwrap_or_default();
        let thickness = inset + width;

        let draw = |renderer: &mut Renderer, color: Color| {
            for &(inset, width) in lines {
                // Insets apply to the sides that are drawn; the others go out of sight.
                let away = thickness + 1.;
                let side = |drawn: bool| if drawn { inset } else { -away };
                let quad = Rectangle {
                    x: bounds.x + side(sides.left),
                    y: bounds.y + side(sides.top),
                    width: bounds.width - side(sides.left) - side(sides.right),
                    height: bounds.height - side(sides.top) - side(sides.bottom),
                };
                let stroke = Stroke { color, width, radius };
                renderer.fill_quad(
                    Quad { bounds: quad, border: stroke, ..Quad::default() },
                    Color::TRANSPARENT,
                );
            }
        };

        let all = sides.top && sides.right && sides.bottom && sides.left;
        let drawn: Vec<Color> = [
            (sides.top, colors.top),
            (sides.right, colors.right),
            (sides.bottom, colors.bottom),
            (sides.left, colors.left),
        ]
        .into_iter()
        .filter_map(|(drawn, color)| drawn.then_some(color))
        .collect();
        let one_color = drawn.windows(2).all(|pair| pair[0] == pair[1]);

        if one_color {
            if all {
                draw(renderer, drawn[0]);
            } else {
                renderer.with_layer(bounds, |renderer| draw(renderer, drawn[0]));
            }
            return;
        }

        // The top and bottom regions are as high as their lines or rounded corners.
        let band = |drawn: bool, corners: f32| match drawn {
            true => thickness.max(corners).min(bounds.height / 2.),
            false => 0.,
        };
        let top = band(sides.top, radius.top_left.max(radius.top_right));
        let bottom = band(sides.bottom, radius.bottom_left.max(radius.bottom_right));
        let middle = bounds.height - top - bottom;
        let half = (bounds.width / 2.).floor();

        let regions = [
            (sides.top, colors.top, Rectangle { height: top, ..bounds }),
            (sides.bottom, colors.bottom, Rectangle {
                y: bounds.y + bounds.height - bottom,
                height: bottom,
                ..bounds
            }),
            (sides.left, colors.left, Rectangle {
                y: bounds.y + top,
                width: half,
                height: middle,
                ..bounds
            }),
            (sides.right, colors.right, Rectangle {
                x: bounds.x + half,
                y: bounds.y + top,
                width: bounds.width - half,
                height: middle,
            }),
        ];
        for (drawn, color, region) in regions {
            if drawn && region.width > 0. && region.height > 0. {
                renderer.with_layer(region, |renderer| draw(renderer, color));
            }
        }
    }

    /// Draw every side as 1 pixel dashes, with a dash at both ends so the corners meet.
    fn dashes<Renderer: renderer::Renderer>(&self, renderer: &mut Renderer, bounds: Rectangle) {
        let (sides, colors) = (self.sides, self.colors);
        let (x, y) = (bounds.x.round(), bounds.y.round());
        let (width, height) = (bounds.width.round(), bounds.height.round());

        let mut fill = |rectangle: Rectangle, color: Color| {
            renderer.fill_quad(Quad { bounds: rectangle, ..Quad::default() }, color);
        };

        // The top and bottom lines run the whole width, the others between them.
        if sides.top {
            for (start, length) in dashes(width) {
                fill(Rectangle::new((x + start, y).into(), Size::new(length, 1.)), colors.top);
            }
        }
        if sides.bottom {
            for (start, length) in dashes(width) {
                let at = (x + start, y + height - 1.).into();
                fill(Rectangle::new(at, Size::new(length, 1.)), colors.bottom);
            }
        }
        let top = if sides.top { 1. } else { 0. };
        let bottom = if sides.bottom { 1. } else { 0. };
        for (drawn, left, color) in
            [(sides.left, x, colors.left), (sides.right, x + width - 1., colors.right)]
        {
            if !drawn {
                continue;
            }
            for (start, length) in dashes(height - top - bottom) {
                let at = (left, y + top + start).into();
                fill(Rectangle::new(at, Size::new(1., length)), color);
            }
        }
    }
}

/// Where the dashes of a dashed line `length` long start, and how long they are: whole pixels,
/// with a dash at both ends and the gaps between them as even as they can be.
fn dashes(length: f32) -> Vec<(f32, f32)> {
    if length <= DASH {
        return if length > 0. { vec![(0., length)] } else { Vec::new() };
    }

    let count = ((length + GAP) / (DASH + GAP)).round().max(2.);
    let gaps = length - count * DASH;
    (0..count as u32)
        .map(|index| {
            let index = index as f32;
            ((index * DASH + (gaps * index / (count - 1.)).round()), DASH)
        })
        .collect()
}

fn color(rgb: Rgb) -> Color {
    Color::from_rgb8(rgb.r, rgb.g, rgb.b)
}

/// `content` with a border drawn over its edge.
pub struct Bordered<'a, Message, Theme, Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    border: BoxBorder,
}

/// `content` with the border of `style` drawn over its edge.
pub fn bordered<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    style: &TabStyle,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    Element::new(Bordered { content: content.into(), border: BoxBorder::new(style) })
}

/// Everything but drawing goes to the content, which also keeps its own place in the widget
/// tree.
impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Bordered<'_, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content.as_widget_mut().operate(tree, layout, renderer, operation);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content
            .as_widget_mut()
            .update(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(tree, renderer, theme, style, layout, cursor, viewport);
        self.border.draw(renderer, layout.bounds());
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(tree, layout, renderer, viewport, translation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::style::TextAttributes;
    use crate::config::window::Spacing;

    fn style(border: Border, sides: Sides<Option<bool>>) -> TabStyle {
        TabStyle {
            foreground: Rgb::new(0xff, 0xff, 0xff),
            background: Rgb::new(0, 0, 0),
            attributes: TextAttributes::default(),
            padding: Spacing::default(),
            margin: Spacing::default(),
            border,
            border_sides: sides,
            border_foreground: Rgb::new(0xff, 0, 0),
            border_side_foreground: Sides::default(),
            width: None,
            max_width: None,
            gap: 1,
            fill: Default::default(),
        }
    }

    #[test]
    fn rounded_corners_need_both_sides() {
        let all = BoxBorder::new(&style(Border::Rounded, Sides::default()));
        assert_eq!(all.radius(), Radius::new(RADIUS));

        let bottom = Sides { bottom: Some(true), ..Sides::default() };
        assert_eq!(BoxBorder::new(&style(Border::Rounded, bottom)).radius(), Radius::new(0.));

        let corner = Sides { top: Some(true), left: Some(true), ..Sides::default() };
        let radius = BoxBorder::new(&style(Border::Rounded, corner)).radius();
        assert_eq!(radius, Radius { top_left: RADIUS, ..Radius::new(0.) });

        // Other kinds have square corners.
        assert_eq!(BoxBorder::new(&style(Border::Double, Sides::default())).radius(), 0.into());
    }

    #[test]
    fn hidden_borders_draw_nothing() {
        assert!(!BoxBorder::new(&style(Border::Hidden, Sides::default())).drawn());
        let none = Sides { top: Some(false), ..Sides::default() };
        assert!(!BoxBorder::new(&style(Border::Normal, none)).drawn());
        assert!(BoxBorder::new(&style(Border::Ascii, Sides::default())).drawn());
    }

    #[test]
    fn dashes_start_and_end_the_line() {
        assert_eq!(dashes(0.), []);
        assert_eq!(dashes(3.), [(0., 3.)]);
        assert_eq!(dashes(10.), [(0., 4.), (6., 4.)]);
        assert_eq!(dashes(22.), [(0., 4.), (6., 4.), (12., 4.), (18., 4.)]);

        // Gaps take up what is left, evenly.
        let line = dashes(100.);
        let (start, length) = *line.last().unwrap();
        assert_eq!(start + length, 100.);
        assert!(line.windows(2).all(|pair| (1. ..=3.).contains(&(pair[1].0 - pair[0].0 - DASH))));
    }
}
