//! Lip Gloss style blocks for everything TankTerm draws on top of the terminal content.
//!
//! Each element (cursor, selection, search matches, hint labels, footer bar, line indicator) is
//! styled by a [`Style`] with Lip Gloss's properties: colors and text attributes. Styles compose
//! like Lip Gloss's `Inherit`: a style can inherit from named styles defined in `[style.defs]`,
//! taking every property it doesn't set itself.
//!
//! Every property resolves in this order:
//!
//! 1. The element's `[style.<element>]` block, then the styles it inherits from.
//! 2. The original Alacritty color setting, like `colors.selection.background`.
//! 3. TankTerm's default for the element.

use std::collections::HashMap;
use std::fmt::{self, Formatter};
use std::sync::LazyLock;

use log::warn;
use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

use tankterm_config::SerdeReplace;
use tankterm_config_derive::{ConfigDeserialize, SerdeReplace};

use crate::config::color::{BarColors, CellColors, Colors};
use crate::display::color::{CellRgb, ColorContext, Rgb};
use crate::logging::LOG_TARGET_CONFIG;

/// The `[style]` table.
#[derive(ConfigDeserialize, Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleConfig {
    /// Named styles to inherit from, like Lip Gloss style variables.
    pub defs: HashMap<String, Style>,

    pub cursor: Style,
    pub vi_mode_cursor: Style,
    pub selection: Style,
    pub search_match: Style,
    pub search_focused_match: Style,
    pub hint_start: Style,
    pub hint_end: Style,
    pub footer_bar: Style,
    pub line_indicator: Style,
    pub help_key: Style,
    pub help_desc: Style,
    pub help_separator: Style,

    /// Effective styles, resolved by [`StyleConfig::resolve`] whenever the configuration changes.
    #[config(skip)]
    #[serde(skip_serializing)]
    resolved: Resolved,
}

/// Storage for the resolved styles, which can't be set from the configuration itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Resolved(Option<Styles>);

impl<'de> Deserialize<'de> for Resolved {
    fn deserialize<D: Deserializer<'de>>(_: D) -> Result<Self, D::Error> {
        Err(de::Error::custom("resolved styles can't be configured"))
    }
}

impl SerdeReplace for Resolved {
    fn replace(&mut self, _: toml::Value) -> Result<(), Box<dyn std::error::Error>> {
        Err("resolved styles can't be configured".into())
    }
}

/// Styles of a configuration without any style settings.
static DEFAULT_STYLES: LazyLock<Styles> = LazyLock::new(|| {
    let mut config = StyleConfig::default();
    config.resolve(&Colors::default());
    config.resolved.0.unwrap()
});

impl StyleConfig {
    /// Resolve the effective style of every element.
    pub fn resolve(&mut self, colors: &Colors) {
        let defs = &self.defs;
        let style = |style: &Style| resolve_inheritance(style, defs, &mut Vec::new());

        self.resolved = Resolved(Some(Styles {
            cursor: CellStyle::new(&style(&self.cursor), &colors.cursor, defaults::CURSOR),
            vi_mode_cursor: CellStyle::new(
                &style(&self.vi_mode_cursor),
                &colors.vi_mode_cursor,
                defaults::VI_MODE_CURSOR,
            ),
            selection: CellStyle::new(
                &style(&self.selection),
                &colors.selection,
                defaults::SELECTION,
            ),
            search_match: CellStyle::new(
                &style(&self.search_match),
                &colors.search.matches,
                defaults::SEARCH_MATCH,
            ),
            search_focused_match: CellStyle::new(
                &style(&self.search_focused_match),
                &colors.search.focused_match,
                defaults::SEARCH_FOCUSED_MATCH,
            ),
            hint_start: CellStyle::new(
                &style(&self.hint_start),
                &colors.hints.start,
                defaults::HINT_START,
            ),
            hint_end: CellStyle::new(&style(&self.hint_end), &colors.hints.end, defaults::HINT_END),
            // Bars default to the primary colors, inverted.
            footer_bar: BarStyle::new(&style(&self.footer_bar), &colors.footer_bar, colors),
            line_indicator: BarStyle::new(
                &style(&self.line_indicator),
                &colors.line_indicator,
                colors,
            ),
            help_key: BarStyle::help(&style(&self.help_key), colors, defaults::HELP_KEY),
            help_desc: BarStyle::help(&style(&self.help_desc), colors, defaults::HELP_DESC),
            help_separator: BarStyle::help(
                &style(&self.help_separator),
                colors,
                defaults::HELP_SEPARATOR,
            ),
        }));
    }

