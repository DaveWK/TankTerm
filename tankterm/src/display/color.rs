use std::cell::RefCell;
use std::fmt::{self, Display, Formatter};
use std::ops::{Add, Deref, Mul};
use std::str::FromStr;

use serde::de::{Error as SerdeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use tankterm_config_derive::SerdeReplace;
use tankterm_terminal::vte::ansi::Rgb as VteRgb;

use crate::config::color::{ColorMode, Colors};
use crate::display::charmtone;

/// Factor for automatic computation of dim colors.
pub const DIM_FACTOR: f32 = 0.66;

#[derive(SerdeReplace, Debug, Eq, PartialEq, Copy, Clone, Default)]
pub struct Rgb(pub VteRgb);

impl Rgb {
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self(VteRgb { r, g, b })
    }

    /// Whether this is a dark background color.
    ///
    /// Like Lip Gloss's `HasDarkBackground`, a color is dark when its HSL lightness is below 50%.
    pub fn is_dark(self) -> bool {
        let max = self.r.max(self.g).max(self.b);
        let min = self.r.min(self.g).min(self.b);
        u16::from(max) + u16::from(min) < 255
    }

    /// Color of an xterm 256 color palette index above the 16 named colors.
    fn xterm_256(index: u8) -> Rgb {
        match index {
            16..=231 => {
                let level = |value: u8| if value == 0 { 0 } else { value * 40 + 55 };
                let index = index - 16;
                Rgb::new(level(index / 36), level(index / 6 % 6), level(index % 6))
            },
            232..=255 => {
                let value = (index - 232) * 10 + 8;
                Rgb::new(value, value, value)
            },
            _ => unreachable!("named colors come from the palette"),
        }
    }
}

impl From<VteRgb> for Rgb {
    fn from(value: VteRgb) -> Self {
        Self(value)
    }
}

impl Deref for Rgb {
    type Target = VteRgb;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Mul<f32> for Rgb {
    type Output = Rgb;

    fn mul(self, rhs: f32) -> Self::Output {
        Rgb(self.0 * rhs)
    }
}

impl Add<Rgb> for Rgb {
    type Output = Rgb;

    fn add(self, rhs: Rgb) -> Self::Output {
        Rgb(self.0 + rhs.0)
    }
}

/// Deserialize Rgb color from a hex string.
/// What Lip Gloss style color values resolve against while the configuration is read.
///
/// ANSI palette indices and light/dark pairs depend on the configured palette and background, so
/// the config loader reads those first and deserializes the rest of the configuration inside
/// [`ColorContext::scope`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorContext {
    /// Whether light/dark pairs use their dark variant.
    pub dark: bool,

    /// The 16 named colors, for ANSI indices 0 to 15.
    pub palette: [Rgb; 16],

    /// Configured `indexed_colors`, overriding the xterm colors for indices 16 to 255.
    pub indexed: Vec<(u8, Rgb)>,
}

thread_local! {
    static COLOR_CONTEXT: RefCell<ColorContext> = RefCell::new(ColorContext::default());
}

impl Default for ColorContext {
    fn default() -> Self {
        Self::from_colors(&Colors::default())
    }
}

impl ColorContext {
    /// Context for an already loaded configuration, used for CLI and IPC overrides.
    pub fn from_colors(colors: &Colors) -> Self {
        let (normal, bright) = (&colors.normal, &colors.bright);
        let palette = [
            normal.black,
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
        ];
        let indexed =
            colors.indexed_colors.iter().map(|color| (color.index(), color.color)).collect();

        Self { dark: Self::is_dark(colors.mode, colors.primary.background), palette, indexed }
    }

    /// Context for a configuration that hasn't been deserialized yet.
    ///
    /// Only the palette, `indexed_colors`, `mode` and the primary background are read. Values
    /// that fail to parse are skipped here and reported when the full configuration is read.
    pub fn from_value(config: &toml::Value) -> Self {
        // Colors referencing the palette inside the palette itself resolve against the defaults.
        Self::default().scope(|| {
            let defaults = Colors::default();
            let mut context = Self::from_colors(&defaults);

            let Some(colors) = config.get("colors") else { return context };
            let color = |table: &str, name: &str| -> Option<Rgb> {
                Rgb::deserialize(colors.get(table)?.get(name)?.clone()).ok()
            };

            const NAMES: [&str; 8] =
                ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
            for (index, name) in NAMES.iter().enumerate() {
                if let Some(color) = color("normal", name) {
                    context.palette[index] = color;
                }
                if let Some(color) = color("bright", name) {
                    context.palette[index + 8] = color;
                }
            }

            if let Some(indexed) = colors.get("indexed_colors").and_then(|value| value.as_array()) {
                context.indexed = indexed
                    .iter()
                    .filter_map(|entry| {
                        let index = u8::try_from(entry.get("index")?.as_integer()?).ok()?;
                        Some((index, Rgb::deserialize(entry.get("color")?.clone()).ok()?))
                    })
                    .collect();
            }

            let mode =
                colors.get("mode").and_then(|mode| ColorMode::deserialize(mode.clone()).ok());
            let background = color("primary", "background").unwrap_or(defaults.primary.background);
            context.dark = Self::is_dark(mode, background);

            context
        })
    }

