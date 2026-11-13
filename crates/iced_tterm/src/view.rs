use crate::backend::{
    Backend, Command, LinkAction, MouseButton, RenderableContent,
};
use crate::bindings::{BindingAction, BindingsLayout, InputKind};
use crate::box_drawing;
use crate::cursor::cursor_rectangles;
use crate::keyboard::{self, KeyInput, KeyState, KittyOutput};
use crate::terminal::{Event, Terminal};
use crate::theme::{TerminalStyle, TextAttributes};
use iced::alignment::Vertical;
use iced::mouse::{Cursor, ScrollDelta};
use iced::widget::canvas::{Path, Text};
use iced::widget::container;
use iced::{Color, Element, Length, Point, Rectangle, Size, Theme};
use iced_core::clipboard::Kind as ClipboardKind;
use iced_core::keyboard::{Key, Modifiers};
use iced_core::mouse::{self, Click};
use iced_core::text::{Alignment, LineHeight, Shaping};
use iced_core::widget::operation::{self, Focusable};
use iced_graphics::core::widget::{tree, Tree};
use iced_graphics::core::Widget;
use iced_graphics::geometry::Stroke;
use tankterm_terminal::index::Point as TerminalGridPoint;
use tankterm_terminal::selection::SelectionType;
use tankterm_terminal::term::{cell, TermMode};
use tankterm_terminal::vte::ansi::{self as ansi, CursorShape, NamedColor};

pub struct TerminalView<'a> {
    term: &'a Terminal,
}

impl<'a> TerminalView<'a> {
    pub fn show(term: &'a Terminal) -> Element<'a, Event> {
        container(Self { term })
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| {
                term.theme
                    .container_style(&term.backend.renderable_content().colors)
            })
            .into()
    }

    pub fn focus<Message: 'static>(
        id: iced::widget::Id,
    ) -> iced::Task<Message> {
        iced::widget::operation::focus(id)
    }

    fn is_cursor_in_layout(
        &self,
        cursor: Cursor,
        layout: iced_graphics::core::Layout<'_>,
    ) -> bool {
        if let Some(cursor_position) = cursor.position() {
            let layout_position = layout.position();
            let layout_size = layout.bounds();
            let is_triggered = cursor_position.x >= layout_position.x
                && cursor_position.y >= layout_position.y
                && cursor_position.x < (layout_position.x + layout_size.width)
                && cursor_position.y < (layout_position.y + layout_size.height);

            return is_triggered;
        }

        false
    }

    /// Where the top left corner of the grid is: the layout's, or with
    /// dynamic padding moved to centre the grid in the layout's bounds.
    ///
    /// Drawing and turning mouse positions into grid points both measure
    /// from here, so that a click lands on the cell drawn under it.
    fn grid_origin(&self, layout: iced_graphics::core::Layout<'_>) -> Point {
        let bounds = layout.bounds();
        if !self.term.dynamic_padding {
            return bounds.position();
        }

        let size = &self.term.backend.renderable_content().terminal_size;
        bounds.position() + size.centring_offset(bounds.size())
    }

    fn is_cursor_hovered_hyperlink(&self, state: &TerminalViewState) -> bool {
        let content = self.term.backend.renderable_content();
        if let Some(hyperlink_range) = &content.hovered_hyperlink {
            return hyperlink_range.contains(&state.mouse_position_on_grid);
        }

        false
    }

    fn handle_resize(
        &mut self,
        state: &mut TerminalViewState,
        layout: iced_graphics::core::Layout<'_>,
        shell: &mut iced_graphics::core::Shell<'_, Event>,
    ) {
        let layout_size = layout.bounds().size();
        if state.size != layout_size {
            state.size = layout_size;
            let cmd = Command::Resize(
                Some(layout_size),
                Some(self.term.font.measure),
            );
            shell.publish(Event::BackendCall(self.term.id, cmd));
        }
    }

    fn handle_focus(
        &self,
        event: &iced_core::Event,
        state: &mut TerminalViewState,
        is_cursor_in_layout: bool,
    ) {
        use iced::Event::Mouse;
        use iced_core::mouse::{Button::Left, Event::ButtonPressed};

        if let Mouse(ButtonPressed(Left)) = event {
            state.focus = is_cursor_in_layout;
        }
    }

    fn handle_mouse_event(
        &self,
        state: &mut TerminalViewState,
        clipboard: &mut dyn iced_graphics::core::Clipboard,
        grid_origin: Point,
        cursor_position: Point,
        event: &iced::mouse::Event,
    ) -> Vec<Command> {
        let mut commands = Vec::new();
        let terminal_content = self.term.backend.renderable_content();
        let terminal_mode = terminal_content.terminal_mode;

        match event {
            iced_core::mouse::Event::ButtonPressed(
                button @ (iced_core::mouse::Button::Middle
                | iced_core::mouse::Button::Right),
            ) => {
                match mouse_action(
                    &self.term.bindings,
                    *button,
                    state.keyboard_modifiers,
                    terminal_mode,
                ) {
                    BindingAction::Paste => commands
                        .extend(paste(clipboard, ClipboardKind::Standard)),
                    BindingAction::PasteSelection => commands
                        .extend(paste(clipboard, ClipboardKind::Primary)),
                    BindingAction::ExpandSelection if state.is_focused() => {
                        commands.push(Self::expand_selection(
                            state,
                            *button,
                            cursor_position,
                            grid_origin,
                        ))
                    },
                    _ => (),
                }
            },
            iced_core::mouse::Event::ButtonPressed(
                iced_core::mouse::Button::Left,
            ) => {
                if !state.is_focused() {
                    return Vec::default();
                }

                Self::handle_left_button_pressed(
                    state,
                    &terminal_mode,
                    cursor_position,
                    grid_origin,
                    &mut commands,
                );
            },
            iced_core::mouse::Event::CursorMoved { position } => {
                if !state.is_focused() {
                    return Vec::default();
                }

                Self::handle_cursor_moved(
                    state,
                    self.term.backend.renderable_content(),
                    position,
                    grid_origin,
                    &mut commands,
                );
            },
            iced_core::mouse::Event::ButtonReleased(
                iced_core::mouse::Button::Left,
            ) => {
                if !state.is_focused() {
                    return Vec::default();
                }

                Self::handle_button_released(
                    state,
                    &terminal_mode,
                    &self.term.bindings,
                    &mut commands,
                );
            },
            iced::mouse::Event::WheelScrolled { delta } => {
                Self::handle_wheel_scrolled(
                    state,
                    *delta,
                    &self.term.font.measure,
                    self.term.scroll_multiplier,
                    &mut commands,
                );
            },
            _ => {},
        }

        commands
    }

    fn handle_left_button_pressed(
        state: &mut TerminalViewState,
        terminal_mode: &TermMode,
        cursor_position: Point,
        grid_origin: Point,
        commands: &mut Vec<Command>,
    ) {
        let cmd = if terminal_mode.intersects(TermMode::MOUSE_MODE) {
            Command::MouseReport(
                MouseButton::LeftButton,
                state.keyboard_modifiers,
                state.mouse_position_on_grid,
                true,
            )
        } else {
            let current_click = Click::new(
                cursor_position,
                mouse::Button::Left,
                state.last_click,
            );
            let selection_type = match current_click.kind() {
                mouse::click::Kind::Single => SelectionType::Simple,
                mouse::click::Kind::Double => SelectionType::Semantic,
                mouse::click::Kind::Triple => SelectionType::Lines,
            };
            state.last_click = Some(current_click);
            Command::SelectStart(
                selection_type,
                (
                    cursor_position.x - grid_origin.x,
                    cursor_position.y - grid_origin.y,
                ),
            )
        };
        commands.push(cmd);
        state.is_dragged = true;
    }

    /// Extend the selection to the pointer. Clicking twice or three times in
    /// a row extends it by words or lines, and Control makes it a block.
    fn expand_selection(
        state: &mut TerminalViewState,
        button: mouse::Button,
        cursor_position: Point,
        grid_origin: Point,
    ) -> Command {
        let click = Click::new(cursor_position, button, state.last_click);
        let selection_type = if state.keyboard_modifiers.control() {
            SelectionType::Block
        } else {
            match click.kind() {
                mouse::click::Kind::Single => SelectionType::Simple,
                mouse::click::Kind::Double => SelectionType::Semantic,
                mouse::click::Kind::Triple => SelectionType::Lines,
            }
        };
        state.last_click = Some(click);

        Command::ExpandSelection(
            selection_type,
            (
                cursor_position.x - grid_origin.x,
                cursor_position.y - grid_origin.y,
            ),
        )
    }

    fn handle_cursor_moved(
        state: &mut TerminalViewState,
        terminal_content: &RenderableContent,
        position: &Point,
        grid_origin: Point,
        commands: &mut Vec<Command>,
    ) {
        let cursor_x = position.x - grid_origin.x;
        let cursor_y = position.y - grid_origin.y;
        state.mouse_position_on_grid = Backend::selection_point(
            cursor_x,
            cursor_y,
            &terminal_content.terminal_size,
            terminal_content.grid.display_offset(),
        );

        // Handle command or selection update based on terminal mode and modifiers
        if state.is_dragged {
            let terminal_mode = terminal_content.terminal_mode;
            // Report drags when the app requested button-motion (1002) or
            // any-motion (1003) tracking. Checking only MOUSE_MOTION left
            // 1002-mode apps (e.g. tmux with `mouse on`) seeing press and
            // release but never the drag, while the widget drew its own
            // selection over the app's — two selection systems at once.
            let cmd = if terminal_mode
                .intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION)
            {
                Command::MouseReport(
                    MouseButton::LeftMove,
                    state.keyboard_modifiers,
                    state.mouse_position_on_grid,
                    true,
                )
            } else {
                Command::SelectUpdate((cursor_x, cursor_y))
            };
            commands.push(cmd);
        }

        // Handle link hover if applicable
        if state.keyboard_modifiers == Modifiers::COMMAND {
            commands.push(Command::ProcessLink(
                LinkAction::Hover,
                state.mouse_position_on_grid,
            ));
        }
    }

    fn handle_button_released(
        state: &mut TerminalViewState,
        terminal_mode: &TermMode,
        bindings: &BindingsLayout, // Use the actual type of your bindings here
        commands: &mut Vec<Command>,
    ) {
        state.is_dragged = false;

        if terminal_mode.intersects(TermMode::MOUSE_MODE) {
            commands.push(Command::MouseReport(
                MouseButton::LeftButton,
                state.keyboard_modifiers,
                state.mouse_position_on_grid,
                false,
            ));
        }

        if bindings.get_action(
            InputKind::Mouse(iced_core::mouse::Button::Left),
            state.keyboard_modifiers,
            *terminal_mode,
        ) == BindingAction::LinkOpen
        {
            commands.push(Command::ProcessLink(
                LinkAction::Open,
                state.mouse_position_on_grid,
            ));
        }
    }

    /// Turn a turn of the mouse wheel into lines to scroll: `multiplier`
    /// lines for every step of a wheel, or for every line's height of a
    /// touchpad's pixels.
    fn handle_wheel_scrolled(
        state: &mut TerminalViewState,
        delta: ScrollDelta,
        font_measure: &Size<f32>,
        multiplier: f32,
        commands: &mut Vec<Command>,
    ) {
        match delta {
            ScrollDelta::Lines { y, .. } => {
                let lines = y.signum() * (y.abs() * multiplier).round();
                if lines != 0.0 {
                    commands.push(Command::Scroll(lines as i32));
                }
            },
            ScrollDelta::Pixels { y, .. } => {
                state.scroll_pixels += y * multiplier;
                let line_height = font_measure.height;
                let lines = (state.scroll_pixels / line_height).trunc();
                state.scroll_pixels %= line_height;
                if lines != 0.0 {
                    commands.push(Command::Scroll(lines as i32));
                }
            },
        }
    }

    fn handle_keyboard_event(
        &self,
        state: &mut TerminalViewState,
        clipboard: &mut dyn iced_graphics::core::Clipboard,
        event: &iced::keyboard::Event,
    ) -> Option<Command> {
        let last_content = self.term.backend.renderable_content();
        let input = match event {
            iced::keyboard::Event::ModifiersChanged(m) => {
                state.keyboard_modifiers = *m;
                let action = if state.keyboard_modifiers == Modifiers::COMMAND {
                    LinkAction::Hover
                } else {
                    LinkAction::Clear
                };
                return Some(Command::ProcessLink(
                    action,
                    state.mouse_position_on_grid,
                ));
            },
            iced::keyboard::Event::KeyPressed {
                key,
                modified_key,
                location,
                modifiers,
                text,
                repeat,
                ..
            } => KeyInput {
                key,
                modified_key,
                location: *location,
                modifiers: *modifiers,
                text: text.as_deref(),
                state: if *repeat {
                    KeyState::Repeat
                } else {
                    KeyState::Pressed
                },
            },
            iced::keyboard::Event::KeyReleased {
                key,
                modified_key,
                location,
                modifiers,
                ..
            } => KeyInput {
                key,
                modified_key,
                location: *location,
                modifiers: *modifiers,
                text: None,
                state: KeyState::Released,
            },
        };

        match key_output(
            &self.term.bindings,
            state.keyboard_modifiers,
            &input,
            last_content.terminal_mode,
        )? {
            KeyOutput::Write(bytes) => Some(Command::Write(bytes)),
            KeyOutput::Paste(kind) => paste(clipboard, kind),
            KeyOutput::Copy => {
                clipboard.write(
                    ClipboardKind::Standard,
                    self.term.backend.selectable_content(),
                );
                None
            },
        }
    }
}