    /// Effective styles of every element.
    pub fn resolved(&self) -> &Styles {
        self.resolved.0.as_ref().unwrap_or(&DEFAULT_STYLES)
    }
}

/// A Lip Gloss style; every property is optional.
#[derive(ConfigDeserialize, Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Style {
    /// Named styles from `[style.defs]` to take unset properties from, in order.
    pub inherit: Inherit,

    pub foreground: Option<CellRgb>,
    pub background: Option<CellRgb>,

    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub faint: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
    pub reverse: Option<bool>,
}

impl Style {
    /// Copy every property that's unset in `self` from `parent`, like `lipgloss.Style.Inherit`.
    pub fn inherit(&self, parent: &Style) -> Style {
        Style {
            inherit: Inherit::default(),
            foreground: self.foreground.or(parent.foreground),
            background: self.background.or(parent.background),
            bold: self.bold.or(parent.bold),
            italic: self.italic.or(parent.italic),
            faint: self.faint.or(parent.faint),
            underline: self.underline.or(parent.underline),
            strikethrough: self.strikethrough.or(parent.strikethrough),
            reverse: self.reverse.or(parent.reverse),
        }
    }

    fn attributes(&self) -> TextAttributes {
        TextAttributes {
            bold: self.bold.unwrap_or_default(),
            italic: self.italic.unwrap_or_default(),
            faint: self.faint.unwrap_or_default(),
            underline: self.underline.unwrap_or_default(),
            strikethrough: self.strikethrough.unwrap_or_default(),
            reverse: self.reverse.unwrap_or_default(),
        }
    }
}

/// Names of styles to inherit from: a single name or a list.
#[derive(SerdeReplace, Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Inherit(pub Vec<String>);

impl<'de> Deserialize<'de> for Inherit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct InheritVisitor;

        impl<'de> Visitor<'de> for InheritVisitor {
            type Value = Inherit;

            fn expecting(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str("a style name or a list of style names")
            }

            fn visit_str<E: de::Error>(self, name: &str) -> Result<Inherit, E> {
                Ok(Inherit(vec![name.to_owned()]))
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Inherit, A::Error> {
                let mut names = Vec::new();
                while let Some(name) = seq.next_element()? {
                    names.push(name);
                }
                Ok(Inherit(names))
            }
        }

        deserializer.deserialize_any(InheritVisitor)
    }
}

/// Apply `style`'s inherited styles, depth first, so earlier names take precedence.
fn resolve_inheritance(
    style: &Style,
    defs: &HashMap<String, Style>,
    stack: &mut Vec<String>,
) -> Style {
    let mut resolved = style.inherit(&Style::default());

    for name in &style.inherit.0 {
        if stack.contains(name) {
            warn!(target: LOG_TARGET_CONFIG, "Ignoring style {name:?}: it inherits from itself");
            continue;
        }

        let Some(parent) = defs.get(name) else {
            warn!(target: LOG_TARGET_CONFIG, "Ignoring unknown style {name:?} in style.defs");
            continue;
        };

        stack.push(name.clone());
        let parent = resolve_inheritance(parent, defs, stack);
        stack.pop();

        resolved = resolved.inherit(&parent);
    }

    resolved
}

/// Text attributes applied to the cells an element covers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextAttributes {
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub reverse: bool,
}

/// Effective style of an element drawn on terminal cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellStyle {
    pub foreground: CellRgb,
    pub background: CellRgb,
    pub attributes: TextAttributes,
}

impl CellStyle {
    fn new(style: &Style, colors: &CellColors, default: (CellRgb, CellRgb)) -> Self {
        Self {
            foreground: style.foreground.or(colors.foreground).unwrap_or(default.0),
            background: style.background.or(colors.background).unwrap_or(default.1),
            attributes: style.attributes(),
        }
    }
}