    /// Resolve colors against this context while running `f`.
    pub fn scope<T>(self, f: impl FnOnce() -> T) -> T {
        let previous = COLOR_CONTEXT.with(|context| context.replace(self));
        let result = f();
        COLOR_CONTEXT.with(|context| context.replace(previous));
        result
    }

    /// Color of an ANSI palette index.
    pub fn ansi(&self, index: u8) -> Rgb {
        match self.indexed.iter().find(|(indexed, _)| *indexed == index) {
            Some((_, color)) if index >= 16 => *color,
            _ if index < 16 => self.palette[usize::from(index)],
            _ => Rgb::xterm_256(index),
        }
    }

    fn is_dark(mode: Option<ColorMode>, background: Rgb) -> bool {
        match mode {
            Some(ColorMode::Dark) => true,
            Some(ColorMode::Light) => false,
            None => background.is_dark(),
        }
    }

    fn current() -> Self {
        COLOR_CONTEXT.with(|context| context.borrow().clone())
    }
}

/// Names of the 16 ANSI colors, like Lip Gloss's color constants.
const ANSI_NAMES: [&str; 16] = [
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "magenta",
    "cyan",
    "white",
    "brightblack",
    "brightred",
    "brightgreen",
    "brightyellow",
    "brightblue",
    "brightmagenta",
    "brightcyan",
    "brightwhite",
];

/// Resolve an ANSI color name, from the configured palette, or a CharmTone color name.
fn named_color(name: &str) -> Option<Rgb> {
    let normalized: String = name
        .chars()
        .filter(|c| !matches!(c, '_' | '-' | ' '))
        .flat_map(char::to_lowercase)
        .collect();

    match ANSI_NAMES.iter().position(|ansi| *ansi == normalized) {
        Some(index) => Some(ColorContext::current().ansi(index as u8)),
        None => charmtone::color(name),
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RgbVisitor;

        // Used for deserializing reftests.
        #[derive(Deserialize)]
        struct RgbDerivedDeser {
            r: u8,
            g: u8,
            b: u8,
        }

        impl Visitor<'_> for RgbVisitor {
            type Value = Rgb;

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str(EXPECTING)
            }

            fn visit_str<E>(self, value: &str) -> Result<Rgb, E>
            where
                E: serde::de::Error,
            {
                // ANSI palette index, like `"5"` or `"201"` in Lip Gloss.
                if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return value
                        .parse::<u8>()
                        .map(|index| ColorContext::current().ansi(index))
                        .map_err(|_| E::custom(format!("ANSI color {value} is not in 0..=255")));
                }

                if let Some(rgb) = named_color(value) {
                    return Ok(rgb);
                }

                Rgb::from_str(value).map_err(|_| {
                    E::custom(format!("failed to parse color {value}; expected {EXPECTING}"))
                })
            }

            fn visit_i64<E>(self, value: i64) -> Result<Rgb, E>
            where
                E: serde::de::Error,
            {
                u8::try_from(value)
                    .map(|index| ColorContext::current().ansi(index))
                    .map_err(|_| E::custom(format!("ANSI color {value} is not in 0..=255")))
            }
        }

        const EXPECTING: &str = "hex color like #ff00ff, ANSI color like \"5\", \"201\" or \
                                 \"bright_red\", CharmTone name like \"charple\", or { light = \
                                 <color>, dark = <color> }";

        // Return an error if the syntax is incorrect.
        let value = toml::Value::deserialize(deserializer)?;

        // Attempt to deserialize from struct form.
        if let Ok(RgbDerivedDeser { r, g, b }) = RgbDerivedDeser::deserialize(value.clone()) {
            return Ok(Rgb::new(r, g, b));
        }

        // Light/dark pair, picked by the background like Lip Gloss's `LightDark`.
        if let Some(table) = value.as_table() {
            let variant = if ColorContext::current().dark { "dark" } else { "light" };
            return match (table.get("light"), table.get("dark")) {
                (Some(_), Some(_)) if table.len() == 2 => {
                    Rgb::deserialize(table[variant].clone()).map_err(D::Error::custom)
                },
                _ => Err(D::Error::custom(format!("expected {EXPECTING}"))),
            };
        }

        // Hex notation (either 0xff00ff or #ff00ff), or an ANSI index.
        match value {
            toml::Value::Integer(index) => RgbVisitor.visit_i64::<D::Error>(index),
            value => value.deserialize_any(RgbVisitor).map_err(D::Error::custom),
        }
    }
}

