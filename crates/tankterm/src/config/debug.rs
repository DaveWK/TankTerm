use log::LevelFilter;
use serde::Serialize;

use tankterm_config_derive::ConfigDeserialize;

/// Debugging options.
#[derive(ConfigDeserialize, Serialize, Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Debug {
    pub log_level: LevelFilter,

    /// Keep the log file after quitting.
    pub persistent_logging: bool,
}

impl Default for Debug {
    fn default() -> Self {
        Self { log_level: LevelFilter::Warn, persistent_logging: Default::default() }
    }
}
