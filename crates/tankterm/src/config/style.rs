//! Lip Gloss style blocks for everything TankTerm draws on top of the terminal content.
//!
//! Each element (cursor, selection, search matches, hint labels, footer bar, line indicator, key
//! help, the parts of the tab bar) is styled by a [`Style`] with Lip Gloss's properties: colors
//! and text attributes, and for the tab bar padding and borders. Styles compose like Lip Gloss's
//! `Inherit`: a style can inherit from named styles defined in `[style.defs]`, taking every
//! property it doesn't set itself.
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

use crate::color::{CellRgb, ColorContext, Rgb};
use crate::config::color::{BarColors, CellColors, Colors};
use crate::config::window::Spacing;
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
    pub tab_bar: Style,
    pub tab_active: Style,
    pub tab_inactive: Style,
    pub tab_button: Style,
    pub tab_hovered: Style,
    pub tab_activity: Style,
    pub tab_attention: Style,

    /// Changes to the styles while the window doesn't have focus.
    pub blurred: BlurredStyleConfig,

    /// Effective styles, resolved by [`StyleConfig::resolve`] whenever the configuration changes.
    #[config(skip)]
    #[serde(skip_serializing)]
    resolved: Resolved,
}

/// The `[style.blurred]` table: like Huh's blurred styles, every style starts as a copy of the
/// focused one, and these properties are changed on top.
#[derive(ConfigDeserialize, Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct BlurredStyleConfig {
    /// Text attributes applied to all terminal content, like `faint = true`.
    pub text: Style,

    pub cursor: Style,
    pub vi_mode_cursor: Style,
    pub selection: Style,
    pub search_match: Style,
    pub search_focused_match: Style,
    pub hint_start: Style,
    pub hint_end: Style,
    pub tab_bar: Style,
    pub tab_active: Style,
    pub tab_inactive: Style,
    pub tab_button: Style,
    pub tab_hovered: Style,
    pub tab_activity: Style,
    pub tab_attention: Style,
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

        let tab_bar = TabBarStyles::new(
            &TabBlocks {
                bar: style(&self.tab_bar),
                active: style(&self.tab_active),
                inactive: style(&self.tab_inactive),
                button: style(&self.tab_button),
                hovered: style(&self.tab_hovered),
                activity: style(&self.tab_activity),
                attention: style(&self.tab_attention),
            },
            colors,
        );

        let mut styles = Styles {
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
            blurred: ElementStyles::default(),
            tab_bar,
            blurred_tab_bar: tab_bar,
        };

        // Like Huh, the blurred styles start as copies of the focused ones.
        let blurred = &self.blurred;
        let focused = styles.elements(true);
        styles.blurred = ElementStyles {
            text: style(&blurred.text).attributes(),
            cursor: focused.cursor.blurred(&style(&blurred.cursor)),
            vi_mode_cursor: focused.vi_mode_cursor.blurred(&style(&blurred.vi_mode_cursor)),
            selection: focused.selection.blurred(&style(&blurred.selection)),
            search_match: focused.search_match.blurred(&style(&blurred.search_match)),
            search_focused_match: focused
                .search_focused_match
                .blurred(&style(&blurred.search_focused_match)),
            hint_start: focused.hint_start.blurred(&style(&blurred.hint_start)),
            hint_end: focused.hint_end.blurred(&style(&blurred.hint_end)),
        };
        styles.blurred_tab_bar = styles.tab_bar.blurred(
            &TabBlocks {
                bar: style(&blurred.tab_bar),
                active: style(&blurred.tab_active),
                inactive: style(&blurred.tab_inactive),
                button: style(&blurred.tab_button),
                hovered: style(&blurred.tab_hovered),
                activity: style(&blurred.tab_activity),
                attention: style(&blurred.tab_attention),
            },
            colors,
        );

        self.resolved = Resolved(Some(styles));
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

    /// Space between the border and the content, in logical pixels, in CSS shorthand like
    /// Lip Gloss's `Padding`. Only the tab bar uses it.
    pub padding: Option<Spacing>,
    /// Space around the border, in logical pixels. Only the tab bar uses it.
    pub margin: Option<Spacing>,
    /// Border around the element. Only the tab bar uses it.
    pub border: Option<Border>,
    /// Color of the border.
    pub border_foreground: Option<CellRgb>,
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
            padding: self.padding.or(parent.padding),
            margin: self.margin.or(parent.margin),
            border: self.border.or(parent.border),
            border_foreground: self.border_foreground.or(parent.border_foreground),
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

