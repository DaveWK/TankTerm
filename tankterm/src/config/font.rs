use std::fmt;

use crossfont::Size as FontSize;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use tankterm_config_derive::{ConfigDeserialize, SerdeReplace};

use crate::config::ui_config::Delta;

/// Font config.
///
/// Defaults are provided at the level of this struct per platform, but not per
/// field in this struct. It might be nice in the future to have defaults for
/// each value independently. Alternatively, maybe erroring when the user
/// doesn't provide complete config is Ok.
#[derive(ConfigDeserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Font {
    /// Extra spacing per character.
    pub offset: Delta<i8>,

    /// Glyph offset within character cell.
    pub glyph_offset: Delta<i8>,

    #[config(removed = "set the AppleFontSmoothing user default instead")]
    pub use_thin_strokes: bool,

    /// Normal font face.
    normal: FontDescription,

    /// Bold font face.
    bold: SecondaryFontDescription,

    /// Italic font face.
    italic: SecondaryFontDescription,

    /// Bold italic font face.
    bold_italic: SecondaryFontDescription,

    /// Font size in points.
    size: Size,

    /// Whether to use the built-in font for box drawing characters.
    pub builtin_box_drawing: bool,
}

impl Font {
    /// Get a font clone with a size modification.
    pub fn with_size(self, size: FontSize) -> Font {
        Font { size: Size(size), ..self }
    }

    #[inline]
    pub fn size(&self) -> FontSize {
        self.size.0
    }

    /// Get normal font description.
    pub fn normal(&self) -> &FontDescription {
        &self.normal
    }

    /// Get bold font description.
    pub fn bold(&self) -> FontDescription {
        self.bold.desc(&self.normal)
    }

    /// Get italic font description.
    pub fn italic(&self) -> FontDescription {
        self.italic.desc(&self.normal)
    }

    /// Get bold italic font description.
    pub fn bold_italic(&self) -> FontDescription {
        self.bold_italic.desc(&self.normal)
    }
}

impl Default for Font {
    fn default() -> Font {
        Self {
            builtin_box_drawing: true,
            glyph_offset: Default::default(),
            use_thin_strokes: Default::default(),
            bold_italic: Default::default(),
            italic: Default::default(),
            offset: Default::default(),
            normal: Default::default(),
            bold: Default::default(),
            size: Default::default(),
        }
    }
}

/// Description of the normal font.
#[derive(ConfigDeserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct FontDescription {
    pub family: String,
    pub style: Option<String>,
}

/// Default font family.
pub const DEFAULT_FAMILY: &str = "Iosevka Term";

/// Monospace font that ships with the operating system.
#[cfg(not(any(target_os = "macos", windows)))]
const SYSTEM_FAMILY: &str = "monospace";
#[cfg(target_os = "macos")]
const SYSTEM_FAMILY: &str = "Menlo";
#[cfg(windows)]
const SYSTEM_FAMILY: &str = "Consolas";

impl Default for FontDescription {
    fn default() -> FontDescription {
        FontDescription { family: DEFAULT_FAMILY.into(), style: None }
    }
}

impl FontDescription {
    /// The operating system's monospace font, used when the configured font can't be loaded.
    ///
    /// Unlike the default font, this is always installed.
    pub fn system() -> FontDescription {
        FontDescription { family: SYSTEM_FAMILY.into(), style: None }
    }
}

/// Description of the italic and bold font.
#[derive(ConfigDeserialize, Serialize, Debug, Default, Clone, PartialEq, Eq)]
pub struct SecondaryFontDescription {
    family: Option<String>,
    style: Option<String>,
}

impl SecondaryFontDescription {
    pub fn desc(&self, fallback: &FontDescription) -> FontDescription {
        FontDescription {
            family: self.family.clone().unwrap_or_else(|| fallback.family.clone()),
            style: self.style.clone(),
        }
    }
}

#[derive(SerdeReplace, Debug, Clone, PartialEq, Eq)]
struct Size(FontSize);

impl Default for Size {
    fn default() -> Self {
        Self(FontSize::new(11.25))
    }
}

impl<'de> Deserialize<'de> for Size {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct NumVisitor;
        impl Visitor<'_> for NumVisitor {
            type Value = Size;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("f64 or i64")
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(Size(FontSize::new(value as f32)))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(Size(FontSize::new(value as f32)))
            }
        }

        deserializer.deserialize_any(NumVisitor)
    }
}

impl Serialize for Size {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f32(self.0.as_pt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::UiConfig;

    #[test]
    fn default_family_is_iosevka() {
        let font = Font::default();
        assert_eq!(font.normal().family, "Iosevka Term");
        assert_eq!(font.normal().style, None);

        // Bold, italic and bold italic inherit the normal family.
        assert_eq!(font.bold().family, "Iosevka Term");
        assert_eq!(font.italic().family, "Iosevka Term");
        assert_eq!(font.bold_italic().family, "Iosevka Term");
    }

    #[test]
    fn system_family_is_platform_monospace() {
        let expected = if cfg!(target_os = "macos") {
            "Menlo"
        } else if cfg!(windows) {
            "Consolas"
        } else {
            "monospace"
        };

        assert_eq!(FontDescription::system().family, expected);
        assert_ne!(FontDescription::system().family, DEFAULT_FAMILY);
    }

    #[test]
    fn config_without_family_uses_default() {
        let config: UiConfig = toml::from_str("[font.normal]\nstyle = \"Bold\"\n").unwrap();
        assert_eq!(config.font.normal().family, DEFAULT_FAMILY);
        assert_eq!(config.font.normal().style.as_deref(), Some("Bold"));
    }

    #[test]
    fn configured_family_overrides_default() {
        let config: UiConfig = toml::from_str("[font.normal]\nfamily = \"Fira Code\"\n").unwrap();
        assert_eq!(config.font.normal().family, "Fira Code");
    }
}
