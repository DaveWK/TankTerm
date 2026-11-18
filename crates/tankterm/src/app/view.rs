//! What the application draws: the tab bar, the panes of the shown tab and what lies over them.

use iced::alignment::Vertical;
use iced::font::{Style as FontStyle, Weight};
use iced::mouse::Interaction;
use iced::widget::pane_grid::{self, PaneGrid};
use iced::widget::text::{Rich, Wrapping};
use iced::widget::{
    Space, button, column, container, keyed_column, mouse_area, row, span, stack, text,
};
use iced::window::Direction;
use iced::{Background, Border, Color, Element, Font, Length, Padding};
use iced_tterm::TerminalView;
use unicode_width::UnicodeWidthStr;

use super::hover_swap::HoverSwap;
use super::icons::{self, Icon, close_tab_id};
use super::tab_border::{BoxBorder, bordered};
use super::tab_strip::{TabStrip, TabWidth};
use super::{App, Layout, MAX_TAB_WIDTH, Message, Shell, TitleDrag};
use crate::color::Rgb;
use crate::config::BindingMode;
use crate::config::style::{Fill, TabStyle};
use crate::config::tabs::{CloseButton, ShowTabs, Side, TabSizing, TabsPosition};
use crate::config::window::Spacing;
use crate::hotkeys::{AppAction, KeyFilter, PANE_LABELS};
use crate::palette::{self, blend, paste_warning};
use crate::session::ordered_panes;
use crate::title_buttons::{TitleButton, TitleButtons};

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let Some(tab) = self.tabs.get(self.active) else {
            // The last tab was closed and the window is about to close.
            return container("").into();
        };

        // Frame the panes of a split tab, to show which one has focus.
        let framed = tab.panes.len() > 1 && tab.panes.maximized().is_none();
        // While a pane is chosen by its label, every pane shows its label.
        let order = self.pane_select.as_ref().map(|_| ordered_panes(tab.panes.layout()));
        let label = |pane: pane_grid::Pane| {
            let position = order.as_ref()?.iter().position(|ordered| *ordered == pane)?;
            PANE_LABELS.chars().nth(position)
        };
        let panes = PaneGrid::new(&tab.panes, |pane, terminal, _| {
            let content = self.pane(*terminal, framed, pane == tab.focus, label(pane));
            pane_grid::Content::new(content)
        })
        .on_click(Message::PaneClicked)
        .on_resize(6, Message::PaneResized);

        // Keyed by tab, so the widget state of one tab isn't reused for another.
        let terminals =
            keyed_column([(tab.id, Element::from(panes))]).width(Length::Fill).height(Length::Fill);

        // A tab bar that is the title bar is always there.
        let show_tabs = match self.options.tabs.show {
            _ if self.title_buttons().is_some() => true,
            ShowTabs::Always => true,
            ShowTabs::Multiple => self.tabs.len() > 1,
            ShowTabs::Never => false,
        };

        let content: Element<'_, Message> = match (show_tabs, self.options.tabs.position) {
            (false, _) => terminals.into(),
            (true, TabsPosition::Top) => column![self.tab_bar(), terminals].into(),
            (true, TabsPosition::Bottom) => column![terminals, self.tab_bar()].into(),
        };

        // The palette is drawn over the terminals and takes the keyboard while it is shown.
        let appearance = palette::Appearance {
            font: self.settings.font.font_type,
            size: self.settings.font.size,
            background: self.options.layout.background,
            foreground: self.options.layout.foreground,
        };
        let layers = stack![content]
            .push(self.palette.as_ref().map(|palette| palette.view(appearance)))
            .push(self.pending_paste.as_ref().map(|paste| paste_warning(&paste.text, appearance)))
            .push(self.resizable_edges().then(resize_edges));

        // The paste warning and the pane labels are answered with a key; nothing typed meanwhile
        // reaches a terminal. The key a vi mode inline search waits for is the character it
        // moves to, and keys typed while hints are labeled choose one, whatever they are bound
        // to. Key tables only apply to the regular bindings.
        let focused = self.focused_shell();
        let inline_search = focused.is_some_and(|shell| {
            shell.terminal.inline_search_pending() || shell.terminal.hint_active()
        });
        let (hotkeys, tables): (_, &[_]) =
            match (&self.pending_paste, &self.palette, &self.pane_select) {
                (Some(_), ..) => (&self.paste_keys, &[]),
                (None, Some(_), _) => (&self.palette_keys, &[]),
                (None, None, Some(_)) => (&self.pane_keys, &[]),
                (None, None, None) if inline_search => (&self.no_keys, &[]),
                (None, None, None) => (&self.options.hotkeys, &self.key_tables),
            };

        // Bindings can be limited to modes of the terminal the keys are typed into, and to its
        // search.
        let mode = focused.map(|shell| shell.terminal.mode()).unwrap_or_default();
        let search = focused.is_some_and(|shell| shell.terminal.search_active());

        KeyFilter::new(layers, hotkeys, Message::Action)
            .exclusive(self.pending_paste.is_some() || self.pane_select.is_some())
            .mode(BindingMode::new(&mode, search))
            .tables(tables)
            .into()
    }

    /// A terminal with its padding and, in a split tab, a frame showing whether it has focus.
    /// `label` is shown over it while a pane is chosen by its label.
    fn pane(
        &self,
        terminal: u64,
        framed: bool,
        focused: bool,
        label: Option<char>,
    ) -> Element<'_, Message> {
        let Layout { padding, foreground, .. } = self.options.layout;
        let Some(shell) = self.terminals.get(&terminal) else { return container("").into() };
        let background = self.pane_background(shell);

        let terminal = container(TerminalView::show(&shell.terminal).map(Message::Terminal))
            .padding(padding)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| container::Style {
                background: Some(background.into()),
                ..Default::default()
            });

        // The visual bell flashes over the terminal and its padding. It is a layer of its own,
        // so the terminal keeps its place among the widgets while the flash comes and goes.
        let flash = shell.rang.and_then(|rang| self.options.bell.flash.color(rang.elapsed()));
        let flash = flash.map(|color| {
            container("").width(Length::Fill).height(Length::Fill).style(move |_| {
                container::Style { background: Some(color.into()), ..Default::default() }
            })
        });
        let label = label.map(|label| {
            let Layout { background, foreground, .. } = self.options.layout;
            let size = self.settings.font.size * 3.;
            let badge = container(
                text(label.to_uppercase().to_string())
                    .font(self.settings.font.font_type)
                    .size(size),
            )
            .padding([4., size / 2.])
            .style(move |_| container::Style {
                background: Some(blend(background, foreground, 0.15).into()),
                text_color: Some(foreground),
                border: Border { color: foreground, width: 1., ..Border::default() },
                ..Default::default()
            });
            container(badge).center(Length::Fill)
        });
        let terminal = stack![terminal].push(flash).push(label);

        // A pane that wants to be looked at stands out from the focused one.
        let frame = match (shell.attention, focused) {
            (true, _) => foreground,
            (false, true) => blend(self.options.layout.background, foreground, 0.6),
            (false, false) => blend(self.options.layout.background, foreground, 0.15),
        };
        // The frame is a border and not a color behind the terminal, where it would show
        // through a transparent background.
        let width = if framed { 1. } else { 0. };
        container(terminal)
            .padding(width)
            .style(move |_| container::Style {
                border: Border { color: frame, width, ..Border::default() },
                ..Default::default()
            })
            .into()
    }

    /// The color behind a terminal and its padding: the one its application set (OSC 11) or
    /// else the configured one, as opaque as `window.opacity` says.
    ///
    /// The terminal widget draws no background of its own, so this is the only layer and the
    /// padding is as transparent as the terminal.
    pub(super) fn pane_background(&self, shell: &Shell) -> Color {
        let background = shell.terminal.background_override().unwrap_or(shell.background);
        Color { a: self.opacity(), ..background }
    }

    /// The tab bar: a button for every tab, each with a close button, and one to add a tab.
    ///
    /// Every part is drawn with its `[style.tab_*]` style. A tab or button under the mouse is
    /// drawn again over itself with the hovered style, in the same place: the hovered style's
    /// margin and widths are ignored, and its padding moves the content but doesn't resize the
    /// tab.
    fn tab_bar(&self) -> Element<'_, Message> {
        let [focused, blurred] = self.options.layout.tab_bar;
        let styles = if self.window_focused { focused } else { blurred };
        let font = self.settings.font.font_type;
        let size = self.settings.font.size;
        // Tab widths are in cells, which are only measured when a width is set.
        let cell = std::cell::OnceCell::new();
        let cells = |cells: Option<u16>| {
            let cells = f32::from(cells.filter(|cells| *cells > 0)?);
            Some(cells * *cell.get_or_init(|| self.settings.font.cell_size().width))
        };

        let tabs = self.tabs.iter().enumerate().map(|(index, tab)| {
            let attention = tab
                .panes
                .iter()
                .any(|(_, id)| self.terminals.get(id).is_some_and(|shell| shell.attention));
            let parts = self.tab_label_parts(index, tab);
            let title: String = parts.iter().map(|(text, _)| text.as_str()).collect();

            let style = if index == self.active { styles.active } else { styles.inactive };
            let style = match (attention, tab.activity) {
                (true, _) => style.with(&styles.attention),
                (false, true) => style.with(&styles.activity),
                (false, false) => style,
            };

            // The close button is drawn while `show_close` is set. With `Hover`, a tab not under
            // the mouse keeps its room, so the tab doesn't change width under the mouse.
            let close_button = self.options.tabs.close_button;
            let view = |style: TabStyle,
                        height: Length,
                        show_close: bool|
             -> Element<'_, Message> {
                let (foreground, _) = style.colors();
                // The tab's number is drawn in the `tab_index` style, the rest in the tab's.
                let index_style = style.with(&styles.index);
                let parts = parts.iter().map(|(text, number)| {
                    label(text.clone(), if *number { &index_style } else { &style }, font, size)
                });
                let title = container(row(parts)).clip(true);
                let close = (close_button != CloseButton::Never).then(|| {
                    let glyph = if show_close { color(foreground) } else { Color::TRANSPARENT };
                    button(icons::icon(Icon::Close, close_size(size), glyph, close_tab_id(index)))
                        .padding(Padding::ZERO.left(CLOSE_PADDING))
                        .style(|_, _| button::Style::default())
                        .on_press_maybe(show_close.then_some(Message::CloseTab(index)))
                });

                let content = row![title.width(Length::Fill)].push(close).align_y(Vertical::Center);
                let select = button(content)
                    .padding(padding(style.padding))
                    .width(Length::Fill)
                    .height(height)
                    .style(move |_, _| button_style(&style))
                    .on_press(Message::Action(AppAction::SelectTab(index)));

                // Like in browsers, a middle click closes the tab.
                mouse_area(bordered(select, &style))
                    .on_middle_press(Message::CloseTab(index))
                    .into()
            };
            let hovered = view(style.with(&styles.hovered), Length::Fill, true);
            let normal = view(style, Length::Shrink, close_button == CloseButton::Always);
            let tab = container(hoverable(normal, hovered))
                .padding(padding(style.margin))
                .width(Length::Fill);

            // Like Lip Gloss's, `width` and `max_width` leave out the margin; the strip lays the
            // tab out with it. A `max_width` narrower than the `width` wins.
            let margin = f32::from(style.margin.left + style.margin.right);
            let max_width = cells(style.max_width);
            let max = max_width.map_or(MAX_TAB_WIDTH, |width| width + margin);
            let fixed = cells(style.width).map(|width| max_width.unwrap_or(width).min(width));
            // A tab as wide as its title: the title, in cells of the terminal's font, its
            // padding and the close button, a cell and its padding, while there is one.
            let fitted = (self.options.tabs.width == TabSizing::Title).then(|| {
                let cell = *cell.get_or_init(|| self.settings.font.cell_size().width);
                let close = match self.options.tabs.close_button {
                    CloseButton::Never => 0.,
                    CloseButton::Hover | CloseButton::Always => close_size(size) + CLOSE_PADDING,
                };
                let text = title.width() as f32 + 1.;
                let padding = f32::from(style.padding.left + style.padding.right);
                (text * cell + padding + close + margin).ceil().min(max)
            });
            // A tab as wide as its title is flexible up to that width, so crowded tabs share the
            // room evenly and tabs with a width of their own keep it.
            let width =
                TabWidth { fixed: fixed.map(|width| width + margin), max: fitted.unwrap_or(max) };
            (tab.into(), width)
        });

        // The status segments at either end, each drawn while it has something to show.
        let [left, right] =
            [Side::Left, Side::Right].map(|side| self.status_row(side, styles.status));

        // A button on the bar. Content that is as high as the padding leaves the other buttons
        // gets only the horizontal padding.
        // The window's close button turns red under the mouse.
        let bar_button = |content: &dyn Fn(&TabStyle) -> Element<'static, Message>,
                          close: bool,
                          action: AppAction| {
            let view = |style: TabStyle, height: Length| -> Element<'static, Message> {
                let button = button(content(&style))
                    .padding(padding(style.padding))
                    .height(height)
                    .style(move |_, _| button_style(&style))
                    .on_press(Message::Action(action.clone()));
                bordered(button, &style)
            };
            let style = styles.button;
            let mut hovered = style.with(&styles.hovered);
            if close {
                hovered = hovered.with(&styles.close_hovered);
            }
            let hovered = view(hovered, Length::Fill);
            container(hoverable(view(style, Length::Shrink), hovered))
                .padding(padding(style.margin))
        };
        let plus = |style: &TabStyle| bar_icon(Icon::Plus, style, size);
        let new_tab = bar_button(&plus, false, AppAction::NewTab);
        // The gear opens the configuration file.
        let gear = |style: &TabStyle| bar_icon(Icon::Gear, style, size);
        let settings = bar_button(&gear, false, AppAction::OpenConfig);

        let tabs = TabStrip::new(tabs, new_tab, styles.bar.gap.into());
        let parts = left.into_iter().chain([tabs.into()]).chain(right).chain([settings.into()]);
        let mut parts: Vec<Element<'_, Message>> = parts.collect();

        // In a window without the system's title bar, the tab bar has the window's buttons at
        // either end.
        let title_bar = self.title_buttons();
        let maximized = self.maximized;
        let group = |buttons: &[TitleButton]| -> Option<Element<'_, Message>> {
            let buttons = buttons.iter().map(|&button| {
                // The app's icon opens the window menu, and isn't a button: the bar doesn't
                // take the press, so the window doesn't move.
                if button == TitleButton::Menu {
                    let icon = container(icons::app_icon((size * LINE_HEIGHT).round()))
                        .padding(padding(styles.button.padding));
                    return mouse_area(icon).on_press(Message::TitleBarMenu).into();
                }
                let action = match button {
                    TitleButton::Minimize => AppAction::Minimize,
                    TitleButton::Maximize => AppAction::ToggleMaximized,
                    TitleButton::Close => AppAction::Quit,
                    TitleButton::Menu => unreachable!("the app's icon isn't a button"),
                };
                let icon = move |style: &TabStyle| window_icon(button, maximized, style, size);
                bar_button(&icon, button == TitleButton::Close, action).into()
            });
            let buttons: Vec<Element<'_, Message>> = buttons.collect();
            (!buttons.is_empty()).then(|| row(buttons).spacing(1).align_y(Vertical::Center).into())
        };
        if let Some(TitleButtons { left, right }) = title_bar {
            parts.splice(0..0, group(left));
            // The gear keeps apart from the window buttons after it, by about two buttons.
            if !right.is_empty() {
                let padding = styles.button.padding;
                let button = size + f32::from(padding.left + padding.right);
                parts.push(Space::new().width(2. * button).into());
            }
            parts.extend(group(right));
        }

        let bar = styles.bar;
        let bar = container(row(parts).spacing(1).align_y(Vertical::Center))
            .width(Length::Fill)
            .padding(padding(bar.padding))
            .style(move |_| container::Style {
                background: Some(self.bar_background().into()),
                border: border(&bar),
                ..Default::default()
            });
        let bar = bordered(bar, &styles.bar);
        if title_bar.is_none() {
            return bar;
        }

        // The window is moved by dragging the bar where nothing else takes the mouse,
        // maximized by double-clicking it there, and its menu opens with a right click.
        let area = mouse_area(bar)
            .on_press(Message::TitleBarPressed)
            .on_release(Message::TitleBarReleased)
            .on_exit(Message::TitleBarReleased)
            .on_double_click(Message::TitleBarDoubleClicked)
            .on_right_press(Message::TitleBarMenu);
        match self.title_drag {
            TitleDrag::Released => area.into(),
            _ => area.on_move(Message::TitleBarMoved).into(),
        }
    }

    /// The color behind the tab bar: its style's background, as see-through as the terminals'.
    pub(super) fn bar_background(&self) -> Color {
        let [focused, blurred] = self.options.layout.tab_bar;
        let bar = if self.window_focused { focused } else { blurred }.bar;
        Color { a: self.opacity(), ..color(bar.colors().1) }
    }

    /// The window buttons of the tab bar, while it is the title bar: in a window opened
    /// without the system's title bar, unless `window.buttons` is `None`.
    pub(super) fn title_buttons(&self) -> Option<&TitleButtons> {
        self.options.title_buttons.as_ref().filter(|_| self.frameless)
    }

    /// Whether dragging the window's edges resizes it: a window without the system's borders
    /// that isn't maximized or fullscreen.
    fn resizable_edges(&self) -> bool {
        self.frameless && !self.maximized && !self.fullscreen
    }

    /// The status segments at one end of the tab bar, in `style` changed by each segment's own,
    /// or `None` if no segment is configured there. Segments touch, so separators can join
    /// them.
    fn status_row(&self, side: Side, style: TabStyle) -> Option<Element<'_, Message>> {
        let font = self.settings.font.font_type;
        let size = self.settings.font.size;

        let segments = self.options.status.iter().filter(|segment| segment.side == side);
        segments.clone().next()?;
        let segments = segments.filter_map(|segment| {
            let content = self.segment_text(segment)?;
            let style = style.with(&segment.layer);
            let segment = container(label(content, &style, font, size))
                .padding(padding(style.padding))
                .style(move |_| container::Style {
                    background: fill(&style),
                    border: border(&style),
                    ..Default::default()
                });
            Some(container(bordered(segment, &style)).padding(padding(style.margin)).into())
        });
        Some(row(segments).align_y(Vertical::Center).into())
    }
}