/// A border, named after Lip Gloss's.
#[derive(ConfigDeserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Border {
    /// No border; it takes the place of one that is inherited.
    #[default]
    Hidden,
    /// A thin line.
    Normal,
    /// A thin line with rounded corners.
    Rounded,
    /// A thick line.
    Thick,
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CellStyle {
    pub foreground: CellRgb,
    pub background: CellRgb,
    pub attributes: TextAttributes,
}

impl CellStyle {
    /// This style with the properties `blurred` sets changed.
    fn blurred(&self, blurred: &Style) -> Self {
        let attributes = &self.attributes;
        Self {
            foreground: blurred.foreground.unwrap_or(self.foreground),
            background: blurred.background.unwrap_or(self.background),
            attributes: TextAttributes {
                bold: blurred.bold.unwrap_or(attributes.bold),
                italic: blurred.italic.unwrap_or(attributes.italic),
                faint: blurred.faint.unwrap_or(attributes.faint),
                underline: blurred.underline.unwrap_or(attributes.underline),
                strikethrough: blurred.strikethrough.unwrap_or(attributes.strikethrough),
                reverse: blurred.reverse.unwrap_or(attributes.reverse),
            },
        }
    }

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

/// Effective style of a part of the tab bar.
///
/// Its colors are drawn as they are: `faint` and `reverse` are applied by [`TabStyle::colors`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabStyle {
    pub foreground: Rgb,
    pub background: Rgb,
    pub attributes: TextAttributes,
    pub padding: Spacing,
    pub margin: Spacing,
    pub border: Border,
    pub border_foreground: Rgb,
}

impl TabStyle {
    /// This style with the properties `layer` sets changed.
    pub fn with(&self, layer: &TabLayer) -> Self {
        let attributes = &self.attributes;
        Self {
            foreground: layer.foreground.unwrap_or(self.foreground),
            background: layer.background.unwrap_or(self.background),
            attributes: TextAttributes {
                bold: layer.bold.unwrap_or(attributes.bold),
                italic: layer.italic.unwrap_or(attributes.italic),
                faint: layer.faint.unwrap_or(attributes.faint),
                underline: layer.underline.unwrap_or(attributes.underline),
                strikethrough: layer.strikethrough.unwrap_or(attributes.strikethrough),
                reverse: layer.reverse.unwrap_or(attributes.reverse),
            },
            padding: layer.padding.unwrap_or(self.padding),
            margin: layer.margin.unwrap_or(self.margin),
            border: layer.border.unwrap_or(self.border),
            border_foreground: layer.border_foreground.unwrap_or(self.border_foreground),
        }
    }

    /// The text and background colors as they are drawn: swapped by `reverse`, and the text
    /// halfway to the background with `faint`.
    pub fn colors(&self) -> (Rgb, Rgb) {
        let (mut foreground, background) = match self.attributes.reverse {
            true => (self.background, self.foreground),
            false => (self.foreground, self.background),
        };
        if self.attributes.faint {
            foreground = mix(foreground, background, 0.5);
        }
        (foreground, background)
    }
}

