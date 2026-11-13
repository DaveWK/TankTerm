//! The row of tabs in the tab bar: every tab as wide as the others, up to a widest, and a
//! button after them.
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
    max_tab_width: f32,
    spacing: f32,
}

impl<'a, Message, Theme, Renderer> TabStrip<'a, Message, Theme, Renderer> {
    pub fn new(
        tabs: impl IntoIterator<Item = Element<'a, Message, Theme, Renderer>>,
        after: impl Into<Element<'a, Message, Theme, Renderer>>,
        max_tab_width: f32,
        spacing: f32,
    ) -> Self {
        let mut children: Vec<_> = tabs.into_iter().collect();
        children.push(after.into());
        Self { children, max_tab_width, spacing }
    }
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
        let width = (room / count).min(self.max_tab_width).max(0.).floor();

        let tab_limits = Limits::new(Size::new(width, 0.), Size::new(width, max.height));
        let mut nodes: Vec<Node> = tabs
            .iter_mut()
            .zip(tab_trees)
            .map(|(tab, tree)| tab.as_widget_mut().layout(tree, renderer, &tab_limits))
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
