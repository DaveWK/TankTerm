//! Key help generated from the key bindings, after Charm's Bubbles `help` view.
//!
//! Like Bubbles, every binding with a description shows up as `key desc`. The short help is a
//! single line of the normal mode bindings separated by ` • `, cut off with `…` when it doesn't
//! fit. The full help shows a column per binding mode.

use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::config::{
    Action, BindingKey, BindingMode, KeyBinding, KeyLocation, SearchAction, UiConfig,
};

/// Separator between short help items, from Bubbles.
const SHORT_SEPARATOR: &str = " • ";

/// Separator between full help columns, from Bubbles.
const FULL_SEPARATOR: &str = "    ";

/// Shown when the short help is cut off, from Bubbles.
const ELLIPSIS: &str = "…";

/// Which help is shown.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum HelpMode {
    #[default]
    Hidden,
    Short,
    Full,
}

impl HelpMode {
    /// Next mode for the toggle help action.
    pub fn next(self) -> Self {
        match self {
            Self::Hidden => Self::Short,
            Self::Short => Self::Full,
            Self::Full => Self::Hidden,
        }
    }
}

/// Part of a help line, styled by `[style.help_key]`, `help_desc` or `help_separator`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpPart {
    Key,
    Description,
    Separator,
}

/// A rendered help line: text segments with their styles.
pub type HelpLine = Vec<(String, HelpPart)>;

/// Binding modes the full help has a column for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    Normal,
    Vi,
    Search,
}

/// One help item: every key bound to the same description in a group.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    group: Group,
    keys: Vec<String>,
    description: String,
}

impl Entry {
    fn key(&self) -> String {
        self.keys.join("/")
    }

    fn width(&self) -> usize {
        self.key().chars().count() + 1 + self.description.chars().count()
    }
}

/// Lines of help to show for `mode`, at most `columns` wide.
pub fn lines(config: &UiConfig, mode: HelpMode, columns: usize) -> Vec<HelpLine> {
    let entries = entries(config.key_bindings());
    match mode {
        HelpMode::Hidden => Vec::new(),
        HelpMode::Short => vec![short_help(&entries, columns)],
        HelpMode::Full => full_help(&entries, columns),
    }
}

/// Help items for every binding with a description, in binding order.
fn entries(bindings: &[KeyBinding]) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for binding in bindings {
        let Some(description) = binding.help.clone().or_else(|| description(&binding.action))
        else {
            continue;
        };
        let Some(key) = key_label(binding) else { continue };

        let group = if binding.mode.contains(BindingMode::SEARCH) {
            Group::Search
        } else if binding.mode.contains(BindingMode::VI) {
            Group::Vi
        } else {
            Group::Normal
        };

        match entries.iter_mut().find(|e| e.group == group && e.description == description) {
            Some(entry) if !entry.keys.contains(&key) => entry.keys.push(key),
            Some(_) => (),
            None => entries.push(Entry { group, keys: vec![key], description }),
        }
    }
    entries
}

/// Single line help of the normal mode bindings.
fn short_help(entries: &[Entry], columns: usize) -> HelpLine {
    let mut line = HelpLine::new();
    let mut width = 0;

    for (index, entry) in entries.iter().filter(|e| e.group == Group::Normal).enumerate() {
        let separator = if index == 0 { "" } else { SHORT_SEPARATOR };
        let item_width = separator.chars().count() + entry.width();

        // Like Bubbles: add the ellipsis if it fits, then stop.
        if width + item_width > columns {
            if width + 1 + ELLIPSIS.chars().count() <= columns {
                line.push((format!(" {ELLIPSIS}"), HelpPart::Separator));
            }
            break;
        }

        if !separator.is_empty() {
            line.push((separator.into(), HelpPart::Separator));
        }
        line.push((entry.key(), HelpPart::Key));
        line.push((format!(" {}", entry.description), HelpPart::Description));
        width += item_width;
    }

    line
}

/// Help with a column for each binding mode, as many as fit.
fn full_help(entries: &[Entry], columns: usize) -> Vec<HelpLine> {
    let mut lines: Vec<HelpLine> = Vec::new();
    let mut width = 0;

    for group in [Group::Normal, Group::Vi, Group::Search] {
        let column: Vec<&Entry> = entries.iter().filter(|e| e.group == group).collect();
        if column.is_empty() {
            continue;
        }

        let key_width = column.iter().map(|e| e.key().chars().count()).max().unwrap_or(0);
        let description_width =
            column.iter().map(|e| e.description.chars().count()).max().unwrap_or(0);

        let separator = if width == 0 { "" } else { FULL_SEPARATOR };
        let column_width = separator.chars().count() + key_width + 1 + description_width;
        if width + column_width > columns {
            break;
        }

        // Pad lines of shorter columns so every column starts at the same position.
        if lines.len() < column.len() {
            lines.resize_with(column.len(), || vec![(" ".repeat(width), HelpPart::Separator)]);
        }

        for (index, line) in lines.iter_mut().enumerate() {
            match column.get(index) {
                Some(entry) => {
                    let key = entry.key();
                    let padding = key_width - key.chars().count();
                    let description = format!(
                        " {:<description_width$}",
                        entry.description,
                        description_width = description_width
                    );
                    line.push((separator.into(), HelpPart::Separator));
                    line.push((format!("{key}{}", " ".repeat(padding)), HelpPart::Key));
                    line.push((description, HelpPart::Description));
                },
                None => line.push((" ".repeat(column_width), HelpPart::Separator)),
            }
        }

        width += column_width;
    }

    lines
}