/// Serialize Rgb color to a hex string.
impl Serialize for Rgb {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl Display for Rgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl FromStr for Rgb {
    type Err = ();

    fn from_str(s: &str) -> Result<Rgb, ()> {
        let chars = if s.starts_with("0x") && s.len() == 8 {
            &s[2..]
        } else if s.starts_with('#') && s.len() == 7 {
            &s[1..]
        } else {
            return Err(());
        };

        match u32::from_str_radix(chars, 16) {
            Ok(mut color) => {
                let b = (color & 0xff) as u8;
                color >>= 8;
                let g = (color & 0xff) as u8;
                color >>= 8;
                let r = color as u8;
                Ok(Rgb::new(r, g, b))
            },
            Err(_) => Err(()),
        }
    }
}

/// RGB color optionally referencing the cell's foreground or background.
#[derive(SerdeReplace, Serialize, Copy, Clone, Debug, PartialEq, Eq)]
pub enum CellRgb {
    CellForeground,
    CellBackground,
    #[serde(untagged)]
    Rgb(Rgb),
}

impl CellRgb {
    pub fn color(self, foreground: Rgb, background: Rgb) -> Rgb {
        match self {
            Self::CellForeground => foreground,
            Self::CellBackground => background,
            Self::Rgb(rgb) => rgb,
        }
    }
}

impl Default for CellRgb {
    fn default() -> Self {
        Self::Rgb(Rgb::default())
    }
}

impl<'de> Deserialize<'de> for CellRgb {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = toml::Value::deserialize(deserializer)?;

        // Attempt to deserialize as enum constants.
        match value.as_str() {
            Some("CellForeground") => return Ok(CellRgb::CellForeground),
            Some("CellBackground") => return Ok(CellRgb::CellBackground),
            _ => (),
        }

