//! Rendering and world-assembly layer for the Die Gilde reimplementation.
//!
//! Coordinate system matches the original Direct3D 8 engine: left-handed, Y up,
//! units in centimetres-ish world units (a terrain cell is 100 units).

pub mod assets;
pub mod camera;
pub mod render;
pub mod terrain;
pub mod world;

pub use assets::Assets;
pub use camera::Camera;
pub use render::Renderer;
pub use world::World;
