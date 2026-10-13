//! The 2D user interface ("d2" in the original engine, `0x459000`–`0x46c000`).
//!
//! This crate is renderer-independent: widgets produce a [`DrawList`] in the
//! original 800×600 coordinate space, which `gilde-engine` rasterises.

pub mod assets;
pub mod draw;
pub mod text;
pub mod widgets;

pub use assets::{Font, UiAssets};
pub use draw::{DrawCmd, DrawList, SpriteRef};
pub use widgets::{Event, Screen, Widget, WidgetId, Window};

/// Reference resolution of all UI layouts.
pub const UI_WIDTH: f32 = 800.0;
pub const UI_HEIGHT: f32 = 600.0;
