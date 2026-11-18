//! The `[ssh]` table: which hosts `tankTerm ssh` checks for terminfo entries.

use serde::Serialize;

use tankterm_config_derive::ConfigDeserialize;

/// How `tankTerm ssh` chooses the `TERM` it sends to a server.
#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Ssh {
    /// Hosts to probe for terminfo entries, as ssh_config-style patterns.
    pub probe: Vec<String>,

    /// Entries to look for on a probed host, best first.
    pub order: Vec<String>,

    /// `TERM` for hosts that aren't probed, or that have none of the entries.
    pub fallback: String,
}

impl Default for Ssh {
    fn default() -> Self {
        Self {
            probe: Vec::new(),
            order: ["alacritty", "xterm-256color", "xterm"].map(String::from).to_vec(),
            fallback: String::from("xterm-256color"),
        }
    }
}
