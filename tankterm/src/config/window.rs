use std::fmt::{self, Formatter};

use log::{error, warn};
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

#[cfg(target_os = "macos")]
use winit::platform::macos::OptionAsAlt as WinitOptionAsAlt;
use winit::window::{Fullscreen, Theme as WinitTheme, WindowLevel as WinitWindowLevel};

use tankterm_config_derive::{ConfigDeserialize, SerdeReplace};

use crate::config::LOG_TARGET_CONFIG;
use crate::config::ui_config::{Delta, Percentage};
use crate::display::WindowPadding;

/// Default TankTerm name, used for window title and class.
pub const DEFAULT_NAME: &str = "TankTerm";

#[derive(ConfigDeserialize, Serialize, Debug, Clone, PartialEq)]
pub struct WindowConfig {
    /// Initial position.
    pub position: Option<Delta<i32>>,

    /// Draw the window with title bar / borders.
    pub decorations: Decorations,

    /// Startup mode.
    pub startup_mode: StartupMode,

    /// XEmbed parent.
    #[config(skip)]
    #[serde(skip_serializing)]
    pub embed: Option<u32>,

    /// Spread out additional padding evenly.
    pub dynamic_padding: bool,

    /// Use dynamic title.
    pub dynamic_title: bool,

    /// Information to identify a particular window.
    #[config(flatten)]
    pub identity: Identity,

    /// Background opacity from 0.0 to 1.0.
    pub opacity: Percentage,

    /// Request blur behind the window.
    pub blur: bool,

    /// Controls which `Option` key should be treated as `Alt`.
    option_as_alt: OptionAsAlt,

    /// Resize increments.
    pub resize_increments: bool,

    /// Space around the terminal grid.
    padding: Padding,

    /// Initial dimensions.
    dimensions: Dimensions,

    /// System decorations theme variant.
    decorations_theme_variant: Option<Theme>,

    /// Window level.
    pub level: WindowLevel,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            dynamic_title: true,
            blur: Default::default(),
            embed: Default::default(),
            padding: Default::default(),
            opacity: Default::default(),
            position: Default::default(),
            identity: Default::default(),
            dimensions: Default::default(),
            decorations: Default::default(),
            startup_mode: Default::default(),
            dynamic_padding: Default::default(),
            resize_increments: Default::default(),
            decorations_theme_variant: Default::default(),
            option_as_alt: Default::default(),
            level: Default::default(),
        }
    }
}

impl WindowConfig {
    #[inline]
    pub fn dimensions(&self) -> Option<Dimensions> {
        let (lines, columns) = (self.dimensions.lines, self.dimensions.columns);
        let (lines_is_non_zero, columns_is_non_zero) = (lines != 0, columns != 0);

        if lines_is_non_zero && columns_is_non_zero {
            // Return dimensions if both `lines` and `columns` are non-zero.
            Some(self.dimensions)
        } else if lines_is_non_zero || columns_is_non_zero {
            // Warn if either `columns` or `lines` is non-zero.

            let (zero_key, non_zero_key, non_zero_value) = if lines_is_non_zero {
                ("columns", "lines", lines)
            } else {
                ("lines", "columns", columns)
            };

            warn!(
                target: LOG_TARGET_CONFIG,
                "Both `lines` and `columns` must be non-zero for `window.dimensions` to take \
                 effect. Configured value of `{zero_key}` is 0 while that of `{non_zero_key}` is {non_zero_value}",
            );

            None
        } else {
            None
        }
    }

    /// Padding around the terminal grid, in physical pixels.
    #[inline]
    pub fn padding(&self, scale_factor: f32, cell_width: f32, cell_height: f32) -> WindowPadding {
        match self.padding {
            Padding::Pixels(Delta { x, y }) => {
                let pixels = |value: u16| (f32::from(value) * scale_factor).floor();
                WindowPadding::symmetric(pixels(x), pixels(y))
            },
            // Cells are already in physical pixels.
            Padding::Cells(Spacing { top, right, bottom, left }) => WindowPadding {
                top: f32::from(top) * cell_height,
                right: f32::from(right) * cell_width,
                bottom: f32::from(bottom) * cell_height,
                left: f32::from(left) * cell_width,
            },
        }
    }

    /// Configured padding, to detect changes.
    #[inline]
    pub fn padding_config(&self) -> Padding {
        self.padding
    }

