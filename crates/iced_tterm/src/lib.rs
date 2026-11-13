pub mod actions;
pub mod bindings;
pub mod settings;

mod backend;
mod box_drawing;
mod cursor;
mod font;
mod keyboard;
mod terminal;
mod theme;
mod view;

pub use backend::Command as BackendCommand;
pub use backend::{LinkAction, MouseButton};
pub use tankterm_terminal::event::Event as AlacrittyEvent;
pub use tankterm_terminal::grid::Scroll;
pub use tankterm_terminal::index::Point as AlacrittyPoint;
pub use tankterm_terminal::selection::SelectionType;
pub use tankterm_terminal::term::TermMode;
pub use tankterm_terminal::vte::ansi::CursorShape;
pub use terminal::{Command, Event, Terminal};
pub use theme::{
    CellColor, CellColors, ColorPalette, Elements, TextAttributes, Theme,
};
pub use view::TerminalView;
