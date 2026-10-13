//! `.form` UI layouts (`forms.bin0`), loaded by `FUN_00463d60`.
//!
//! `"FRM2"`, `u32 count`, then `count` window records of 0x101c bytes (the older
//! format without magic uses 0xed4-byte records and is not shipped). Field
//! offsets are documented in `docs/formats.md`.

use crate::reader::fixed_cstr;
use crate::{Error, Result};

pub const RECORD_SIZE: usize = 0x101c;
/// Records of the old, magic-less format (2 shipped tooltip forms).
pub const OLD_RECORD_SIZE: usize = 0xed4;
pub const MAX_CONTROLS: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    /// `< 0x40`: gfx image (the value is passed to the image constructor).
    Image(i32),
    /// `0x40`: slot for a child window (param byte 0 = window number).
    WindowSlot,
    /// `0x41`: input/edit field.
    Input,
    /// `0x43`: text label (name = text key).
    Text,
    /// `0x45`: slider.
    Slider,
    Other(i32),
}

impl ControlKind {
    fn from_raw(v: i32) -> Self {
        match v {
            v if v < 0x40 => ControlKind::Image(v),
            0x40 => ControlKind::WindowSlot,
            0x41 => ControlKind::Input,
            0x43 => ControlKind::Text,
            0x45 => ControlKind::Slider,
            v => ControlKind::Other(v),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Control {
    pub x: i16,
    pub y: i16,
    pub kind: ControlKind,
    pub name: String,
    pub param: [u8; 4],
    /// Per-control byte at `0xf1c` (font for sliders).
    pub extra: u8,
}

#[derive(Debug, Clone)]
pub struct FormWindow {
    pub x: i16,
    pub y: i16,
    pub w: i16,
    pub h: i16,
    /// Window style bits (`FUN_00461f70`): 1 translucent fill, 4 border,
    /// 8 icon, 0x10 text area, 0x20 close icons.
    pub style: u32,
    pub controls: Vec<Control>,
    pub title_key: String,
    pub font: String,
    /// 1-based index of the parent window in this form; 0 = top level.
    pub parent: i32,
    pub image: String,
    pub margins: [i16; 4],
    pub texture: String,
    pub alpha: f32,
    pub border: String,
}

#[derive(Debug, Clone)]
pub struct Form {
    pub windows: Vec<FormWindow>,
}

fn i16_at(b: &[u8], o: usize) -> i16 {
    i16::from_le_bytes([b[o], b[o + 1]])
}

fn i32_at(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

impl Form {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let (old, start, size) = if data.starts_with(b"FRM2") { (false, 8, RECORD_SIZE) } else { (true, 4, OLD_RECORD_SIZE) };
        if data.len() < start {
            return Err(Error::Eof { offset: data.len(), wanted: start });
        }
        let n = i32_at(data, start - 4).max(0) as usize;
        if data.len() < start + n * size {
            return Err(Error::Eof { offset: data.len(), wanted: start + n * size });
        }
        let windows = (0..n)
            .map(|i| {
                let r = &data[start + i * size..start + (i + 1) * size];
                if old {
                    return FormWindow {
                        x: i16_at(r, 0),
                        y: i16_at(r, 2),
                        w: i16_at(r, 4),
                        h: i16_at(r, 6),
                        style: i32_at(r, 8) as u32,
                        controls: Vec::new(),
                        title_key: fixed_cstr(&r[0xe50..0xe90]),
                        font: String::new(),
                        parent: i32_at(r, 0xe94),
                        image: String::new(),
                        margins: [4; 4],
                        texture: String::new(),
                        alpha: 0.65,
                        border: String::new(),
                    };
                }
                let count = (i16_at(r, 0xcc).max(0) as usize).min(MAX_CONTROLS);
                let controls = (0..count)
                    .map(|k| Control {
                        x: i16_at(r, 0x0c + 2 * k),
                        y: i16_at(r, 0x6c + 2 * k),
                        kind: ControlKind::from_raw(i32_at(r, 0xd0 + 4 * k)),
                        name: fixed_cstr(&r[0x190 + 0x40 * k..0x1d0 + 0x40 * k]),
                        param: r[0xd90 + 4 * k..0xd94 + 4 * k].try_into().unwrap(),
                        extra: r[0xf1c + k],
                    })
                    .collect();
                FormWindow {
                    x: i16_at(r, 0),
                    y: i16_at(r, 2),
                    w: i16_at(r, 4),
                    h: i16_at(r, 6),
                    style: i32_at(r, 8) as u32,
                    controls,
                    title_key: fixed_cstr(&r[0xe50..0xe90]),
                    font: fixed_cstr(&r[0xe90..0xed0]),
                    parent: i32_at(r, 0xed0),
                    image: fixed_cstr(&r[0xed4..0xf14]),
                    margins: [i16_at(r, 0xf14), i16_at(r, 0xf16), i16_at(r, 0xf18), i16_at(r, 0xf1a)],
                    texture: fixed_cstr(&r[0xf4c..0xf8c]),
                    alpha: f32::from_le_bytes(r[0xf8c..0xf90].try_into().unwrap()),
                    border: fixed_cstr(&r[0xf90..0xfd0]),
                }
            })
            .collect();
        Ok(Form { windows })
    }
}