    #[inline]
    pub fn fullscreen(&self) -> Option<Fullscreen> {
        if self.startup_mode == StartupMode::Fullscreen {
            Some(Fullscreen::Borderless(None))
        } else {
            None
        }
    }

    #[inline]
    pub fn maximized(&self) -> bool {
        self.startup_mode == StartupMode::Maximized
    }

    #[cfg(target_os = "macos")]
    pub fn option_as_alt(&self) -> WinitOptionAsAlt {
        match self.option_as_alt {
            OptionAsAlt::OnlyLeft => WinitOptionAsAlt::OnlyLeft,
            OptionAsAlt::OnlyRight => WinitOptionAsAlt::OnlyRight,
            OptionAsAlt::Both => WinitOptionAsAlt::Both,
            OptionAsAlt::None => WinitOptionAsAlt::None,
        }
    }

    pub fn theme(&self) -> Option<WinitTheme> {
        self.decorations_theme_variant.map(WinitTheme::from)
    }
}

#[derive(ConfigDeserialize, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// Window title.
    pub title: String,

    /// Window class.
    pub class: Class,
}

impl Default for Identity {
    fn default() -> Self {
        Self { title: DEFAULT_NAME.into(), class: Default::default() }
    }
}

#[derive(ConfigDeserialize, Serialize, Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum StartupMode {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
    SimpleFullscreen,
}

#[derive(ConfigDeserialize, Serialize, Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum Decorations {
    #[default]
    Full,
    Transparent,
    Buttonless,
    None,
}

/// Window Dimensions.
///
/// Newtype to avoid passing values incorrectly.
#[derive(ConfigDeserialize, Serialize, Default, Debug, Copy, Clone, PartialEq, Eq)]
pub struct Dimensions {
    /// Window width in character columns.
    pub columns: usize,

    /// Window Height in character lines.
    pub lines: usize,
}

/// Window class hint.
#[derive(SerdeReplace, Serialize, Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub general: String,
    pub instance: String,
}

impl Class {
    pub fn new(general: impl ToString, instance: impl ToString) -> Self {
        Self { general: general.to_string(), instance: instance.to_string() }
    }
}

impl Default for Class {
    fn default() -> Self {
        Self::new(DEFAULT_NAME, DEFAULT_NAME)
    }
}

impl<'de> Deserialize<'de> for Class {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ClassVisitor;
        impl<'a> Visitor<'a> for ClassVisitor {
            type Value = Class;

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a mapping")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Self::Value { instance: value.into(), ..Self::Value::default() })
            }

            fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
            where
                M: MapAccess<'a>,
            {
                let mut class = Self::Value::default();

                while let Some((key, value)) = map.next_entry::<String, toml::Value>()? {
                    match key.as_str() {
                        "instance" => match String::deserialize(value) {
                            Ok(instance) => class.instance = instance,
                            Err(err) => {
                                error!(
                                    target: LOG_TARGET_CONFIG,
                                    "Config error: class.instance: {err}"
                                );
                            },
                        },
                        "general" => match String::deserialize(value) {
                            Ok(general) => class.general = general,
                            Err(err) => {
                                error!(
                                    target: LOG_TARGET_CONFIG,
                                    "Config error: class.instance: {err}"
                                );
                            },
                        },
                        key => warn!(target: LOG_TARGET_CONFIG, "Unrecognized class field: {key}"),
                    }
                }

                Ok(class)
            }
        }

        deserializer.deserialize_any(ClassVisitor)
    }
}

#[derive(ConfigDeserialize, Serialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionAsAlt {
    /// The left `Option` key is treated as `Alt`.
    OnlyLeft,

    /// The right `Option` key is treated as `Alt`.
    OnlyRight,

    /// Both `Option` keys are treated as `Alt`.
    Both,

    /// No special handling is applied for `Option` key.
    #[default]
    None,
}

/// System decorations theme variant.
#[derive(ConfigDeserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl From<Theme> for WinitTheme {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => WinitTheme::Light,
            Theme::Dark => WinitTheme::Dark,
        }
    }
}

#[derive(ConfigDeserialize, Serialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowLevel {
    #[default]
    Normal,
    AlwaysOnTop,
}

impl From<WindowLevel> for WinitWindowLevel {
    fn from(level: WindowLevel) -> Self {
        match level {
            WindowLevel::Normal => WinitWindowLevel::Normal,
            WindowLevel::AlwaysOnTop => WinitWindowLevel::AlwaysOnTop,
        }
    }
}

