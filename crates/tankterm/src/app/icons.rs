//! The tab bar's icons, drawn as thin lines so they look the same in every font.

use std::f32::consts::PI;

use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::widget::{Id, container, svg};
use iced::{Color, Element, Point, Rectangle, Renderer, Size, Theme, mouse};
use iced_core::svg::Svg;

/// An icon of the tab bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// The new tab button.
    Plus,
    /// The button that opens the configuration.
    Gear,
    Minimize,
    Maximize,
    /// The maximize button of a maximized window: two overlapping squares.
    Restore,
    /// The window's close button, and a tab's.
    Close,
}

/// The ID of the window-wide `icon`, for finding it.
pub fn id(icon: Icon) -> Id {
    Id::new(match icon {
        Icon::Plus => "icon-plus",
        Icon::Gear => "icon-gear",
        Icon::Minimize => "icon-minimize",
        Icon::Maximize => "icon-maximize",
        Icon::Restore => "icon-restore",
        Icon::Close => "icon-close",
    })
}

/// The ID of the close button of the tab at `index`.
pub fn close_tab_id(index: usize) -> Id {
    Id::from(format!("close-tab-{index}"))
}

/// The ID of the app's icon at the end of the title bar.
pub fn app_icon_id() -> Id {
    Id::new("app-icon")
}

/// TankTerm's icon, in a square `size` wide.
pub fn app_icon<'a, Message: 'a>(size: f32) -> Element<'a, Message> {
    static LOGO: &[u8] = include_bytes!("../../extra/logo/tankterm-small.svg");
    let logo = svg(svg::Handle::from_memory(LOGO)).width(size).height(size);
    container(logo).id(app_icon_id()).into()
}

/// `icon` drawn in `color` with lines about a logical pixel wide, in a square `size` wide, on
/// a screen with `scale` pixels to a logical pixel.
pub fn icon<'a, Message: 'a>(
    icon: Icon,
    size: f32,
    color: Color,
    id: Id,
    scale: f32,
) -> Element<'a, Message> {
    let canvas = Canvas::new(Lines { icon, color, scale }).width(size).height(size);
    container(canvas).id(id).into()
}

/// What draws an icon.
struct Lines {
    icon: Icon,
    color: Color,
    /// Pixels of the screen to a logical pixel.
    scale: f32,
}

/// How wide the lines of icons are, in pixels of a screen with `scale` of them to a logical
/// pixel: a logical pixel, in whole pixels, and at least one.
fn line_width(scale: f32) -> f32 {
    scale.round().max(1.)
}

/// Where a line `width` pixels wide is drawn, near `at` in a frame at `origin`, so that it
/// covers whole pixels of a screen with `scale` of them to a logical pixel. Everything is in
/// logical pixels but `width`.
fn on_pixels(at: f32, origin: f32, width: f32, scale: f32) -> f32 {
    let edge = ((origin + at) * scale - width / 2.).round();
    (edge + width / 2.) / scale - origin
}

impl<Message> canvas::Program<Message> for Lines {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let side = bounds.width.min(bounds.height);
        // Points as shares of the square, where the lines cover whole pixels of the screen, so
        // they stay sharp also where the frame doesn't start on one, and on scaled screens.
        let scale = if self.scale > 0. { self.scale } else { 1. };
        let width = line_width(scale);
        let at = |x: f32, y: f32| {
            let snap = |share: f32, origin: f32| on_pixels(share * side, origin, width, scale);
            Point::new(snap(x, bounds.x), snap(y, bounds.y))
        };
        let stroke = Stroke::default().with_width(width / scale).with_color(self.color);
        let line = |frame: &mut Frame, from: Point, to: Point| {
            frame.stroke(&Path::line(from, to), stroke);
        };

        match self.icon {
            Icon::Plus => {
                line(&mut frame, at(0.5, 0.1), at(0.5, 0.9));
                line(&mut frame, at(0.1, 0.5), at(0.9, 0.5));
            },
            Icon::Close => {
                line(&mut frame, at(0.15, 0.15), at(0.85, 0.85));
                line(&mut frame, at(0.85, 0.15), at(0.15, 0.85));
            },
            Icon::Minimize => line(&mut frame, at(0.15, 0.5), at(0.85, 0.5)),
            Icon::Maximize => {
                let corner = at(0.15, 0.15);
                let size = at(0.85, 0.85) - corner;
                frame.stroke(&Path::rectangle(corner, Size::new(size.x, size.y)), stroke);
            },
            Icon::Restore => {
                // The front square, and the parts of the one behind it that show.
                let corner = at(0.15, 0.35);
                let size = at(0.65, 0.85) - corner;
                frame.stroke(&Path::rectangle(corner, Size::new(size.x, size.y)), stroke);
                let back = Path::new(|path| {
                    path.move_to(at(0.35, 0.35));
                    path.line_to(at(0.35, 0.15));
                    path.line_to(at(0.85, 0.15));
                    path.line_to(at(0.85, 0.65));
                    path.line_to(at(0.65, 0.65));
                });
                frame.stroke(&back, stroke);
            },
            Icon::Gear => {
                // The GPU smooths the slanted edges of the teeth with only a few samples, which
                // leaves them blocky, so the cog is drawn as an image that is smoothed exactly,
                // on whole pixels of the screen, so that it isn't blurred when it is placed.
                let pixels = (side * scale).round().max(1.);
                let onto = |at: f32| (at * scale).round() / scale - at;
                let square = Rectangle::new(
                    Point::new(onto(bounds.x), onto(bounds.y)),
                    Size::new(pixels / scale, pixels / scale),
                );
                frame.draw_svg(square, Svg::new(cog_svg(pixels, width)).color(self.color));
            },
        }

