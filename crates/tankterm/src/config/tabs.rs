//! Tab bar configuration.

use log::warn;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use tankterm_config::SerdeReplace;
use tankterm_config_derive::ConfigDeserialize;

use crate::config::style::Style;
use crate::logging::LOG_TARGET_CONFIG;
use crate::template::Template;

#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Tabs {
    /// When the tab bar is shown.
    pub show: ShowTabs,

    /// Edge of the window the tab bar is placed at.
    pub position: TabsPosition,

    /// Open the tabs and split panes the last window had when TankTerm starts.
    pub restore: bool,

    /// Start the shells of new tabs and panes in the directory of the current pane's shell.
    pub inherit_working_directory: bool,

    /// What every tab shows in the tab bar.
    pub title: TabTitle,

    /// Segments at the end of the tab bar, in order.
    pub status: Vec<StatusSegment>,
}

impl Default for Tabs {
    fn default() -> Self {
        Self {
            show: Default::default(),
            position: Default::default(),
            restore: false,
            inherit_working_directory: true,
            title: TabTitle::default(),
            status: vec![StatusSegment::default()],
        }
    }
}

/// When the tab bar is shown.
#[derive(ConfigDeserialize, Serialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShowTabs {
    /// Always show the tab bar.
    Always,

    /// Only show the tab bar while there is more than one tab.
    #[default]
    Multiple,

    /// Never show the tab bar.
    Never,
}

/// Edge of the window the tab bar is placed at.
#[derive(ConfigDeserialize, Serialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabsPosition {
    #[default]
    Top,
    Bottom,
}

/// The `tabs.title` format: what a tab shows in the tab bar, with placeholders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabTitle {
    format: String,
    template: Template,
}

impl TabTitle {
    /// The placeholders a tab title can have.
    pub const FIELDS: &[&str] =
        &["index", "title", "panes", "cwd", "path", "process", "marker", "zoom"];

    fn new(format: String) -> Self {
        let template = Template::parse(&format);
        for field in template.fields().filter(|field| !Self::FIELDS.contains(field)) {
            warn!(target: LOG_TARGET_CONFIG, "Unknown placeholder {{{field}}} in tabs.title");
        }
        Self { format, template }
    }

    pub fn template(&self) -> &Template {
        &self.template
    }

    /// Whether the format has the placeholder `field`.
    pub fn uses(&self, field: &str) -> bool {
        self.template.fields().any(|used| used == field)
    }
}

impl Default for TabTitle {
    fn default() -> Self {
        Self::new("{index}: {title}{zoom}{marker}".into())
    }
}

impl SerdeReplace for TabTitle {
    fn replace(&mut self, value: toml::Value) -> Result<(), Box<dyn std::error::Error>> {
        *self = Self::deserialize(value)?;
        Ok(())
    }
}

impl Serialize for TabTitle {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.format.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TabTitle {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::new)
    }
}

/// A segment at the end of the tab bar.
#[derive(ConfigDeserialize, Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct StatusSegment {
    /// What the segment shows.
    pub segment: SegmentKind,

    /// How it is shown: placeholders like `{name}`, or a strftime format for the clock. Each
    /// kind has its default.
    pub format: Option<String>,

    /// Changes to `[style.tab_status]` for this segment.
    pub style: Style,
}

/// What a status segment shows.
#[derive(ConfigDeserialize, Serialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    /// The key table that was activated last, while one is active: `{name}`.
    #[default]
    KeyTable,

    /// The local time, in a strftime format.
    Clock,

    /// The directory of the focused pane's shell: `{path}`, with the home directory as `~`,
    /// and `{basename}`.
    Cwd,

    /// The charge of the batteries: `{percent}` and `{status}`.
    Battery,
}

impl SegmentKind {
    /// The format of a segment that doesn't give one.
    pub fn default_format(self) -> &'static str {
        match self {
            Self::KeyTable => "{name}",
            Self::Clock => "%H:%M",
            Self::Cwd => "{path}",
            Self::Battery => "{percent}%",
        }
    }

    /// The placeholders the segment's format can have; the clock's format has none.
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            Self::KeyTable => &["name"],
            Self::Clock => &[],
            Self::Cwd => &["path", "basename"],
            Self::Battery => &["percent", "status"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::cli::ParsedOptions;
    use crate::config::UiConfig;

    #[test]
    fn title_format_is_read_and_written_as_text() {
        let config: UiConfig = toml::from_str("[tabs]\ntitle = \"{index} {cwd}\"\n").unwrap();
        let title = &config.tabs.title;

        assert!(title.uses("index") && title.uses("cwd") && !title.uses("title"));
        let written = toml::to_string(&config.tabs).unwrap();
        assert!(written.contains("title = \"{index} {cwd}\""), "{written}");
    }

    #[test]
    fn title_format_can_be_given_on_the_command_line() {
        let mut config = UiConfig::default();
        let mut overrides = ParsedOptions::from_options(&["tabs.title = '{process}'".into()]);

        overrides.override_config(&mut config);

        assert!(config.tabs.title.uses("process"));
    }
}
