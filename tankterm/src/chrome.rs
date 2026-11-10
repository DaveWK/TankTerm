//! Window chrome around the terminal, drawn from Lip Gloss style blocks.
//!
//! The layout follows Lip Gloss's box model: every block is the content, surrounded by padding,
//! then a border, then a margin. Padding and margins are given in terminal cells and converted to
//! pixels with the terminal's cell size, so the chrome lines up with the grid.

use iced::alignment::Horizontal;
use iced::font::{Style as FontStyle, Weight};
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{Column, container, row, text};
use iced::{Background, Border as IcedBorder, Color, Element, Font, Length, Padding, Size};
use iced_core::alignment::Vertical;
use iced_core::text::{Alignment, Paragraph as _, Shaping, Text};
use iced_graphics::text::paragraph::Paragraph;
use iced_term::settings::FontSettings;

use crate::config::style::{Align, Border, Spacing, Style, Styles};
use crate::display::color::Rgb;

/// Text of the leftmost status bar segment.
const ACCENT_LABEL: &str = "TANKTERM";

/// Gap between the two lines of a double border, in pixels.
const DOUBLE_BORDER_GAP: f32 = 2.;

/// Opacity of faint text.
const FAINT_ALPHA: f32 = 0.6;

/// Everything needed to draw the chrome.
#[derive(Debug, Clone)]
pub struct Chrome {
    styles: Styles,
    font: Font,
    text_size: f32,
    line_height: f32,
    cell: Size,
    background: Color,
    foreground: Color,
}

impl Chrome {
    /// Build the chrome for the terminal's font and colors.
    pub fn new(styles: Styles, font: &FontSettings, foreground: Rgb, background: Rgb) -> Self {
        Self {
            styles,
            font: font.font_type,
            text_size: font.size,
            line_height: font.scale_factor,
            cell: cell_size(font),
            background: color(background),
            foreground: color(foreground),
        }
    }

    /// Window background, shown in margins.
    pub fn background(&self) -> Color {
        self.background
    }

    /// Lay out the chrome around `terminal` for a window of `size`.
    pub fn view<'a, M: 'a>(
        &self,
        size: Size,
        terminal: Element<'a, M>,
        title: &str,
        shell: &str,
    ) -> Element<'a, M> {
        let styles = &self.styles;
        let mut layout = Column::new().width(Length::Fill).height(Length::Fill);

        if styles.title_bar.is_visible() {
            layout = layout.push(self.title_bar(title));
        }

        layout = layout.push(self.block(terminal, &styles.frame, Length::Fill, Length::Fill));

        if styles.status_bar.is_visible() {
            let (columns, lines) = self.grid_size(size);
            layout = layout.push(self.status_bar(shell, &format!("{columns}×{lines}")));
        }

