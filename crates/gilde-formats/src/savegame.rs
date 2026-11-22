//! Savegame-only state after the embedded scene (`docs/savegame.md` §5).
//!
//! Decoded so far: sections 1–5 of the dynamic state, which end with the
//! pending notices (`sv_LoadNotices` `0x50de20`). The sections before them
//! are skipped by their known sizes.

use crate::reader::{Reader, fixed_cstr};
use crate::world::WorldFile;
use crate::{Error, Result};

/// A pending notice (HE event; 0x14c bytes in memory at `0x726d00`).
#[derive(Debug, Clone)]
pub struct Notice {
    /// Type (`docs/notices.tsv`).
    pub kind: u8,
    pub f04: u32,
    /// Character slot the notice belongs to.
    pub character: u16,
    pub f0c: u32,
    pub f10: u32,
    pub f14: Vec<u8>,
    /// Dates at `+0x44`, `+0x52` (deadline) and `+0x60`, 14 bytes each.
    pub dates: [[u8; 14]; 3],
    /// `+0x70`: timeout, -1 expires the notice.
    pub f70: i32,
    /// `+0x78`: flags (bit 3: an open message box).
    pub flags: u8,
    pub f8c: [u32; 8],
    /// Message text (game markup).
    pub text: String,
    pub payload: Vec<u8>,
}

impl Notice {
    /// The deadline (`+0x52`).
    pub fn deadline(&self) -> [u8; 14] {
        self.dates[1]
    }
}

fn arr<const N: usize>(r: &mut Reader) -> Result<[u8; N]> {
    Ok(r.bytes(N)?.try_into().unwrap())
}

/// Pending notices of a full savegame (`.SAV`); empty for cities and
/// snapshots. `data` is the decompressed file, `world` its parsed prefix.
pub fn notices(data: &[u8], world: &WorldFile) -> Result<Vec<Notice>> {
    if world.header.is_city() {
        return Ok(Vec::new());
    }
    let v = world.header.version;
    let mut r = Reader::new(data);
    r.set_pos(world.dynamic_offset);
    let bad = |what: &str| Error::Invalid(format!("savegame dynamic state: unexpected {what}"));
    // 1. FUN_004e0d00
    if r.u32()? != 0x1d {
        return Err(bad("section 1 magic"));
    }
    r.skip(30 * 4)?;
    let (n1, n2) = if v > 0x10040 { (r.u32()? as usize, r.u32()? as usize) } else { (0x80, 0x200) };
    if n1 > 0x10000 || n2 > 0x10000 {
        return Err(bad("section 1 counts"));
    }
    r.skip(n1 * 41 + n2 * 8)?;
    // 2. two n × n grids
    if v >= 0x10016 {
        let n = if v < 0x1003a { 0x100 } else if v > 0x10041 { 0x300 } else { 0x200 };
        r.skip(n * n * if v > 0x10035 { 2 } else { 1 })?;
    }
    // 3. FUN_0050d9b0
    r.skip(25 * 4 + 2 + 16 * 5 * 4 + 5 * 0x44 + 64 * 9)?;
    // 4. FUN_004db440
    if v >= 0x1002a {
        let n = r.u32()? as usize;
        if n > 256 {
            return Err(bad("avatar count"));
        }
        r.skip(n * 5)?;
    }
    // 5. sv_LoadNotices
    let n = r.u32()? as usize;
    let size = r.u32()? as usize;
    if n > 0x1000 || size > 0x1000 {
        return Err(bad("notice count"));
    }
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let kind = r.u8()?;
        let f04 = r.u32()?;
        let character = r.u16()?;
        let f0c = r.u32()?;
        let f10 = r.u32()?;
        let f14 = r.bytes(0x30)?.to_vec();
        let dates = [arr(&mut r)?, arr(&mut r)?, arr(&mut r)?];
        let f70 = r.i32()?;
        let flags = r.u8()?;
        let mut f8c = [0u32; 8];
        if v > 0x1002e {
            for x in &mut f8c {
                *x = r.u32()?;
            }
        }
        let len = r.u32()? as usize;
        if len > 0x400 {
            return Err(bad("notice text length"));
        }
        let text = fixed_cstr(r.bytes(len)?);
        let payload = r.bytes(size)?.to_vec();
        out.push(Notice { kind, f04, character, f0c, f10, f14, dates, f70, flags, f8c, text, payload });
    }
    Ok(out)
}