/// The command pasting a clipboard, unless it is empty.
///
/// Where there is no selection clipboard, the regular one stands in for it,
/// like in Alacritty.
fn paste(
    clipboard: &mut dyn iced_graphics::core::Clipboard,
    kind: ClipboardKind,
) -> Option<Command> {
    let text = clipboard.read(kind).or_else(|| match kind {
        ClipboardKind::Primary => clipboard.read(ClipboardKind::Standard),
        ClipboardKind::Standard => None,
    })?;

    (!text.is_empty()).then_some(Command::Paste(text))
}

/// What a press of a mouse button other than the left one does, going by the
/// mouse bindings.
///
/// While the application in the terminal asks for mouse reports, clicks belong
/// to it; holding Shift takes them back, as usual.
fn mouse_action(
    bindings: &BindingsLayout,
    button: mouse::Button,
    modifiers: Modifiers,
    terminal_mode: TermMode,
) -> BindingAction {
    let modifiers = if terminal_mode.intersects(TermMode::MOUSE_MODE) {
        if !modifiers.contains(Modifiers::SHIFT) {
            return BindingAction::Ignore;
        }
        modifiers - Modifiers::SHIFT
    } else {
        modifiers
    };

    bindings.get_action(InputKind::Mouse(button), modifiers, terminal_mode)
}

/// The clipboard a press of a mouse button other than the left one pastes.
#[cfg(test)]
fn mouse_paste(
    bindings: &BindingsLayout,
    button: mouse::Button,
    modifiers: Modifiers,
    terminal_mode: TermMode,
) -> Option<ClipboardKind> {
    match mouse_action(bindings, button, modifiers, terminal_mode) {
        BindingAction::Paste => Some(ClipboardKind::Standard),
        BindingAction::PasteSelection => Some(ClipboardKind::Primary),
        _ => None,
    }
}

/// What a key event turns into.
#[derive(Debug, Clone, PartialEq, Eq)]
enum KeyOutput {
    /// Send these bytes to the application.
    Write(Vec<u8>),
    /// Copy the selection.
    Copy,
    /// Paste this clipboard.
    Paste(ClipboardKind),
}

/// The binding table's action for a key. Character keys are looked up by
/// their unmodified, lowercased character with the tracked modifier state
/// (`keyboard_modifiers`), named keys with the event's modifiers.
fn binding_action(
    bindings: &BindingsLayout,
    keyboard_modifiers: Modifiers,
    input: &KeyInput,
    terminal_mode: TermMode,
) -> BindingAction {
    match input.key {
        // Use the physical character key for bindings even when text is None
        // (e.g., Ctrl/Cmd combos)
        Key::Character(k) => bindings.get_action(
            InputKind::Char(k.to_ascii_lowercase()),
            keyboard_modifiers,
            terminal_mode,
        ),
        Key::Named(code) => bindings.get_action(
            InputKind::KeyCode(*code),
            input.modifiers,
            terminal_mode,
        ),
        Key::Unidentified => BindingAction::Ignore,
    }
}

