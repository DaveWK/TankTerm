//! A filterable list shown over the terminals: the command palette, the profile selector and
//! the tab navigator, and the prompt for a tab's title.

use iced::widget::{button, column, container, opaque, row, text, text_input};
use iced::{Border, Color, Element, Font, Length, Padding, widget};

use crate::app::Message;
use crate::hotkeys::{AppAction, Hotkeys, PaneSelectMode, ScrollTo};

/// Most items shown at once; the list scrolls with the selection.
const VISIBLE_ITEMS: usize = 10;

/// Width of the list, in logical pixels.
const WIDTH: f32 = 520.;

/// Commands offered by the command palette, with the names shown for them.
const COMMANDS: &[(&str, AppAction)] = {
    use iced::widget::pane_grid::{Axis, Direction};

    &[
        ("New Tab", AppAction::NewTab),
        ("Close Tab", AppAction::CloseTab),
        ("Next Tab", AppAction::NextTab),
        ("Previous Tab", AppAction::PreviousTab),
        ("Move Tab Left", AppAction::MoveTabLeft),
        ("Move Tab Right", AppAction::MoveTabRight),
        ("Split Right", AppAction::Split(Axis::Vertical)),
        ("Split Down", AppAction::Split(Axis::Horizontal)),
        ("Close Pane", AppAction::ClosePane),
        ("Zoom Pane", AppAction::ToggleZoom),
        ("Focus Pane Left", AppAction::FocusPane(Direction::Left)),
        ("Focus Pane Right", AppAction::FocusPane(Direction::Right)),
        ("Focus Pane Up", AppAction::FocusPane(Direction::Up)),
        ("Focus Pane Down", AppAction::FocusPane(Direction::Down)),
        ("Focus Next Pane", AppAction::NextPane),
        ("Focus Previous Pane", AppAction::PreviousPane),
        ("Resize Pane Left", AppAction::ResizePane(Direction::Left)),
        ("Resize Pane Right", AppAction::ResizePane(Direction::Right)),
        ("Resize Pane Up", AppAction::ResizePane(Direction::Up)),
        ("Resize Pane Down", AppAction::ResizePane(Direction::Down)),
        ("Select Pane...", AppAction::PaneSelect(PaneSelectMode::Focus)),
        ("Swap Pane...", AppAction::PaneSelect(PaneSelectMode::Swap)),
        ("Rotate Panes Clockwise", AppAction::RotatePanes(true)),
        ("Rotate Panes Counterclockwise", AppAction::RotatePanes(false)),
        ("Go to Tab...", AppAction::ShowTabNavigator),
        ("Rename Tab...", AppAction::RenameTab),
        ("Open Profile...", AppAction::ShowProfiles),
        ("Open Config", AppAction::OpenConfig),
        ("Scroll to Previous Prompt", AppAction::Scroll(ScrollTo::PreviousPrompt)),
        ("Scroll to Next Prompt", AppAction::Scroll(ScrollTo::NextPrompt)),
        ("Select Command Output", AppAction::SelectCommandOutput),
    ]
};

/// What choosing an item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Action(AppAction),
    /// Open the profile at this position in a new tab.
    Profile(usize),
    /// Give the tab this title, or take its title away if it is empty.
    RenameTab(String),
}

/// An entry of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    /// Shown next to the label: a key binding or a command line.
    pub detail: String,
    pub command: Command,
}

/// How the list is drawn.
#[derive(Debug, Clone, Copy)]
pub struct Appearance {
    pub font: Font,
    pub size: f32,
    pub background: Color,
    pub foreground: Color,
}

/// The list with the text typed to filter it and the selected item.
pub struct Palette {
    placeholder: String,
    /// Whether this asks for text, which is the result, instead of offering a list.
    prompt: bool,
    query: String,
    /// Position of the selected item among the items matching the query.
    selected: usize,
    items: Vec<Item>,
    input: widget::Id,
}

impl Palette {
    /// The command palette: every tab and pane action, with its key binding.
    pub fn commands(hotkeys: &Hotkeys) -> Self {
        let items = COMMANDS.iter().map(|(label, action)| Item {
            label: (*label).to_owned(),
            detail: hotkeys.describe(action).unwrap_or_default(),
            command: Command::Action(action.clone()),
        });

        Self::new("Run a command", items.collect())
    }

    /// The profile selector: the default shell followed by the configured profiles.
    pub fn profiles(profiles: impl IntoIterator<Item = (String, String)>) -> Self {
        let default = Item {
            label: "Default".into(),
            detail: String::new(),
            command: Command::Action(AppAction::NewTab),
        };
        let profiles = profiles.into_iter().enumerate().map(|(index, (label, detail))| Item {
            label,
            detail,
            command: Command::Profile(index),
        });

        Self::new("Open a profile in a new tab", [default].into_iter().chain(profiles).collect())
    }

    /// The tab navigator: the tabs with their titles, with the shown one selected.
    pub fn tabs(titles: impl IntoIterator<Item = String>, active: usize) -> Self {
        let items = titles.into_iter().enumerate().map(|(index, title)| Item {
            label: format!("{}: {title}", index + 1),
            detail: String::new(),
            command: Command::Action(AppAction::SelectTab(index)),
        });

        let mut palette = Self::new("Go to a tab", items.collect());
        palette.selected = active.min(palette.items.len().saturating_sub(1));
        palette
    }

