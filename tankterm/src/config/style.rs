//! Styles for the window chrome drawn around the terminal, modeled on Charm's Lip Gloss.
//!
//! Every block (`frame`, `title_bar`, `status_bar`, ...) is a [`Style`] with the same optional
//! properties as a Lip Gloss style: text attributes, colors, padding and margin with CSS-style
//! shorthand, a border and alignment. Properties that aren't set come from the selected
//! [`Preset`].

use std::fmt::{self, Formatter};

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

use tankterm_config_derive::{ConfigDeserialize, SerdeReplace};

use crate::display::color::Rgb;

/// The `[style]` table: Lip Gloss styles for each part of the window chrome.
#[derive(ConfigDeserialize, Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleConfig {
    /// Values used for every property that isn't set explicitly.
    pub preset: Preset,

    /// Box around the terminal grid.
    pub frame: Style,

    /// Header showing the window title.
    pub title_bar: Style,

    /// Footer, and the segment showing the shell.
    pub status_bar: Style,

    /// Leftmost footer segment. Unset properties also inherit from `status_bar`.
    pub status_bar_accent: Style,

    /// Rightmost footer segment, showing the terminal size. Unset properties also inherit from
    /// `status_bar`.
    pub status_bar_info: Style,
}

/// Fully resolved styles, with the preset and parent styles applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Styles {
    pub frame: Style,
    pub title_bar: Style,
    pub status_bar: Style,
    pub status_bar_accent: Style,
    pub status_bar_info: Style,
}

impl StyleConfig {
    /// Apply the preset to every unset property.
    pub fn resolve(&self) -> Styles {
        let preset = self.preset.styles();
        let status_bar = self.status_bar.with_defaults(&preset.status_bar);

        Styles {
            frame: self.frame.with_defaults(&preset.frame),
            title_bar: self.title_bar.with_defaults(&preset.title_bar),
            status_bar,
            // Segments are drawn on top of the status bar, so they inherit from it like the
            // segments in Lip Gloss's status bar example.
            status_bar_accent: self
                .status_bar_accent
                .with_defaults(&preset.status_bar_accent)
                .inherit(&status_bar),
            status_bar_info: self
                .status_bar_info
                .with_defaults(&preset.status_bar_info)
                .inherit(&status_bar),
        }
    }
}

/// A Lip Gloss style; every property is optional.
#[derive(ConfigDeserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    /// Whether the block is drawn at all. Not a Lip Gloss property; the frame is always drawn.
    pub visible: Option<bool>,

    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub faint: Option<bool>,

    pub foreground: Option<Rgb>,
    pub background: Option<Rgb>,

    /// Space inside the border, in terminal cells.
    pub padding: Option<Spacing>,

    /// Space outside the border, in terminal cells.
    pub margin: Option<Spacing>,

    /// Border style. This isn't an `Option`, since the config treats `"none"` as unset for those,
    /// and `border = "none"` has to be able to remove a preset's border.
    pub border: Border,
    pub border_foreground: Option<Rgb>,
    pub border_background: Option<Rgb>,

    /// Horizontal alignment of the block's text.
    pub align: Option<Align>,
}

impl Style {
    /// Copy every property that's unset in `self` from `defaults`.
    pub fn with_defaults(&self, defaults: &Style) -> Style {
        let base = defaults;
        Style {
            visible: self.visible.or(base.visible),
            bold: self.bold.or(base.bold),
            italic: self.italic.or(base.italic),
            faint: self.faint.or(base.faint),
            foreground: self.foreground.or(base.foreground),
            background: self.background.or(base.background),
            padding: self.padding.or(base.padding),
            margin: self.margin.or(base.margin),
            border: if self.border == Border::Unset { base.border } else { self.border },
            border_foreground: self.border_foreground.or(base.border_foreground),
            border_background: self.border_background.or(base.border_background),
            align: self.align.or(base.align),
        }
    }

