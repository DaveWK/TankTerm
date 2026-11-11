//! Shell profiles: named commands that can be opened in a new tab.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::ui_config::Program;

/// A named shell with its own command, directory, environment and theme.
///
/// Everything a profile doesn't set is taken from the rest of the configuration.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Name shown in the profile selector.
    pub name: String,

    /// Program to run instead of the shell.
    #[serde(default)]
    pub shell: Option<Program>,

    /// Directory the program is started in.
    #[serde(default)]
    pub working_directory: Option<PathBuf>,

    /// Environment variables set in addition to the `[env]` section.
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Theme layered underneath the configuration for this profile's terminals.
    #[serde(default)]
    pub theme: Option<String>,
}
