//! Tab bar configuration.

use log::warn;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use tankterm_config::SerdeReplace;
use tankterm_config_derive::ConfigDeserialize;

use crate::config::style::Style;
use crate::config::ui_config::Program;
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

    /// Segments at the ends of the tab bar, in order.
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

/// A segment at an end of the tab bar.
#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct StatusSegment {
    /// What the segment shows.
    pub segment: SegmentKind,

    /// How it is shown: placeholders like `{name}`, or a strftime format for the clock. Each
    /// kind has its default.
    pub format: Option<String>,

    /// The end of the tab bar the segment is at.
    pub side: Side,

    /// The program a `Command` segment runs, without a shell.
    pub command: Option<Program>,

    /// Seconds between the runs of a `Command` segment's program, at least 1.
    pub interval: u64,

    /// Changes to `[style.tab_status]` for this segment.
    pub style: Style,
}

impl Default for StatusSegment {
    fn default() -> Self {
        Self {
            segment: Default::default(),
            format: None,
            side: Default::default(),
            command: None,
            interval: 5,
            style: Default::default(),
        }
    }
}

/// An end of the tab bar.
#[derive(ConfigDeserialize, Serialize, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// Before the tabs, after the window buttons on that side.
    Left,

    /// After the tabs, before the gear.
    #[default]
    Right,
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

    /// The format, while the shown tab has a zoomed pane.
    Zoom,

    /// How many tabs and panes there are: `{tabs}`, `{panes}` in the shown tab, and `{tab}`,
    /// the position of the shown tab.
    Count,

    /// The format, always: separators or a label.
    Text,

    /// Who and where: `{user}`, `{host}` and `{fqdn}`.
    Host,

    /// The program running in the focused pane, while a command runs: `{name}`.
    Process,

    /// The load average over 1, 5 and 15 minutes: `{load1}`, `{load5}` and `{load15}`.
    Load,

    /// The first line a program printed, run every `interval` seconds: `{output}`.
    Command,
}

impl SegmentKind {
    /// The format of a segment that doesn't give one.
    pub fn default_format(self) -> &'static str {
        match self {
            Self::KeyTable => "{name}",
            Self::Clock => "%H:%M",
            Self::Cwd => "{path}",
            Self::Battery => "{percent}%",
            Self::Zoom => "ZOOM",
            Self::Count => "{tab}/{tabs}",
            Self::Text => "",
            Self::Host => "{user}@{host}",
            Self::Process => "{name}",
            Self::Load => "{load1}",
            Self::Command => "{output}",
        }
    }

    /// The placeholders the segment's format can have; the clock's format has none.
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            Self::KeyTable => &["name"],
            Self::Clock | Self::Zoom | Self::Text => &[],
            Self::Cwd => &["path", "basename"],
            Self::Battery => &["percent", "status"],
            Self::Count => &["tabs", "panes", "tab"],
            Self::Host => &["user", "host", "fqdn"],
            Self::Process => &["name"],
            Self::Load => &["load1", "load5", "load15"],
            Self::Command => &["output"],
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

    #[test]
    fn status_segments_take_a_side_and_default_formats() {
        let config: UiConfig = toml::from_str(
            "[[tabs.status]]\nsegment = \"Host\"\nside = \"Left\"\n\n[[tabs.status]]\nsegment = \
             \"Load\"\nformat = \"{load5}\"\n",
        )
        .unwrap();
        let [host, load] = &config.tabs.status[..] else { panic!("{:?}", config.tabs.status) };

        assert_eq!(
            (host.segment, host.side, host.format.as_deref()),
            (SegmentKind::Host, Side::Left, None)
        );
        assert_eq!(
            (load.segment, load.side, load.format.as_deref()),
            (SegmentKind::Load, Side::Right, Some("{load5}"))
        );

        // A command is a program with arguments, run every 5 seconds unless it says otherwise.
        let config: UiConfig = toml::from_str(
            "[[tabs.status]]\nsegment = \"Command\"\ncommand = { program = \"sh\", args = \
             [\"-c\", \"date\"] }\n\n[[tabs.status]]\nsegment = \"Command\"\ncommand = \
             \"uptime\"\ninterval = 60\n",
        )
        .unwrap();
        let [date, uptime] = &config.tabs.status[..] else { panic!("{:?}", config.tabs.status) };
        let program = date.command.as_ref().unwrap();
        assert_eq!(
            (program.program(), program.args(), date.interval),
            ("sh", &["-c".to_owned(), "date".to_owned()][..], 5)
        );
        assert_eq!(uptime.command, Some(Program::Just("uptime".into())));
        assert_eq!(uptime.interval, 60);

        // Every default format has only placeholders its segment knows.
        use SegmentKind::*;
        for kind in [KeyTable, Cwd, Battery, Zoom, Count, Text, Host, Process, Load, Command] {
            let template = Template::parse(kind.default_format());
            assert!(template.fields().all(|field| kind.fields().contains(&field)), "{kind:?}");
        }
    }
}
