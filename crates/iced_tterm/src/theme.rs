use crate::settings::ThemeSettings;
use iced::{widget::container, Color};
use std::collections::HashMap;
use tankterm_terminal::term::color::{self, Colors};
use tankterm_terminal::term::ColorScheme;
use tankterm_terminal::vte::ansi::{self, NamedColor, Rgb};

pub(crate) trait TerminalStyle {
    /// The widget container's style. `overrides` are the colors the
    /// application set at runtime; an OSC 11 background replaces the theme's.
    fn container_style(&self, overrides: &Colors) -> container::Style;
}

/// A color of something drawn on a cell: one of the cell's own colors, or a
/// color of its own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CellColor {
    CellForeground,
    CellBackground,
    Color(Color),
}

impl CellColor {
    /// The color on a cell with these colors.
    pub(crate) fn on(self, foreground: Color, background: Color) -> Color {
        match self {
            Self::CellForeground => foreground,
            Self::CellBackground => background,
            Self::Color(color) => color,
        }
    }
}

/// The text and background colors of something drawn on cells, like the
/// cursor or the selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellColors {
    pub foreground: CellColor,
    pub background: CellColor,
}

impl Default for CellColors {
    /// The cell's colors swapped.
    fn default() -> Self {
        Self {
            foreground: CellColor::CellBackground,
            background: CellColor::CellForeground,
        }
    }
}

/// Text attributes of something drawn on cells, added to the cells' own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextAttributes {
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// Swap the text and background colors.
    pub reverse: bool,
}

#[derive(Debug, Clone)]
pub struct ColorPalette {
    pub foreground: String,
    pub background: String,
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
    pub bright_foreground: Option<String>,
    pub dim_foreground: String,
    pub dim_black: String,
    pub dim_red: String,
    pub dim_green: String,
    pub dim_yellow: String,
    pub dim_blue: String,
    pub dim_magenta: String,
    pub dim_cyan: String,
    pub dim_white: String,
    /// Colors replacing entries `16..=255` of the 256 color table.
    pub indexed_colors: Vec<(u8, String)>,
    /// Draw bold text in the normal colors with the bright colors instead.
    pub draw_bold_text_with_bright_colors: bool,
    /// The block cursor and the text under it. A cursor color an
    /// application sets wins.
    pub cursor: CellColors,
    /// Selected text.
    pub selection: CellColors,
    /// Attributes selected text is drawn with, besides its own.
    pub selection_attributes: TextAttributes,
}

