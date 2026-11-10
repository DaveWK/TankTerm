use serde::de::Error as SerdeError;
use serde::{Deserialize, Deserializer, Serialize};

use tankterm_config_derive::ConfigDeserialize;

use crate::display::color::{CellRgb, Rgb};

#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Colors {
    /// Whether light/dark color pairs use their dark variant. Detected from the primary
    /// background when unset.
    pub mode: Option<ColorMode>,
    pub primary: PrimaryColors,
    pub cursor: InvertedCellColors,
    pub vi_mode_cursor: InvertedCellColors,
    pub selection: InvertedCellColors,
    pub normal: NormalColors,
    pub bright: BrightColors,
    pub dim: Option<DimColors>,
    pub indexed_colors: Vec<IndexedColor>,
    pub search: SearchColors,
    pub line_indicator: LineIndicatorColors,
    pub hints: HintColors,
    pub transparent_background_colors: bool,
    pub draw_bold_text_with_bright_colors: bool,
    footer_bar: BarColors,
}

/// Black background used by the default "green screen" color scheme.
const BLACK: Rgb = Rgb::new(0x00, 0x00, 0x00);

/// Default foreground: classic green screen phosphor green.
const PHOSPHOR_GREEN: Rgb = Rgb::new(0x00, 0xff, 0x00);

impl Default for Colors {
    fn default() -> Self {
        Self {
            mode: Default::default(),
            primary: Default::default(),
            cursor: InvertedCellColors {
                foreground: CellRgb::Rgb(BLACK),
                background: CellRgb::Rgb(PHOSPHOR_GREEN),
            },
            vi_mode_cursor: InvertedCellColors {
                foreground: CellRgb::Rgb(BLACK),
                background: CellRgb::Rgb(Rgb::new(0x66, 0xff, 0x66)),
            },
            selection: InvertedCellColors {
                foreground: CellRgb::Rgb(BLACK),
                background: CellRgb::Rgb(Rgb::new(0x00, 0x80, 0x00)),
            },
            normal: Default::default(),
            bright: Default::default(),
            dim: Default::default(),
            indexed_colors: Default::default(),
            search: Default::default(),
            line_indicator: Default::default(),
            hints: Default::default(),
            transparent_background_colors: Default::default(),
            draw_bold_text_with_bright_colors: Default::default(),
            footer_bar: Default::default(),
        }
    }
}

/// Which variant of light/dark color pairs to use.
#[derive(ConfigDeserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Dark,
    Light,
}

impl Colors {
    pub fn footer_bar_foreground(&self) -> Rgb {
        self.footer_bar.foreground.unwrap_or(self.primary.background)
    }

    pub fn footer_bar_background(&self) -> Rgb {
        self.footer_bar.background.unwrap_or(self.primary.foreground)
    }
}

#[derive(ConfigDeserialize, Serialize, Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct LineIndicatorColors {
    pub foreground: Option<Rgb>,
    pub background: Option<Rgb>,
}

#[derive(ConfigDeserialize, Serialize, Default, Copy, Clone, Debug, PartialEq, Eq)]
pub struct HintColors {
    pub start: HintStartColors,
    pub end: HintEndColors,
}

#[derive(ConfigDeserialize, Serialize, Copy, Clone, Debug, PartialEq, Eq)]
pub struct HintStartColors {
    pub foreground: CellRgb,
    pub background: CellRgb,
}

impl Default for HintStartColors {
    fn default() -> Self {
        Self {
            foreground: CellRgb::Rgb(BLACK),
            background: CellRgb::Rgb(Rgb::new(0xf4, 0xd3, 0x5e)),
        }
    }
}

#[derive(ConfigDeserialize, Serialize, Copy, Clone, Debug, PartialEq, Eq)]
pub struct HintEndColors {
    pub foreground: CellRgb,
    pub background: CellRgb,
}

impl Default for HintEndColors {
    fn default() -> Self {
        Self {
            foreground: CellRgb::Rgb(BLACK),
            background: CellRgb::Rgb(Rgb::new(0xff, 0x4d, 0x5a)),
        }
    }
}

#[derive(Deserialize, Serialize, Copy, Clone, Default, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IndexedColor {
    pub color: Rgb,

    index: ColorIndex,
}

impl IndexedColor {
    #[inline]
    pub fn index(&self) -> u8 {
        self.index.0
    }
}

