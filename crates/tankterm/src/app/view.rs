//! What the application draws: the tab bar, the panes of the shown tab and what lies over them.

use iced::widget::pane_grid::{self, PaneGrid};
use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, keyed_column, mouse_area, row, stack, text};
use iced::{Element, Length};
use iced_tterm::TerminalView;

use super::{App, Layout, MAX_TAB_WIDTH, Message};
use crate::config::BindingMode;
use crate::config::tabs::{ShowTabs, TabsPosition};
use crate::hotkeys::{AppAction, KeyFilter};
use crate::palette::{self, blend, paste_warning};

impl App {
    pub fn view(&self) -> Element<'_, Message> {
        let Some(tab) = self.tabs.get(self.active) else {
            // The last tab was closed and the window is about to close.
            return container("").into();
        };

        // Frame the panes of a split tab, to show which one has focus.
        let framed = tab.panes.len() > 1 && tab.panes.maximized().is_none();
        let panes = PaneGrid::new(&tab.panes, |pane, terminal, _| {
            pane_grid::Content::new(self.pane(*terminal, framed, pane == tab.focus))
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

        // The paste warning is answered with a key; nothing typed meanwhile reaches a terminal.
        let hotkeys = match (&self.pending_paste, &self.palette) {
            (Some(_), _) => &self.paste_keys,
            (None, Some(_)) => &self.palette_keys,
            (None, None) => &self.options.hotkeys,
        };

        // Bindings can be limited to modes of the terminal the keys are typed into.
        let mode = self.focused_shell().map(|shell| shell.terminal.mode()).unwrap_or_default();

        KeyFilter::new(layers, hotkeys, Message::Action)
            .exclusive(self.pending_paste.is_some())
            .mode(BindingMode::new(&mode, false))
            .into()
    }

    /// A terminal with its padding and, in a split tab, a frame showing whether it has focus.
    fn pane(&self, terminal: u64, framed: bool, focused: bool) -> Element<'_, Message> {
        let Layout { padding, foreground, .. } = self.options.layout;
        let Some(shell) = self.terminals.get(&terminal) else { return container("").into() };
        let background = shell.background;

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
        let terminal = stack![terminal].push(flash);

        // A pane that wants to be looked at stands out from the focused one.
        let frame = match (shell.attention, focused) {
            (true, _) => foreground,
            (false, true) => blend(self.options.layout.background, foreground, 0.6),
            (false, false) => blend(self.options.layout.background, foreground, 0.15),
        };
        container(terminal)
            .padding(if framed { 1 } else { 0 })
            .style(move |_| container::Style {
                background: Some(frame.into()),
                ..Default::default()
            })
            .into()
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

        let new_tab = button(label("+".into()))
            .padding([3, 8])
            .style(move |_, status| button::Style {
                background: Some(bar.into()),
                text_color: match status {
                    button::Status::Hovered => foreground,
                    _ => blend(foreground, background, 0.4),
                },
                ..Default::default()
            })
            .on_press(Message::Action(AppAction::NewTab));

        container(row(tabs).push(new_tab).spacing(1))
            .width(Length::Fill)
            .style(move |_| container::Style { background: Some(bar.into()), ..Default::default() })
            .into()
    }
}