        layout.into()
    }

    /// Number of terminal columns and lines that fit in a window of `size`.
    pub fn grid_size(&self, size: Size) -> (usize, usize) {
        let styles = &self.styles;
        let frame = self.extent(&styles.frame);

        let mut height = size.height - frame.height;
        if styles.title_bar.is_visible() {
            height -= self.cell.height + self.extent(&styles.title_bar).height;
        }
        if styles.status_bar.is_visible() {
            height -= self.status_bar_height();
        }
        let width = size.width - frame.width;

        let fit = |space: f32, cell: f32| (space / cell).floor().max(0.) as usize;
        (fit(width, self.cell.width), fit(height, self.cell.height))
    }

    fn title_bar<'a, M: 'a>(&self, title: &str) -> Element<'a, M> {
        let style = &self.styles.title_bar;
        let label = self.label(title, style).width(Length::Fill);
        self.block(label.into(), style, Length::Fill, Length::Shrink)
    }

    fn status_bar<'a, M: 'a>(&self, shell: &str, size: &str) -> Element<'a, M> {
        let styles = &self.styles;
        let bar = &styles.status_bar;

        // The middle segment uses the status bar's own padding; its border and margin wrap the
        // whole bar.
        let middle = Style { margin: None, border: Border::None, ..*bar };
        let segments = row![
            self.segment(ACCENT_LABEL, &styles.status_bar_accent, Length::Shrink),
            self.segment(shell, &middle, Length::Fill),
            self.segment(size, &styles.status_bar_info, Length::Shrink),
        ];

        let content = Style { padding: None, ..*bar };
        self.block(segments.into(), &content, Length::Fill, Length::Shrink)
    }

    fn segment<'a, M: 'a>(&self, label: &str, style: &Style, width: Length) -> Element<'a, M> {
        let label = self.label(label, style).width(width);
        self.block(label.into(), style, width, Length::Shrink)
    }

    /// Height of the status bar, including its tallest segment.
    fn status_bar_height(&self) -> f32 {
        let styles = &self.styles;
        let bar = &styles.status_bar;
        let middle = self.cell.height + self.spacing(bar.padding()).y();

        let tallest = [&styles.status_bar_accent, &styles.status_bar_info]
            .into_iter()
            .map(|segment| self.cell.height + self.extent(segment).height)
            .fold(middle, f32::max);

        let outer = self.spacing(bar.margin()).y() + 2. * self.border_width(bar.border());
        tallest + outer
    }

    /// Text styled like a Lip Gloss block.
    fn label<'a>(&self, content: &str, style: &Style) -> iced::widget::Text<'a> {
        let mut font = self.font;
        if style.bold == Some(true) {
            font.weight = Weight::Bold;
        }
        if style.italic == Some(true) {
            font.style = FontStyle::Italic;
        }

        let mut foreground = style.foreground.map(color).unwrap_or(self.foreground);
        if style.faint == Some(true) {
            foreground.a *= FAINT_ALPHA;
        }

        let align = match style.align.unwrap_or_default() {
            Align::Left => Horizontal::Left,
            Align::Center => Horizontal::Center,
            Align::Right => Horizontal::Right,
        };

        text(content.to_owned())
            .font(font)
            .size(self.text_size)
            .line_height(LineHeight::Relative(self.line_height))
            .color(foreground)
            .align_x(align)
            .wrapping(Wrapping::None)
    }

    /// Wrap `content` in the style's padding, border and margin.
    fn block<'a, M: 'a>(
        &self,
        content: Element<'a, M>,
        style: &Style,
        width: Length,
        height: Length,
    ) -> Element<'a, M> {
        let background = style.background.map(|bg| Background::Color(color(bg)));
        let padded = container(content)
            .padding(self.spacing(style.padding()))
            .width(width)
            .height(height)
            .style(move |_| container::Style { background, ..Default::default() });

        let bordered = self.border(padded.into(), style, width, height);

        container(bordered).padding(self.spacing(style.margin())).width(width).height(height).into()
    }

    /// Draw the style's border around `content`.
    fn border<'a, M: 'a>(
        &self,
        content: Element<'a, M>,
        style: &Style,
        width: Length,
        height: Length,
    ) -> Element<'a, M> {
        let border = style.border();
        let line_color = match border {
            Border::Hidden => Color::TRANSPARENT,
            _ => style.border_foreground.map(color).unwrap_or(self.foreground),
        };
        let background = style.border_background.map(|bg| Background::Color(color(bg)));

        let line = |width: f32, radius: f32| {
            move |_: &iced::Theme| container::Style {
                background,
                border: IcedBorder { color: line_color, width, radius: radius.into() },
                ..Default::default()
            }
        };

        let radius = if border == Border::Rounded { self.cell.width } else { 0. };
        match border {
            Border::Unset | Border::None => content,
            // Two thin lines with a gap, drawn as nested borders.
            Border::Double => {
                let inner = container(content).padding(1.).width(width).height(height);
                container(inner.style(line(1., 0.)))
                    .padding(1. + DOUBLE_BORDER_GAP)
                    .width(width)
                    .height(height)
                    .style(line(1., 0.))
                    .into()
            },
            _ => {
                let border_width = self.border_width(border);
                container(content)
                    .padding(border_width)
                    .width(width)
                    .height(height)
                    .style(line(border_width, radius))
                    .into()
            },
        }
    }

    /// Thickness of a border on each side, in pixels.
    fn border_width(&self, border: Border) -> f32 {
        match border {
            Border::Unset | Border::None => 0.,
            Border::Hidden | Border::Normal | Border::Rounded | Border::Ascii => 1.,
            Border::Thick => 3.,
            // Outer line, gap, inner line.
            Border::Double => 2. + DOUBLE_BORDER_GAP,
            Border::Block => (self.cell.width / 2.).round(),
        }
    }

    /// Convert cell spacing to pixels.
    fn spacing(&self, spacing: Spacing) -> Padding {
        Padding {
            top: f32::from(spacing.top) * self.cell.height,
            right: f32::from(spacing.right) * self.cell.width,
            bottom: f32::from(spacing.bottom) * self.cell.height,
            left: f32::from(spacing.left) * self.cell.width,
        }
    }

    /// Space a block's padding, border and margin take around its content.
    fn extent(&self, style: &Style) -> Size {
        let padding = self.spacing(style.padding());
        let margin = self.spacing(style.margin());
        let border = 2. * self.border_width(style.border());

        Size::new(padding.x() + margin.x() + border, padding.y() + margin.y() + border)
    }
}

