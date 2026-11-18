use serde::{Deserialize, Deserializer, Serialize, de};
use toml::Value;

use tankterm_config_derive::{ConfigDeserialize, SerdeReplace};
use tankterm_terminal::term::Osc52;

use crate::config::ui_config::{Program, StringVisitor};

#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq)]
pub struct Terminal {
    /// OSC52 support mode.
    pub osc52: SerdeOsc52,
    /// Path to a shell program to run on startup.
    pub shell: Option<Program>,
    /// Ask before pasting text with line breaks into a program without bracketed paste.
    pub confirm_multiline_paste: bool,
    /// Mark where prompts start at the left edge, for shells with shell integration.
    pub prompt_marks: bool,
    /// Whether bash, zsh and fish get the shell integration when TankTerm starts them.
    pub shell_integration: ShellIntegration,
}

/// Whether the shells TankTerm starts get the shell integration.
#[derive(ConfigDeserialize, Serialize, Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum ShellIntegration {
    /// bash, zsh and fish load it, on Unix.
    #[default]
    Auto,
    /// No shell is changed; the scripts can still be sourced by hand.
    None,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            osc52: Default::default(),
            shell: None,
            confirm_multiline_paste: true,
            prompt_marks: true,
            shell_integration: Default::default(),
        }
    }
}

#[derive(SerdeReplace, Serialize, Default, Copy, Clone, Debug, PartialEq)]
pub struct SerdeOsc52(pub Osc52);

impl<'de> Deserialize<'de> for SerdeOsc52 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = deserializer.deserialize_str(StringVisitor)?;
        Osc52::deserialize(Value::String(value)).map(SerdeOsc52).map_err(de::Error::custom)
    }
}