    /// The prompt for the title of a tab, starting with the title it was given, if any. `shown`
    /// is the title the tab shows without one.
    pub fn rename(title: Option<&str>, shown: &str) -> Self {
        let mut palette =
            Self::new(format!("Title of the tab, or empty for {shown:?}"), Vec::new());
        palette.prompt = true;
        palette.query = title.unwrap_or_default().to_owned();
        palette
    }

    fn new(placeholder: impl Into<String>, items: Vec<Item>) -> Self {
        Self {
            placeholder: placeholder.into(),
            prompt: false,
            query: String::new(),
            selected: 0,
            items,
            input: widget::Id::unique(),
        }
    }

    /// The text input, which needs keyboard focus while the list is shown.
    pub fn input(&self) -> &widget::Id {
        &self.input
    }

    /// Filter the list, selecting the first match.
    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.selected = 0;
    }

    /// Select the next or previous match, wrapping around at the ends.
    pub fn step(&mut self, forward: bool) {
        let count = self.matches().len();
        if count > 0 {
            let step = if forward { 1 } else { count - 1 };
            self.selected = (self.selected + step) % count;
        }
    }

    /// The items matching the query: those containing all of its words, ignoring case.
    pub fn matches(&self) -> Vec<&Item> {
        let query = self.query.to_lowercase();
        let matches = |item: &&Item| {
            let label = item.label.to_lowercase();
            query.split_whitespace().all(|word| label.contains(word))
        };

        self.items.iter().filter(matches).collect()
    }

    /// The command of the match at `position`.
    pub fn command(&self, position: usize) -> Option<Command> {
        self.matches().get(position).map(|item| item.command.clone())
    }

    /// The command of the selected match, or the text typed into a prompt.
    pub fn selected(&self) -> Option<Command> {
        if self.prompt {
            return Some(Command::RenameTab(self.query.trim().to_owned()));
        }

        self.command(self.selected)
    }

    /// The list, centered at the top of a layer that covers the window.
    pub fn view(&self, appearance: Appearance) -> Element<'_, Message> {
        let Appearance { font, size, background, foreground } = appearance;
        let dim = blend(foreground, background, 0.45);
        let highlight = blend(background, foreground, 0.2);
        let label = move |content: &str| text(content.to_owned()).font(font).size(size);

        let input = text_input(&self.placeholder, &self.query)
            .id(self.input.clone())
            .font(font)
            .size(size)
            .padding([4, 8])
            .on_input(Message::PaletteQuery)
            .on_submit(Message::PaletteSubmit)
            .style(move |_, _| text_input::Style {
                background: background.into(),
                border: Border { color: dim, width: 1., radius: 0.into() },
                icon: foreground,
                placeholder: dim,
                value: foreground,
                selection: highlight,
            });

        // Keep the selected item in view.
        let matches = self.matches();
        let first = (self.selected + 1).saturating_sub(VISIBLE_ITEMS);
        let items =
            matches.iter().enumerate().skip(first).take(VISIBLE_ITEMS).map(|(position, item)| {
                let selected = position == self.selected;
                let line =
                    row![label(&item.label).width(Length::Fill), label(&item.detail).color(dim)]
                        .spacing(16);

                button(line)
                    .width(Length::Fill)
                    .padding([3, 8])
                    .style(move |_, status| button::Style {
                        background: (selected || status == button::Status::Hovered)
                            .then_some(highlight.into()),
                        text_color: foreground,
                        ..Default::default()
                    })
                    .on_press(Message::PaletteChoose(position))
                    .into()
            });

        let list = container(column![input, column(items)].spacing(4))
            .padding(8)
            .width(WIDTH)
            .style(move |_| container::Style {
                background: Some(blend(background, foreground, 0.06).into()),
                border: Border { color: dim, width: 1., radius: 0.into() },
                ..Default::default()
            });

        // The layer keeps clicks away from the terminals underneath.
        opaque(
            container(list)
                .center_x(Length::Fill)
                .height(Length::Fill)
                .padding(Padding::ZERO.top(40))
                .style(|_| container::Style {
                    background: Some(Color { a: 0.4, ..Color::BLACK }.into()),
                    ..Default::default()
                }),
        )
    }
}

/// Most lines of pasted text shown by the paste warning.
const PREVIEW_LINES: usize = 6;

/// Most characters of a line shown by the paste warning.
const PREVIEW_COLUMNS: usize = 60;