    /// Copy unset text attributes, colors and alignment from `parent`, like
    /// `lipgloss.Style.Inherit`.
    ///
    /// As in Lip Gloss, padding and margins are never inherited. Borders aren't either, so a
    /// parent's border isn't repeated around every child.
    pub fn inherit(&self, parent: &Style) -> Style {
        Style {
            bold: self.bold.or(parent.bold),
            italic: self.italic.or(parent.italic),
            faint: self.faint.or(parent.faint),
            foreground: self.foreground.or(parent.foreground),
            background: self.background.or(parent.background),
            align: self.align.or(parent.align),
            ..*self
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible.unwrap_or(true)
    }

    pub fn padding(&self) -> Spacing {
        self.padding.unwrap_or_default()
    }

    pub fn margin(&self) -> Spacing {
        self.margin.unwrap_or_default()
    }

    pub fn border(&self) -> Border {
        match self.border {
            Border::Unset => Border::None,
            border => border,
        }
    }
}

/// Lip Gloss border styles.
#[derive(ConfigDeserialize, Serialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Border {
    /// Not configured; inherit the border from the preset.
    #[default]
    #[config(skip)]
    Unset,
    /// No border.
    None,
    /// Invisible border that still takes up space.
    Hidden,
    /// Thin square border, Lip Gloss `NormalBorder`.
    Normal,
    /// Thin border with rounded corners, Lip Gloss `RoundedBorder`.
    Rounded,
    /// Heavy border, Lip Gloss `ThickBorder`.
    Thick,
    /// Two thin lines, Lip Gloss `DoubleBorder`.
    Double,
    /// Solid border as wide as a terminal cell, Lip Gloss `BlockBorder`.
    Block,
    /// Lip Gloss `ASCIIBorder`, drawn like `Normal`.
    Ascii,
}

/// Horizontal text alignment.
#[derive(ConfigDeserialize, Serialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// Space around the four sides of a block, in terminal cells.
///
/// Like Lip Gloss's `Padding` and `Margin`, it accepts CSS-style shorthand: a single number for all
/// sides, or a list of 1 to 4 numbers.
#[derive(SerdeReplace, Serialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
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

    pub const fn horizontal(&self) -> u16 {
        self.left + self.right
    }

    pub const fn vertical(&self) -> u16 {
        self.top + self.bottom
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
                f.write_str("a number of cells, or a list of 1 to 4 numbers like CSS padding")
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Spacing, E> {
                let value = u16::try_from(value).map_err(E::custom)?;
                Ok(Spacing::from_shorthand(&[value]).unwrap())
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Spacing, E> {
                let value = u16::try_from(value).map_err(E::custom)?;
                Ok(Spacing::from_shorthand(&[value]).unwrap())
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

/// Built-in sets of default styles.
#[derive(SerdeReplace, Serialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// Phosphor green chrome on black, matching the default color scheme.
    #[default]
    GreenScreen,
    /// Charm's pink and purple, after the Lip Gloss examples.
    Charm,
}

impl<'de> Deserialize<'de> for Preset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.to_lowercase().replace(['-', ' '], "_").as_str() {
            "green_screen" | "greenscreen" => Ok(Self::GreenScreen),
            "charm" => Ok(Self::Charm),
            _ => Err(de::Error::custom(format!(
                "unknown style preset {value:?}, expected `green_screen` or `charm`"
            ))),
        }
    }
}

