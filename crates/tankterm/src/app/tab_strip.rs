//! The row of tabs in the tab bar: tabs with a width of their own, the others sharing the rest
//! of the room evenly, each up to its widest, and a button after them.
//!
//! iced's rows give every child that fills its share of the room the whole share, whatever
//! its maximum width, so a single tab would fill the bar and leave no empty space in it.

use iced_core::layout::{Layout, Limits, Node};
use iced_core::widget::{Operation, Tree};
use iced_core::{
    Clipboard, Element, Event, Length, Rectangle, Shell, Size, Vector, Widget, mouse, overlay,
    renderer,
};

pub struct TabStrip<'a, Message, Theme, Renderer> {
    /// The tabs, then the button after them.
    children: Vec<Element<'a, Message, Theme, Renderer>>,
    /// How wide every tab gets.
    widths: Vec<TabWidth>,
    spacing: f32,
}

/// How wide a tab gets, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabWidth {
    /// A width of its own, which the tab keeps while there is room for it.
    pub fixed: Option<f32>,
    /// Widest the tab gets while it shares the room with the other tabs.
    pub max: f32,
}

impl<'a, Message, Theme, Renderer> TabStrip<'a, Message, Theme, Renderer> {
    pub fn new(
        tabs: impl IntoIterator<Item = (Element<'a, Message, Theme, Renderer>, TabWidth)>,
        after: impl Into<Element<'a, Message, Theme, Renderer>>,
        spacing: f32,
    ) -> Self {
        let (mut children, widths): (Vec<_>, Vec<_>) = tabs.into_iter().unzip();
        children.push(after.into());
        Self { children, widths, spacing }
    }
}

/// The width of every tab in `room` logical pixels.
///
/// Tabs with a fixed width get it, and the other tabs share what is left evenly, each up to its
/// widest. Where the fixed widths alone don't fit, they shrink by the same share and leave
/// nothing for the others.
fn tab_widths(room: f32, tabs: &[TabWidth]) -> Vec<f32> {
    let room = room.max(0.);
    let fixed: f32 = tabs.iter().filter_map(|tab| tab.fixed).sum();
    let scale = if fixed > room { room / fixed } else { 1. };

    // The flexible tabs fill the room left up to a level, like water: tabs whose widest is
    // below it stop there, and the rest share what they leave.
    let mut left = (room - fixed).max(0.);
    let mut caps: Vec<f32> =
        tabs.iter().filter(|tab| tab.fixed.is_none()).map(|tab| tab.max.max(0.)).collect();
    caps.sort_by(f32::total_cmp);
    let mut level = 0.;
    for (index, cap) in caps.iter().enumerate() {
        let share = left / (caps.len() - index) as f32;
        if *cap > share {
            level = share;
            break;
        }
        left -= cap;
        level = *cap;
    }

    tabs.iter()
        .map(|tab| match tab.fixed {
            Some(width) => (width * scale).floor(),
            None => tab.max.min(level).max(0.).floor(),
        })
        .collect()
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for TabStrip<'_, Message, Theme, Renderer>
where
    Renderer: iced_core::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    /// The button after the tabs gets the room it needs, and the tabs share the rest evenly,
    /// each up to the widest a tab gets. Everything is centered from top to bottom.
    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        let limits = limits.width(Length::Fill).height(Length::Shrink);
        let max = limits.max();
        let Some((after, tabs)) = self.children.split_last_mut() else {
            return Node::new(Size::new(max.width, 0.));
        };
        let (after_tree, tab_trees) = tree.children.split_last_mut().expect("a tree per child");

        let after = after.as_widget_mut().layout(after_tree, renderer, &limits.loose());
        let count = tabs.len() as f32;
        let room = max.width - after.size().width - self.spacing * count;
        let widths = tab_widths(room, &self.widths);

        let mut nodes: Vec<Node> = tabs
            .iter_mut()
            .zip(tab_trees)
            .zip(widths)
            .map(|((tab, tree), width)| {
                let limits = Limits::new(Size::new(width, 0.), Size::new(width, max.height));
                tab.as_widget_mut().layout(tree, renderer, &limits)
            })
            .collect();
        nodes.push(after);

        let height = nodes.iter().map(|node| node.size().height).fold(0., f32::max);
        let mut x = 0.;
        for node in &mut nodes {
            let size = node.size();
            node.move_to_mut((x, ((height - size.height) / 2.).floor()));
            x += size.width + self.spacing;
        }

        Node::with_children(Size::new(max.width, height), nodes)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((child, state), layout) in
                self.children.iter_mut().zip(&mut tree.children).zip(layout.children())
            {
                child.as_widget_mut().operate(state, layout, renderer, operation);
            }
        });
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
        for ((child, state), layout) in
            self.children.iter_mut().zip(&mut tree.children).zip(layout.children())
        {
            child
                .as_widget_mut()
                .update(state, event, layout, cursor, renderer, clipboard, shell, viewport);
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child.as_widget().mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
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
        for ((child, state), layout) in self
            .children
            .iter()
            .zip(&tree.children)
            .zip(layout.children())
            .filter(|(_, layout)| layout.bounds().intersects(viewport))
        {
            child.as_widget().draw(state, renderer, theme, style, layout, cursor, viewport);
        }
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        overlay::from_children(&mut self.children, tree, layout, renderer, viewport, translation)
    }
}

impl<'a, Message, Theme, Renderer> From<TabStrip<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced_core::Renderer + 'a,
{
    fn from(strip: TabStrip<'a, Message, Theme, Renderer>) -> Self {
        Element::new(strip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flexible(max: f32) -> TabWidth {
        TabWidth { fixed: None, max }
    }

    fn fixed(width: f32) -> TabWidth {
        TabWidth { fixed: Some(width), max: width }
    }

    #[test]
    fn flexible_tabs_share_the_room_up_to_their_widest() {
        assert_eq!(tab_widths(1000., &[flexible(240.); 2]), [240., 240.]);
        assert_eq!(tab_widths(300., &[flexible(240.); 3]), [100., 100., 100.]);
        assert_eq!(tab_widths(-5., &[flexible(240.)]), [0.]);
    }

    #[test]
    fn narrow_tabs_leave_their_share_to_the_others() {
        let tabs = [flexible(50.), flexible(240.), flexible(240.)];
        assert_eq!(tab_widths(400., &tabs), [50., 175., 175.]);
        assert_eq!(tab_widths(90., &tabs), [30., 30., 30.]);
    }

    #[test]
    fn fixed_tabs_keep_their_width() {
        let tabs = [fixed(150.), flexible(240.), flexible(240.)];
        assert_eq!(tab_widths(1000., &tabs), [150., 240., 240.]);
        assert_eq!(tab_widths(350., &tabs), [150., 100., 100.]);

        // Without room for them, they shrink and the others get nothing.
        assert_eq!(tab_widths(100., &[fixed(100.), fixed(100.), flexible(240.)]), [50., 50., 0.]);
    }
}
