//! Parsers for the data formats used by *Die Gilde Gold* (The Guild Gold, 2003).
//!
//! Every format here was reverse engineered from `GildeGold_TL.exe` and the shipped
//! data files. See `docs/formats.md` in the repository root for the byte layouts and
//! the addresses of the original loader routines.

pub mod ai;
pub mod baf;
pub mod bgf;
pub mod error;
pub mod form;
pub mod gfx;
pub mod proto;
pub mod reader;
pub mod res;
pub mod savegame;
pub mod scene;
pub mod text;
pub mod vfs;
pub mod world;

pub use error::{Error, Result};