impl Default for ColorPalette {
    fn default() -> Self {
        Self {
            foreground: String::from("#d8d8d8"),
            background: String::from("#181818"),
            black: String::from("#181818"),
            red: String::from("#ac4242"),
            green: String::from("#90a959"),
            yellow: String::from("#f4bf75"),
            blue: String::from("#6a9fb5"),
            magenta: String::from("#aa759f"),
            cyan: String::from("#75b5aa"),
            white: String::from("#d8d8d8"),
            bright_black: String::from("#6b6b6b"),
            bright_red: String::from("#c55555"),
            bright_green: String::from("#aac474"),
            bright_yellow: String::from("#feca88"),
            bright_blue: String::from("#82b8c8"),
            bright_magenta: String::from("#c28cb8"),
            bright_cyan: String::from("#93d3c3"),
            bright_white: String::from("#f8f8f8"),
            bright_foreground: None,
            dim_foreground: String::from("#828482"),
            dim_black: String::from("#0f0f0f"),
            dim_red: String::from("#712b2b"),
            dim_green: String::from("#5f6f3a"),
            dim_yellow: String::from("#a17e4d"),
            dim_blue: String::from("#456877"),
            dim_magenta: String::from("#704d68"),
            dim_cyan: String::from("#4d7770"),
            dim_white: String::from("#8e8e8e"),
            indexed_colors: Vec::new(),
            draw_bold_text_with_bright_colors: false,
            cursor: CellColors::default(),
            selection: CellColors::default(),
            selection_attributes: TextAttributes::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Theme {
    palette: Box<ColorPalette>,
    ansi256_colors: HashMap<u8, Color>,
    /// How opaque the widget's background is drawn.
    background_opacity: f32,
}

impl Default for Theme {
    fn default() -> Self {
        Self::new(ThemeSettings::default())
    }
}

impl Theme {
    /// Whether the palette's background is dark, reported to applications.
    ///
    /// Like Lip Gloss's `HasDarkBackground`, the background is dark when its HSL lightness is
    /// below 50%.
    pub(crate) fn color_scheme(&self) -> ColorScheme {
        match hex_to_color(&self.palette.background) {
            Ok(Color { r, g, b, .. })
                if (r.max(g).max(b) + r.min(g).min(b)) / 2. >= 0.5 =>
            {
                ColorScheme::Light
            },
            _ => ColorScheme::Dark,
        }
    }

    pub fn new(settings: ThemeSettings) -> Self {
        let mut ansi256_colors = build_ansi256_colors();
        // The first 16 entries are the named colors of the palette.
        for (index, color) in &settings.color_pallete.indexed_colors {
            if *index >= 16 {
                let color = hex_to_color(color)
                    .unwrap_or_else(|_| panic!("invalid color {}", color));
                ansi256_colors.insert(*index, color);
            }
        }

        Self {
            palette: settings.color_pallete,
            ansi256_colors,
            background_opacity: settings.background_opacity.clamp(0.0, 1.0),
        }
    }

    /// The colors of the block cursor and the text under it.
    pub(crate) fn cursor_colors(&self) -> CellColors {
        self.palette.cursor
    }

    /// The colors of selected text.
    pub(crate) fn selection_colors(&self) -> CellColors {
        self.palette.selection
    }

    /// The attributes selected text is drawn with, besides its own.
    pub(crate) fn selection_attributes(&self) -> TextAttributes {
        self.palette.selection_attributes
    }

    /// The color text in `color` is drawn with when it is bold: the bright
    /// variant of a normal color if the palette asks for it, or `color`.
    pub(crate) fn bold_color(&self, color: ansi::Color) -> ansi::Color {
        if !self.palette.draw_bold_text_with_bright_colors {
            return color;
        }

        match color {
            ansi::Color::Named(
                named @ (NamedColor::Foreground
                | NamedColor::Black
                | NamedColor::Red
                | NamedColor::Green
                | NamedColor::Yellow
                | NamedColor::Blue
                | NamedColor::Magenta
                | NamedColor::Cyan
                | NamedColor::White),
            ) => ansi::Color::Named(named.to_bright()),
            ansi::Color::Indexed(index @ 0..=7) => {
                ansi::Color::Indexed(index + 8)
            },
            color => color,
        }
    }

    /// How opaque the widget's background is drawn, from `0.0` to `1.0`.
    pub(crate) fn background_opacity(&self) -> f32 {
        self.background_opacity
    }

    pub(crate) fn set_background_opacity(&mut self, opacity: f32) {
        self.background_opacity = opacity.clamp(0.0, 1.0);
    }

    pub fn get_color(&self, c: ansi::Color) -> Color {
        match c {
            ansi::Color::Spec(rgb) => Color::from_rgb8(rgb.r, rgb.g, rgb.b),
            ansi::Color::Indexed(index) => {
                if index <= 15 {
                    let color = match index {
                        // Normal terminal colors
                        0 => &self.palette.black,
                        1 => &self.palette.red,
                        2 => &self.palette.green,
                        3 => &self.palette.yellow,
                        4 => &self.palette.blue,
                        5 => &self.palette.magenta,
                        6 => &self.palette.cyan,
                        7 => &self.palette.white,
                        // Bright terminal colors
                        8 => &self.palette.bright_black,
                        9 => &self.palette.bright_red,
                        10 => &self.palette.bright_green,
                        11 => &self.palette.bright_yellow,
                        12 => &self.palette.bright_blue,
                        13 => &self.palette.bright_magenta,
                        14 => &self.palette.bright_cyan,
                        15 => &self.palette.bright_white,
                        _ => &self.palette.background,
                    };

                    return hex_to_color(color)
                        .unwrap_or_else(|_| panic!("invalid color {}", color));
                }

                // Other colors
                match self.ansi256_colors.get(&index) {
                    Some(color) => *color,
                    None => Color::from_rgb8(0, 0, 0),
                }
            },
            ansi::Color::Named(c) => {
                let color = match c {
                    NamedColor::Foreground => &self.palette.foreground,
                    NamedColor::Background => &self.palette.background,
                    // Normal terminal colors
                    NamedColor::Black => &self.palette.black,
                    NamedColor::Red => &self.palette.red,
                    NamedColor::Green => &self.palette.green,
                    NamedColor::Yellow => &self.palette.yellow,
                    NamedColor::Blue => &self.palette.blue,
                    NamedColor::Magenta => &self.palette.magenta,
                    NamedColor::Cyan => &self.palette.cyan,
                    NamedColor::White => &self.palette.white,
                    // Bright terminal colors
                    NamedColor::BrightBlack => &self.palette.bright_black,
                    NamedColor::BrightRed => &self.palette.bright_red,
                    NamedColor::BrightGreen => &self.palette.bright_green,
                    NamedColor::BrightYellow => &self.palette.bright_yellow,
                    NamedColor::BrightBlue => &self.palette.bright_blue,
                    NamedColor::BrightMagenta => &self.palette.bright_magenta,
                    NamedColor::BrightCyan => &self.palette.bright_cyan,
                    NamedColor::BrightWhite => &self.palette.bright_white,
                    NamedColor::BrightForeground => {
                        match &self.palette.bright_foreground {
                            Some(color) => color,
                            None => &self.palette.foreground,
                        }
                    },
                    // Dim terminal colors
                    NamedColor::DimForeground => &self.palette.dim_foreground,
                    NamedColor::DimBlack => &self.palette.dim_black,
                    NamedColor::DimRed => &self.palette.dim_red,
                    NamedColor::DimGreen => &self.palette.dim_green,
                    NamedColor::DimYellow => &self.palette.dim_yellow,
                    NamedColor::DimBlue => &self.palette.dim_blue,
                    NamedColor::DimMagenta => &self.palette.dim_magenta,
                    NamedColor::DimCyan => &self.palette.dim_cyan,
                    NamedColor::DimWhite => &self.palette.dim_white,
                    _ => &self.palette.background,
                };

                hex_to_color(color)
                    .unwrap_or_else(|_| panic!("invalid color {}", color))
            },
        }
    }
}

impl Theme {
    /// Resolve `c` the way the widget renders it: a color the application
    /// set at runtime (OSC 4 for the indexed palette, OSC 10/11/12 for the
    /// foreground, background and cursor) wins over the theme's palette.
    pub(crate) fn get_color_with_overrides(
        &self,
        c: ansi::Color,
        overrides: &Colors,
    ) -> Color {
        let index = match c {
            ansi::Color::Spec(_) => None,
            ansi::Color::Indexed(index) => Some(index as usize),
            ansi::Color::Named(named) => Some(named as usize),
        };

        match index
            .filter(|&i| i < color::COUNT)
            .and_then(|i| overrides[i])
        {
            Some(rgb) => Color::from_rgb8(rgb.r, rgb.g, rgb.b),
            None => self.get_color(c),
        }
    }

    /// The color the widget renders for the terminal color table entry
    /// `index`, as carried by
    /// [`tankterm_terminal::event::Event::ColorRequest`]: `0..=255` are the
    /// indexed colors and `256..` are [`NamedColor`]s (`Foreground` = 256,
    /// `Background` = 257, `Cursor` = 258, then the dim/bright variants).
    ///
    /// Returns `None` for an index outside the color table.
    pub(crate) fn rgb_for_index(
        &self,
        index: usize,
        overrides: &Colors,
    ) -> Option<Rgb> {
        if index >= color::COUNT {
            return None;
        }

        if let Some(rgb) = overrides[index] {
            return Some(rgb);
        }

        let color = if index < 256 {
            ansi::Color::Indexed(index as u8)
        } else {
            let named = named_color_from_index(index)?;
            // The palette has no cursor entry: unless the application set
            // one (OSC 12), the block cursor is painted with the cell's
            // foreground, which for default cells is the theme foreground.
            let named = if named == NamedColor::Cursor {
                NamedColor::Foreground
            } else {
                named
            };
            ansi::Color::Named(named)
        };

        let [r, g, b, _] = self.get_color(color).into_rgba8();
        Some(Rgb { r, g, b })
    }

    /// The widget's background color, honoring an OSC 11 override.
    pub(crate) fn background_with_overrides(
        &self,
        overrides: &Colors,
    ) -> Color {
        self.get_color_with_overrides(
            ansi::Color::Named(NamedColor::Background),
            overrides,
        )
    }
}

/// Map a color table index `>= 256` back to its [`NamedColor`].
fn named_color_from_index(index: usize) -> Option<NamedColor> {
    const NAMED: [NamedColor; 13] = [
        NamedColor::Foreground,
        NamedColor::Background,
        NamedColor::Cursor,
        NamedColor::DimBlack,
        NamedColor::DimRed,
        NamedColor::DimGreen,
        NamedColor::DimYellow,
        NamedColor::DimBlue,
        NamedColor::DimMagenta,
        NamedColor::DimCyan,
        NamedColor::DimWhite,
        NamedColor::BrightForeground,
        NamedColor::DimForeground,
    ];

    let named = *NAMED.get(index.checked_sub(256)?)?;
    debug_assert_eq!(named as usize, index);
    Some(named)
}

fn build_ansi256_colors() -> HashMap<u8, Color> {
    let mut ansi256_colors = HashMap::new();

    for r in 0..6 {
        for g in 0..6 {
            for b in 0..6 {
                // Reserve the first 16 colors for config.
                let index = 16 + r * 36 + g * 6 + b;
                let color = Color::from_rgb8(
                    if r == 0 { 0 } else { r * 40 + 55 },
                    if g == 0 { 0 } else { g * 40 + 55 },
                    if b == 0 { 0 } else { b * 40 + 55 },
                );
                ansi256_colors.insert(index, color);
            }
        }
    }

    let index: u8 = 232;
    for i in 0..24 {
        let value = i * 10 + 8;
        ansi256_colors.insert(index + i, Color::from_rgb8(value, value, value));
    }

    ansi256_colors
}

fn hex_to_color(hex: &str) -> anyhow::Result<Color> {
    if hex.len() != 7 {
        return Err(anyhow::format_err!("input string is in non valid format"));
    }

    let r = u8::from_str_radix(&hex[1..3], 16)?;
    let g = u8::from_str_radix(&hex[3..5], 16)?;
    let b = u8::from_str_radix(&hex[5..7], 16)?;

    Ok(Color::from_rgb8(r, g, b))
}

impl TerminalStyle for Theme {
    fn container_style(&self, overrides: &Colors) -> container::Style {
        let background = Color {
            a: self.background_opacity,
            ..self.background_with_overrides(overrides)
        };

        container::Style {
            // A background that isn't there needs no drawing.
            background: (background.a > 0.0).then(|| background.into()),
            ..container::Style::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tankterm_terminal::vte::ansi;

    #[test]
    fn background_is_drawn_at_its_opacity() {
        let theme = |opacity| {
            Theme::new(ThemeSettings {
                background_opacity: opacity,
                ..ThemeSettings::default()
            })
        };
        let background = |theme: &Theme, overrides: &Colors| {
            theme.container_style(overrides).background
        };
        let palette = Color::from_rgb8(0x18, 0x18, 0x18);
        let mut overridden = Colors::default();
        overridden[NamedColor::Background] = Some(Rgb {
            r: 0x10,
            g: 0x20,
            b: 0x30,
        });

        assert_eq!(
            background(&Theme::default(), &Colors::default()),
            Some(palette.into())
        );
        assert_eq!(
            background(&theme(0.5), &Colors::default()),
            Some(Color { a: 0.5, ..palette }.into())
        );
        // A background the application set is as transparent as the
        // theme's.
        assert_eq!(
            background(&theme(0.5), &overridden),
            Some(
                Color {
                    a: 0.5,
                    ..Color::from_rgb8(0x10, 0x20, 0x30)
                }
                .into()
            )
        );
        assert_eq!(background(&theme(0.0), &overridden), None);

        // Opacities outside of the range are the nearest one inside.
        assert_eq!(theme(7.0).background_opacity(), 1.0);
        assert_eq!(theme(-1.0).background_opacity(), 0.0);
    }

    #[test]
    fn color_scheme_follows_background() {
        let theme = |background: &str| {
            let palette = ColorPalette {
                background: background.into(),
                ..ColorPalette::default()
            };
            Theme::new(ThemeSettings::new(Box::new(palette))).color_scheme()
        };

        assert_eq!(theme("#000000"), ColorScheme::Dark);
        assert_eq!(theme("#181818"), ColorScheme::Dark);
        assert_eq!(theme("#ffffff"), ColorScheme::Light);
        assert_eq!(theme("#eff1f5"), ColorScheme::Light);
        // Unparsable backgrounds count as dark, the default.
        assert_eq!(theme("not a color"), ColorScheme::Dark);
    }

    #[test]
    fn hex_to_color_valid_convertion() {
        assert!(hex_to_color("#000000").is_ok())
    }

    #[test]
    fn hex_to_color_short_string() {
        assert!(hex_to_color("GG").is_err());
    }

    #[test]
    fn hex_to_color_long_string() {
        assert!(hex_to_color("GG000000").is_err());
    }

    #[test]
    fn hex_to_color_non_valid_hex_string() {
        assert!(hex_to_color("#KKLLOO").is_err());
    }

    #[test]
    fn get_basic_indexed_colors() {
        let default_theme = Theme::default();
        let basic_indexed_colors_map: HashMap<u8, String> = HashMap::from([
            (0, default_theme.palette.black.clone()),
            (1, default_theme.palette.red.clone()),
            (2, default_theme.palette.green.clone()),
            (3, default_theme.palette.yellow.clone()),
            (4, default_theme.palette.blue.clone()),
            (5, default_theme.palette.magenta.clone()),
            (6, default_theme.palette.cyan.clone()),
            (7, default_theme.palette.white.clone()),
            (8, default_theme.palette.bright_black.clone()),
            (9, default_theme.palette.bright_red.clone()),
            (10, default_theme.palette.bright_green.clone()),
            (11, default_theme.palette.bright_yellow.clone()),
            (12, default_theme.palette.bright_blue.clone()),
            (13, default_theme.palette.bright_magenta.clone()),
            (14, default_theme.palette.bright_cyan.clone()),
            (15, default_theme.palette.bright_white.clone()),
        ]);

        for index in 0..16 {
            let color = default_theme.get_color(ansi::Color::Indexed(index));
            let expected_color = basic_indexed_colors_map.get(&index).unwrap();
            assert_eq!(color, hex_to_color(expected_color).unwrap())
        }
    }

    fn rgb(r: u8, g: u8, b: u8) -> Rgb {
        Rgb { r, g, b }
    }

    #[test]
    fn indexed_colors_replace_the_256_color_table_above_16() {
        let palette = ColorPalette {
            indexed_colors: vec![
                (16, "#010203".into()),
                (255, "#040506".into()),
                // The first 16 are the palette's named colors.
                (1, "#070809".into()),
            ],
            ..ColorPalette::default()
        };
        let theme = Theme::new(ThemeSettings {
            color_pallete: Box::new(palette),
            ..ThemeSettings::default()
        });
        let color = |index| theme.get_color(ansi::Color::Indexed(index));

        assert_eq!(color(16), Color::from_rgb8(1, 2, 3));
        assert_eq!(color(255), Color::from_rgb8(4, 5, 6));
        assert_eq!(color(1), hex_to_color("#ac4242").unwrap());
        assert_eq!(
            color(17),
            Theme::default().get_color(ansi::Color::Indexed(17))
        );
    }

    #[test]
    fn bold_text_takes_bright_colors_only_when_asked() {
        let named = |color| ansi::Color::Named(color);
        assert_eq!(
            Theme::default().bold_color(named(NamedColor::Red)),
            named(NamedColor::Red)
        );

        let palette = ColorPalette {
            draw_bold_text_with_bright_colors: true,
            ..ColorPalette::default()
        };
        let theme = Theme::new(ThemeSettings {
            color_pallete: Box::new(palette),
            ..ThemeSettings::default()
        });
        for (color, bright) in [
            (named(NamedColor::Red), named(NamedColor::BrightRed)),
            (
                named(NamedColor::Foreground),
                named(NamedColor::BrightForeground),
            ),
            (ansi::Color::Indexed(3), ansi::Color::Indexed(11)),
            // Colors that are bright already, or not one of the eight, stay.
            (named(NamedColor::BrightRed), named(NamedColor::BrightRed)),
            (ansi::Color::Indexed(9), ansi::Color::Indexed(9)),
            (ansi::Color::Indexed(100), ansi::Color::Indexed(100)),
        ] {
            assert_eq!(theme.bold_color(color), bright, "{color:?}");
        }
    }

    #[test]
    fn cell_colors_are_the_cells_or_their_own() {
        let (fg, bg) = (Color::WHITE, Color::BLACK);
        let own = Color::from_rgb8(0, 255, 0);

        assert_eq!(CellColor::CellForeground.on(fg, bg), fg);
        assert_eq!(CellColor::CellBackground.on(fg, bg), bg);
        assert_eq!(CellColor::Color(own).on(fg, bg), own);

        // By default the cursor and the selection swap the cell's colors.
        let swapped = CellColors::default();
        assert_eq!(
            (swapped.foreground.on(fg, bg), swapped.background.on(fg, bg)),
            (bg, fg)
        );
    }

    #[test]
    fn rgb_for_index_uses_palette_without_overrides() {
        let theme = Theme::default();
        let colors = Colors::default();

        // Indexed: named ANSI colors, the 6x6x6 cube and the grayscale ramp.
        assert_eq!(
            theme.rgb_for_index(1, &colors),
            Some(rgb(0xac, 0x42, 0x42))
        );
        assert_eq!(theme.rgb_for_index(196, &colors), Some(rgb(255, 0, 0)));
        assert_eq!(theme.rgb_for_index(232, &colors), Some(rgb(8, 8, 8)));

        // Named: foreground, background, and the cursor falling back to the
        // foreground it is painted with.
        let fg = rgb(0xd8, 0xd8, 0xd8);
        let bg = rgb(0x18, 0x18, 0x18);
        assert_eq!(
            theme.rgb_for_index(NamedColor::Foreground as usize, &colors),
            Some(fg)
        );
        assert_eq!(
            theme.rgb_for_index(NamedColor::Background as usize, &colors),
            Some(bg)
        );
        assert_eq!(
            theme.rgb_for_index(NamedColor::Cursor as usize, &colors),
            Some(fg)
        );
        assert_eq!(
            theme.rgb_for_index(NamedColor::DimRed as usize, &colors),
            Some(rgb(0x71, 0x2b, 0x2b))
        );
        assert_eq!(
            theme.rgb_for_index(NamedColor::DimForeground as usize, &colors),
            Some(rgb(0x82, 0x84, 0x82))
        );

        // Out of the table: no answer rather than a panic.
        assert_eq!(theme.rgb_for_index(color::COUNT, &colors), None);
    }

    #[test]
    fn rgb_for_index_prefers_overrides() {
        let theme = Theme::default();
        let mut colors = Colors::default();
        colors[4] = Some(rgb(1, 2, 3));
        colors[NamedColor::Background] = Some(rgb(4, 5, 6));
        colors[NamedColor::Cursor] = Some(rgb(7, 8, 9));

        assert_eq!(theme.rgb_for_index(4, &colors), Some(rgb(1, 2, 3)));
        assert_eq!(
            theme.rgb_for_index(NamedColor::Background as usize, &colors),
            Some(rgb(4, 5, 6))
        );
        assert_eq!(
            theme.rgb_for_index(NamedColor::Cursor as usize, &colors),
            Some(rgb(7, 8, 9))
        );
        // Untouched entries still come from the palette.
        assert_eq!(
            theme.rgb_for_index(5, &colors),
            Some(rgb(0xaa, 0x75, 0x9f))
        );
    }

    #[test]
    fn named_color_indices_round_trip() {
        for index in 256..color::COUNT {
            let named = named_color_from_index(index).unwrap();
            assert_eq!(named as usize, index);
        }
        assert_eq!(named_color_from_index(255), None);
        assert_eq!(named_color_from_index(color::COUNT), None);
    }

    #[test]
    fn get_color_with_overrides_matches_rendering() {
        let theme = Theme::default();
        let mut colors = Colors::default();
        colors[NamedColor::Foreground] = Some(rgb(10, 20, 30));
        colors[2] = Some(rgb(40, 50, 60));

        assert_eq!(
            theme.get_color_with_overrides(
                ansi::Color::Named(NamedColor::Foreground),
                &colors
            ),
            Color::from_rgb8(10, 20, 30)
        );
        // Indexed(2) and Named(Green) are the same table entry.
        assert_eq!(
            theme.get_color_with_overrides(ansi::Color::Indexed(2), &colors),
            Color::from_rgb8(40, 50, 60)
        );
        assert_eq!(
            theme.get_color_with_overrides(
                ansi::Color::Named(NamedColor::Green),
                &colors
            ),
            Color::from_rgb8(40, 50, 60)
        );
        // Truecolor cells are never overridden.
        assert_eq!(
            theme.get_color_with_overrides(
                ansi::Color::Spec(rgb(1, 1, 1)),
                &colors
            ),
            Color::from_rgb8(1, 1, 1)
        );
        // No override: the palette.
        assert_eq!(
            theme.background_with_overrides(&colors),
            theme.get_color(ansi::Color::Named(NamedColor::Background))
        );
    }
}