/// Space around the terminal grid.
#[derive(SerdeReplace, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Padding {
    /// Alacritty's `{ x, y }` form: pixels on the left and right, and on the top and bottom.
    Pixels(Delta<u16>),
    /// Lip Gloss and CSS shorthand: 1 to 4 numbers of terminal cells.
    Cells(Spacing),
}

impl Default for Padding {
    fn default() -> Self {
        Self::Pixels(Delta::default())
    }
}

impl<'de> Deserialize<'de> for Padding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = toml::Value::deserialize(deserializer)?;
        if value.is_table() {
            Delta::deserialize(value).map(Self::Pixels).map_err(de::Error::custom)
        } else {
            Spacing::deserialize(value).map(Self::Cells).map_err(de::Error::custom)
        }
    }
}

/// Space around the four sides of the grid, in terminal cells.
///
/// Like Lip Gloss's `Padding`, it accepts CSS shorthand: a number for all sides, or a list of 1 to
/// 4 numbers.
#[derive(Serialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spacing {
    pub top: u16,
    pub right: u16,
    pub bottom: u16,
    pub left: u16,
}

impl Spacing {
    /// Expand CSS shorthand:
    ///
    /// - `[all]`
    /// - `[vertical, horizontal]`
    /// - `[top, horizontal, bottom]`
    /// - `[top, right, bottom, left]`
    pub fn from_shorthand(values: &[u16]) -> Result<Self, String> {
        let (top, right, bottom, left) = match *values {
            [all] => (all, all, all, all),
            [vertical, horizontal] => (vertical, horizontal, vertical, horizontal),
            [top, horizontal, bottom] => (top, horizontal, bottom, horizontal),
            [top, right, bottom, left] => (top, right, bottom, left),
            _ => return Err(format!("expected 1 to 4 values, got {}", values.len())),
        };

        Ok(Self { top, right, bottom, left })
    }
}

impl<'de> Deserialize<'de> for Spacing {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SpacingVisitor;

        impl<'de> Visitor<'de> for SpacingVisitor {
            type Value = Spacing;

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str(
                    "{ x = <pixels>, y = <pixels> }, a number of cells, or a list of 1 to 4 \
                     numbers of cells like CSS padding",
                )
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Spacing, E> {
                let value = u16::try_from(value).map_err(E::custom)?;
                Ok(Spacing { top: value, right: value, bottom: value, left: value })
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Spacing, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element::<u16>()? {
                    values.push(value);
                }
                Spacing::from_shorthand(&values).map_err(de::Error::custom)
            }
        }

        deserializer.deserialize_any(SpacingVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::UiConfig;

    fn padding(toml: &str) -> Padding {
        let config: UiConfig = toml::from_str(&format!("[window]\npadding = {toml}\n")).unwrap();
        config.window.padding
    }

    fn cells(top: u16, right: u16, bottom: u16, left: u16) -> Padding {
        Padding::Cells(Spacing { top, right, bottom, left })
    }

    #[test]
    fn alacritty_pixel_padding() {
        assert_eq!(padding("{ x = 3, y = 4 }"), Padding::Pixels(Delta { x: 3, y: 4 }));
    }

    #[test]
    fn shorthand_follows_css() {
        assert_eq!(padding("1"), cells(1, 1, 1, 1));
        assert_eq!(padding("[1]"), cells(1, 1, 1, 1));
        assert_eq!(padding("[1, 2]"), cells(1, 2, 1, 2));
        assert_eq!(padding("[1, 2, 3]"), cells(1, 2, 3, 2));
        assert_eq!(padding("[1, 2, 3, 4]"), cells(1, 2, 3, 4));
    }

    #[test]
    fn invalid_shorthand_keeps_default() {
        assert_eq!(padding("[1, 2, 3, 4, 5]"), Padding::default());
        assert_eq!(padding("[]"), Padding::default());
        assert_eq!(padding("-1"), Padding::default());
    }

    #[test]
    fn pixel_padding_scales_with_display() {
        let window =
            WindowConfig { padding: Padding::Pixels(Delta { x: 3, y: 5 }), ..Default::default() };

        assert_eq!(window.padding(2.5, 10., 20.), WindowPadding::symmetric(7., 12.));
    }

    #[test]
    fn cell_padding_uses_cell_size() {
        let window = WindowConfig { padding: cells(1, 2, 3, 4), ..Default::default() };

        let padding = window.padding(2., 10., 20.);

        assert_eq!(padding, WindowPadding { top: 20., right: 20., bottom: 60., left: 40. });
    }
}
