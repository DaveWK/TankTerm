//! The tab bar's icons, drawn as thin lines so they look the same in every font.

use std::f32::consts::PI;

use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Stroke};
use iced::widget::{Id, container};
use iced::{Color, Element, Point, Rectangle, Renderer, Size, Theme, mouse};

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

/// `icon` drawn in `color` with lines a pixel wide, in a square `size` wide.
pub fn icon<'a, Message: 'a>(icon: Icon, size: f32, color: Color, id: Id) -> Element<'a, Message> {
    let canvas = Canvas::new(Lines { icon, color }).width(size).height(size);
    container(canvas).id(id).into()
}

/// What draws an icon.
struct Lines {
    icon: Icon,
    color: Color,
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
        // Points as shares of the square, on pixel centers so the lines stay sharp.
        let at = |x: f32, y: f32| Point::new((x * side).round() + 0.5, (y * side).round() + 0.5);
        let stroke = Stroke::default().with_width(1.).with_color(self.color);
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
                let center = Point::new(side / 2., side / 2.);
                frame.stroke(&Path::circle(center, side * 0.24), stroke);
                frame.stroke(&Path::circle(center, side * 0.08), stroke);
                for tooth in 0..8 {
                    let angle = tooth as f32 * PI / 4.;
                    let (sin, cos) = angle.sin_cos();
                    let point = |radius: f32| {
                        Point::new(center.x + cos * radius * side, center.y + sin * radius * side)
                    };
                    line(&mut frame, point(0.24), point(0.45));
                }
            },
        }

        vec![frame.into_geometry()]
    }
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
    fn every_icon_draws_something_of_its_own() {
        let draw = |shown: Icon| icon(shown, 16., Color::WHITE, id(shown));
        for (index, shown) in ICONS.into_iter().enumerate() {
            assert!(!same_picture(draw(shown), Space::new().into()), "{shown:?}");
            for other in &ICONS[index + 1..] {
                assert!(!same_picture(draw(shown), draw(*other)), "{shown:?} {other:?}");
            }
        }
    }
}