/// Space between a tab's title and its close button.
const CLOSE_PADDING: f32 = 8.;

/// How high a line of text is, in font sizes: iced's default line height.
const LINE_HEIGHT: f32 = 1.3;

/// How far from the window's border its edges can be dragged.
const EDGE: f32 = 5.;

/// How far along the border its corners reach, so they are easier to catch than the edges.
const CORNER: f32 = 16.;

/// Strips along the window's edges and corners that resize it when dragged, drawn over
/// everything else. Between them the mouse goes through to what is below.
fn resize_edges<'a>() -> Element<'a, Message> {
    let edge = |direction: Direction, width: Length, height: Length| -> Element<'a, Message> {
        let interaction = match direction {
            Direction::North | Direction::South => Interaction::ResizingVertically,
            Direction::East | Direction::West => Interaction::ResizingHorizontally,
            Direction::NorthEast | Direction::SouthWest => Interaction::ResizingDiagonallyUp,
            Direction::NorthWest | Direction::SouthEast => Interaction::ResizingDiagonallyDown,
        };
        mouse_area(Space::new().width(width).height(height))
            .interaction(interaction)
            .on_press(Message::ResizeWindow(direction))
            .into()
    };

    let (band, corner, rest) = (Length::Fixed(EDGE), Length::Fixed(CORNER), CORNER - EDGE);
    let end = |start: Direction, middle: Direction, end: Direction| {
        row![edge(start, corner, band), edge(middle, Length::Fill, band), edge(end, corner, band)]
    };
    let side = |start: Direction, middle: Direction, end: Direction| {
        column![
            edge(start, band, Length::Fixed(rest)),
            edge(middle, band, Length::Fill),
            edge(end, band, Length::Fixed(rest)),
        ]
    };

    column![
        end(Direction::NorthWest, Direction::North, Direction::NorthEast),
        row![
            side(Direction::NorthWest, Direction::West, Direction::SouthWest),
            Space::new().width(Length::Fill).height(Length::Fill),
            side(Direction::NorthEast, Direction::East, Direction::SouthEast),
        ]
        .height(Length::Fill),
        end(Direction::SouthWest, Direction::South, Direction::SouthEast),
    ]
    .into()
}

