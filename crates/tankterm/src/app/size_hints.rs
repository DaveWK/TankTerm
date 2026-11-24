//! Resizing the window in whole cells, `window.resize_increments`.

use iced::{Size, Task, window};

use super::{App, Message};

/// The sizes the window is asked to have.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SizeHints {
    /// The steps the window resizes in: the size of a cell.
    pub increments: Size,
    /// The smallest size, which the steps start from: a cell and what is around the terminals.
    pub min: Size,
}

impl App {
    /// The sizes the window should have, or `None` while it resizes freely.
    ///
    /// The panes of a split tab share the window, so only a single pane always has whole cells.
    pub(super) fn size_hints(&self) -> Option<SizeHints> {
        if !self.options.resize_increments {
            return None;
        }

        let padding = self.options.layout.padding;
        let bar = if self.shows_tab_bar() { self.tab_bar_height } else { 0. };
        let min = Size::new(
            padding.left + padding.right + self.cell.width,
            bar + padding.top + padding.bottom + self.cell.height,
        );

        // Windows have whole pixels, and less than a cell's would lose the cell.
        Some(SizeHints {
            increments: self.cell,
            min: Size::new(min.width.ceil(), min.height.ceil()),
        })
    }

    /// Ask the window for its sizes, if they aren't those it was last asked for.
    pub(super) fn ask_size_hints(&mut self) -> Task<Message> {
        let hints = self.size_hints();
        if !self.window_open || hints == self.size_hints {
            return Task::none();
        }
        self.size_hints = hints;

        window::latest().and_then(move |id| {
            Task::batch([
                window::set_min_size(id, hints.map(|hints| hints.min)),
                window::set_resize_increments(id, hints.map(|hints| hints.increments)),
            ])
        })
    }
}
