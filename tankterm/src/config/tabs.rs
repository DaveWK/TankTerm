//! Tab bar configuration.

use serde::Serialize;

use tankterm_config_derive::ConfigDeserialize;

#[derive(ConfigDeserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tabs {
    /// When the tab bar is shown.
    pub show: ShowTabs,

    /// Edge of the window the tab bar is placed at.
    pub position: TabsPosition,

    /// Open the tabs and split panes the last window had when TankTerm starts.
    pub restore: bool,

    /// Start the shells of new tabs and panes in the directory of the current pane's shell.
    pub inherit_working_directory: bool,
}

impl Default for Tabs {
    fn default() -> Self {
        Self {
            show: Default::default(),
            position: Default::default(),
            restore: false,
            inherit_working_directory: true,
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
