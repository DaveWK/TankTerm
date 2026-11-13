use std::time::Duration;
use std::{collections::HashMap, path::PathBuf};

use crate::ColorPalette;
use iced::{Font, Vector};
use tankterm_terminal::term::SEMANTIC_ESCAPE_CHARS;
use tankterm_terminal::vte::ansi::CursorShape;

#[cfg(target_os = "windows")]
const DEFAULT_SHELL: &str = "wsl.exe";

#[cfg(not(target_os = "windows"))]
const DEFAULT_SHELL: &str = "/bin/bash";

#[derive(Default, Clone)]
pub struct Settings {
    pub font: FontSettings,
    pub theme: ThemeSettings,
    pub backend: BackendSettings,
    pub scroll: ScrollSettings,
    pub cursor: CursorSettings,
    pub selection: SelectionSettings,
    pub mouse: MouseSettings,
    /// Centre the grid in the widget, sharing the space that is left over
    /// by whole cells evenly between both sides, instead of leaving it all
    /// at the right and the bottom.
    pub dynamic_padding: bool,
}

/// How text is selected with the mouse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionSettings {
    /// Characters that end a word for a double click, besides whitespace.
    pub semantic_escape_chars: String,
}

impl Default for SelectionSettings {
    fn default() -> Self {
        Self {
            semantic_escape_chars: SEMANTIC_ESCAPE_CHARS.to_owned(),
        }
    }
}

/// How the mouse pointer behaves over the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MouseSettings {
    /// Hide the pointer when a key is typed into the terminal, until the
    /// mouse moves.
    pub hide_when_typing: bool,
}

/// How the cursor looks and blinks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CursorSettings {
    /// The shape of the cursor until the application in the terminal asks
    /// for another one (DECSCUSR).
    pub shape: CursorShape,
    /// Whether the cursor blinks.
    pub blinking: CursorBlinking,
    /// How long the cursor is lit and dark for while it blinks.
    pub blink_interval: Duration,
    /// How long the cursor blinks before it stays lit, until something is
    /// typed; `None` lets it blink forever.
    pub blink_timeout: Option<Duration>,
    /// Draw the cursor of a terminal without the keyboard focus as a hollow
    /// block.
    pub unfocused_hollow: bool,
    /// Thickness of the beam, underline and hollow block cursors, relative
    /// to the width of a cell.
    pub thickness: f32,
}

impl Default for CursorSettings {
    fn default() -> Self {
        Self {
            shape: CursorShape::Block,
            blinking: CursorBlinking::Off,
            blink_interval: Duration::from_millis(750),
            blink_timeout: Some(Duration::from_secs(5)),
            unfocused_hollow: true,
            thickness: 0.15,
        }
    }
}

/// Whether the cursor blinks.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum CursorBlinking {
    /// Never, whatever the application in the terminal asks for.
    Never,
    /// Not until the application asks for a blinking cursor.
    #[default]
    Off,
    /// Until the application asks for a steady cursor.
    On,
    /// Always, whatever the application asks for.
    Always,
}

/// The scrollback buffer and how far the mouse wheel scrolls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollSettings {
    /// Most lines kept in the scrollback buffer; `0` keeps none.
    pub history: usize,
    /// Lines scrolled for every step of the mouse wheel.
    pub multiplier: f32,
}

impl Default for ScrollSettings {
    fn default() -> Self {
        Self {
            history: 10_000,
            multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackendSettings {
    pub program: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub working_directory: Option<PathBuf>,
    /// Keep the terminal and what it shows once the program exits: the output it wrote last is
    /// read before the terminal reports the exit.
    pub hold: bool,
}

impl Default for BackendSettings {
    fn default() -> Self {
        Self {
            program: DEFAULT_SHELL.to_string(),
            args: vec![],
            env: HashMap::new(),
            working_directory: None,
            hold: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FontSettings {
    pub size: f32,
    pub scale_factor: f32,
    /// The font of regular text.
    pub font_type: Font,
    /// The font of bold text; without one it is `font_type` in bold.
    pub bold: Option<Font>,
    /// The font of italic text; without one it is `font_type` in italic.
    pub italic: Option<Font>,
    /// The font of bold italic text; without one it is `font_type` in bold
    /// italic.
    pub bold_italic: Option<Font>,
    /// Space added to every cell, in logical pixels; it can be negative.
    pub offset: Vector,
    /// How far glyphs are moved from the middle of their cell, in logical
    /// pixels.
    pub glyph_offset: Vector,
    /// Draw box drawing characters, block elements, sextants and powerline
    /// symbols as shapes that fill their cell, instead of with the font.
    /// They ignore `glyph_offset`.
    pub builtin_box_drawing: bool,
}

impl Default for FontSettings {
    fn default() -> Self {
        Self {
            size: 14.0,
            scale_factor: 1.3,
            font_type: Font::MONOSPACE,
            bold: None,
            italic: None,
            bold_italic: None,
            offset: Vector::ZERO,
            glyph_offset: Vector::ZERO,
            builtin_box_drawing: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThemeSettings {
    pub color_pallete: Box<ColorPalette>,
    /// How opaque the widget draws its background, from `0.0`, not at all,
    /// to `1.0`. Cells with a background color of their own stay opaque.
    ///
    /// What is behind the widget only shows through a window that is
    /// transparent itself.
    pub background_opacity: f32,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self::new(Box::default())
    }
}

impl ThemeSettings {
    /// A palette on an opaque background.
    pub fn new(color_pallete: Box<ColorPalette>) -> Self {
        Self {
            color_pallete,
            background_opacity: 1.0,
        }
    }
}
