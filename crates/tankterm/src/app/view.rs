//! What the application draws: the tab bar, the panes of the shown tab and what lies over them.

use iced::widget::pane_grid::{self, PaneGrid};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, keyed_column, mouse_area, row, stack, text};
use iced::{Border, Color, Element, Length};
use iced_tterm::TerminalView;

use super::{App, Layout, MAX_TAB_WIDTH, Message, Shell};
use crate::config::BindingMode;
use crate::config::tabs::{ShowTabs, TabsPosition};
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
    fn tab_bar(&self) -> Element<'_, Message> {
        let Layout { background, foreground, .. } = self.options.layout;
        let bar = blend(background, foreground, 0.12);
        let font = self.settings.font.font_type;
        let size = self.settings.font.size;
        let label = move |content: String| text(content).font(font).size(size);

        let tabs = self.tabs.iter().enumerate().map(|(index, tab)| {
            let active = index == self.active;
            let color = if active { foreground } else { blend(foreground, background, 0.4) };
            let style = move |hovered: bool| button::Style {
                background: Some(if active { background } else { bar }.into()),
                text_color: if hovered { foreground } else { color },
                ..Default::default()
            };

            let attention = tab
                .panes
                .iter()
                .any(|(_, id)| self.terminals.get(id).is_some_and(|shell| shell.attention));
            let marker = match (attention, tab.activity) {
                (true, _) => " !",
                (false, true) => " *",
                (false, false) => "",
            };
            let title = label(format!("{}: {}{marker}", index + 1, self.tab_title(tab)));
            let select = button(container(title.wrapping(Wrapping::None)).clip(true))
                .padding([3, 8])
                .width(Length::Fill)
                .style(move |_, status| style(status == button::Status::Hovered))
                .on_press(Message::Action(AppAction::SelectTab(index)));
            let close = button(label("\u{d7}".into()))
                .padding([3, 6])
                .style(move |_, status| style(status == button::Status::Hovered))
                .on_press(Message::CloseTab(index));

            // Like in browsers, a middle click closes the tab.
            let tab = mouse_area(row![select, close]).on_middle_press(Message::CloseTab(index));
            container(tab).width(Length::FillPortion(1)).max_width(MAX_TAB_WIDTH).into()
        });

        // The key table that was activated last, while one is active.
        let table = self.key_tables.last().map(|table| {
            container(label(table.name.to_string()).color(foreground)).padding([3, 8])
        });

        let bar_button = move |content: Element<'static, Message>, action: AppAction| {
            button(content)
                .padding([3, 8])
                .style(move |_, status| button::Style {
                    background: Some(bar.into()),
                    text_color: match status {
                        button::Status::Hovered => foreground,
                        _ => blend(foreground, background, 0.4),
                    },
                    ..Default::default()
                })
                .on_press(Message::Action(action))
        };
        let new_tab = bar_button(label("+".into()).into(), AppAction::NewTab);
        // The gear at the right end opens the configuration file. The font draws it small, so
        // it is larger, in the height the padding leaves the other buttons.
        let gear = text("\u{2699}").font(font).size(size * 1.3).height(size * 1.3 + 6.);
        let settings = bar_button(gear.center().into(), AppAction::OpenConfig).padding([0, 8]);

        let tabs = container(row(tabs).push(new_tab).push(table).spacing(1)).width(Length::Fill);
        container(row![tabs, settings])
            .width(Length::Fill)
            .style(move |_| container::Style { background: Some(bar.into()), ..Default::default() })
            .into()
    }
}
