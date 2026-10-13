//! `gfx/Gilde_add_on_<lang>.gfx` — the 2D graphics archive (UI, fonts, icons,
//! backgrounds). Opened by `FUN_004635e0` (`d2_Open`).
//!
//! ```text
//! u32 count (≤ 0x800)
//! count × 0x54-byte entries:
//!   char name[48]      e.g. "_FONT", "_FONT+1", "_SYSTEM_ICONS+3"
//!   u32  offset        file offset of the SHAPBANK (0 for "+N" frame aliases)
//!   u32  runtime       (loaded pointer, 0 on disk)
//!   u32  size
//!   u32  format        < 0x10: converted to textures on load
//!   u32  _
//!   u32  flags         bit 0: preload at startup
//!   u32  _, _
//! ```
//!
//! A `"NAME+N"` entry without data refers to frame `N` of `"NAME"`.
//!
//! ## SHAPBANK (`FUN_00469ce0`, `FUN_00469320`, blitter `FUN_00469700`)
//!
//! ```text
//! 0x00 "SHAPBANK"    0x09 u8 kind
//! 0x2a u16 frame_count   0x2c u16 max_w   0x2e u16 max_h
//! 0x45 u32 frame_offset[frame_count]   (relative to the bank)
//! frame: +0x06 u16 width, +0x0a u16 height, +0x26 i32 (-1 = empty)
//!        +0x32 rows: u32 runs, runs × { u32 skip_bytes, u32 n, n × RGB }
//! ```
//!
//! Each run skips `skip_bytes / 3` transparent pixels, then draws `n` pixels.
//! The blitter gives the first (for runs of ≥ 4 pixels) and last pixel of every
//! run alpha `0xc0`, which softens sprite edges.

use crate::reader::fixed_cstr;
use crate::{Error, Result};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct GfxEntry {
    pub name: String,
    pub offset: u32,
    pub size: u32,
    pub format: u32,
    pub flags: u32,
}

pub struct GfxArchive {
    file: Mutex<File>,
    pub entries: Vec<GfxEntry>,
    by_name: HashMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct Sprite {
    pub width: u32,
    pub height: u32,
    /// Ink bounding box from the frame header (`+0x16 x0, +0x18 y0, +0x1a x1,
    /// +0x1c y1`, inclusive). The text renderer kerns by `x0` and advances by
    /// `x1 + spacing` (shape fields `+0x11`/`+0x15` in `FUN_0045f3a0`).
    pub ink: [i16; 4],
    pub rgba: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct ShapBank {
    pub kind: u8,
    pub max_w: u16,
    pub max_h: u16,
    pub frames: Vec<Sprite>,
}

impl GfxArchive {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = File::open(path.as_ref())?;
        let mut b4 = [0u8; 4];
        file.read_exact(&mut b4)?;
        let n = u32::from_le_bytes(b4);
        if n > 0x800 {
            return Err(Error::Invalid(format!("gfx: too many objects ({n})")));
        }
        let mut table = vec![0u8; n as usize * 0x54];
        file.read_exact(&mut table)?;
        let u = |o: usize| u32::from_le_bytes(table[o..o + 4].try_into().unwrap());
        let mut entries = Vec::with_capacity(n as usize);
        let mut by_name = HashMap::new();
        for i in 0..n as usize {
            let o = i * 0x54;
            let e = GfxEntry {
                name: fixed_cstr(&table[o..o + 0x30]),
                offset: u(o + 0x30),
                size: u(o + 0x38),
                format: u(o + 0x3c),
                flags: u(o + 0x44),
            };
            by_name.insert(e.name.to_uppercase(), i);
            entries.push(e);
        }
        Ok(Self { file: Mutex::new(file), entries, by_name })
    }

    pub fn find(&self, name: &str) -> Option<&GfxEntry> {
        self.by_name.get(&name.to_uppercase()).map(|&i| &self.entries[i])
    }

    /// Raw SHAPBANK bytes of an entry with its own data.
    pub fn raw(&self, e: &GfxEntry) -> Result<Vec<u8>> {
        let mut f = self.file.lock().unwrap();
        f.seek(SeekFrom::Start(e.offset as u64))?;
        let mut buf = vec![0u8; e.size as usize];
        f.read_exact(&mut buf)?;
        Ok(buf)
    }

    /// Load the bank that holds `name` and the frame index it refers to
    /// (`"X+3"` → bank `"X"`, frame 3).
    pub fn resolve(&self, name: &str) -> Option<(&GfxEntry, usize)> {
        let e = self.find(name)?;
        if e.size > 0 {
            return Some((e, 0));
        }
        let (base, idx) = name.rsplit_once('+')?;
        let idx: usize = idx.parse().ok()?;
        let b = self.find(base)?;
        (b.size > 0).then_some((b, idx))
    }

    pub fn bank(&self, name: &str) -> Result<ShapBank> {
        let e = self.find(name).filter(|e| e.size > 0).ok_or_else(|| Error::NotFound(name.into()))?;
        ShapBank::parse(&self.raw(e)?)
    }

    pub fn sprite(&self, name: &str) -> Result<Sprite> {
        let (e, idx) = self.resolve(name).ok_or_else(|| Error::NotFound(name.into()))?;
        let bank = self.raw(e)?;
        decode_frame(&bank, idx)
    }
}

fn rd16(b: &[u8], o: usize) -> Result<u16> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes(s.try_into().unwrap())).ok_or(Error::Eof { offset: o, wanted: 2 })
}