/// A window button's icon. The maximize button of a maximized window shows the restore icon,
/// two overlapping squares.
fn window_icon(
    button: TitleButton,
    maximized: bool,
    style: &TabStyle,
    size: f32,
) -> Element<'static, Message> {
    let icon = match button {
        TitleButton::Minimize => Icon::Minimize,
        TitleButton::Maximize if maximized => Icon::Restore,
        TitleButton::Maximize => Icon::Maximize,
        TitleButton::Close => Icon::Close,
        TitleButton::Menu => unreachable!("the app's icon isn't drawn as a line icon"),
    };
    bar_icon(icon, style, size)
}

/// An icon of a button on the bar, in the button's text color, as high as a line of text.
fn bar_icon(icon: Icon, style: &TabStyle, size: f32) -> Element<'static, Message> {
    let drawn = icons::icon(icon, (size * 0.75).round(), color(style.colors().0), icons::id(icon));
    container(drawn).center_x(size).center_y(size * LINE_HEIGHT).into()
}

/// How wide a tab's close icon is, for a font `size`.
fn close_size(size: f32) -> f32 {
    (size * 0.6).round()
}

/// `normal`, drawn as `hovered` while the mouse is over it, and not under it: parts of the bar
/// are see-through.
///
/// `hovered` is laid out in the place of `normal`, so it has the same size, and it gets the
/// mouse first.
fn hoverable<'a>(
    normal: Element<'a, Message>,
    hovered: Element<'a, Message>,
) -> Element<'a, Message> {
    HoverSwap::new(normal, container(hovered).width(Length::Fill).height(Length::Fill)).into()
}

