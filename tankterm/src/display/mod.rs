//! Display types kept for configuration parsing.

pub mod color;

/// Window padding in pixels, which can differ on every side.
#[derive(Debug, Copy, Clone, Default, PartialEq)]
pub struct WindowPadding {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl WindowPadding {
    /// The same padding on the left and right, and on the top and bottom.
    pub const fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
    }
}