/// Properties of a style that change a [`TabStyle`], like the hovered or attention style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TabLayer {
    foreground: Option<Rgb>,
    background: Option<Rgb>,
    bold: Option<bool>,
    italic: Option<bool>,
    faint: Option<bool>,
    underline: Option<bool>,
    strikethrough: Option<bool>,
    reverse: Option<bool>,
    padding: Option<Spacing>,
    margin: Option<Spacing>,
    border: Option<Border>,
    border_foreground: Option<Rgb>,
}

impl TabLayer {
    /// The properties `style` sets. `CellForeground` and `CellBackground` refer to the primary
    /// colors.
    fn new(style: &Style, colors: &Colors) -> Self {
        let primary = &colors.primary;
        let rgb = |color: CellRgb| color.color(primary.foreground, primary.background);
        Self {
            foreground: style.foreground.map(rgb),
            background: style.background.map(rgb),
            bold: style.bold,
            italic: style.italic,
            faint: style.faint,
            underline: style.underline,
            strikethrough: style.strikethrough,
            reverse: style.reverse,
            padding: style.padding,
            margin: style.margin,
            border: style.border,
            border_foreground: style.border_foreground.map(rgb),
        }
    }

    /// This layer with the properties `above` sets changed.
    fn below(&self, above: &TabLayer) -> Self {
        Self {
            foreground: above.foreground.or(self.foreground),
            background: above.background.or(self.background),
            bold: above.bold.or(self.bold),
            italic: above.italic.or(self.italic),
            faint: above.faint.or(self.faint),
            underline: above.underline.or(self.underline),
            strikethrough: above.strikethrough.or(self.strikethrough),
            reverse: above.reverse.or(self.reverse),
            padding: above.padding.or(self.padding),
            margin: above.margin.or(self.margin),
            border: above.border.or(self.border),
            border_foreground: above.border_foreground.or(self.border_foreground),
        }
    }
}

/// The `[style.tab_*]` blocks, with their inherited styles applied.
struct TabBlocks {
    bar: Style,
    active: Style,
    inactive: Style,
    button: Style,
    hovered: Style,
    activity: Style,
    attention: Style,
}

/// Effective styles of the tab bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabBarStyles {
    /// The bar itself, and the text on it outside of tabs and buttons.
    pub bar: TabStyle,
    /// The tab that is shown.
    pub active: TabStyle,
    /// Every other tab.
    pub inactive: TabStyle,
    /// The buttons on the bar: the new tab button and the gear.
    pub button: TabStyle,
    /// Changes to a tab or button under the mouse.
    pub hovered: TabLayer,
    /// Changes to a tab whose terminals printed something since it was last shown.
    pub activity: TabLayer,
    /// Changes to a tab with a terminal that rang the bell or finished a command unseen.
    pub attention: TabLayer,
}

impl TabBarStyles {
    /// The bar is a blend of the primary colors, the shown tab has the primary colors, and the
    /// other tabs and the buttons sit on the bar with dimmer text.
    fn new(blocks: &TabBlocks, colors: &Colors) -> Self {
        let primary = &colors.primary;
        let (foreground, background) = (primary.foreground, primary.background);
        let layer = |style: &Style| TabLayer::new(style, colors);

        let base = TabStyle {
            foreground,
            background,
            attributes: TextAttributes::default(),
            padding: Spacing::new(3, 8),
            margin: Spacing::default(),
            border: Border::Hidden,
            border_foreground: foreground,
        };
        let bar = TabStyle {
            background: mix(background, foreground, 0.12),
            padding: Spacing::default(),
            ..base
        }
        .with(&layer(&blocks.bar));

        // Tabs and buttons that aren't shown sit on the bar's background.
        let dim = mix(foreground, background, 0.4);
        let on_bar = TabStyle { foreground: dim, background: bar.background, ..base };
        Self {
            bar,
            active: base.with(&layer(&blocks.active)),
            inactive: on_bar.with(&layer(&blocks.inactive)),
            button: on_bar.with(&layer(&blocks.button)),
            hovered: TabLayer { foreground: Some(foreground), ..TabLayer::default() }
                .below(&layer(&blocks.hovered)),
            activity: layer(&blocks.activity),
            attention: layer(&blocks.attention),
        }
    }