/// Text in a tab bar style, on one line.
fn label<'a>(content: String, style: &TabStyle, font: Font, size: f32) -> Element<'a, Message> {
    let attributes = style.attributes;
    let font = Font {
        weight: if attributes.bold { Weight::Bold } else { font.weight },
        style: if attributes.italic { FontStyle::Italic } else { font.style },
        ..font
    };
    let foreground = color(style.colors().0);

    // Only rich text draws lines; plain text is what tests can find.
    if attributes.underline || attributes.strikethrough {
        let span =
            span(content).underline(attributes.underline).strikethrough(attributes.strikethrough);
        Rich::<(), Message>::with_spans([span])
            .font(font)
            .size(size)
            .color(foreground)
            .wrapping(Wrapping::None)
            .into()
    } else {
        text(content).font(font).size(size).color(foreground).wrapping(Wrapping::None).into()
    }
}

/// How a button of the tab bar looks in `style`, whatever the mouse does.
fn button_style(style: &TabStyle) -> button::Style {
    let (foreground, _) = style.colors();
    button::Style {
        background: fill(style),
        text_color: color(foreground),
        border: border(style),
        ..Default::default()
    }
}

/// The background of a part of the tab bar in `style`, if it has one of its own.
pub(super) fn fill(style: &TabStyle) -> Option<Background> {
    let (foreground, background) = style.colors();
    match style.fill {
        Fill::Color => Some(color(background).into()),
        Fill::Bar => None,
        Fill::Lift(percent) => {
            Some(Color { a: f32::from(percent) / 100., ..color(foreground) }.into())
        },
    }
}

/// The corners of a background in `style`. The border itself is drawn by [`bordered`].
fn border(style: &TabStyle) -> Border {
    Border { radius: BoxBorder::new(style).radius(), ..Border::default() }
}

fn padding(spacing: Spacing) -> Padding {
    Padding {
        top: spacing.top.into(),
        right: spacing.right.into(),
        bottom: spacing.bottom.into(),
        left: spacing.left.into(),
    }
}

fn color(rgb: Rgb) -> Color {
    Color::from_rgb8(rgb.r, rgb.g, rgb.b)
}