        Rgb::deserialize(value).map(CellRgb::Rgb).map_err(|err| {
            D::Error::custom(format!("{err}; CellForeground and CellBackground are also allowed"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: &str) -> Result<Rgb, toml::de::Error> {
        let value: toml::Value = toml::from_str(&format!("color = {value}")).unwrap();
        Rgb::deserialize(value["color"].clone())
    }

    fn context(dark: bool) -> ColorContext {
        let mut context = ColorContext { dark, ..ColorContext::default() };
        context.palette[5] = Rgb::new(0x12, 0x34, 0x56);
        context
    }

    #[test]
    fn hex_colors() {
        assert_eq!(parse("\"#ff00ff\"").unwrap(), Rgb::new(0xff, 0x00, 0xff));
        assert_eq!(parse("\"0x00ff00\"").unwrap(), Rgb::new(0x00, 0xff, 0x00));
        assert!(parse("\"#ff00f\"").is_err());
        assert!(parse("\"#gggggg\"").is_err());
    }

    #[test]
    fn ansi_16_uses_palette() {
        context(true).scope(|| {
            assert_eq!(parse("\"5\"").unwrap(), Rgb::new(0x12, 0x34, 0x56));
            assert_eq!(parse("5").unwrap(), Rgb::new(0x12, 0x34, 0x56));
        });
    }

    #[test]
    fn ansi_256_colors() {
        // The 6x6x6 cube and the gray ramp, as in xterm.
        assert_eq!(parse("\"16\"").unwrap(), Rgb::new(0x00, 0x00, 0x00));
        assert_eq!(parse("\"196\"").unwrap(), Rgb::new(0xff, 0x00, 0x00));
        assert_eq!(parse("\"201\"").unwrap(), Rgb::new(0xff, 0x00, 0xff));
        assert_eq!(parse("\"231\"").unwrap(), Rgb::new(0xff, 0xff, 0xff));
        assert_eq!(parse("\"232\"").unwrap(), Rgb::new(0x08, 0x08, 0x08));
        assert_eq!(parse("\"255\"").unwrap(), Rgb::new(0xee, 0xee, 0xee));
        assert!(parse("\"256\"").is_err());
        assert!(parse("-1").is_err());
    }

    #[test]
    fn indexed_colors_override_256() {
        let context =
            ColorContext { indexed: vec![(201, Rgb::new(1, 2, 3))], ..Default::default() };
        context.scope(|| assert_eq!(parse("\"201\"").unwrap(), Rgb::new(1, 2, 3)));
    }

    #[test]
    fn light_dark_pairs_follow_context() {
        let pair = "{ light = \"#ffffff\", dark = \"#000000\" }";
        context(true).scope(|| assert_eq!(parse(pair).unwrap(), Rgb::new(0, 0, 0)));
        context(false).scope(|| assert_eq!(parse(pair).unwrap(), Rgb::new(0xff, 0xff, 0xff)));
    }

    #[test]
    fn light_dark_pairs_nest_other_forms() {
        let pair = "{ light = \"5\", dark = \"196\" }";
        context(false).scope(|| assert_eq!(parse(pair).unwrap(), Rgb::new(0x12, 0x34, 0x56)));
        context(true).scope(|| assert_eq!(parse(pair).unwrap(), Rgb::new(0xff, 0, 0)));
    }

    #[test]
    fn incomplete_light_dark_pairs_are_rejected() {
        assert!(parse("{ light = \"#ffffff\" }").is_err());
        assert!(parse("{ light = \"#ffffff\", dark = \"#000000\", extra = 1 }").is_err());
    }

    #[test]
    fn ansi_names_use_palette() {
        context(true).scope(|| {
            assert_eq!(parse("\"magenta\"").unwrap(), Rgb::new(0x12, 0x34, 0x56));
            let bright_red = ColorContext::current().ansi(9);
            for name in ["bright_red", "bright-red", "BrightRed", "brightred"] {
                assert_eq!(parse(&format!("\"{name}\"")).unwrap(), bright_red, "{name}");
            }
        });
    }

    #[test]
    fn charmtone_names() {
        assert_eq!(parse("\"charple\"").unwrap(), Rgb::new(0x6b, 0x50, 0xff));
        assert_eq!(parse("\"Dolly\"").unwrap(), Rgb::new(0xff, 0x60, 0xff));
        assert!(parse("\"not_a_color\"").is_err());
    }

    #[test]
    fn names_work_in_light_dark_pairs() {
        let pair = "{ light = \"pepper\", dark = \"butter\" }";
        context(true).scope(|| assert_eq!(parse(pair).unwrap(), Rgb::new(0xff, 0xfa, 0xf1)));
        context(false).scope(|| assert_eq!(parse(pair).unwrap(), Rgb::new(0x20, 0x1f, 0x26)));
    }

    #[test]
    fn struct_form_still_works() {
        // Used by ref tests.
        assert_eq!(parse("{ r = 1, g = 2, b = 3 }").unwrap(), Rgb::new(1, 2, 3));
    }

    #[test]
    fn dark_detection_matches_lipgloss() {
        assert!(Rgb::new(0, 0, 0).is_dark());
        assert!(Rgb::new(0x18, 0x18, 0x18).is_dark());
        assert!(!Rgb::new(0xff, 0xff, 0xff).is_dark());
        assert!(!Rgb::new(0xf1, 0xff, 0xf5).is_dark());
        // HSL lightness of exactly 50% isn't dark.
        assert!(!Rgb::new(0x00, 0xff, 0x00).is_dark());
    }

    #[test]
    fn cell_rgb_accepts_all_color_forms() {
        let parse_cell = |value: &str| {
            let value: toml::Value = toml::from_str(&format!("color = {value}")).unwrap();
            CellRgb::deserialize(value["color"].clone())
        };

        context(true).scope(|| {
            assert_eq!(parse_cell("\"CellForeground\"").unwrap(), CellRgb::CellForeground);
            assert_eq!(parse_cell("\"CellBackground\"").unwrap(), CellRgb::CellBackground);
            assert_eq!(parse_cell("\"#010203\"").unwrap(), CellRgb::Rgb(Rgb::new(1, 2, 3)));
            assert_eq!(parse_cell("\"5\"").unwrap(), CellRgb::Rgb(Rgb::new(0x12, 0x34, 0x56)));
            assert_eq!(
                parse_cell("{ light = \"#ffffff\", dark = \"#000001\" }").unwrap(),
                CellRgb::Rgb(Rgb::new(0, 0, 1))
            );
            assert!(parse_cell("\"Cellforeground\"").is_err());
        });
    }
}
