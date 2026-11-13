use crate::settings::FontSettings;
use iced::font::{Style, Weight};
use iced::{Font, Size, Vector};
use iced_core::{
    alignment::Vertical,
    text::{Alignment, LineHeight, Paragraph, Shaping as TextShaping},
    Text,
};
use iced_graphics::text::paragraph;

/// Smallest size of a cell, whatever the offset takes away from it.
const MIN_CELL: f32 = 1.;

#[derive(Debug, Clone)]
pub struct TermFont {
    pub(crate) size: f32,
    pub(crate) font_type: Font,
    pub(crate) bold: Font,
    pub(crate) italic: Font,
    pub(crate) bold_italic: Font,
    pub(crate) glyph_offset: Vector,
    /// Draw the characters [`crate::box_drawing`] has shapes for with those.
    pub(crate) builtin_box_drawing: bool,
    pub(crate) scale_factor: f32,
    pub(crate) measure: Size<f32>,
}

impl TermFont {
    pub fn new(settings: FontSettings) -> Self {
        let regular = settings.font_type;
        let with = |weight, style| Font {
            weight,
            style,
            ..regular
        };

        Self {
            size: settings.size,
            font_type: regular,
            bold: settings.bold.unwrap_or(with(Weight::Bold, regular.style)),
            italic: settings
                .italic
                .unwrap_or(with(regular.weight, Style::Italic)),
            bold_italic: settings
                .bold_italic
                .unwrap_or(with(Weight::Bold, Style::Italic)),
            glyph_offset: settings.glyph_offset,
            builtin_box_drawing: settings.builtin_box_drawing,
            scale_factor: settings.scale_factor,
            measure: settings.cell_size(),
        }
    }

    /// The font of text that is bold, italic, both or neither.
    pub(crate) fn face(&self, bold: bool, italic: bool) -> Font {
        match (bold, italic) {
            (false, false) => self.font_type,
            (true, false) => self.bold,
            (false, true) => self.italic,
            (true, true) => self.bold_italic,
        }
    }
}

impl FontSettings {
    /// Size of a terminal cell with this font: the size of an `m`, with the
    /// offset added.
    pub fn cell_size(&self) -> Size<f32> {
        let glyph = font_measure(self.size, self.scale_factor, self.font_type);

        Size::new(
            (glyph.width + self.offset.x).max(MIN_CELL),
            (glyph.height + self.offset.y).max(MIN_CELL),
        )
    }
}

fn font_measure(
    font_size: f32,
    scale_factor: f32,
    font_type: Font,
) -> Size<f32> {
    let paragraph = paragraph::Paragraph::with_text(Text {
        content: "m",
        font: font_type,
        size: iced_core::Pixels(font_size),
        align_y: Vertical::Center,
        align_x: Alignment::Center,
        shaping: TextShaping::Advanced,
        line_height: LineHeight::Relative(scale_factor),
        bounds: Size::INFINITE,
        wrapping: iced_core::text::Wrapping::Glyph,
    });

    paragraph.min_bounds()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_changes_the_cell_but_not_below_a_pixel() {
        let plain = FontSettings::default();
        let cell = plain.cell_size();

        let wider = FontSettings {
            offset: Vector::new(2., -1.),
            ..FontSettings::default()
        };
        assert_eq!(
            wider.cell_size(),
            Size::new(cell.width + 2., cell.height - 1.)
        );

        let crushed = FontSettings {
            offset: Vector::new(-1000., -1000.),
            ..FontSettings::default()
        };
        assert_eq!(crushed.cell_size(), Size::new(MIN_CELL, MIN_CELL));
    }

    #[test]
    fn faces_fall_back_to_the_regular_font_in_bold_and_italic() {
        let font = TermFont::new(FontSettings::default());
        let regular = Font::MONOSPACE;

        assert_eq!(font.face(false, false), regular);
        assert_eq!(
            font.face(true, false),
            Font {
                weight: Weight::Bold,
                ..regular
            }
        );
        assert_eq!(
            font.face(false, true),
            Font {
                style: Style::Italic,
                ..regular
            }
        );
        assert_eq!(
            font.face(true, true),
            Font {
                weight: Weight::Bold,
                style: Style::Italic,
                ..regular
            }
        );

        let heavy = Font {
            weight: Weight::Black,
            ..Font::DEFAULT
        };
        let font = TermFont::new(FontSettings {
            bold: Some(heavy),
            ..FontSettings::default()
        });
        assert_eq!(font.face(true, false), heavy);
        assert_eq!(font.face(false, false), regular);
    }

    #[test]
    fn builtin_box_drawing_follows_the_settings() {
        assert!(TermFont::new(FontSettings::default()).builtin_box_drawing);

        let font = TermFont::new(FontSettings {
            builtin_box_drawing: false,
            ..FontSettings::default()
        });
        assert!(!font.builtin_box_drawing);
    }
}