/// Effective style of a bar drawn across the terminal.
///
/// Bars are drawn with the regular font, so only colors apply. `CellForeground` and
/// `CellBackground` refer to the primary colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarStyle {
    pub foreground: Rgb,
    pub background: Rgb,
}

impl BarStyle {
    fn new(style: &Style, colors: &BarColors, all: &Colors) -> Self {
        let primary = &all.primary;
        let rgb = |color: CellRgb| color.color(primary.foreground, primary.background);

        let foreground = style.foreground.map(rgb).or(colors.foreground);
        let background = style.background.map(rgb).or(colors.background);
        Self {
            foreground: foreground.unwrap_or(primary.background),
            background: background.unwrap_or(primary.foreground),
        }
    }

    /// Style for part of the key help, on the primary background like Bubbles' help view.
    ///
    /// `default` is the (light, dark) foreground, picked by the color scheme like Bubbles'
    /// `DefaultStyles(isDark)`.
    fn help(style: &Style, colors: &Colors, default: (Rgb, Rgb)) -> Self {
        let primary = &colors.primary;
        let rgb = |color: CellRgb| color.color(primary.foreground, primary.background);
        let fallback = if ColorContext::from_colors(colors).dark { default.1 } else { default.0 };

        Self {
            foreground: style.foreground.map(rgb).unwrap_or(fallback),
            background: style.background.map(rgb).unwrap_or(primary.background),
        }
    }
}

/// Effective styles of every element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Styles {
    pub cursor: CellStyle,
    pub vi_mode_cursor: CellStyle,
    pub selection: CellStyle,
    pub search_match: CellStyle,
    pub search_focused_match: CellStyle,
    pub hint_start: CellStyle,
    pub hint_end: CellStyle,
    pub footer_bar: BarStyle,
    pub line_indicator: BarStyle,
    pub help_key: BarStyle,
    pub help_desc: BarStyle,
    pub help_separator: BarStyle,
}

/// Default colors of each element, matching the green screen color scheme.
mod defaults {
    use crate::display::color::{CellRgb, Rgb};

    const BLACK: CellRgb = CellRgb::Rgb(Rgb::new(0x00, 0x00, 0x00));
    const fn rgb(r: u8, g: u8, b: u8) -> CellRgb {
        CellRgb::Rgb(Rgb::new(r, g, b))
    }

    /// Text and background colors.
    pub const CURSOR: (CellRgb, CellRgb) = (BLACK, rgb(0x00, 0xff, 0x00));
    pub const VI_MODE_CURSOR: (CellRgb, CellRgb) = (BLACK, rgb(0x66, 0xff, 0x66));
    pub const SELECTION: (CellRgb, CellRgb) = (BLACK, rgb(0x00, 0x80, 0x00));
    pub const SEARCH_MATCH: (CellRgb, CellRgb) = (BLACK, rgb(0x00, 0xaa, 0x00));
    pub const SEARCH_FOCUSED_MATCH: (CellRgb, CellRgb) = (BLACK, rgb(0xf4, 0xd3, 0x5e));
    pub const HINT_START: (CellRgb, CellRgb) = (BLACK, rgb(0xf4, 0xd3, 0x5e));
    pub const HINT_END: (CellRgb, CellRgb) = (BLACK, rgb(0xff, 0x4d, 0x5a));

    /// Key help foregrounds on light and dark backgrounds, from Bubbles' help view.
    pub const HELP_KEY: (Rgb, Rgb) = (Rgb::new(0x90, 0x90, 0x90), Rgb::new(0x62, 0x62, 0x62));
    pub const HELP_DESC: (Rgb, Rgb) = (Rgb::new(0xb2, 0xb2, 0xb2), Rgb::new(0x4a, 0x4a, 0x4a));
    pub const HELP_SEPARATOR: (Rgb, Rgb) = (Rgb::new(0xda, 0xda, 0xda), Rgb::new(0x3c, 0x3c, 0x3c));
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    use crate::cli::{Options, ParsedOptions};
    use crate::config::UiConfig;