/// Default description of an action, like Bubbles' `key.WithHelp`.
fn description(action: &Action) -> Option<String> {
    let description = match action {
        Action::Copy => "copy",
        Action::Paste => "paste",
        Action::PasteSelection => "paste selection",
        Action::SearchForward => "search",
        Action::SearchBackward => "search backward",
        Action::ToggleViMode => "vi mode",
        Action::ToggleHelp => "help",
        Action::IncreaseFontSize => "zoom in",
        Action::DecreaseFontSize => "zoom out",
        Action::ResetFontSize => "reset zoom",
        Action::ScrollPageUp => "page up",
        Action::ScrollPageDown => "page down",
        Action::ScrollToTop => "top",
        Action::ScrollToBottom => "bottom",
        Action::ClearSelection => "clear selection",
        Action::ClearHistory => "clear history",
        Action::SpawnNewInstance => "new instance",
        Action::CreateNewWindow => "new window",
        Action::CreateNewTab => "new tab",
        Action::ToggleFullscreen => "fullscreen",
        Action::Quit => "quit",
        Action::Hint(_) => "hints",
        Action::Search(SearchAction::SearchConfirm) => "confirm",
        Action::Search(SearchAction::SearchCancel) => "cancel",
        Action::Search(SearchAction::SearchFocusNext) => "next match",
        Action::Search(SearchAction::SearchFocusPrevious) => "previous match",
        Action::Search(SearchAction::SearchHistoryPrevious) => "older search",
        Action::Search(SearchAction::SearchHistoryNext) => "newer search",
        _ => return None,
    };
    Some(description.into())
}

/// Key label like Bubbles uses: lowercase modifiers and key joined by `+`, like `ctrl+shift+c`.
fn key_label(binding: &KeyBinding) -> Option<String> {
    let key = match &binding.trigger {
        BindingKey::Keycode { key, location } => {
            let key = key_name(key)?;
            match location {
                KeyLocation::Numpad => format!("kp{key}"),
                _ => key,
            }
        },
        // Scancodes have no printable name.
        BindingKey::Scancode(_) => return None,
    };

    let mods = binding.mods;
    let mut label = String::new();
    for (modifier, name) in [
        (ModifiersState::CONTROL, "ctrl"),
        (ModifiersState::ALT, "alt"),
        (ModifiersState::SHIFT, "shift"),
        (ModifiersState::SUPER, if cfg!(target_os = "macos") { "cmd" } else { "super" }),
    ] {
        if mods.contains(modifier) {
            label.push_str(name);
            label.push('+');
        }
    }
    label.push_str(&key);
    Some(label)
}

