//! Tab bar configuration.

use serde::Serialize;

use tankterm_config_derive::ConfigDeserialize;

use crate::config::style::Style;

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