        vec![frame.into_geometry()]
    }
}

/// The cog's teeth, and how far from its center their tips and roots and its hole are, as
/// shares of the icon's side.
const COG_TEETH: usize = 8;
const COG_TIP: f32 = 0.47;
const COG_ROOT: f32 = 0.33;
const COG_HOLE: f32 = 0.13;

/// A cog in a square `pixels` wide, with lines `width` wide, as an SVG picture of its rim and the
/// hole for the axle.
fn cog_svg(pixels: f32, width: f32) -> svg::Handle {
    let rim: Vec<String> =
        cog(pixels).iter().map(|point| format!("{},{}", point.x, point.y)).collect();
    let center = pixels / 2.;
    let picture = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{pixels}' height='{pixels}' viewBox='0 0 \
         {pixels} {pixels}'><g fill='none' stroke='white' stroke-width='{width}' \
         stroke-linejoin='round'><polygon points='{}'/><circle cx='{center}' cy='{center}' \
         r='{}'/></g></svg>",
        rim.join(" "),
        pixels * COG_HOLE,
    );
    svg::Handle::from_memory(picture.into_bytes())
}

/// The outline of a cog in a square `side` wide: for every tooth, the root before it, the two
/// corners of its tip, and the root after it. The teeth are narrower at their tips, like a
/// gear's.
fn cog(side: f32) -> Vec<Point> {
    let center = side / 2.;
    let step = 2. * PI / COG_TEETH as f32;
    let point = |radius: f32, angle: f32| {
        let (sin, cos) = angle.sin_cos();
        Point::new(center + cos * radius * side, center + sin * radius * side)
    };

    (0..COG_TEETH)
        .flat_map(|tooth| {
            // Start at the top, so the cog stands upright.
            let angle = tooth as f32 * step - PI / 2.;
            let (root, tip) = (step * 0.3, step * 0.17);
            [
                point(COG_ROOT, angle - root),
                point(COG_TIP, angle - tip),
                point(COG_TIP, angle + tip),
                point(COG_ROOT, angle + root),
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use iced::widget::Space;
    use iced_test::simulator::Simulator;

    use super::*;

    const ICONS: [Icon; 6] =
        [Icon::Plus, Icon::Gear, Icon::Minimize, Icon::Maximize, Icon::Restore, Icon::Close];

    /// Whether two elements draw the same picture in a 20 by 20 window.
    fn same_picture(one: Element<'static, ()>, other: Element<'static, ()>) -> bool {
        let directory = tempfile::tempdir().unwrap();
        let picture = directory.path().join("picture");
        let window = Size::new(20., 20.);

        let mut ui = Simulator::with_size(Default::default(), window, one);
        assert!(ui.snapshot(&Theme::Dark).unwrap().matches_hash(&picture).unwrap());
        let mut ui = Simulator::with_size(Default::default(), window, other);
        ui.snapshot(&Theme::Dark).unwrap().matches_hash(&picture).unwrap()
    }

    #[test]
    fn the_gear_is_a_cog_with_eight_teeth() {
        let side = 16.;
        let center = Point::new(side / 2., side / 2.);
        let outline = cog(side);
        let radius = |point: &Point| point.distance(center) / side;

        // Every tooth is a root, two corners of its tip and another root, all the way round.
        assert_eq!(outline.len(), COG_TEETH * 4);
        for tooth in outline.chunks(4) {
            assert!((radius(&tooth[0]) - COG_ROOT).abs() < 1e-4);
            assert!((radius(&tooth[1]) - COG_TIP).abs() < 1e-4);
            assert!((radius(&tooth[2]) - COG_TIP).abs() < 1e-4);
            assert!((radius(&tooth[3]) - COG_ROOT).abs() < 1e-4);
        }
        // The teeth are narrower at the tip than at the root.
        let width = |a: &Point, b: &Point| a.distance(*b);
        assert!(width(&outline[1], &outline[2]) < width(&outline[0], &outline[3]));
        // And the whole cog fits in its square.
        assert!(
            outline
                .iter()
                .all(|point| (0. ..=side).contains(&point.x) && (0. ..=side).contains(&point.y))
        );
    }

    #[test]
    fn lines_cover_whole_pixels_of_the_screen() {
        for scale in [1., 1.25, 1.5, 1.75, 2., 3.] {
            let width = line_width(scale);
            assert!(width >= 1. && width.fract() == 0., "{scale}");
            for origin in [0., 10.25, 13.5, 21.7] {
                for at in [0., 1.6, 7.5, 13.] {
                    let line = on_pixels(at, origin, width, scale);
                    // The line's edges are on the screen's pixels' edges, and near `at`.
                    let edge = (origin + line) * scale - width / 2.;
                    assert!((edge - edge.round()).abs() < 1e-3, "{scale} {origin} {at}");
                    assert!((line - at).abs() <= 0.5 / scale + 1e-3, "{scale} {origin} {at}");
                }
            }
        }
        // Where the frame starts on a pixel, at a scale of one, they are on the pixels' centers
        // as before.
        assert_eq!(on_pixels(4., 10., 1., 1.), 4.5);
    }

    #[test]
    fn every_icon_draws_something_of_its_own() {
        let draw = |shown: Icon| icon(shown, 16., Color::WHITE, id(shown), 1.);
        for (index, shown) in ICONS.into_iter().enumerate() {
            assert!(!same_picture(draw(shown), Space::new().into()), "{shown:?}");
            for other in &ICONS[index + 1..] {
                assert!(!same_picture(draw(shown), draw(*other)), "{shown:?} {other:?}");
            }
        }
    }
}