fn key_name(key: &Key) -> Option<String> {
    let name = match key {
        Key::Character(character) => return Some(character.to_lowercase()),
        Key::Named(NamedKey::ArrowUp) => "↑",
        Key::Named(NamedKey::ArrowDown) => "↓",
        Key::Named(NamedKey::ArrowLeft) => "←",
        Key::Named(NamedKey::ArrowRight) => "→",
        Key::Named(NamedKey::PageUp) => "pgup",
        Key::Named(NamedKey::PageDown) => "pgdown",
        Key::Named(NamedKey::Escape) => "esc",
        Key::Named(NamedKey::Space) => "space",
        Key::Named(named) => return Some(format!("{named:?}").to_lowercase()),
        _ => return None,
    };
    Some(name.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    use winit::keyboard::SmolStr;

    fn binding(key: &str, mods: ModifiersState, action: Action) -> KeyBinding {
        KeyBinding {
            trigger: BindingKey::Keycode {
                key: Key::Character(SmolStr::new(key)),
                location: KeyLocation::Standard,
            },
            mods,
            action,
            mode: BindingMode::empty(),
            notmode: BindingMode::empty(),
            help: None,
        }
    }

    fn text(line: &HelpLine) -> String {
        line.iter().map(|(text, _)| text.as_str()).collect()
    }

    const CTRL_SHIFT: ModifiersState = ModifiersState::CONTROL.union(ModifiersState::SHIFT);

    #[test]
    fn mode_cycles() {
        assert_eq!(HelpMode::Hidden.next(), HelpMode::Short);
        assert_eq!(HelpMode::Short.next(), HelpMode::Full);
        assert_eq!(HelpMode::Full.next(), HelpMode::Hidden);
    }

    #[test]
    fn labels_like_bubbles() {
        let copy = binding("C", CTRL_SHIFT, Action::Copy);
        assert_eq!(key_label(&copy).as_deref(), Some("ctrl+shift+c"));

        let arrow = KeyBinding {
            trigger: BindingKey::Keycode {
                key: Key::Named(NamedKey::ArrowUp),
                location: KeyLocation::Standard,
            },
            ..binding("x", ModifiersState::empty(), Action::ScrollPageUp)
        };
        assert_eq!(key_label(&arrow).as_deref(), Some("↑"));
    }

    #[test]
    fn bindings_without_description_are_hidden() {
        let bindings = [
            binding("c", CTRL_SHIFT, Action::Copy),
            binding("x", CTRL_SHIFT, Action::Esc("x".into())),
        ];
        let entries = entries(&bindings);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].description, "copy");
    }

    #[test]
    fn custom_help_overrides_default() {
        let mut esc = binding("x", CTRL_SHIFT, Action::Esc("x".into()));
        esc.help = Some("send x".into());

        let entries = entries(&[esc]);
        assert_eq!(entries[0].description, "send x");
    }

    #[test]
    fn keys_for_the_same_description_are_joined() {
        let bindings = [
            binding("=", ModifiersState::CONTROL, Action::IncreaseFontSize),
            binding("+", ModifiersState::CONTROL, Action::IncreaseFontSize),
        ];
        let entries = entries(&bindings);

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].key(), "ctrl+=/ctrl++");
    }

    #[test]
    fn modes_are_grouped() {
        let mut vi = binding("i", ModifiersState::empty(), Action::ToggleViMode);
        vi.mode = BindingMode::VI;
        let mut search =
            binding("n", ModifiersState::empty(), SearchAction::SearchFocusNext.into());
        search.mode = BindingMode::SEARCH;

        let entries = entries(&[vi, search, binding("c", CTRL_SHIFT, Action::Copy)]);
        let groups: Vec<_> = entries.iter().map(|e| e.group).collect();

        assert_eq!(groups, [Group::Vi, Group::Search, Group::Normal]);
    }

    #[test]
    fn short_help_separates_items() {
        let bindings =
            [binding("c", CTRL_SHIFT, Action::Copy), binding("v", CTRL_SHIFT, Action::Paste)];
        let line = short_help(&entries(&bindings), 80);

        assert_eq!(text(&line), "ctrl+shift+c copy • ctrl+shift+v paste");
        assert_eq!(line[0].1, HelpPart::Key);
        assert_eq!(line[1].1, HelpPart::Description);
        assert_eq!(line[2].1, HelpPart::Separator);
    }

    #[test]
    fn short_help_truncates_with_ellipsis() {
        let bindings =
            [binding("c", CTRL_SHIFT, Action::Copy), binding("v", CTRL_SHIFT, Action::Paste)];
        let entries = entries(&bindings);

        // "ctrl+shift+c copy" is 17 wide; the next item doesn't fit, the ellipsis does.
        assert_eq!(text(&short_help(&entries, 20)), "ctrl+shift+c copy …");
        // Not even the ellipsis fits.
        assert_eq!(text(&short_help(&entries, 18)), "ctrl+shift+c copy");
        assert_eq!(text(&short_help(&entries, 5)), " …");
    }

    #[test]
    fn short_help_only_shows_normal_mode() {
        let mut vi = binding("i", ModifiersState::empty(), Action::ToggleViMode);
        vi.mode = BindingMode::VI;

        assert_eq!(text(&short_help(&entries(&[vi]), 80)), "");
    }

    #[test]
    fn full_help_has_aligned_columns() {
        let mut vi = binding("i", ModifiersState::empty(), Action::ToggleViMode);
        vi.mode = BindingMode::VI;
        let bindings =
            [binding("c", CTRL_SHIFT, Action::Copy), binding("v", CTRL_SHIFT, Action::Paste), vi];

        let lines = full_help(&entries(&bindings), 80);

        assert_eq!(lines.len(), 2);
        assert_eq!(text(&lines[0]), "ctrl+shift+c copy     i vi mode");
        // The second column is empty on this line, but padded so the lines line up.
        assert_eq!(text(&lines[1]).trim_end(), "ctrl+shift+v paste");
        assert_eq!(text(&lines[1]).chars().count(), text(&lines[0]).chars().count());
    }

    #[test]
    fn full_help_drops_columns_that_dont_fit() {
        let mut vi = binding("i", ModifiersState::empty(), Action::ToggleViMode);
        vi.mode = BindingMode::VI;
        let bindings = [binding("c", CTRL_SHIFT, Action::Copy), vi];

        let lines = full_help(&entries(&bindings), 20);

        assert_eq!(lines.len(), 1);
        assert_eq!(text(&lines[0]), "ctrl+shift+c copy");
    }

    #[test]
    fn default_bindings_have_help() {
        let config = UiConfig::default();
        let full = lines(&config, HelpMode::Full, 200);
        let all: String = full.iter().map(text).collect();

        assert!(all.contains("vi mode"), "{all}");
        assert!(all.contains("confirm"), "{all}");
        assert!(lines(&config, HelpMode::Hidden, 200).is_empty());
    }
}