#[derive(Serialize, Copy, Clone, Default, Debug, PartialEq, Eq)]
struct ColorIndex(u8);

impl<'de> Deserialize<'de> for ColorIndex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let index = u8::deserialize(deserializer)?;

        if index < 16 {
            Err(SerdeError::custom(
                "Config error: indexed_color's index is {}, but a value bigger than 15 was \
                 expected; ignoring setting",
            ))
        } else {
            Ok(Self(index))
        }
    }
}

#[derive(ConfigDeserialize, Serialize, Debug, Copy, Clone, PartialEq, Eq)]
pub struct InvertedCellColors {
    #[config(alias = "text")]
    pub foreground: CellRgb,
    #[config(alias = "cursor")]
    pub background: CellRgb,
}

impl Default for InvertedCellColors {
    fn default() -> Self {
        Self { foreground: CellRgb::CellBackground, background: CellRgb::CellForeground }
    }
}

#[derive(ConfigDeserialize, Serialize, Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct SearchColors {
    pub focused_match: FocusedMatchColors,
    pub matches: MatchColors,
}

#[derive(ConfigDeserialize, Serialize, Debug, Copy, Clone, PartialEq, Eq)]
pub struct FocusedMatchColors {
    pub foreground: CellRgb,
    pub background: CellRgb,
}

impl Default for FocusedMatchColors {
    fn default() -> Self {
        Self {
            background: CellRgb::Rgb(Rgb::new(0xf4, 0xd3, 0x5e)),
            foreground: CellRgb::Rgb(BLACK),
        }
    }
}

#[derive(ConfigDeserialize, Serialize, Debug, Copy, Clone, PartialEq, Eq)]
pub struct MatchColors {
    pub foreground: CellRgb,
    pub background: CellRgb,
}

impl Default for MatchColors {
    fn default() -> Self {
        Self {
            background: CellRgb::Rgb(Rgb::new(0x00, 0xaa, 0x00)),
            foreground: CellRgb::Rgb(BLACK),
        }
    }
}

#[derive(ConfigDeserialize, Serialize, Debug, Copy, Clone, Default, PartialEq, Eq)]
pub struct BarColors {
    foreground: Option<Rgb>,
    background: Option<Rgb>,
}

#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct PrimaryColors {
    pub foreground: Rgb,
    pub background: Rgb,
    pub bright_foreground: Option<Rgb>,
    pub dim_foreground: Option<Rgb>,
}

impl Default for PrimaryColors {
    fn default() -> Self {
        PrimaryColors {
            background: BLACK,
            foreground: PHOSPHOR_GREEN,
            bright_foreground: Some(Rgb::new(0xf1, 0xff, 0xf5)),
            dim_foreground: Default::default(),
        }
    }
}

#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct NormalColors {
    pub black: Rgb,
    pub red: Rgb,
    pub green: Rgb,
    pub yellow: Rgb,
    pub blue: Rgb,
    pub magenta: Rgb,
    pub cyan: Rgb,
    pub white: Rgb,
}

impl Default for NormalColors {
    fn default() -> Self {
        NormalColors {
            black: Rgb::new(0x00, 0x00, 0x00),
            red: Rgb::new(0xff, 0x4d, 0x5a),
            green: Rgb::new(0x00, 0xaa, 0x00),
            yellow: Rgb::new(0xf4, 0xd3, 0x5e),
            blue: Rgb::new(0x4d, 0xb8, 0xff),
            magenta: Rgb::new(0xd6, 0x87, 0xff),
            cyan: Rgb::new(0x35, 0xe6, 0xd0),
            white: Rgb::new(0xd8, 0xff, 0xe7),
        }
    }
}

#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct BrightColors {
    pub black: Rgb,
    pub red: Rgb,
    pub green: Rgb,
    pub yellow: Rgb,
    pub blue: Rgb,
    pub magenta: Rgb,
    pub cyan: Rgb,
    pub white: Rgb,
}

impl Default for BrightColors {
    fn default() -> Self {
        // Brighter variants of the "green screen" palette.
        BrightColors {
            black: Rgb::new(0x40, 0x60, 0x4d),
            red: Rgb::new(0xff, 0x7a, 0x83),
            green: Rgb::new(0x66, 0xff, 0x66),
            yellow: Rgb::new(0xff, 0xe7, 0x8a),
            blue: Rgb::new(0x8a, 0xd1, 0xff),
            magenta: Rgb::new(0xe9, 0xb3, 0xff),
            cyan: Rgb::new(0x8a, 0xff, 0xf0),
            white: Rgb::new(0xf1, 0xff, 0xf5),
        }
    }
}

