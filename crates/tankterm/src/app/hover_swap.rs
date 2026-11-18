//! A widget drawn one way normally and another way under the mouse, for the tab bar.
//!
//! Unlike iced's `hover`, which draws its top layer over the base, the hovered content is drawn
//! in place of the normal content: parts of the tab bar are see-through, so the normal content
//! would show through the hovered one.

use iced_core::layout::{Layout, Limits, Node};
use iced_core::widget::{Operation, Tree};
use iced_core::{
    Clipboard, Element, Event, Length, Rectangle, Shell, Size, Vector, Widget, mouse, overlay,
    renderer,
};

pub struct HoverSwap<'a, Message, Theme, Renderer> {
    /// The normal content, then the hovered content.
    children: [Element<'a, Message, Theme, Renderer>; 2],
}

impl<'a, Message, Theme, Renderer> HoverSwap<'a, Message, Theme, Renderer> {
    /// `normal`, drawn as `hovered` while the mouse is over it. `hovered` is laid out in the
    /// place of `normal`, so it has the same size, and gets the mouse first.
    pub fn new(
        normal: impl Into<Element<'a, Message, Theme, Renderer>>,
        hovered: impl Into<Element<'a, Message, Theme, Renderer>>,
    ) -> Self {
        Self { children: [normal.into(), hovered.into()] }
    }
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for HoverSwap<'_, Message, Theme, Renderer>
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
        self.children[0].as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.children[0].as_widget().size_hint()
    }

    fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
        let [normal, hovered] = &mut self.children;
        let normal = normal.as_widget_mut().layout(&mut tree.children[0], renderer, limits);
        let limits = Limits::new(Size::ZERO, normal.size());
        let hovered = hovered.as_widget_mut().layout(&mut tree.children[1], renderer, &limits);
        Node::with_children(normal.size(), vec![normal, hovered])
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
        let shown = usize::from(cursor.is_over(layout.bounds()));
        let child = layout.children().nth(shown).expect("a layout per child");
        self.children[shown].as_widget().draw(
            &tree.children[shown],
            renderer,
            theme,
            style,
            child,
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        for ((child, tree), layout) in
            self.children.iter_mut().zip(&mut tree.children).zip(layout.children())
        {
            child.as_widget_mut().operate(tree, layout, renderer, operation);
        }
    }

    /// The hovered content gets the mouse first, and the normal content what it leaves. Both
    /// see the mouse move and the buttons released, so they can tell when the mouse leaves.
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
        let hovered = cursor.is_over(layout.bounds());
        let mut layouts = layout.children();
        let (normal_layout, hovered_layout) = (layouts.next().unwrap(), layouts.next().unwrap());
        let [normal, top] = &mut self.children;
        let (normal_tree, top_tree) = tree.children.split_at_mut(1);

        let passing = matches!(
            event,
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::ButtonReleased(_))
        );
        if hovered || passing {
            top.as_widget_mut().update(
                &mut top_tree[0],
                event,
                hovered_layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
            if shell.is_event_captured() {
                return;
            }
        }
        normal.as_widget_mut().update(
            &mut normal_tree[0],
            event,
            normal_layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        let shown = usize::from(cursor.is_over(layout.bounds()));
        let child = layout.children().nth(shown).expect("a layout per child");
        self.children[shown].as_widget().mouse_interaction(
            &tree.children[shown],
            child,
            cursor,
            viewport,
            renderer,
        )
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

impl<'a, Message, Theme, Renderer> From<HoverSwap<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: iced_core::Renderer + 'a,
{
    fn from(swap: HoverSwap<'a, Message, Theme, Renderer>) -> Self {
        Element::new(swap)
    }
}