    fn rgb(r: u8, g: u8, b: u8) -> CellRgb {
        CellRgb::Rgb(Rgb::new(r, g, b))
    }

    /// Load a config file through TankTerm's startup path, including style resolution.
    fn load(content: &str) -> UiConfig {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tankterm.toml");
        fs::write(&path, content).unwrap();

        let mut options = Options::default();
        options.config_file = Some(path);
        crate::config::load(&mut options)
    }

    #[test]
    fn defaults_match_green_screen() {
        let config = load("");
        let styles = config.style.resolved();

        assert_eq!(styles.cursor.background, rgb(0x00, 0xff, 0x00));
        assert_eq!(styles.vi_mode_cursor.background, rgb(0x66, 0xff, 0x66));
        assert_eq!(styles.selection.background, rgb(0x00, 0x80, 0x00));
        assert_eq!(styles.search_match.background, rgb(0x00, 0xaa, 0x00));
        assert_eq!(styles.search_focused_match.background, rgb(0xf4, 0xd3, 0x5e));
        assert_eq!(styles.hint_start.background, rgb(0xf4, 0xd3, 0x5e));
        assert_eq!(styles.hint_end.background, rgb(0xff, 0x4d, 0x5a));
        assert_eq!(styles.selection.attributes, TextAttributes::default());

        // Bars invert the primary colors.
        assert_eq!(styles.footer_bar.foreground, Rgb::new(0x00, 0x00, 0x00));
        assert_eq!(styles.footer_bar.background, Rgb::new(0x00, 0xff, 0x00));

        // Without any config, the lazily resolved defaults are the same.
        assert_eq!(StyleConfig::default().resolved(), styles);
    }

    #[test]
    fn style_block_sets_colors_and_attributes() {
        let config = load(
            "[style.selection]\nbackground = '#123456'\nbold = true\nunderline = true\nreverse = \
             true\n",
        );
        let selection = config.style.resolved().selection;

        assert_eq!(selection.background, rgb(0x12, 0x34, 0x56));
        // Unset properties keep their defaults.
        assert_eq!(selection.foreground, rgb(0x00, 0x00, 0x00));
        assert!(selection.attributes.bold && selection.attributes.underline);
        assert!(selection.attributes.reverse && !selection.attributes.italic);
    }

    #[test]
    fn style_takes_precedence_over_colors() {
        let config = load(
            "[colors.selection]\ntext = '#111111'\nbackground = \
             '#222222'\n\n[style.selection]\nbackground = '#333333'\n",
        );
        let selection = config.style.resolved().selection;

        assert_eq!(selection.background, rgb(0x33, 0x33, 0x33));
        assert_eq!(selection.foreground, rgb(0x11, 0x11, 0x11));
    }

    #[test]
    fn partial_colors_table_keeps_other_default() {
        // Setting only one color of an Alacritty `[colors]` table keeps the default for the other.
        let config = load("[colors.selection]\ntext = '#ffffff'\n");
        let selection = config.style.resolved().selection;

        assert_eq!(selection.foreground, rgb(0xff, 0xff, 0xff));
        assert_eq!(selection.background, rgb(0x00, 0x80, 0x00));
    }

    #[test]
    fn legacy_colors_still_apply() {
        let config = load(
            "[colors.cursor]\ncursor = '#010203'\n\n[colors.hints.start]\nbackground = \
             '#040506'\n\n[colors.footer_bar]\nbackground = '#070809'\n",
        );
        let styles = config.style.resolved();

        assert_eq!(styles.cursor.background, rgb(1, 2, 3));
        assert_eq!(styles.hint_start.background, rgb(4, 5, 6));
        assert_eq!(styles.footer_bar.background, Rgb::new(7, 8, 9));
    }

    #[test]
    fn inherit_named_style() {
        let config = load(
            "[style.defs.accent]\nforeground = '#000000'\nbackground = '#00ff00'\nbold = \
             true\n\n[style.hint_start]\ninherit = 'accent'\nbackground = '#ff00ff'\n",
        );
        let hint = config.style.resolved().hint_start;

        // Own properties win, everything else comes from the parent.
        assert_eq!(hint.background, rgb(0xff, 0x00, 0xff));
        assert_eq!(hint.foreground, rgb(0x00, 0x00, 0x00));
        assert!(hint.attributes.bold);
    }