/// Size of a terminal cell, measured the same way `iced_term` measures its grid.
fn cell_size(font: &FontSettings) -> Size {
    Paragraph::with_text(Text {
        content: "m",
        font: font.font_type,
        size: iced::Pixels(font.size),
        align_y: Vertical::Center,
        align_x: Alignment::Center,
        shaping: Shaping::Advanced,
        line_height: LineHeight::Relative(font.scale_factor),
        bounds: Size::INFINITE,
        wrapping: Wrapping::Glyph,
    })
    .min_bounds()
}

fn color(rgb: Rgb) -> Color {
    Color::from_rgb8(rgb.r, rgb.g, rgb.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::config::style::{Preset, StyleConfig};

    fn chrome(styles: Styles) -> Chrome {
        let mut chrome =
            Chrome::new(styles, &FontSettings::default(), Rgb::new(0, 0xff, 0), Rgb::new(0, 0, 0));
        // Use round numbers instead of measuring a font.
        chrome.cell = Size::new(10., 20.);
        chrome
    }

    fn spacing(values: &[u16]) -> Option<Spacing> {
        Some(Spacing::from_shorthand(values).unwrap())
    }

    #[test]
    fn spacing_is_measured_in_cells() {
        let chrome = chrome(StyleConfig::default().resolve());

        let padding = chrome.spacing(Spacing::from_shorthand(&[1, 2, 3, 4]).unwrap());

        assert_eq!(padding, Padding { top: 20., right: 20., bottom: 60., left: 40. });
    }

    #[test]
    fn border_widths() {
        let chrome = chrome(StyleConfig::default().resolve());

        assert_eq!(chrome.border_width(Border::None), 0.);
        assert_eq!(chrome.border_width(Border::Hidden), 1.);
        assert_eq!(chrome.border_width(Border::Rounded), 1.);
        assert_eq!(chrome.border_width(Border::Thick), 3.);
        assert_eq!(chrome.border_width(Border::Double), 4.);
        assert_eq!(chrome.border_width(Border::Block), 5.);
    }

    #[test]
    fn extent_adds_padding_margin_and_border() {
        let chrome = chrome(StyleConfig::default().resolve());
        let style = Style {
            padding: spacing(&[1, 2]),
            margin: spacing(&[0, 1]),
            border: Border::Thick,
            ..Style::default()
        };

        // Horizontal: 2+2 cells padding, 1+1 cells margin, 3+3 px border.
        // Vertical: 1+1 cells padding, no margin, 3+3 px border.
        assert_eq!(chrome.extent(&style), Size::new(66., 46.));
    }

    #[test]
    fn grid_size_without_chrome_is_whole_window() {
        let bare = Style { visible: Some(false), ..Style::default() };
        let styles = Styles {
            frame: Style::default(),
            title_bar: bare,
            status_bar: bare,
            status_bar_accent: Style::default(),
            status_bar_info: Style::default(),
        };

        // 800x600 window with 10x20 cells.
        assert_eq!(chrome(styles).grid_size(Size::new(800., 600.)), (80, 30));
    }

    #[test]
    fn grid_size_subtracts_chrome() {
        let chrome = chrome(StyleConfig::default().resolve());

        // Green screen: rounded frame (1px) with 1 cell of horizontal padding and margin, and
        // one line tall title and status bars.
        //   width:  800 - (2 * 10 padding + 2 * 10 margin + 2 border) = 758 -> 75 columns
        //   height: 600 - 2 border - 20 title - 20 status = 558 -> 27 lines
        assert_eq!(chrome.grid_size(Size::new(800., 600.)), (75, 27));
    }

    #[test]
    fn grid_size_never_negative() {
        let chrome = chrome(StyleConfig::default().resolve());
        assert_eq!(chrome.grid_size(Size::new(5., 5.)), (0, 0));
    }

    #[test]
    fn charm_and_green_screen_share_layout() {
        let green = chrome(StyleConfig::default().resolve());
        let charm = chrome(StyleConfig { preset: Preset::Charm, ..Default::default() }.resolve());

        let size = Size::new(1024., 768.);
        assert_eq!(green.grid_size(size), charm.grid_size(size));
    }

    #[test]
    fn real_font_cell_is_measured() {
        let cell = cell_size(&FontSettings::default());
        assert!(cell.width > 0. && cell.height > cell.width, "unexpected cell {cell:?}");
    }
}

/// Headless rendering of the chrome around a live terminal.
#[cfg(all(test, unix))]
mod render_tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use iced::Theme;
    use iced_term::settings::{BackendSettings, Settings};
    use iced_test::simulator;

    use super::*;
    use crate::config::UiConfig;
    use crate::config::style::{Preset, StyleConfig};
    use crate::term_settings;

    /// Script printing a small test pattern after the first layout, then waiting so the terminal
    /// stays open.
    const PATTERN: &str = "sleep 0.3; printf 'TankTerm chrome check\\n\\033[31mred \\033[32mgreen \
                           \\033[33myellow \\033[34mblue\\033[0m \\033[1mbold\\033[0m\\n'; ls -F \
                           /; sleep 5";

    fn view<'a>(
        chrome: &Chrome,
        terminal: &'a iced_term::Terminal,
        size: Size,
    ) -> Element<'a, iced_term::Event> {
        let background = chrome.background();
        container(chrome.view(
            size,
            iced_term::TerminalView::show(terminal),
            "TankTerm — tank, I need an exit",
            "sh",
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(background.into()),
            ..Default::default()
        })
        .into()
    }

    fn render(style: StyleConfig, size: Size) -> iced_test::simulator::Snapshot {
        let mut config = UiConfig::default();
        config.style = style;
        let mut settings: Settings =
            term_settings::settings(&config, &crate::cli::Options::default());
        settings.backend = BackendSettings {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), PATTERN.into()],
            ..BackendSettings::default()
        };

        let primary = &config.colors.primary;
        let chrome = Chrome::new(
            config.style.resolve(),
            &settings.font,
            primary.foreground,
            primary.background,
        );
        let mut terminal = iced_term::Terminal::new(0, settings).unwrap();

        // First pass: the terminal widget publishes its laid out size. Apply it like
        // `App::update` does, before the shell prints anything.
        let messages: Vec<_> = {
            let mut simulator = simulator::Simulator::with_size(
                Default::default(),
                size,
                view(&chrome, &terminal, size),
            );
            simulator.snapshot(&Theme::Dark).unwrap();
            simulator.into_messages().collect()
        };
        assert!(!messages.is_empty(), "terminal widget didn't publish its size");
        for iced_term::Event::BackendCall(_, command) in messages {
            terminal.handle(iced_term::Command::ProxyToBackend(command));
        }

        // Let the shell print, then deliver the wakeup the event loop would send.
        std::thread::sleep(Duration::from_millis(800));
        let wakeup =
            iced_term::BackendCommand::ProcessAlacrittyEvent(iced_term::AlacrittyEvent::Wakeup);
        terminal.handle(iced_term::Command::ProxyToBackend(wakeup));

        let mut simulator = simulator::Simulator::with_size(
            Default::default(),
            size,
            view(&chrome, &terminal, size),
        );
        simulator.snapshot(&Theme::Dark).unwrap()
    }

    /// Write snapshots of both presets to `$TANKTERM_SNAPSHOT_DIR` for manual inspection.
    #[test]
    #[ignore = "writes images for manual inspection; set TANKTERM_SNAPSHOT_DIR"]
    fn write_preset_snapshots() {
        let dir = PathBuf::from(std::env::var("TANKTERM_SNAPSHOT_DIR").unwrap());
        let size = Size::new(900., 420.);

        for (name, preset) in [("green_screen", Preset::GreenScreen), ("charm", Preset::Charm)] {
            let snapshot = render(StyleConfig { preset, ..Default::default() }, size);
            assert!(snapshot.matches_image(dir.join(name)).unwrap());
        }
    }
}
