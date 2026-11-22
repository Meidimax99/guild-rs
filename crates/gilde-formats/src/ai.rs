//! AI action catalogue `gamedata/ai/ai_data_2.dfn` (loaded by `FUN_004832e0`
//! into the 0x94-byte records at `0xfb0080`).
//!
//! ```text
//! "nFDa"  u32 count (73)
//! count × { u8 id, char name[32], 4 × (u8 drive, f32 weight) short, 4 × (u8 drive, f32 weight) long }
//! ```
//!
//! Each action (buy a residence, apply for an office, sabotage, study, …) is
//! weighted against the 13 character drives (see `gilde_sim::drives`), once
//! as a short-term and once as a long-term desire (the developer ini
//! `ai_method_addon.ini` names them `ShortDesire_li_N`/`ShortChange_li_N` and
//! `LongDesire_li_N`/`LongChange_li_N`). Unused slots have drive `0xff` and
//! weight 0. The rest of an action (flags, score/target/execute functions) is
//! registered in code; see `gilde_sim::actions`.

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
    /// Short-term desire: (drive, weight) slots; weight 0 = unused.
    pub short: [DriveWeight; 4],
    /// Long-term desire.
    pub long: [DriveWeight; 4],
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
        let list = |r: &mut Reader| -> Result<[DriveWeight; 4]> {
            let mut v = [DriveWeight { drive: 0xff, weight: 0.0 }; 4];
            for w in &mut v {
                *w = DriveWeight { drive: r.u8()?, weight: r.f32()? };
            }
            Ok(v)
        };
        for _ in 0..n {
            let id = r.u8()?;
            let name = fixed_cstr(r.bytes(32)?);
            let short = list(&mut r)?;
            let long = list(&mut r)?;
            actions.push(AiAction { id, name, short, long });
        }
        if !r.at_end() {
            return Err(Error::Invalid(format!("ai_data: {} trailing bytes", r.remaining())));
        }
        Ok(AiActions { actions })
    }
}
