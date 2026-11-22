//! Runtime world for the simulation, built from a parsed city or savegame,
//! and the per-minute step (`cm_ExSetDate` `0x4b6c20`, see
//! `docs/architecture.md` §5).
//!
//! Ported so far: the character drives (`chr_UpdateDrives`) and the AI
//! scheduling (`ai_Schedule`). The other parts of the step (notices, economy,
//! master evaluation, the AI decision itself, the hourly character update)
//! are reported but not executed yet.

use crate::ai::{AiScheduler, SLOTS, Visit};
use crate::date::Date;
use crate::drives::{DriveContext, Drives};
use gilde_formats::world::WorldFile;

#[derive(Debug, Clone)]
pub struct SimCharacter {
    pub slot: u16,
    pub kind: u8,
    pub name: String,
    pub drives: Drives,
    /// `+0x08`, 0 = dead.
    pub life: u8,
    pub action_points: i32,
    pub residence: i32,
    pub greed: f32,
}

impl SimCharacter {
    fn drive_context(&self) -> DriveContext {
        DriveContext { action_points: self.action_points, residence: self.residence, kind: self.kind, greed: self.greed }
    }
}

/// What one minute step did.
#[derive(Debug, Default, Clone)]
pub struct StepReport {
    /// Characters whose activity changed: (slot, new activity).
    pub activity_changes: Vec<(u16, u8)>,
    /// Characters visited by the AI scheduler.
    pub visits: Vec<Visit>,
}

#[derive(Debug, Clone)]
pub struct World {
    pub date: Date,
    /// Character slots (`g_characters`, 768).
    pub characters: Vec<Option<SimCharacter>>,
    pub ai: AiScheduler,
    /// Session mode flags (`0x71d8d0`); bit `0x80` disables AI decisions.
    pub session_flags: u32,
}

impl World {
    /// Build the runtime world from a city or save. The activity byte is not
    /// stored and starts at 0, as in the game.
    pub fn from_file(file: &WorldFile) -> Self {
        let mut characters: Vec<Option<SimCharacter>> = vec![None; SLOTS];
        for c in &file.characters.list {
            let slot = c.id as usize;
            if slot >= SLOTS {
                continue;
            }
            let Some(drives) = Drives::from_bytes(&c.f88, 0) else { continue };
            let name = format!("{} {}", gilde_formats::reader::fixed_cstr(&c.f30), gilde_formats::reader::fixed_cstr(&c.f40));
            characters[slot] = Some(SimCharacter {
                slot: c.id,
                kind: c.f02,
                name: name.trim().to_owned(),
                drives,
                life: c.f08,
                action_points: c.f194 as i32,
                residence: c.f170 as i32,
                greed: f32::from_bits(c.f1e4),
            });
        }
        let d = &file.globals.date;
        let date = Date::from_bytes(d).unwrap_or_default();
        World { date, characters, ai: AiScheduler { last: date, ..Default::default() }, session_flags: 0x01 }
    }

    pub fn character(&self, slot: usize) -> Option<&SimCharacter> {
        self.characters.get(slot).and_then(Option::as_ref)
    }

    /// The simulation step for a tick command carrying `date`
    /// (`cm_ExSetDate`): only runs if the date moves forward.
    pub fn step(&mut self, date: Date) -> Option<StepReport> {
        if date <= self.date {
            return None;
        }
        self.date = date;
        let mut report = StepReport::default();
        // chr_UpdateDrives: every character.
        for c in self.characters.iter_mut().flatten() {
            let ctx = c.drive_context();
            if let Some(a) = c.drives.update(&ctx) {
                report.activity_changes.push((c.slot, a));
            }
        }
        // ai_Schedule.
        let kinds: Vec<Option<u8>> = self.characters.iter().map(|c| c.as_ref().map(|c| c.kind)).collect();
        report.visits = self.ai.step(&date, self.session_flags & 0x80 != 0, |slot| kinds[slot]);
        Some(report)
    }
}