    /// These styles with the properties the `[style.blurred.tab_*]` blocks set changed.
    fn blurred(&self, blocks: &TabBlocks, colors: &Colors) -> Self {
        let layer = |style: &Style| TabLayer::new(style, colors);
        Self {
            bar: self.bar.with(&layer(&blocks.bar)),
            active: self.active.with(&layer(&blocks.active)),
            inactive: self.inactive.with(&layer(&blocks.inactive)),
            button: self.button.with(&layer(&blocks.button)),
            hovered: self.hovered.below(&layer(&blocks.hovered)),
            activity: self.activity.below(&layer(&blocks.activity)),
            attention: self.attention.below(&layer(&blocks.attention)),
        }
    }
}

/// `color` moved `amount` of the way to `other`.
fn mix(color: Rgb, other: Rgb, amount: f32) -> Rgb {
    let channel = |a: u8, b: u8| {
        let (a, b) = (f32::from(a), f32::from(b));
        (a + (b - a) * amount).round() as u8
    };
    Rgb::new(channel(color.r, other.r), channel(color.g, other.g), channel(color.b, other.b))
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

    /// Element styles while the window doesn't have focus.
    pub blurred: ElementStyles,

    pub tab_bar: TabBarStyles,
    /// Tab bar styles while the window doesn't have focus.
    pub blurred_tab_bar: TabBarStyles,
}

impl Styles {
    /// Styles of the elements drawn on terminal cells, for a focused or unfocused window.
    pub fn elements(&self, focused: bool) -> ElementStyles {
        if !focused {
            return self.blurred;
        }

        ElementStyles {
            text: TextAttributes::default(),
            cursor: self.cursor,
            vi_mode_cursor: self.vi_mode_cursor,
            selection: self.selection,
            search_match: self.search_match,
            search_focused_match: self.search_focused_match,
            hint_start: self.hint_start,
            hint_end: self.hint_end,
        }
    }

    /// Styles of the tab bar, for a focused or unfocused window.
    pub fn tab_bar(&self, focused: bool) -> TabBarStyles {
        if focused { self.tab_bar } else { self.blurred_tab_bar }
    }

    /// Whether losing or gaining focus changes how terminal content is drawn.
    pub fn blur_changes_content(&self) -> bool {
        self.elements(false) != self.elements(true)
    }
}

/// Styles of the elements drawn on terminal cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ElementStyles {
    /// Text attributes applied to all terminal content.
    pub text: TextAttributes,
    pub cursor: CellStyle,
    pub vi_mode_cursor: CellStyle,
    pub selection: CellStyle,
    pub search_match: CellStyle,
    pub search_focused_match: CellStyle,
    pub hint_start: CellStyle,
    pub hint_end: CellStyle,
}

/// Default colors of each element, matching the green screen color scheme.
mod defaults {
    use crate::color::{CellRgb, Rgb};

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
    fn blurred_defaults_to_focused() {
        let styles = *load("").style.resolved();

        assert_eq!(styles.elements(false), styles.elements(true));
        assert!(!styles.blur_changes_content());
    }

    #[test]
    fn blurred_changes_only_set_properties() {
        let config = load(
            "[style.selection]\nbackground = '#111111'\nbold = \
             true\n\n[style.blurred.selection]\nbackground = '#222222'\n",
        );
        let styles = config.style.resolved();
        let (focused, blurred) = (styles.elements(true), styles.elements(false));

        assert_eq!(focused.selection.background, rgb(0x11, 0x11, 0x11));
        assert_eq!(blurred.selection.background, rgb(0x22, 0x22, 0x22));
        // Copied from the focused style.
        assert_eq!(blurred.selection.foreground, focused.selection.foreground);
        assert!(blurred.selection.attributes.bold);
        // Other elements are unchanged.
        assert_eq!(blurred.cursor, focused.cursor);
        assert!(styles.blur_changes_content());
    }

