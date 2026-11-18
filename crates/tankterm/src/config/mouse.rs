use serde::{Deserialize, Deserializer, Serialize, Serializer};

use tankterm_config_derive::{ConfigDeserialize, SerdeReplace};

use crate::config::bindings::{self, MouseBinding};
use crate::config::ui_config;

#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq)]
pub struct Mouse {
    pub hide_when_typing: bool,
    /// Move the cursor of a shell that takes clicks in its command line to a click there.
    pub click_to_move: bool,
    pub bindings: MouseBindings,
}

impl Default for Mouse {
    fn default() -> Self {
        Self { hide_when_typing: false, click_to_move: true, bindings: Default::default() }
    }
}

/// Active mouse bindings and the user's binding entries they were created from.
#[derive(SerdeReplace, Clone, Debug, PartialEq)]
pub struct MouseBindings {
    pub bindings: Vec<MouseBinding>,
    pub(super) user_bindings: Vec<toml::Value>,
}

impl Default for MouseBindings {
    fn default() -> Self {
        Self { bindings: bindings::default_mouse_bindings(), user_bindings: Vec::new() }
    }
}

impl<'de> Deserialize<'de> for MouseBindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let mut user_bindings = Vec::<toml::Value>::deserialize(deserializer)?;
        let bindings =
            ui_config::deserialize_bindings(&mut user_bindings, bindings::default_mouse_bindings());
        ui_config::warn_unsupported(
            &user_bindings,
            &bindings,
            crate::hotkeys::unsupported_mouse_binding,
        );
        Ok(Self { bindings, user_bindings })
    }
}

impl Serialize for MouseBindings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.user_bindings.serialize(serializer)
    }
}