impl Preset {
    /// Styles this preset provides for every block.
    pub fn styles(self) -> Styles {
        let rgb = |hex: u32| Some(Rgb::new((hex >> 16) as u8, (hex >> 8) as u8, hex as u8));
        let spacing = |values: &[u16]| Some(Spacing::from_shorthand(values).unwrap());
        let segment = |foreground, background, bold| Style {
            bold: Some(bold),
            foreground: rgb(foreground),
            background: rgb(background),
            padding: spacing(&[0, 1]),
            ..Style::default()
        };

        match self {
            Self::GreenScreen => Styles {
                frame: Style {
                    border: Border::Rounded,
                    border_foreground: rgb(0x00ff00),
                    padding: spacing(&[0, 1]),
                    margin: spacing(&[0, 1]),
                    ..Style::default()
                },
                title_bar: Style {
                    visible: Some(true),
                    align: Some(Align::Center),
                    ..segment(0x66ff66, 0x005500, true)
                },
                status_bar: Style { visible: Some(true), ..segment(0x66ff66, 0x005500, false) },
                status_bar_accent: segment(0x000000, 0x00ff00, true),
                status_bar_info: segment(0x000000, 0x00aa00, false),
            },
            Self::Charm => Styles {
                frame: Style {
                    border: Border::Rounded,
                    border_foreground: rgb(0x7d56f4),
                    padding: spacing(&[0, 1]),
                    margin: spacing(&[0, 1]),
                    ..Style::default()
                },
                title_bar: Style {
                    visible: Some(true),
                    italic: Some(true),
                    align: Some(Align::Center),
                    ..segment(0xfff7db, 0x7d56f4, true)
                },
                status_bar: Style { visible: Some(true), ..segment(0xc1c6b2, 0x353533, false) },
                status_bar_accent: segment(0xfffdf5, 0xff5f87, true),
                status_bar_info: segment(0xfffdf5, 0xa550df, false),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::UiConfig;

    fn rgb(hex: u32) -> Option<Rgb> {
        Some(Rgb::new((hex >> 16) as u8, (hex >> 8) as u8, hex as u8))
    }

    fn spacing(top: u16, right: u16, bottom: u16, left: u16) -> Spacing {
        Spacing { top, right, bottom, left }
    }

    #[test]
    fn spacing_shorthand_follows_css() {
        assert_eq!(Spacing::from_shorthand(&[1]), Ok(spacing(1, 1, 1, 1)));
        assert_eq!(Spacing::from_shorthand(&[1, 2]), Ok(spacing(1, 2, 1, 2)));
        assert_eq!(Spacing::from_shorthand(&[1, 2, 3]), Ok(spacing(1, 2, 3, 2)));
        assert_eq!(Spacing::from_shorthand(&[1, 2, 3, 4]), Ok(spacing(1, 2, 3, 4)));
        assert!(Spacing::from_shorthand(&[]).is_err());
        assert!(Spacing::from_shorthand(&[1, 2, 3, 4, 5]).is_err());
    }

    #[test]
    fn spacing_parses_number_or_list() {
        let config: UiConfig =
            toml::from_str("[style.frame]\npadding = 2\nmargin = [0, 1, 2]\n").unwrap();

        assert_eq!(config.style.frame.padding, Some(spacing(2, 2, 2, 2)));
        assert_eq!(config.style.frame.margin, Some(spacing(0, 1, 2, 1)));
    }

    #[test]
    fn invalid_spacing_is_rejected() {
        let config: UiConfig =
            toml::from_str("[style.frame]\npadding = [1, 2, 3, 4, 5]\n").unwrap();

        // Invalid values are reported and ignored, like other config errors.
        assert_eq!(config.style.frame.padding, None);
    }

    #[test]
    fn border_names_match_lipgloss() {
        for (name, border) in [
            ("none", Border::None),
            ("hidden", Border::Hidden),
            ("normal", Border::Normal),
            ("rounded", Border::Rounded),
            ("thick", Border::Thick),
            ("double", Border::Double),
            ("block", Border::Block),
            ("ascii", Border::Ascii),
        ] {
            let config: UiConfig =
                toml::from_str(&format!("[style.frame]\nborder = \"{name}\"\n")).unwrap();
            assert_eq!(config.style.frame.border, border, "border {name}");
        }
    }

    #[test]
    fn preset_names() {
        for (name, preset) in [
            ("green_screen", Preset::GreenScreen),
            ("green-screen", Preset::GreenScreen),
            ("GreenScreen", Preset::GreenScreen),
            ("charm", Preset::Charm),
        ] {
            let config: UiConfig =
                toml::from_str(&format!("[style]\npreset = \"{name}\"\n")).unwrap();
            assert_eq!(config.style.preset, preset, "preset {name}");
        }
    }

    #[test]
    fn default_is_green_screen() {
        let styles = StyleConfig::default().resolve();

        assert_eq!(styles.frame.border(), Border::Rounded);
        assert_eq!(styles.frame.border_foreground, rgb(0x00ff00));
        assert_eq!(styles.title_bar.background, rgb(0x005500));
        assert_eq!(styles.status_bar_accent.background, rgb(0x00ff00));
        assert!(styles.title_bar.is_visible());
        assert!(styles.status_bar.is_visible());
    }

    #[test]
    fn charm_preset_uses_charm_colors() {
        let config = StyleConfig { preset: Preset::Charm, ..StyleConfig::default() };
        let styles = config.resolve();

        assert_eq!(styles.frame.border_foreground, rgb(0x7d56f4));
        assert_eq!(styles.title_bar.background, rgb(0x7d56f4));
        assert_eq!(styles.title_bar.italic, Some(true));
        assert_eq!(styles.status_bar_accent.background, rgb(0xff5f87));
        assert_eq!(styles.status_bar_info.background, rgb(0xa550df));
    }

    #[test]
    fn properties_override_preset_individually() {
        let config: UiConfig = toml::from_str(
            "[style]\npreset = \"charm\"\n\n[style.frame]\nborder = \
             \"double\"\n\n[style.title_bar]\nvisible = false\n",
        )
        .unwrap();
        let styles = config.style.resolve();

        // Overridden properties win...
        assert_eq!(styles.frame.border(), Border::Double);
        assert!(!styles.title_bar.is_visible());

        // ...and everything else still comes from the preset.
        assert_eq!(styles.frame.border_foreground, rgb(0x7d56f4));
        assert_eq!(styles.frame.padding, Some(spacing(0, 1, 0, 1)));
        assert!(styles.status_bar.is_visible());
    }

    #[test]
    fn border_none_removes_preset_border() {
        let config: UiConfig = toml::from_str("[style.frame]\nborder = \"none\"\n").unwrap();
        assert_eq!(config.style.resolve().frame.border(), Border::None);

        // Without a border setting, the preset's border is used.
        assert_eq!(StyleConfig::default().resolve().frame.border(), Border::Rounded);
    }

    #[test]
    fn segments_inherit_from_status_bar() {
        let config: UiConfig =
            toml::from_str("[style.status_bar]\nitalic = true\nforeground = \"#123456\"\n")
                .unwrap();
        let styles = config.style.resolve();

        // Unset in the preset's segment style, so it comes from the status bar.
        assert_eq!(styles.status_bar_accent.italic, Some(true));
        assert_eq!(styles.status_bar_info.italic, Some(true));

        // Set in the preset's segment style, so the status bar doesn't override it.
        assert_eq!(styles.status_bar_accent.foreground, rgb(0x000000));
    }

    #[test]
    fn segments_keep_their_own_spacing() {
        let config: UiConfig = toml::from_str("[style.status_bar]\nmargin = [1, 2]\n").unwrap();
        let styles = config.style.resolve();

        assert_eq!(styles.status_bar.margin, Some(spacing(1, 2, 1, 2)));
        assert_eq!(styles.status_bar_accent.margin, None);
        assert_eq!(styles.status_bar_accent.padding, Some(spacing(0, 1, 0, 1)));
    }

    #[test]
    fn with_defaults_only_fills_unset_properties() {
        let defaults = Style {
            bold: Some(true),
            padding: Some(spacing(1, 1, 1, 1)),
            border: Border::Thick,
            ..Style::default()
        };
        let style = Style { bold: Some(false), ..Style::default() };

        let styled = style.with_defaults(&defaults);

        assert_eq!(styled.bold, Some(false));
        assert_eq!(styled.padding, Some(spacing(1, 1, 1, 1)));
        assert_eq!(styled.border(), Border::Thick);
    }

    #[test]
    fn inherit_skips_spacing_and_borders_like_lipgloss() {
        let parent = Style {
            bold: Some(true),
            foreground: rgb(0x123456),
            padding: Some(spacing(1, 1, 1, 1)),
            margin: Some(spacing(2, 2, 2, 2)),
            border: Border::Double,
            ..Style::default()
        };

        let child = Style::default().inherit(&parent);

        assert_eq!(child.bold, Some(true));
        assert_eq!(child.foreground, rgb(0x123456));
        assert_eq!(child.padding, None);
        assert_eq!(child.margin, None);
        assert_eq!(child.border(), Border::None);
    }
}