    #[test]
    fn inherit_list_earlier_wins() {
        let config = load(
            "[style.defs.first]\nbackground = '#111111'\n\n[style.defs.second]\nbackground = \
             '#222222'\nitalic = true\n\n[style.search_match]\ninherit = ['first', 'second']\n",
        );
        let search = config.style.resolved().search_match;

        assert_eq!(search.background, rgb(0x11, 0x11, 0x11));
        assert!(search.attributes.italic);
    }

    #[test]
    fn inherit_is_transitive() {
        let config = load(
            "[style.defs.base]\nunderline = true\n\n[style.defs.loud]\ninherit = 'base'\nbold = \
             true\n\n[style.hint_end]\ninherit = 'loud'\n",
        );
        let attributes = config.style.resolved().hint_end.attributes;

        assert!(attributes.bold && attributes.underline);
    }

    #[test]
    fn unknown_and_cyclic_inherits_are_ignored() {
        let config = load(
            "[style.defs.a]\ninherit = 'b'\nbold = true\n\n[style.defs.b]\ninherit = 'a'\nitalic \
             = true\n\n[style.selection]\ninherit = ['missing', 'a']\n",
        );
        let attributes = config.style.resolved().selection.attributes;

        // Resolution terminates and still applies everything it can.
        assert!(attributes.bold && attributes.italic);
    }

    #[test]
    fn bar_cell_colors_refer_to_primary_colors() {
        let config = load(
            "[colors.primary]\nforeground = '#aaaaaa'\nbackground = \
             '#111111'\n\n[style.footer_bar]\nforeground = 'CellForeground'\nbackground = \
             'CellBackground'\n",
        );
        let footer = config.style.resolved().footer_bar;

        assert_eq!(footer.foreground, Rgb::new(0xaa, 0xaa, 0xaa));
        assert_eq!(footer.background, Rgb::new(0x11, 0x11, 0x11));
    }

    #[test]
    fn style_colors_accept_lipgloss_forms() {
        let config = load(
            "[style.selection]\nbackground = '2'\nforeground = { light = '#ffffff', dark = '232' \
             }\n",
        );
        let selection = config.style.resolved().selection;

        assert_eq!(selection.background, rgb(0x00, 0xaa, 0x00));
        assert_eq!(selection.foreground, rgb(8, 8, 8));
    }

    #[test]
    fn cli_overrides_resolve_styles() {
        let mut config = load("");
        let mut overrides = ParsedOptions::from_options(&["style.selection.bold = true".into()]);

        overrides.override_config(&mut config);

        assert!(config.style.resolved().selection.attributes.bold);
    }

    #[test]
    fn help_defaults_follow_color_scheme() {
        let dark = load("");
        assert_eq!(dark.style.resolved().help_key.foreground, Rgb::new(0x62, 0x62, 0x62));
        assert_eq!(dark.style.resolved().help_desc.background, Rgb::new(0, 0, 0));

        let light = load("[colors]\nmode = 'light'\n");
        assert_eq!(light.style.resolved().help_key.foreground, Rgb::new(0x90, 0x90, 0x90));
        assert_eq!(light.style.resolved().help_separator.foreground, Rgb::new(0xda, 0xda, 0xda));
    }

    #[test]
    fn help_styles_are_configurable() {
        let config = load("[style.help_key]\nforeground = 'charple'\nbackground = 'pepper'\n");
        let key = config.style.resolved().help_key;

        assert_eq!(key.foreground, Rgb::new(0x6b, 0x50, 0xff));
        assert_eq!(key.background, Rgb::new(0x20, 0x1f, 0x26));
    }

    #[test]
    fn resolved_styles_cant_be_configured() {
        let config = load("[style]\nresolved = 1\n");
        assert_eq!(config.style.resolved(), StyleConfig::default().resolved());
    }
}