/// Decide what a key event sends.
///
/// Without the kitty keyboard protocol this is the binding table, falling
/// back to the key's text, and releases send nothing. While the application
/// has enabled the protocol, keys are encoded by [`keyboard::encode`]; only
/// bindings to widget actions (copy, paste) still take precedence, and the
/// protocol defers to the binding table for the keys that keep their legacy
/// encoding.
fn key_output(
    bindings: &BindingsLayout,
    keyboard_modifiers: Modifiers,
    input: &KeyInput,
    terminal_mode: TermMode,
) -> Option<KeyOutput> {
    let binding_action =
        binding_action(bindings, keyboard_modifiers, input, terminal_mode);

    match binding_action {
        // A key bound to a widget action is never sent, nor is its release.
        BindingAction::Copy if input.state != KeyState::Released => {
            return Some(KeyOutput::Copy);
        },
        BindingAction::Paste if input.state != KeyState::Released => {
            return Some(KeyOutput::Paste(ClipboardKind::Standard));
        },
        BindingAction::PasteSelection if input.state != KeyState::Released => {
            return Some(KeyOutput::Paste(ClipboardKind::Primary));
        },
        BindingAction::Copy
        | BindingAction::Paste
        | BindingAction::PasteSelection => return None,
        _ => {},
    }

    match keyboard::encode(input, terminal_mode) {
        Some(KittyOutput::Write(bytes)) => {
            return Some(KeyOutput::Write(bytes))
        },
        Some(KittyOutput::Nothing) => return None,
        Some(KittyOutput::Legacy) | None => {},
    }

    // Releases are only reported through the kitty keyboard protocol.
    if input.state == KeyState::Released {
        return None;
    }

    match binding_action {
        BindingAction::Char(c) => {
            let mut buf = [0, 0, 0, 0];
            let str = c.encode_utf8(&mut buf);
            Some(KeyOutput::Write(str.as_bytes().to_vec()))
        },
        BindingAction::Esc(seq) => Some(KeyOutput::Write(seq.into_bytes())),
        // If no binding matched a character key, only write printable text
        // (when provided).
        BindingAction::Ignore if matches!(input.key, Key::Character(_)) => {
            input
                .text
                .map(|text| KeyOutput::Write(text.as_bytes().to_vec()))
        },
        _ => None,
    }
}

