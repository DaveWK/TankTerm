//! What the application draws: the tab bar, the panes of the shown tab and what lies over them.

use iced::alignment::Vertical;
use iced::font::{Style as FontStyle, Weight};
use iced::widget::pane_grid::{self, PaneGrid};
use iced::widget::text::{Rich, Wrapping};
use iced::widget::{
    button, column, container, hover, keyed_column, mouse_area, row, span, stack, text,
};
use iced::{Border, Color, Element, Font, Length, Padding};
use iced_tterm::TerminalView;

use super::{App, Layout, MAX_TAB_WIDTH, Message, Shell};
use crate::color::Rgb;
use crate::config::BindingMode;
use crate::config::style::{Border as StyleBorder, TabStyle};
use crate::config::tabs::{ShowTabs, TabsPosition};
use crate::config::window::Spacing;
use crate::hotkeys::{AppAction, KeyFilter, PANE_LABELS};
use crate::palette::{self, blend, paste_warning};
use crate::session::ordered_panes;

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

        let show_tabs = match self.options.tabs.show {
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
            .push(self.pending_paste.as_ref().map(|paste| paste_warning(&paste.text, appearance)));

        // The paste warning and the pane labels are answered with a key; nothing typed meanwhile
        // reaches a terminal. Key tables only apply to the regular bindings.
        let (hotkeys, tables): (_, &[_]) =
            match (&self.pending_paste, &self.palette, &self.pane_select) {
                (Some(_), ..) => (&self.paste_keys, &[]),
                (None, Some(_), _) => (&self.palette_keys, &[]),
                (None, None, Some(_)) => (&self.pane_keys, &[]),
                (None, None, None) => (&self.options.hotkeys, &self.key_tables),
            };

        // Bindings can be limited to modes of the terminal the keys are typed into.
        let mode = self.focused_shell().map(|shell| shell.terminal.mode()).unwrap_or_default();

        KeyFilter::new(layers, hotkeys, Message::Action)
            .exclusive(self.pending_paste.is_some() || self.pane_select.is_some())
            .mode(BindingMode::new(&mode, false))
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
    /// margin is ignored, and its padding moves the content but doesn't resize the tab.
    fn tab_bar(&self) -> Element<'_, Message> {
        let [focused, blurred] = self.options.layout.tab_bar;
        let styles = if self.window_focused { focused } else { blurred };
        let font = self.settings.font.font_type;
        let size = self.settings.font.size;

        let tabs = self.tabs.iter().enumerate().map(|(index, tab)| {
            let attention = tab
                .panes
                .iter()
                .any(|(_, id)| self.terminals.get(id).is_some_and(|shell| shell.attention));
            let marker = match (attention, tab.activity) {
                (true, _) => " !",
                (false, true) => " *",
                (false, false) => "",
            };
            let title = format!("{}: {}{marker}", index + 1, self.tab_title(tab));

            let style = if index == self.active { styles.active } else { styles.inactive };
            let style = match (attention, tab.activity) {
                (true, _) => style.with(&styles.attention),
                (false, true) => style.with(&styles.activity),
                (false, false) => style,
            };

            let view = |style: TabStyle, height: Length| -> Element<'_, Message> {
                let (foreground, _) = style.colors();
                let title = container(label(title.clone(), &style, font, size)).clip(true);
                let close = button(text("\u{d7}").font(font).size(size).color(color(foreground)))
                    .padding(Padding::ZERO.left(8))
                    .style(|_, _| button::Style::default())
                    .on_press(Message::CloseTab(index));

                let content = row![title.width(Length::Fill), close].align_y(Vertical::Center);
                let select = button(content)
                    .padding(padding(style.padding))
                    .width(Length::Fill)
                    .height(height)
                    .style(move |_, _| button_style(&style))
                    .on_press(Message::Action(AppAction::SelectTab(index)));

                // Like in browsers, a middle click closes the tab.
                mouse_area(select).on_middle_press(Message::CloseTab(index)).into()
            };
            let hovered = view(style.with(&styles.hovered), Length::Fill);
            container(hoverable(view(style, Length::Shrink), hovered))
                .padding(padding(style.margin))
                .width(Length::FillPortion(1))
                .max_width(MAX_TAB_WIDTH)
                .into()
        });

        // The status segments, each drawn while it has something to show.
        let status = self.options.status.iter().filter_map(|segment| {
            let content = self.segment_text(segment)?;
            let style = styles.status.with(&segment.layer);
            let segment = container(label(content, &style, font, size))
                .padding(padding(style.padding))
                .style(move |_| container::Style {
                    background: Some(color(style.colors().1).into()),
                    border: border(&style),
                    ..Default::default()
                });
            Some(container(segment).padding(padding(style.margin)).into())
        });
        let status = row(status).spacing(1).align_y(Vertical::Center);

        // A button on the bar. Content that is as high as the padding leaves the other buttons
        // gets only the horizontal padding.
        let bar_button = |content: &dyn Fn(&TabStyle) -> Element<'static, Message>,
                          vertical_padding: bool,
                          action: AppAction| {
            let view = |style: TabStyle, height: Length| -> Element<'static, Message> {
                let mut spacing = padding(style.padding);
                if !vertical_padding {
                    spacing = spacing.top(0).bottom(0);
                }
                button(content(&style))
                    .padding(spacing)
                    .height(height)
                    .style(move |_, _| button_style(&style))
                    .on_press(Message::Action(action.clone()))
                    .into()
            };
            let style = styles.button;
            let hovered = view(style.with(&styles.hovered), Length::Fill);
            container(hoverable(view(style, Length::Shrink), hovered))
                .padding(padding(style.margin))
        };
        let new_tab =
            bar_button(&|style| label("+".into(), style, font, size), true, AppAction::NewTab);
        // The gear at the right end opens the configuration file. The font draws it small, so
        // it is larger, in the height the padding leaves the other buttons.
        let gear = |style: &TabStyle| -> Element<'static, Message> {
            let height = size * 1.3 + f32::from(style.padding.top + style.padding.bottom);
            text("\u{2699}")
                .font(font)
                .size(size * 1.3)
                .height(height)
                .color(color(style.colors().0))
                .center()
                .into()
        };
        let settings = bar_button(&gear, false, AppAction::OpenConfig);

        let tabs = row(tabs).push(new_tab).spacing(1).align_y(Vertical::Center);
        let tabs = container(tabs).width(Length::Fill);
        let bar = styles.bar;
        container(row![tabs, status, settings].spacing(1).align_y(Vertical::Center))
            .width(Length::Fill)
            .padding(padding(bar.padding))
            .style(move |_| container::Style {
                background: Some(color(bar.colors().1).into()),
                border: border(&bar),
                ..Default::default()
            })
            .into()
    }
}

/// `normal`, drawn as `hovered` while the mouse is over it.
///
/// `hovered` is laid out in the place of `normal`, so it has the same size, and it gets the
/// mouse first.
fn hoverable<'a>(
    normal: Element<'a, Message>,
    hovered: Element<'a, Message>,
) -> Element<'a, Message> {
    hover(normal, container(hovered).width(Length::Fill).height(Length::Fill))
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
    let (foreground, background) = style.colors();
    button::Style {
        background: Some(color(background).into()),
        text_color: color(foreground),
        border: border(style),
        ..Default::default()
    }
}

fn border(style: &TabStyle) -> Border {
    let (width, radius) = match style.border {
        StyleBorder::Hidden => (0., 0.),
        StyleBorder::Normal => (1., 0.),
        StyleBorder::Rounded => (1., 6.),
        StyleBorder::Thick => (2., 0.),
    };
    Border { color: color(style.border_foreground), width, radius: radius.into() }
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
