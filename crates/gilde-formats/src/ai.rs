//! AI action catalogue `gamedata/ai/ai_data_2.dfn` (loaded by `FUN_004832e0`
//! into the 0x94-byte records at `0xfb0080`).
//!
//! ```text
//! "nFDa"  u32 count (73)
//! count × { u8 id, char name[32], 4 × (u8 drive, f32 weight), 4 × (u8 drive, f32 weight) }
//! ```
//!
//! Each action (buy a residence, apply for an office, sabotage, study, …) is
//! weighted against the 13 character drives (see `gilde_sim::drives`). The
//! first list scores how much a character wants the action: the sum of
//! `weight / k · drive value`, with `k` smaller for the character's current
//! activity (`FUN_00492e10`); performing it lowers those drives by half the
//! weight (`FUN_004b2230`). The meaning of the second list is open.

use crate::reader::{Reader, fixed_cstr};
use crate::{Error, Result};

pub const MAGIC: &[u8; 4] = b"nFDa";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DriveWeight {
    pub drive: u8,
    pub weight: f32,
}

#[derive(Debug, Clone)]
pub struct AiAction {
    pub id: u8,
    pub name: String,
    /// Drives that motivate the action (zero weights are unused slots).
    pub motivation: Vec<DriveWeight>,
    /// Second weight list (open).
    pub second: Vec<DriveWeight>,
}

#[derive(Debug, Clone)]
pub struct AiActions {
    pub actions: Vec<AiAction>,
}

impl AiActions {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.get(..4) != Some(MAGIC) {
            return Err(Error::Invalid("not an AI action catalogue (magic nFDa)".into()));
        }
        let mut r = Reader::new(data);
        r.skip(4)?;
        let n = r.u32()?;
        let mut actions = Vec::with_capacity(n as usize);
        let list = |r: &mut Reader| -> Result<Vec<DriveWeight>> {
            let mut v = Vec::new();
            for _ in 0..4 {
                let w = DriveWeight { drive: r.u8()?, weight: r.f32()? };
                if w.weight != 0.0 {
                    v.push(w);
                }
            }
            Ok(v)
        };
        for _ in 0..n {
            let id = r.u8()?;
            let name = fixed_cstr(r.bytes(32)?);
            let motivation = list(&mut r)?;
            let second = list(&mut r)?;
            actions.push(AiAction { id, name, motivation, second });
        }
        if !r.at_end() {
            return Err(Error::Invalid(format!("ai_data: {} trailing bytes", r.remaining())));
        }
        Ok(AiActions { actions })
    }
}