    #[test]
    fn blurred_text_attributes() {
        let config = load("[style.blurred.text]\nfaint = true\n");
        let styles = config.style.resolved();

        assert!(styles.elements(false).text.faint);
        assert_eq!(styles.elements(true).text, TextAttributes::default());
    }

    #[test]
    fn blurred_styles_inherit() {
        let config = load(
            "[style.defs.dim]\nfaint = true\nitalic = true\n\n[style.blurred.cursor]\ninherit = \
             'dim'\n",
        );
        let cursor = config.style.resolved().elements(false).cursor;

        assert!(cursor.attributes.faint && cursor.attributes.italic);
    }

    #[test]
    fn tab_bar_defaults_blend_primary_colors() {
        let config = load("[colors.primary]\nforeground = '#ffffff'\nbackground = '#000000'\n");
        let tabs = config.style.resolved().tab_bar(true);

        assert_eq!(tabs.bar.background, Rgb::new(31, 31, 31));
        assert_eq!(tabs.active.foreground, Rgb::new(0xff, 0xff, 0xff));
        assert_eq!(tabs.active.background, Rgb::new(0, 0, 0));
        assert_eq!(tabs.inactive.foreground, Rgb::new(153, 153, 153));
        assert_eq!(tabs.inactive.background, tabs.bar.background);
        assert_eq!(tabs.button, tabs.inactive);
        assert_eq!(tabs.active.padding, Spacing::new(3, 8));
        assert_eq!(tabs.bar.padding, Spacing::default());
        assert_eq!(tabs.active.border, Border::Hidden);

        // Hovering brings the text up to the primary foreground and changes nothing else.
        let hovered = tabs.inactive.with(&tabs.hovered);
        assert_eq!(hovered, TabStyle { foreground: Rgb::new(0xff, 0xff, 0xff), ..tabs.inactive });
        assert_eq!(tabs.inactive.with(&tabs.activity), tabs.inactive);
        assert_eq!(tabs.inactive.with(&tabs.attention), tabs.inactive);

        // The unfocused window looks the same.
        assert_eq!(config.style.resolved().tab_bar(false), tabs);
    }

    #[test]
    fn tab_styles_set_padding_borders_and_attributes() {
        let config = load(
            "[style.tab_active]\npadding = [2, 10]\nmargin = 1\nborder = \
             'rounded'\nborder_foreground = '#ff0000'\nbold = \
             true\n\n[style.tab_attention]\nbackground = '#880000'\nitalic = true\n",
        );
        let tabs = config.style.resolved().tab_bar(true);

        assert_eq!(tabs.active.padding, Spacing { top: 2, right: 10, bottom: 2, left: 10 });
        assert_eq!(tabs.active.border, Border::Rounded);
        assert_eq!(tabs.active.margin, Spacing::new(1, 1));
        assert_eq!(tabs.active.border_foreground, Rgb::new(0xff, 0, 0));
        assert!(tabs.active.attributes.bold);
        // Other tabs keep their defaults.
        assert_eq!(tabs.inactive.border, Border::Hidden);

        // The attention style changes only what it sets.
        let attention = tabs.active.with(&tabs.attention);
        assert_eq!(attention.background, Rgb::new(0x88, 0, 0));
        assert!(attention.attributes.bold && attention.attributes.italic);
        assert_eq!(attention.padding, tabs.active.padding);
    }

    #[test]
    fn tab_bar_background_carries_to_tabs_on_it() {
        let config = load("[style.tab_bar]\nbackground = '#123456'\n");
        let tabs = config.style.resolved().tab_bar(true);

        assert_eq!(tabs.inactive.background, Rgb::new(0x12, 0x34, 0x56));
        assert_eq!(tabs.button.background, Rgb::new(0x12, 0x34, 0x56));
        assert_ne!(tabs.active.background, Rgb::new(0x12, 0x34, 0x56));
    }