fn rd32(b: &[u8], o: usize) -> Result<u32> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or(Error::Eof { offset: o, wanted: 4 })
}

pub fn frame_count(bank: &[u8]) -> Result<usize> {
    if !bank.starts_with(b"SHAPBANK") {
        return Err(Error::BadMagic { expected: b"SHAPBANK", found: bank.get(..8).unwrap_or(bank).to_vec() });
    }
    Ok(rd16(bank, 0x2a)? as usize)
}

pub fn decode_frame(bank: &[u8], idx: usize) -> Result<Sprite> {
    let count = frame_count(bank)?;
    if idx >= count {
        return Err(Error::Invalid(format!("frame {idx} out of range ({count})")));
    }
    let f = rd32(bank, 0x45 + 4 * idx)? as usize;
    let w = rd16(bank, f + 6)? as u32;
    let h = rd16(bank, f + 10)? as u32;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    let ink = [0x16, 0x18, 0x1a, 0x1c].map(|o| rd16(bank, f + o).unwrap_or(0) as i16);
    if rd32(bank, f + 0x26)? as i32 == -1 {
        return Ok(Sprite { width: w, height: h, ink, rgba });
    }
    let mut p = f + 0x32;
    for y in 0..h {
        let runs = rd32(bank, p)?;
        p += 4;
        let mut x = 0u32;
        for _ in 0..runs {
            let skip = rd32(bank, p)?;
            let n = rd32(bank, p + 4)?;
            p += 8;
            x += skip / 3;
            let px = bank.get(p..p + n as usize * 3).ok_or(Error::Eof { offset: p, wanted: n as usize * 3 })?;
            for k in 0..n {
                if x < w {
                    let o = ((y * w + x) * 4) as usize;
                    let a = if (n >= 4 && k == 0) || k == n - 1 { 0xc0 } else { 0xff };
                    let s = (k * 3) as usize;
                    rgba[o..o + 4].copy_from_slice(&[px[s], px[s + 1], px[s + 2], a]);
                }
                x += 1;
            }
            p += n as usize * 3;
        }
    }
    Ok(Sprite { width: w, height: h, ink, rgba })
}

impl ShapBank {
    pub fn parse(bank: &[u8]) -> Result<Self> {
        let n = frame_count(bank)?;
        Ok(ShapBank {
            kind: bank[9],
            max_w: rd16(bank, 0x2c)?,
            max_h: rd16(bank, 0x2e)?,
            frames: (0..n).map(|i| decode_frame(bank, i)).collect::<Result<_>>()?,
        })
    }
}