#[derive(ConfigDeserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct DimColors {
    pub black: Rgb,
    pub red: Rgb,
    pub green: Rgb,
    pub yellow: Rgb,
    pub blue: Rgb,
    pub magenta: Rgb,
    pub cyan: Rgb,
    pub white: Rgb,
}

impl Default for DimColors {
    fn default() -> Self {
        // Used for any field missing from a configured `[colors.dim]`. Without that table, dim
        // colors are computed from the normal ones instead.
        DimColors {
            black: Rgb::new(0x07, 0x11, 0x0b),
            red: Rgb::new(0xa8, 0x2f, 0x3a),
            green: Rgb::new(0x00, 0x55, 0x00),
            yellow: Rgb::new(0xa6, 0x8d, 0x3b),
            blue: Rgb::new(0x2a, 0x6e, 0x99),
            magenta: Rgb::new(0x80, 0x4c, 0x99),
            cyan: Rgb::new(0x21, 0x8f, 0x82),
            white: Rgb::new(0x7f, 0xaf, 0x91),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::UiConfig;

    fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb::new(r, g, b)
    }

    #[test]
    fn default_scheme_is_green_on_black() {
        let colors = Colors::default();

        assert_eq!(colors.primary.background, rgb(0x00, 0x00, 0x00));
        assert_eq!(colors.primary.foreground, rgb(0x00, 0xff, 0x00));
        assert_eq!(colors.cursor.background, CellRgb::Rgb(rgb(0x00, 0xff, 0x00)));
        assert_eq!(colors.cursor.foreground, CellRgb::Rgb(rgb(0x00, 0x00, 0x00)));
        assert_eq!(colors.vi_mode_cursor.background, CellRgb::Rgb(rgb(0x66, 0xff, 0x66)));
        assert_eq!(colors.selection.background, CellRgb::Rgb(rgb(0x00, 0x80, 0x00)));
        assert_eq!(colors.selection.foreground, CellRgb::Rgb(rgb(0x00, 0x00, 0x00)));
        assert_eq!(colors.normal.green, rgb(0x00, 0xaa, 0x00));
        assert_eq!(colors.bright.green, rgb(0x66, 0xff, 0x66));
        assert_eq!(colors.search.matches.background, CellRgb::Rgb(rgb(0x00, 0xaa, 0x00)));

        // Dim colors are derived from the normal colors unless configured.
        assert_eq!(colors.dim, None);
        assert_eq!(colors.primary.dim_foreground, None);
    }

    #[test]
    fn default_scheme_is_readable() {
        // Every default foreground must differ from the black background.
        let colors = Colors::default();
        let background = colors.primary.background;
        let normal = &colors.normal;
        let bright = &colors.bright;

        for color in [
            colors.primary.foreground,
            normal.red,
            normal.green,
            normal.yellow,
            normal.blue,
            normal.magenta,
            normal.cyan,
            normal.white,
            bright.black,
            bright.red,
            bright.green,
            bright.yellow,
            bright.blue,
            bright.magenta,
            bright.cyan,
            bright.white,
        ] {
            assert_ne!(color, background);
        }
    }

    #[test]
    fn configured_colors_override_defaults() {
        let config: UiConfig = toml::from_str(
            "[colors.primary]\nforeground = '#ffffff'\n\n[colors.normal]\nred = '#123456'\n",
        )
        .unwrap();
        let colors = &config.colors;

        assert_eq!(colors.primary.foreground, rgb(0xff, 0xff, 0xff));
        assert_eq!(colors.normal.red, rgb(0x12, 0x34, 0x56));

        // Unset fields keep the green screen defaults.
        assert_eq!(colors.primary.background, rgb(0x00, 0x00, 0x00));
        assert_eq!(colors.normal.green, rgb(0x00, 0xaa, 0x00));
        assert_eq!(colors.selection, Colors::default().selection);
    }

    #[test]
    fn partial_dim_table_uses_default_dims() {
        let config: UiConfig = toml::from_str("[colors.dim]\nred = '#010203'\n").unwrap();
        let dim = config.colors.dim.unwrap();

        assert_eq!(dim.red, rgb(0x01, 0x02, 0x03));
        assert_eq!(dim.green, rgb(0x00, 0x55, 0x00));
    }
}