    #[test]
    fn tab_styles_inherit_and_refer_to_primary_colors() {
        let config = load(
            "[colors.primary]\nforeground = '#aaaaaa'\nbackground = \
             '#111111'\n\n[style.defs.boxed]\nborder = 'thick'\npadding = \
             4\n\n[style.tab_button]\ninherit = 'boxed'\nforeground = \
             'CellBackground'\nbackground = 'CellForeground'\n",
        );
        let button = config.style.resolved().tab_bar(true).button;

        assert_eq!(button.border, Border::Thick);
        assert_eq!(button.padding, Spacing::new(4, 4));
        assert_eq!(button.foreground, Rgb::new(0x11, 0x11, 0x11));
        assert_eq!(button.background, Rgb::new(0xaa, 0xaa, 0xaa));
    }

    #[test]
    fn padding_takes_one_to_four_values() {
        let padding = |value: &str| {
            let config = load(&format!("[style.tab_active]\npadding = {value}\n"));
            config.style.resolved().tab_bar(true).active.padding
        };
        let spacing = |top, right, bottom, left| Spacing { top, right, bottom, left };

        assert_eq!(padding("5"), spacing(5, 5, 5, 5));
        assert_eq!(padding("[5]"), spacing(5, 5, 5, 5));
        assert_eq!(padding("[1, 2]"), spacing(1, 2, 1, 2));
        assert_eq!(padding("[1, 2, 3]"), spacing(1, 2, 3, 2));
        assert_eq!(padding("[1, 2, 3, 4]"), spacing(1, 2, 3, 4));

        // Lists of other lengths and negative values keep the default.
        assert_eq!(padding("[1, 2, 3, 4, 5]"), Spacing::new(3, 8));
        assert_eq!(padding("-1"), Spacing::new(3, 8));
    }

    #[test]
    fn faint_and_reverse_change_drawn_colors() {
        let config = load(
            "[style.tab_active]\nforeground = '#ffffff'\nbackground = '#000000'\nfaint = true\n",
        );
        let active = config.style.resolved().tab_bar(true).active;
        assert_eq!(active.colors(), (Rgb::new(128, 128, 128), Rgb::new(0, 0, 0)));

        let config = load(
            "[style.tab_active]\nforeground = '#ffffff'\nbackground = '#000000'\nreverse = true\n",
        );
        let active = config.style.resolved().tab_bar(true).active;
        assert_eq!(active.colors(), (Rgb::new(0, 0, 0), Rgb::new(0xff, 0xff, 0xff)));
    }

    #[test]
    fn blurred_tab_styles_change_only_set_properties() {
        let config = load(
            "[style.tab_active]\nbold = true\n\n[style.blurred.tab_active]\nforeground = \
             '#555555'\n\n[style.blurred.tab_hovered]\nunderline = true\n",
        );
        let styles = config.style.resolved();
        let (focused, blurred) = (styles.tab_bar(true), styles.tab_bar(false));

        assert_eq!(blurred.active.foreground, Rgb::new(0x55, 0x55, 0x55));
        assert!(blurred.active.attributes.bold);
        assert_eq!(blurred.inactive, focused.inactive);

        // The blurred hovered style keeps the focused one's text color and adds an underline.
        let hovered = blurred.inactive.with(&blurred.hovered);
        assert_eq!(hovered.foreground, focused.inactive.with(&focused.hovered).foreground);
        assert!(hovered.attributes.underline);
        assert!(!focused.inactive.with(&focused.hovered).attributes.underline);
    }

    #[test]
    fn resolved_styles_cant_be_configured() {
        let config = load("[style]\nresolved = 1\n");
        assert_eq!(config.style.resolved(), StyleConfig::default().resolved());
    }
}