/// The question shown before pasting text that a shell would run line by line.
pub fn paste_warning(pasted: &str, appearance: Appearance) -> Element<'_, Message> {
    let Appearance { font, size, background, foreground } = appearance;
    let dim = blend(foreground, background, 0.45);
    let highlight = blend(background, foreground, 0.2);
    let label = move |content: String| text(content).font(font).size(size);

    let lines: Vec<&str> = pasted.lines().collect();
    let preview = lines.iter().take(PREVIEW_LINES).map(|line| {
        let mut shown: String = line.chars().take(PREVIEW_COLUMNS).collect();
        if line.chars().count() > PREVIEW_COLUMNS {
            shown.push_str("...");
        }
        label(shown).color(dim).into()
    });
    let more = (lines.len() > PREVIEW_LINES)
        .then(|| label(format!("... and {} more", lines.len() - PREVIEW_LINES)).color(dim));

    let answer = move |content: &str, action: AppAction| {
        button(label(content.to_owned()))
            .padding([3, 8])
            .style(move |_, status| button::Style {
                background: Some(
                    if status == button::Status::Hovered { highlight } else { background }.into(),
                ),
                text_color: foreground,
                border: Border { color: dim, width: 1., radius: 0.into() },
                ..Default::default()
            })
            .on_press(Message::Action(action))
    };

    let question = match lines.len() {
        1 => "Paste this line? Its line break runs it.".to_owned(),
        count => format!("Paste {count} lines? Every line break runs a line."),
    };
    let content = column![
        label(question),
        column(preview).push(more),
        row![
            answer("Paste (Enter)", AppAction::PasteConfirm),
            answer("Cancel (Escape)", AppAction::PasteCancel)
        ]
        .spacing(8),
    ]
    .spacing(8);

    let dialog = container(content).padding(8).width(WIDTH).style(move |_| container::Style {
        background: Some(blend(background, foreground, 0.06).into()),
        border: Border { color: dim, width: 1., radius: 0.into() },
        ..Default::default()
    });

    opaque(
        container(dialog)
            .center_x(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::ZERO.top(40))
            .style(|_| container::Style {
                background: Some(Color { a: 0.4, ..Color::BLACK }.into()),
                ..Default::default()
            }),
    )
}

/// Blend `amount` of `other` into `color`.
pub fn blend(color: Color, other: Color, amount: f32) -> Color {
    let mix = |a: f32, b: f32| a + (b - a) * amount;
    Color::from_rgb(mix(color.r, other.r), mix(color.g, other.g), mix(color.b, other.b))
}

#[cfg(test)]
mod tests {
    use iced::keyboard::{Key, Modifiers};

    use super::*;

    fn labels(palette: &Palette) -> Vec<&str> {
        palette.matches().iter().map(|item| item.label.as_str()).collect()
    }

    #[test]
    fn commands_are_filtered_by_all_words_ignoring_case() {
        let mut palette = Palette::commands(&Hotkeys::default());
        assert_eq!(palette.matches().len(), COMMANDS.len());

        palette.set_query("PANE left".into());
        assert_eq!(labels(&palette), ["Focus Pane Left", "Resize Pane Left"]);

        palette.set_query("no such command".into());
        assert_eq!(palette.selected(), None);
    }

    #[test]
    fn selection_wraps_around_the_matches() {
        let mut palette = Palette::commands(&Hotkeys::default());
        palette.set_query("split".into());
        assert_eq!(labels(&palette), ["Split Right", "Split Down"]);

        palette.step(true);
        palette.step(true);
        assert_eq!(palette.selected(), palette.command(0));
        palette.step(false);
        assert_eq!(palette.selected(), palette.command(1));

        // Filtering again starts at the first match.
        palette.set_query("tab".into());
        assert_eq!(palette.selected(), Some(Command::Action(AppAction::NewTab)));
    }

    #[test]
    fn commands_show_their_key_bindings() {
        let hotkeys = Hotkeys::default().with(
            Key::Character("t".into()),
            Modifiers::CTRL | Modifiers::SHIFT,
            AppAction::NewTab,
        );
        let palette = Palette::commands(&hotkeys);

        let details: Vec<_> = palette.matches().iter().map(|item| item.detail.clone()).collect();
        assert_eq!(details[0], "Ctrl+Shift+T");
        assert_eq!(details[1], "");
    }

    #[test]
    fn tab_navigator_starts_at_the_shown_tab() {
        let mut palette = Palette::tabs(["vim".to_owned(), "logs".to_owned()], 1);
        assert_eq!(labels(&palette), ["1: vim", "2: logs"]);
        assert_eq!(palette.selected(), Some(Command::Action(AppAction::SelectTab(1))));

        palette.set_query("vim".into());
        assert_eq!(palette.selected(), Some(Command::Action(AppAction::SelectTab(0))));
    }

    #[test]
    fn rename_prompt_returns_the_typed_title() {
        let mut palette = Palette::rename(Some("build"), "bash");
        assert!(palette.matches().is_empty());
        assert_eq!(palette.selected(), Some(Command::RenameTab("build".into())));

        palette.set_query("  ".into());
        assert_eq!(palette.selected(), Some(Command::RenameTab(String::new())));
    }

    #[test]
    fn profiles_follow_the_default_shell() {
        let palette = Palette::profiles([("Server".to_owned(), "ssh server".to_owned())]);

        assert_eq!(labels(&palette), ["Default", "Server"]);
        assert_eq!(palette.command(0), Some(Command::Action(AppAction::NewTab)));
        assert_eq!(palette.command(1), Some(Command::Profile(0)));
    }
}