impl Widget<Event, Theme, iced::Renderer> for TerminalView<'_> {
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fill,
            height: Length::Fill,
        }
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<TerminalViewState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(TerminalViewState::new(self.term.id))
    }

    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<TerminalViewState>();
        if state.terminal_id != self.term.id {
            tree.state = self.state();
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &iced_core::layout::Limits,
    ) -> iced_core::layout::Node {
        let size = limits.resolve(Length::Fill, Length::Fill, Size::ZERO);
        iced::advanced::layout::Node::new(size)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: iced_core::Layout<'_>,
        _renderer: &iced::Renderer,
        operation: &mut dyn operation::Operation,
    ) {
        let state = tree.state.downcast_mut::<TerminalViewState>();
        let wid = self.term.widget_id();
        operation.focusable(Some(wid), layout.bounds(), state);
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        layout: iced::advanced::Layout,
        _cursor: Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<TerminalViewState>();
        let content = self.term.backend.renderable_content();
        let term_size = content.terminal_size;
        let cell_width = term_size.cell_width as f32;
        let cell_height = term_size.cell_height as f32;
        let font_size = self.term.font.size;
        let font_scale_factor = self.term.font.scale_factor;
        let grid_origin = self.grid_origin(layout);
        let layout_offset_x = grid_origin.x;
        let layout_offset_y = grid_origin.y;

        let geom = self.term.cache.draw(renderer, viewport.size(), |frame| {
            // Precompute constants used in the inner loop
            let display_offset = content.grid.display_offset() as f32;
            let cell_size = Size::new(cell_width, cell_height);
            let half_w = cell_width * 0.5;
            let half_h = cell_height * 0.5;
            // We use the background pallete color as a default
            // because the widget global background color must be the same
            let theme = &self.term.theme;
            let overrides = &content.colors;
            let default_bg = theme.get_color_with_overrides(
                ansi::Color::Named(NamedColor::Background),
                overrides,
            );

            // Hidden while the application hides the cursor and in the
            // dark half of a blink.
            let cursor_shape = self.term.cursor_shape();
            let cursor_thickness = self.term.cursor.settings.thickness;

            let mut last_line: Option<i32> = None;
            let mut bg_batch_rect = BackgroundRect::default();
            // Lines and cursor shapes are drawn after the backgrounds, which
            // are only filled once a run of them ends and would cover them.
            let mut lines: Vec<(Path, f32, Color)> = Vec::new();
            let mut cursor_rects: Vec<(Point, Size, Color)> = Vec::new();
            // Built-in characters' shapes go over the backgrounds and the
            // cursor, where text goes too.
            let mut builtin_glyphs: Vec<(Path, Color)> = Vec::new();

            for indexed in content.grid.display_iter() {
                // Compute per-cell geometry cheaply
                let line = indexed.point.line.0;
                let col = indexed.point.column.0 as f32;

                // Resolve position point for this cell
                let x = layout_offset_x + (col * cell_width);
                let y = layout_offset_y
                    + (((line as f32) + display_offset) * cell_height);
                let cell_center_y = y + half_h;
                let cell_center_x = x + half_w;

                // Selected text adds the selection's attributes to its own.
                let selected = content
                    .selectable_range
                    .is_some_and(|r| r.contains(indexed.point));
                let attributes = if selected {
                    theme.selection_attributes()
                } else {
                    TextAttributes::default()
                };

                // Resolve colors for this cell. Bold text that isn't dim can
                // take the bright variant of its color.
                let flags = indexed.cell.flags;
                let fg_color =
                    if flags & cell::Flags::DIM_BOLD == cell::Flags::BOLD {
                        theme.bold_color(indexed.fg)
                    } else {
                        indexed.fg
                    };
                let mut fg =
                    theme.get_color_with_overrides(fg_color, overrides);
                let mut bg =
                    theme.get_color_with_overrides(indexed.bg, overrides);
                // The cell's own colors, before INVERSE and the selection:
                // the block cursor and the glyph under it are drawn in
                // these, so they contrast regardless of INVERSE/selection.
                let (cell_fg, cell_bg) = (fg, bg);

                // If the new line was detected,
                // need to flush pending background rect and init the new one
                if last_line != Some(line) {
                    if bg_batch_rect.can_flush() {
                        let line = last_line.unwrap_or(line);
                        frame.fill(
                            &bg_batch_rect.build(line),
                            bg_batch_rect.color,
                        );
                    }

                    last_line = Some(line);
                    bg_batch_rect = BackgroundRect::default()
                        .with_cell_height(cell_height)
                        .with_display_offset(display_offset)
                        .with_layout_offset_y(layout_offset_y);
                }

                // Handle dim, inverse, and selected text
                if indexed
                    .cell
                    .flags
                    .intersects(cell::Flags::DIM | cell::Flags::DIM_BOLD)
                {
                    fg.a *= 0.7;
                }
                if indexed.cell.flags.contains(cell::Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if selected {
                    let selection = theme.selection_colors();
                    (fg, bg) = (
                        selection.foreground.on(fg, bg),
                        selection.background.on(fg, bg),
                    );
                }
                if attributes.faint {
                    fg.a *= 0.7;
                }
                if attributes.reverse {
                    std::mem::swap(&mut fg, &mut bg);
                }

                // Batch draw backgrounds: skip default background (container already paints it)
                if bg != default_bg {
                    if bg_batch_rect.can_extend(bg, x) {
                        // Same color and contiguous: extend current run
                        bg_batch_rect.extend(cell_width);
                    } else {
                        // New colored run (or non-contiguous): flush previous run if any
                        if bg_batch_rect.can_flush() {
                            frame.fill(
                                &bg_batch_rect.build(line),
                                bg_batch_rect.color,
                            );
                        }

                        // Start a new run but do not draw yet; wait for potential extensions
                        bg_batch_rect = BackgroundRect::default()
                            .with_cell_height(cell_height)
                            .with_display_offset(display_offset)
                            .with_layout_offset_y(layout_offset_y)
                            .activate()
                            .with_color(bg)
                            .with_start_x(x)
                            .with_width(cell_width);
                    }
                } else if bg_batch_rect.can_flush() {
                    // Background returns to default, flush current background rect and init the new one
                    frame.fill(&bg_batch_rect.build(line), bg_batch_rect.color);

                    bg_batch_rect = BackgroundRect::default()
                        .with_cell_height(cell_height)
                        .with_display_offset(display_offset)
                        .with_layout_offset_y(layout_offset_y);
                }

                // Draw hovered hyperlink underline (rare; keep per-cell for correctness)
                if content.hovered_hyperlink.as_ref().is_some_and(|range| {
                    range.contains(&indexed.point)
                        && range.contains(&state.mouse_position_on_grid)
                }) || indexed.cell.flags.contains(cell::Flags::UNDERLINE)
                    || attributes.underline
                {
                    // Inside the cell, where the next line's background
                    // doesn't cover it.
                    let width = font_size * 0.15;
                    let underline_height = y + cell_size.height - width / 2.;
                    let underline = Path::line(
                        Point::new(x, underline_height),
                        Point::new(x + cell_size.width, underline_height),
                    );
                    lines.push((underline, width, fg));
                }

                if flags.contains(cell::Flags::STRIKEOUT)
                    || attributes.strikethrough
                {
                    let middle = y + cell_size.height / 2.;
                    let strikethrough = Path::line(
                        Point::new(x, middle),
                        Point::new(x + cell_size.width, middle),
                    );
                    lines.push((strikethrough, font_size * 0.08, fg));
                }

                // Handle cursor rendering
                let is_cursor = content.grid.cursor.point == indexed.point;
                if is_cursor {
                    // An application-set cursor color (OSC 12) wins;
                    // otherwise the theme's, on the cell's colors.
                    let cursor_color = match overrides[NamedColor::Cursor] {
                        Some(rgb) => Color::from_rgb8(rgb.r, rgb.g, rgb.b),
                        None => theme.cursor_colors().background.on(
                            theme.get_color_with_overrides(
                                content.cursor.fg,
                                overrides,
                            ),
                            cell_bg,
                        ),
                    };
                    for rectangle in cursor_rectangles(
                        cursor_shape,
                        Point::new(x, y),
                        cell_size,
                        cursor_thickness,
                    ) {
                        cursor_rects.push((
                            rectangle.position(),
                            rectangle.size(),
                            cursor_color,
                        ));
                    }
                }

                // Draw text
                if indexed.c != ' ' && indexed.c != '\t' {
                    // The glyph under the block cursor takes the theme's
                    // cursor text color, on the cell's own colors before
                    // INVERSE and the selection, so it contrasts with the
                    // cursor rect. Using the post-swap colors — or gating
                    // this on APP_CURSOR, a keypad mode unrelated to
                    // rendering — made the glyph invisible whenever the cell
                    // was INVERSE or inside a selection.
                    // The other cursor shapes leave the glyph as it is.
                    if is_cursor && cursor_shape == CursorShape::Block {
                        fg = theme
                            .cursor_colors()
                            .foreground
                            .on(cell_fg, cell_bg);
                    }
                    if self.term.font.builtin_box_drawing
                        && box_drawing::is_builtin(indexed.c)
                    {
                        // On whole pixels, so the shapes of neighbouring
                        // cells meet without a seam.
                        let origin = Point::new(x.round(), y.round());
                        let mut opacity = 0.;
                        let glyph = Path::new(|path| {
                            opacity = box_drawing::draw(
                                indexed.c, cell_size, origin, path,
                            )
                            .unwrap_or(0.);
                        });
                        builtin_glyphs.push((glyph, fg.scale_alpha(opacity)));
                        continue;
                    }
                    let font = self.term.font.face(
                        flags.intersects(
                            cell::Flags::BOLD | cell::Flags::DIM_BOLD,
                        ) || attributes.bold,
                        flags.contains(cell::Flags::ITALIC)
                            || attributes.italic,
                    );
                    let glyph_offset = self.term.font.glyph_offset;
                    let text = Text {
                        content: indexed.cell.c.to_string(),
                        position: Point::new(
                            cell_center_x + glyph_offset.x,
                            cell_center_y + glyph_offset.y,
                        ),
                        font,
                        size: iced_core::Pixels(font_size),
                        color: fg,
                        align_x: Alignment::Center,
                        align_y: Vertical::Center,
                        shaping: Shaping::Advanced,
                        line_height: LineHeight::Relative(font_scale_factor),
                        ..Default::default()
                    };
                    frame.fill_text(text);
                }
            }

            // Flush any remaining background run at the end
            if bg_batch_rect.can_flush() {
                frame.fill(
                    &bg_batch_rect.build(last_line.unwrap_or(0)),
                    bg_batch_rect.color,
                );
            }

            for (line, width, color) in &lines {
                frame.stroke(
                    line,
                    Stroke::default().with_width(*width).with_color(*color),
                );
            }
            for (position, size, color) in cursor_rects {
                frame.fill_rectangle(position, size, color);
            }
            for (glyph, color) in &builtin_glyphs {
                frame.fill(glyph, *color);
            }
        });

        use iced::advanced::graphics::geometry::Renderer as _;
        renderer.draw_geometry(geom);
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced_core::Event,
        layout: iced_graphics::core::Layout<'_>,
        cursor: Cursor,
        _renderer: &iced::Renderer,
        clipboard: &mut dyn iced_graphics::core::Clipboard,
        shell: &mut iced_graphics::core::Shell<'_, Event>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<TerminalViewState>();
        self.handle_resize(state, layout, shell);

        if let iced::Event::Mouse(mouse::Event::CursorMoved { .. }) = event {
            state.pointer_hidden = false;
        }

        let is_cursor_in_layout = self.is_cursor_in_layout(cursor, layout);
        self.handle_focus(event, state, is_cursor_in_layout);

        // Releasing the left button ends a selection, wherever the pointer
        // is by then, unless the drag was reported to the application.
        let released_left = matches!(
            event,
            iced::Event::Mouse(mouse::Event::ButtonReleased(
                mouse::Button::Left
            ))
        );
        let selection_finished = released_left
            && state.is_dragged
            && !self
                .term
                .backend
                .renderable_content()
                .terminal_mode
                .intersects(TermMode::MOUSE_MODE);

        let commands = match event {
            iced::Event::Mouse(mouse_event) if is_cursor_in_layout => self
                .handle_mouse_event(
                    state,
                    clipboard,
                    self.grid_origin(layout),
                    cursor.position().unwrap(),
                    mouse_event,
                ),
            iced::Event::Mouse(_) if released_left => {
                state.is_dragged = false;
                Vec::new()
            },
            iced::Event::Keyboard(keyboard_event) => {
                if !state.is_focused() {
                    return;
                }

                let commands: Vec<_> = self
                    .handle_keyboard_event(state, clipboard, keyboard_event)
                    .into_iter()
                    .collect();
                if self.term.mouse.hide_when_typing
                    && commands
                        .iter()
                        .any(|cmd| matches!(cmd, Command::Write(_)))
                {
                    state.pointer_hidden = true;
                }
                commands
            },
            _ => Vec::new(),
        };
        let selection_finished = selection_finished
            || commands
                .iter()
                .any(|cmd| matches!(cmd, Command::ExpandSelection(..)));

        if !commands.is_empty() {
            shell.capture_event();
        }

        for cmd in commands {
            shell.publish(Event::BackendCall(self.term.id, cmd));
        }

        if selection_finished {
            shell.publish(Event::SelectionFinished(self.term.id));
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: iced_core::Layout<'_>,
        cursor: iced_core::mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> iced_core::mouse::Interaction {
        let state = tree.state.downcast_ref::<TerminalViewState>();
        if state.pointer_hidden && self.is_cursor_in_layout(cursor, layout) {
            return iced_core::mouse::Interaction::Hidden;
        }

        let mut cursor_mode = iced_core::mouse::Interaction::Idle;
        let terminal_mode =
            self.term.backend.renderable_content().terminal_mode;
        if self.is_cursor_in_layout(cursor, layout)
            && !terminal_mode.contains(TermMode::SGR_MOUSE)
        {
            cursor_mode = iced_core::mouse::Interaction::Text;
        }

        if self.is_cursor_hovered_hyperlink(state) {
            cursor_mode = iced_core::mouse::Interaction::Pointer;
        }

        cursor_mode
    }
}

impl<'a> From<TerminalView<'a>> for Element<'a, Event, Theme, iced::Renderer> {
    fn from(widget: TerminalView<'a>) -> Self {
        Self::new(widget)
    }
}

#[derive(Debug, Clone)]
struct TerminalViewState {
    focus: bool,
    is_dragged: bool,
    last_click: Option<mouse::Click>,
    scroll_pixels: f32,
    keyboard_modifiers: Modifiers,
    size: Size<f32>,
    mouse_position_on_grid: TerminalGridPoint,
    terminal_id: u64,
    /// The pointer was hidden for typing and the mouse hasn't moved since.
    pointer_hidden: bool,
}

impl TerminalViewState {
    fn new(terminal_id: u64) -> Self {
        Self {
            focus: false,
            is_dragged: false,
            last_click: None,
            scroll_pixels: 0.0,
            keyboard_modifiers: Modifiers::empty(),
            size: Size::from([0.0, 0.0]),
            mouse_position_on_grid: TerminalGridPoint::default(),
            terminal_id,
            pointer_hidden: false,
        }
    }
}

impl operation::Focusable for TerminalViewState {
    fn is_focused(&self) -> bool {
        self.focus
    }

    fn focus(&mut self) {
        self.focus = true;
    }

    fn unfocus(&mut self) {
        self.focus = false;
    }
}

#[derive(Default)]
struct BackgroundRect {
    display_offset: f32,
    cell_height: f32,
    layout_offset_y: f32,
    is_active: bool,
    color: Color,
    start_x: f32,
    width: f32,
}

impl BackgroundRect {
    fn with_display_offset(mut self, value: f32) -> Self {
        self.display_offset = value;
        self
    }

    fn with_cell_height(mut self, value: f32) -> Self {
        self.cell_height = value;
        self
    }

    fn with_layout_offset_y(mut self, value: f32) -> Self {
        self.layout_offset_y = value;
        self
    }

    fn with_width(mut self, value: f32) -> Self {
        self.width = value;
        self
    }

    fn with_start_x(mut self, value: f32) -> Self {
        self.start_x = value;
        self
    }

    fn with_color(mut self, value: Color) -> Self {
        self.color = value;
        self
    }

    fn activate(mut self) -> Self {
        self.is_active = true;
        self
    }

    fn build(&self, line: i32) -> Path {
        let flush_y = self.layout_offset_y
            + ((line as f32 + self.display_offset) * self.cell_height);
        Path::rectangle(
            Point::new(self.start_x, flush_y),
            Size::new(self.width, self.cell_height),
        )
    }

    fn can_flush(&self) -> bool {
        self.is_active && self.width > 0.0
    }

    fn can_extend(&self, bg: Color, x: f32) -> bool {
        self.is_active
            && bg == self.color
            && (self.start_x + self.width - x).abs() < f32::EPSILON
    }

    fn extend(&mut self, value: f32) {
        self.width += value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod key_output_tests {
        use super::*;
        use iced_core::keyboard::key::Named;
        use iced_core::keyboard::Location;

        const KITTY: TermMode = TermMode::DISAMBIGUATE_ESC_CODES;
        const KITTY_EVENTS: TermMode = TermMode::DISAMBIGUATE_ESC_CODES
            .union(TermMode::REPORT_EVENT_TYPES);

        /// Run a key through `key_output` with the default bindings, the
        /// way iced reports it: `key` unmodified, `modified` with Shift
        /// applied, `text` with all modifiers applied.
        fn output(
            key: Key,
            modified: Key,
            text: Option<&str>,
            modifiers: Modifiers,
            state: KeyState,
            mode: TermMode,
        ) -> Option<KeyOutput> {
            let input = KeyInput {
                key: &key,
                modified_key: &modified,
                location: Location::Standard,
                modifiers,
                text,
                state,
            };
            key_output(&BindingsLayout::default(), modifiers, &input, mode)
        }

        fn press_named(
            named: Named,
            modifiers: Modifiers,
            mode: TermMode,
        ) -> Option<KeyOutput> {
            let text = match named {
                Named::Enter => Some("\r"),
                Named::Tab => Some("\t"),
                Named::Escape => Some("\x1b"),
                Named::Backspace => Some("\x08"),
                _ => None,
            };
            output(
                Key::Named(named),
                Key::Named(named),
                text,
                modifiers,
                KeyState::Pressed,
                mode,
            )
        }

        fn press_char(
            key: &str,
            modified: &str,
            text: &str,
            modifiers: Modifiers,
            mode: TermMode,
        ) -> Option<KeyOutput> {
            output(
                Key::Character(key.into()),
                Key::Character(modified.into()),
                Some(text),
                modifiers,
                KeyState::Pressed,
                mode,
            )
        }

        fn write(bytes: &str) -> Option<KeyOutput> {
            Some(KeyOutput::Write(bytes.as_bytes().to_vec()))
        }

        const NONE: Modifiers = Modifiers::empty();

        #[test]
        fn legacy_encodings() {
            let mode = TermMode::empty();
            assert_eq!(press_named(Named::Escape, NONE, mode), write("\x1b"));
            assert_eq!(press_named(Named::Enter, NONE, mode), write("\r"));
            assert_eq!(press_named(Named::Tab, NONE, mode), write("\t"));
            assert_eq!(
                press_named(Named::Backspace, NONE, mode),
                write("\x7f")
            );
            assert_eq!(
                press_char("i", "i", "\t", Modifiers::CTRL, mode),
                write("\t")
            );
            assert_eq!(
                press_char("a", "A", "A", Modifiers::SHIFT, mode),
                write("A")
            );
            assert_eq!(press_named(Named::F5, NONE, mode), write("\x1b[15~"));
            assert_eq!(
                press_named(Named::ArrowUp, NONE, mode),
                write("\x1b[A")
            );
            assert_eq!(
                press_named(Named::ArrowUp, Modifiers::CTRL, mode),
                write("\x1b[1;5A")
            );
            assert_eq!(
                press_named(Named::ArrowUp, Modifiers::SHIFT, mode),
                write("\x1b[1;2A")
            );
            assert_eq!(
                press_named(Named::ArrowUp, NONE, TermMode::APP_CURSOR),
                write("\x1bOA")
            );
            assert_eq!(
                press_named(Named::Tab, Modifiers::SHIFT, mode),
                write("\x1b[Z")
            );
        }

        #[test]
        fn legacy_mode_sends_nothing_on_release() {
            let release = output(
                Key::Named(Named::Escape),
                Key::Named(Named::Escape),
                None,
                NONE,
                KeyState::Released,
                TermMode::empty(),
            );
            assert_eq!(release, None);
        }

        #[test]
        fn kitty_encodings() {
            assert_eq!(
                press_named(Named::Escape, NONE, KITTY),
                write("\x1b[27u")
            );
            // Enter and Tab keep their legacy bytes.
            assert_eq!(press_named(Named::Enter, NONE, KITTY), write("\r"));
            assert_eq!(press_named(Named::Tab, NONE, KITTY), write("\t"));
            assert_eq!(
                press_named(Named::Backspace, NONE, KITTY),
                write("\x7f")
            );
            // Ctrl+I is distinguishable from Tab.
            assert_eq!(
                press_char("i", "i", "\t", Modifiers::CTRL, KITTY),
                write("\x1b[105;5u")
            );
            assert_eq!(
                press_named(Named::Tab, Modifiers::SHIFT, KITTY),
                write("\x1b[9;2u")
            );
            assert_eq!(
                press_char("a", "A", "A", Modifiers::SHIFT, KITTY),
                write("A")
            );
            assert_eq!(press_named(Named::F5, NONE, KITTY), write("\x1b[15~"));
            assert_eq!(
                press_named(Named::ArrowUp, Modifiers::CTRL, KITTY),
                write("\x1b[1;5A")
            );
            // Application cursor mode keeps SS3 for unmodified arrows.
            assert_eq!(
                press_named(Named::ArrowUp, NONE, KITTY | TermMode::APP_CURSOR),
                write("\x1bOA")
            );
        }

        #[test]
        fn kitty_release_events() {
            let release = |named: Named, mode: TermMode| {
                output(
                    Key::Named(named),
                    Key::Named(named),
                    None,
                    NONE,
                    KeyState::Released,
                    mode,
                )
            };
            assert_eq!(release(Named::Escape, KITTY), None);
            assert_eq!(
                release(Named::Escape, KITTY_EVENTS),
                write("\x1b[27;1:3u")
            );
            assert_eq!(
                release(Named::ArrowLeft, KITTY_EVENTS),
                write("\x1b[1;1:3D")
            );
            assert_eq!(release(Named::Enter, KITTY_EVENTS), None);
            assert_eq!(
                output(
                    Key::Character("a".into()),
                    Key::Character("a".into()),
                    None,
                    NONE,
                    KeyState::Released,
                    KITTY_EVENTS,
                ),
                write("\x1b[97;1:3u")
            );
            assert_eq!(
                output(
                    Key::Named(Named::ArrowUp),
                    Key::Named(Named::ArrowUp),
                    None,
                    NONE,
                    KeyState::Repeat,
                    KITTY_EVENTS,
                ),
                write("\x1b[1;1:2A")
            );
        }

        #[test]
        fn copy_paste_bindings_win_over_kitty_encoding() {
            // The platform copy/paste shortcuts.
            let (mods, c, v) = if cfg!(target_os = "macos") {
                (Modifiers::LOGO, "c", "v")
            } else {
                (Modifiers::SHIFT | Modifiers::CTRL, "C", "V")
            };
            for mode in [TermMode::empty(), KITTY, KITTY_EVENTS] {
                assert_eq!(
                    press_char("c", c, "\x03", mods, mode),
                    Some(KeyOutput::Copy)
                );
                assert_eq!(
                    press_char("v", v, "\x16", mods, mode),
                    Some(KeyOutput::Paste(ClipboardKind::Standard))
                );
                // Nor is the shortcut's release reported.
                let release = output(
                    Key::Character("c".into()),
                    Key::Character(c.into()),
                    None,
                    mods,
                    KeyState::Released,
                    mode,
                );
                assert_eq!(release, None);
            }
        }
    }

    mod paste_tests {
        use super::*;
        use iced_core::keyboard::key::Named;

        /// A clipboard with a regular and, optionally, a selection clipboard.
        struct TestClipboard {
            standard: Option<String>,
            primary: Option<String>,
        }

        impl iced_graphics::core::Clipboard for TestClipboard {
            fn read(&self, kind: ClipboardKind) -> Option<String> {
                match kind {
                    ClipboardKind::Standard => self.standard.clone(),
                    ClipboardKind::Primary => self.primary.clone(),
                }
            }

            fn write(&mut self, kind: ClipboardKind, contents: String) {
                match kind {
                    ClipboardKind::Standard => self.standard = Some(contents),
                    ClipboardKind::Primary => self.primary = Some(contents),
                }
            }
        }

        fn pasted(command: Option<Command>) -> Option<String> {
            match command {
                Some(Command::Paste(text)) => Some(text),
                Some(command) => panic!("not a paste: {command:?}"),
                None => None,
            }
        }

        #[test]
        fn pastes_the_requested_clipboard() {
            let mut clipboard = TestClipboard {
                standard: Some("copied".into()),
                primary: Some("selected".into()),
            };

            let standard = paste(&mut clipboard, ClipboardKind::Standard);
            assert_eq!(pasted(standard).as_deref(), Some("copied"));
            let primary = paste(&mut clipboard, ClipboardKind::Primary);
            assert_eq!(pasted(primary).as_deref(), Some("selected"));
        }

        #[test]
        fn regular_clipboard_stands_in_for_a_missing_selection_clipboard() {
            let mut clipboard = TestClipboard {
                standard: Some("copied".into()),
                primary: None,
            };

            let primary = paste(&mut clipboard, ClipboardKind::Primary);
            assert_eq!(pasted(primary).as_deref(), Some("copied"));
        }

        #[test]
        fn empty_clipboards_paste_nothing() {
            let mut clipboard = TestClipboard {
                standard: Some(String::new()),
                primary: None,
            };

            assert!(paste(&mut clipboard, ClipboardKind::Standard).is_none());
            assert!(paste(&mut clipboard, ClipboardKind::Primary).is_none());
        }

        #[cfg(not(target_os = "macos"))]
        #[test]
        fn shift_insert_pastes_the_selection() {
            let input = KeyInput {
                key: &Key::Named(Named::Insert),
                modified_key: &Key::Named(Named::Insert),
                location: iced_core::keyboard::Location::Standard,
                modifiers: Modifiers::SHIFT,
                text: None,
                state: KeyState::Pressed,
            };

            let output = key_output(
                &BindingsLayout::default(),
                Modifiers::SHIFT,
                &input,
                TermMode::empty(),
            );
            assert_eq!(output, Some(KeyOutput::Paste(ClipboardKind::Primary)));
        }

        #[test]
        fn middle_click_pastes_the_selection_by_default() {
            let bindings = BindingsLayout::default();
            let paste = |button, modifiers, mode| {
                mouse_paste(&bindings, button, modifiers, mode)
            };
            let none = Modifiers::empty();

            assert_eq!(
                paste(mouse::Button::Middle, none, TermMode::empty()),
                Some(ClipboardKind::Primary)
            );
            assert_eq!(
                paste(mouse::Button::Right, none, TermMode::empty()),
                None
            );
        }

        #[test]
        fn mouse_paste_follows_added_bindings() {
            let mut bindings = BindingsLayout::default();
            bindings.add_bindings(vec![(
                crate::bindings::Binding {
                    target: InputKind::Mouse(mouse::Button::Right),
                    modifiers: Modifiers::empty(),
                    terminal_mode_include: TermMode::empty(),
                    terminal_mode_exclude: TermMode::empty(),
                },
                BindingAction::Paste,
            )]);

            assert_eq!(
                mouse_paste(
                    &bindings,
                    mouse::Button::Right,
                    Modifiers::empty(),
                    TermMode::empty()
                ),
                Some(ClipboardKind::Standard)
            );
        }

        #[test]
        fn clicks_belong_to_applications_reporting_the_mouse() {
            let bindings = BindingsLayout::default();
            let mode = TermMode::MOUSE_REPORT_CLICK;
            let paste = |modifiers| {
                mouse_paste(&bindings, mouse::Button::Middle, modifiers, mode)
            };

            assert_eq!(paste(Modifiers::empty()), None);
            // Shift takes the click back from the application.
            assert_eq!(paste(Modifiers::SHIFT), Some(ClipboardKind::Primary));
        }
    }

    mod handle_left_button_pressed_tests {
        use super::*;
        use tankterm_terminal::index::{Column, Line};

        #[test]
        fn handles_mouse_mode_with_left_click() {
            let mut state = TerminalViewState::new(0);
            let terminal_mode = TermMode::MOUSE_MODE;
            let layout_position = Point { x: 5.0, y: 5.0 };
            let cursor_position = Point { x: 100.0, y: 150.0 };
            let mut commands = Vec::new();
            let _modifiers = Modifiers::empty();

            TerminalView::handle_left_button_pressed(
                &mut state,
                &terminal_mode,
                cursor_position,
                layout_position,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(
                commands[0],
                Command::MouseReport(
                    MouseButton::LeftButton,
                    _modifiers,
                    TerminalGridPoint {
                        line: Line(0),
                        column: Column(0),
                    },
                    true,
                )
            ));
            assert!(state.is_dragged);
        }

        #[test]
        fn starts_simple_selection_with_left_click() {
            let terminal_mode = TermMode::SGR_MOUSE;
            let cursor_position = Point { x: 200.0, y: 150.0 };
            let layout_position = Point { x: 50.0, y: 50.0 };

            let cases = vec![
                SelectionType::Simple,
                SelectionType::Semantic,
                SelectionType::Lines,
            ];

            for _selection_type in cases {
                let mut state = TerminalViewState::new(0);
                state.keyboard_modifiers = Modifiers::SHIFT;
                let mut commands = Vec::new();

                TerminalView::handle_left_button_pressed(
                    &mut state,
                    &terminal_mode,
                    cursor_position,
                    layout_position,
                    &mut commands,
                );

                assert_eq!(commands.len(), 1);
                assert!(matches!(
                    commands[0],
                    Command::SelectStart(_selection_type, (150.0, 100.0))
                ),);
                assert!(state.is_dragged);
            }
        }
    }

    #[cfg(unix)]
    mod grid_origin_tests {
        use super::*;
        use crate::settings::{BackendSettings, Settings};
        use iced_core::layout::Node;
        use iced_core::Layout;
        use tankterm_terminal::index::{Column, Line};

        /// A terminal running `cat` that was laid out at `size`.
        fn terminal(dynamic_padding: bool, size: Size) -> Terminal {
            let settings = Settings {
                backend: BackendSettings {
                    program: "cat".into(),
                    ..BackendSettings::default()
                },
                dynamic_padding,
                ..Settings::default()
            };
            let mut terminal = Terminal::new(0, settings).unwrap();
            terminal.handle(crate::Command::ProxyToBackend(Command::Resize(
                Some(size),
                None,
            )));
            terminal
        }

        #[test]
        fn dynamic_padding_centres_the_grid_and_the_mouse_follows() {
            let position = Point::new(30., 20.);
            // Room for 10 by 4 cells and a few pixels more.
            let cell = terminal(false, Size::new(1000., 1000.))
                .backend
                .renderable_content()
                .terminal_size;
            let (width, height) =
                (f32::from(cell.cell_width), f32::from(cell.cell_height));
            let size = Size::new(10. * width + 7., 4. * height + 5.);
            let node = Node::new(size).move_to(position);
            let layout = Layout::new(&node);

            // Without dynamic padding the grid starts at the layout.
            let term = terminal(false, size);
            assert_eq!(
                TerminalView { term: &term }.grid_origin(layout),
                position
            );

            let mut term = terminal(true, size);
            let origin = TerminalView { term: &term }.grid_origin(layout);
            assert_eq!(origin, Point::new(33., 22.));

            // A pointer over the middle of the cell drawn at line 1, column 2
            // points at that cell, and a click there selects from it.
            let pointer = origin + iced::Vector::new(2.5 * width, 1.5 * height);
            let mut state = TerminalViewState::new(0);
            TerminalView::handle_cursor_moved(
                &mut state,
                term.backend.renderable_content(),
                &pointer,
                origin,
                &mut Vec::new(),
            );
            assert_eq!(
                state.mouse_position_on_grid,
                TerminalGridPoint::new(Line(1), Column(2))
            );

            let mut commands = Vec::new();
            TerminalView::handle_left_button_pressed(
                &mut state,
                &TermMode::empty(),
                pointer,
                origin,
                &mut commands,
            );
            let [Command::SelectStart(_, (x, y))] = commands[..] else {
                panic!("not a selection: {commands:?}");
            };
            assert_eq!((x, y), (2.5 * width, 1.5 * height));

            // Turned off by a configuration reload, the grid is at the
            // layout again.
            term.handle(crate::Command::ChangeDynamicPadding(false));
            assert_eq!(
                TerminalView { term: &term }.grid_origin(layout),
                position
            );
        }
    }

    mod handle_cursor_moved_tests {
        use tankterm_terminal::index::{Column, Line};

        use super::*;

        #[test]
        fn updates_mouse_position_on_grid() {
            let mut state = TerminalViewState::new(0);
            let terminal_content = RenderableContent::default();
            let mut commands = Vec::new();
            let cases = vec![
                (
                    Point { x: 0.0, y: 0.0 },
                    Point { x: 1.0, y: 1.0 },
                    TerminalGridPoint {
                        line: Line(1),
                        column: Column(1),
                    },
                ),
                (
                    Point { x: 0.0, y: 0.0 },
                    Point { x: 2.0, y: 2.0 },
                    TerminalGridPoint {
                        line: Line(2),
                        column: Column(2),
                    },
                ),
                (
                    Point { x: 0.0, y: 0.0 },
                    Point { x: 30.0, y: 2.0 },
                    TerminalGridPoint {
                        line: Line(2),
                        column: Column(30),
                    },
                ),
                (
                    Point { x: 10.0, y: 0.0 },
                    Point { x: 30.0, y: 2.0 },
                    TerminalGridPoint {
                        line: Line(2),
                        column: Column(20),
                    },
                ),
                (
                    Point { x: 10.0, y: 10.0 },
                    Point { x: 30.0, y: 2.0 },
                    TerminalGridPoint {
                        line: Line(0),
                        column: Column(20),
                    },
                ),
            ];

            for (layout_position, cursor_position, expected) in cases {
                TerminalView::handle_cursor_moved(
                    &mut state,
                    &terminal_content,
                    &cursor_position,
                    layout_position,
                    &mut commands,
                );

                assert_eq!(state.mouse_position_on_grid, expected);
            }
        }

        #[test]
        fn expanding_follows_the_clicks_and_control_makes_a_block() {
            let layout_position = Point { x: 5.0, y: 5.0 };
            let at = Point { x: 100.0, y: 150.0 };
            let expand = |state: &mut TerminalViewState| {
                TerminalView::expand_selection(
                    state,
                    mouse::Button::Right,
                    at,
                    layout_position,
                )
            };
            let selection_type = |command| match command {
                Command::ExpandSelection(selection_type, (95.0, 145.0)) => {
                    selection_type
                },
                command => panic!("not an expansion: {command:?}"),
            };

            let mut state = TerminalViewState::new(0);
            assert_eq!(
                selection_type(expand(&mut state)),
                SelectionType::Simple
            );
            assert_eq!(
                selection_type(expand(&mut state)),
                SelectionType::Semantic
            );
            assert_eq!(
                selection_type(expand(&mut state)),
                SelectionType::Lines
            );

            let mut state = TerminalViewState::new(0);
            state.keyboard_modifiers = Modifiers::CTRL;
            assert_eq!(
                selection_type(expand(&mut state)),
                SelectionType::Block
            );
            assert_eq!(
                selection_type(expand(&mut state)),
                SelectionType::Block
            );
        }

        #[test]
        fn right_click_expands_the_selection_by_default() {
            let bindings = BindingsLayout::default();
            let action = |modifiers, mode| {
                mouse_action(&bindings, mouse::Button::Right, modifiers, mode)
            };

            let expand = BindingAction::ExpandSelection;
            assert_ne!(action(Modifiers::empty(), TermMode::empty()), expand);

            let mut bindings = BindingsLayout::default();
            for modifiers in [Modifiers::empty(), Modifiers::CTRL] {
                bindings.add_bindings(vec![(
                    crate::bindings::Binding {
                        target: InputKind::Mouse(mouse::Button::Right),
                        modifiers,
                        terminal_mode_include: TermMode::empty(),
                        terminal_mode_exclude: TermMode::empty(),
                    },
                    BindingAction::ExpandSelection,
                )]);
            }
            let action = |modifiers, mode| {
                mouse_action(&bindings, mouse::Button::Right, modifiers, mode)
            };
            assert_eq!(action(Modifiers::empty(), TermMode::empty()), expand);
            assert_eq!(action(Modifiers::CTRL, TermMode::empty()), expand);
            // Applications that ask for mouse reports get right clicks too.
            assert_eq!(
                action(Modifiers::empty(), TermMode::MOUSE_REPORT_CLICK),
                BindingAction::Ignore
            );
            assert_eq!(
                action(Modifiers::SHIFT, TermMode::MOUSE_REPORT_CLICK),
                expand
            );
        }

        #[test]
        fn generates_drag_update_command_when_dragged() {
            let mut state = TerminalViewState::new(0);
            state.is_dragged = true; // Simulate an ongoing drag operation
            let terminal_content = RenderableContent::default();
            let layout_position = Point { x: 5.0, y: 5.0 };
            let cursor_position = Point { x: 100.0, y: 150.0 };
            let mut commands = Vec::new();

            TerminalView::handle_cursor_moved(
                &mut state,
                &terminal_content,
                &cursor_position,
                layout_position,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(
                commands[0],
                Command::SelectUpdate((95.0, 145.0))
            ));
        }

        #[test]
        fn generates_drag_update_command_when_dragged_in_mouse_motion_mode() {
            let mut state = TerminalViewState::new(0);
            state.is_dragged = true; // Simulate an ongoing drag operation
            let terminal_content = RenderableContent {
                terminal_mode: TermMode::MOUSE_MOTION,
                ..Default::default()
            };
            let layout_position = Point { x: 5.0, y: 5.0 };
            let cursor_position = Point { x: 100.0, y: 150.0 };
            let mut commands = Vec::new();
            let _modifiers = Modifiers::empty();

            TerminalView::handle_cursor_moved(
                &mut state,
                &terminal_content,
                &cursor_position,
                layout_position,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(
                commands[0],
                Command::MouseReport(
                    MouseButton::LeftMove,
                    _modifiers,
                    TerminalGridPoint {
                        line: Line(49),
                        column: Column(79),
                    },
                    true,
                )
            ));
        }

        #[test]
        fn reports_drag_when_dragged_in_mouse_drag_mode() {
            // Button-motion tracking (1002) — what e.g. tmux with `mouse on`
            // requests — must also report drags, not fall back to the
            // widget's own selection.
            let mut state = TerminalViewState::new(0);
            state.is_dragged = true;
            let terminal_content = RenderableContent {
                terminal_mode: TermMode::MOUSE_DRAG,
                ..Default::default()
            };
            let layout_position = Point { x: 5.0, y: 5.0 };
            let cursor_position = Point { x: 100.0, y: 150.0 };
            let mut commands = Vec::new();
            let _modifiers = Modifiers::empty();

            TerminalView::handle_cursor_moved(
                &mut state,
                &terminal_content,
                &cursor_position,
                layout_position,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(
                commands[0],
                Command::MouseReport(
                    MouseButton::LeftMove,
                    _modifiers,
                    TerminalGridPoint {
                        line: Line(49),
                        column: Column(79),
                    },
                    true,
                )
            ));
        }

        #[test]
        fn generates_drag_update_command_when_dragged_in_srg_mode_with_key_mods(
        ) {
            let mut state = TerminalViewState::new(0);
            state.keyboard_modifiers = Modifiers::SHIFT;
            state.is_dragged = true; // Simulate an ongoing drag operation
            let terminal_content = RenderableContent {
                terminal_mode: TermMode::SGR_MOUSE,
                ..Default::default()
            };
            let layout_position = Point { x: 5.0, y: 5.0 };
            let cursor_position = Point { x: 100.0, y: 150.0 };
            let mut commands = Vec::new();

            TerminalView::handle_cursor_moved(
                &mut state,
                &terminal_content,
                &cursor_position,
                layout_position,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(
                commands[0],
                Command::SelectUpdate((95.0, 145.0))
            ));
        }

        #[test]
        fn generates_drag_update_and_link_open() {
            let mut state = TerminalViewState::new(0);
            state.keyboard_modifiers = Modifiers::COMMAND;
            state.is_dragged = true; // Simulate an ongoing drag operation
            let terminal_content = RenderableContent {
                terminal_mode: TermMode::SGR_MOUSE,
                ..Default::default()
            };
            let layout_position = Point { x: 5.0, y: 5.0 };
            let cursor_position = Point { x: 100.0, y: 150.0 };
            let mut commands = Vec::new();

            TerminalView::handle_cursor_moved(
                &mut state,
                &terminal_content,
                &cursor_position,
                layout_position,
                &mut commands,
            );

            assert_eq!(commands.len(), 2);
            assert!(matches!(
                commands[0],
                Command::SelectUpdate((95.0, 145.0))
            ));
            assert!(matches!(
                commands[1],
                Command::ProcessLink(
                    LinkAction::Hover,
                    TerminalGridPoint {
                        line: Line(49),
                        column: Column(79),
                    },
                )
            ));
        }
    }

    mod handle_button_released_tests {
        use super::*;
        use tankterm_terminal::index::{Column, Line};

        #[test]
        fn mouse_mode_activated() {
            let mut state = TerminalViewState::new(0);
            let terminal_mode = TermMode::MOUSE_MODE;
            let bindings = BindingsLayout::new();
            let mut commands = Vec::new();
            let _modifiers = Modifiers::empty();

            TerminalView::handle_button_released(
                &mut state,
                &terminal_mode,
                &bindings,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(
                commands[0],
                Command::MouseReport(
                    MouseButton::LeftButton,
                    _modifiers,
                    TerminalGridPoint {
                        line: Line(0),
                        column: Column(0)
                    },
                    false
                )
            ));
        }

        #[test]
        fn link_open_on_button_release() {
            let mut state = TerminalViewState::new(0);
            state.keyboard_modifiers = Modifiers::COMMAND;
            let terminal_mode = TermMode::MOUSE_MODE;
            let bindings = BindingsLayout::new();
            let mut commands = Vec::new();
            let _modifiers = Modifiers::empty();

            TerminalView::handle_button_released(
                &mut state,
                &terminal_mode,
                &bindings,
                &mut commands,
            );

            assert_eq!(commands.len(), 2);
            assert!(matches!(
                commands[0],
                Command::MouseReport(
                    MouseButton::LeftButton,
                    _modifiers,
                    TerminalGridPoint {
                        line: Line(0),
                        column: Column(0)
                    },
                    false
                )
            ));
            assert!(matches!(
                commands[1],
                Command::ProcessLink(
                    LinkAction::Open,
                    TerminalGridPoint {
                        line: Line(0),
                        column: Column(0)
                    }
                ),
            ));
        }

        #[test]
        fn link_open_on_button_release_in_non_mouse_mode() {
            let mut state = TerminalViewState::new(0);
            state.keyboard_modifiers = Modifiers::COMMAND;
            state.mouse_position_on_grid = TerminalGridPoint {
                line: Line(4),
                column: Column(10),
            };
            let terminal_mode = TermMode::empty(); // Assume SGR_MOUSE mode doesn't affect link opening
            let bindings = BindingsLayout::new();
            let mut commands = Vec::new();

            TerminalView::handle_button_released(
                &mut state,
                &terminal_mode,
                &bindings,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(
                commands[0],
                Command::ProcessLink(
                    LinkAction::Open,
                    TerminalGridPoint {
                        line: Line(4),
                        column: Column(10)
                    }
                ),
            ));
        }
    }

    mod handle_wheel_scrolled_tests {
        use super::*;
        use crate::font::TermFont;
        use crate::settings::FontSettings;

        #[test]
        fn scroll_with_lines_downward() {
            let mut state = TerminalViewState::new(0);
            let font = TermFont::new(FontSettings::default());
            let mut commands = Vec::new();

            TerminalView::handle_wheel_scrolled(
                &mut state,
                ScrollDelta::Lines { y: 3.0, x: 0.0 }, // Scroll down 3 lines
                &font.measure,
                1.0,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(commands[0], Command::Scroll(3)));
        }

        #[test]
        fn scroll_with_lines_upward() {
            let mut state = TerminalViewState::new(0);
            let font = TermFont::new(FontSettings::default());
            let mut commands = Vec::new();

            TerminalView::handle_wheel_scrolled(
                &mut state,
                ScrollDelta::Lines { y: -2.0, x: 0.0 },
                &font.measure,
                1.0,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(commands[0], Command::Scroll(-2)));
        }

        #[test]
        fn scroll_with_pixels_accumulating_downward() {
            let mut state = TerminalViewState::new(0);
            let font = TermFont::new(FontSettings::default());
            let mut commands = Vec::new();

            TerminalView::handle_wheel_scrolled(
                &mut state,
                ScrollDelta::Pixels { y: 45.0, x: 0.0 },
                &font.measure,
                1.0,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(commands[0], Command::Scroll(2)));
            assert_eq!(state.scroll_pixels, 8.600002);
        }

        #[test]
        fn scroll_with_pixels_accumulating_upward() {
            let mut state = TerminalViewState::new(0);
            let font = TermFont::new(FontSettings::default());
            let mut commands = Vec::new();

            TerminalView::handle_wheel_scrolled(
                &mut state,
                ScrollDelta::Pixels { y: -60.0, x: 0.0 },
                &font.measure,
                1.0,
                &mut commands,
            );

            assert_eq!(commands.len(), 1);
            assert!(matches!(commands[0], Command::Scroll(-3)));
            assert_eq!(state.scroll_pixels, -5.4000034);
        }

        #[test]
        fn wheel_steps_scroll_the_multiplier_in_lines() {
            let font = TermFont::new(FontSettings::default());
            let scrolled = |delta, multiplier| {
                let mut state = TerminalViewState::new(0);
                let mut commands = Vec::new();
                TerminalView::handle_wheel_scrolled(
                    &mut state,
                    delta,
                    &font.measure,
                    multiplier,
                    &mut commands,
                );
                commands
            };

            let step = |y| ScrollDelta::Lines { y, x: 0.0 };
            assert!(matches!(
                scrolled(step(1.0), 3.0)[..],
                [Command::Scroll(3)]
            ));
            assert!(matches!(
                scrolled(step(-2.0), 3.0)[..],
                [Command::Scroll(-6)]
            ));
            // Part of a step still scrolls if the multiplier makes a line
            // of it.
            assert!(matches!(
                scrolled(step(0.5), 3.0)[..],
                [Command::Scroll(2)]
            ));
            assert!(scrolled(step(0.2), 1.0).is_empty());
            assert!(scrolled(step(1.0), 0.0).is_empty());

            // A touchpad's pixels are multiplied as well.
            let pixels = ScrollDelta::Pixels {
                y: font.measure.height,
                x: 0.0,
            };
            assert!(matches!(scrolled(pixels, 1.0)[..], [Command::Scroll(1)]));
            assert!(matches!(scrolled(pixels, 3.0)[..], [Command::Scroll(3)]));
        }
    }
}
