//! Runtime world for the simulation, built from a parsed city or savegame,
//! and the per-minute step (`cm_ExSetDate` `0x4b6c20`, see
//! `docs/architecture.md` §5).
//!
//! Ported so far: notice deadlines (`msg_ProcessNotices`, handlers not yet),
//! the character drives (`chr_UpdateDrives`), the AI scheduling
//! (`ai_Schedule`) and the AI decision (`ai_Decide`) with the actions whose
//! target and execute functions are ported (see [`crate::actions`]). The
//! other parts of the step (economy, master evaluation, the hourly character
//! update, workshop staffing in `ai_Decide`) are not executed yet.

use crate::actions::{self, ActionTable, BasicPlanner, Choice, DriveDelta, FLAG_REPEATABLE};
use crate::ai::{AiScheduler, SLOTS, Visit};
use crate::command::{Command, kind};
use crate::date::Date;
use crate::drives::{DriveContext, Drives};
use crate::notices::Notices;
use gilde_formats::world::WorldFile;

#[derive(Debug, Clone)]
pub struct SimCharacter {
    pub slot: u16,
    /// Object id (`+0x04`), how commands address the character.
    pub object_id: u32,
    pub kind: u8,
    pub name: String,
    pub drives: Drives,
    /// `+0x08`, 0 = dead.
    pub life: u8,
    pub action_points: i32,
    pub residence: i32,
    pub greed: f32,
    /// `+0x218`: which actions the character may use (`& flags`).
    pub action_mask: u8,
    /// `+0x1b1`: no AI actions while set.
    pub no_actions: u8,
    /// `+0x1cc`: bits `0x3000` suspend the AI.
    pub state: u32,
    /// `+0x131`/`+0x132`: last decision (not stored, 0 after loading).
    pub choice: Choice,
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
    /// Notices whose deadline passed: (type, character slot).
    pub notices_due: Vec<(u8, u16)>,
    /// AI decisions: (slot, choice, carried out).
    pub decisions: Vec<(u16, Choice, bool)>,
    /// Commands the step sends (single player executes them right away,
    /// see [`World::apply`]).
    pub commands: Vec<Command>,
}

#[derive(Debug, Clone)]
pub struct World {
    pub date: Date,
    /// Character slots (`g_characters`, 768).
    pub characters: Vec<Option<SimCharacter>>,
    pub ai: AiScheduler,
    pub notices: Notices,
    /// Session mode flags (`0x71d8d0`); bit `0x80` disables AI decisions.
    pub session_flags: u32,
    /// AI actions (empty: no decisions are made).
    pub actions: ActionTable,
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
                object_id: c.f04,
                kind: c.f02,
                name: name.trim().to_owned(),
                drives,
                life: c.f08,
                action_points: c.f194 as i32,
                residence: c.f170 as i32,
                greed: f32::from_bits(c.f1e4),
                action_mask: c.f214[4],
                no_actions: c.f1b1,
                state: c.f1cc,
                choice: Choice::default(),
            });
        }
        let d = &file.globals.date;
        let date = Date::from_bytes(d).unwrap_or_default();
        World {
            date,
            characters,
            ai: AiScheduler { last: date, ..Default::default() },
            notices: Notices::default(),
            session_flags: 0x01,
            actions: ActionTable::default(),
        }
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
        // msg_ProcessNotices.
        let mut report = StepReport { notices_due: self.notices.process(&date), ..Default::default() };
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
        let decide: Vec<usize> = report.visits.iter().filter(|v| v.decide).map(|v| v.slot).collect();
        for slot in decide {
            if let Some((choice, delta)) = self.decide(slot) {
                report.decisions.push((slot as u16, choice, delta.is_some()));
                report.commands.extend(delta.map(|d| d.to_command()));
            }
        }
        Some(report)
    }

    /// `ai_Decide` (`0x46c7d0`) for one character: choose an action and, if
    /// it is new (or repeatable) and succeeds, the drive changes to send.
    /// `None` if the character is not up for a decision.
    pub fn decide(&mut self, slot: usize) -> Option<(Choice, Option<DriveDelta>)> {
        let table = &self.actions;
        let c = self.characters.get_mut(slot)?.as_mut()?;
        // ai_CanDecide (0x46c200): AI-run kinds; network split not modelled
        // (single player processes every character).
        if !matches!(c.kind, 2..=5) || c.state & 0x3000 != 0 || c.life == 0 || c.no_actions != 0 || table.is_empty() {
            return None;
        }
        let last = c.choice.action;
        let choice = actions::select(table, &c.drives, c.action_mask, last, &mut BasicPlanner);
        c.choice = choice;
        let repeatable = table.get(choice.action).is_some_and(|a| a.flags & FLAG_REPEATABLE != 0);
        let delta = if choice.action != 0 && (choice.action != last || repeatable) {
            actions::execute(table, choice, c.object_id, &mut BasicPlanner)
        } else {
            None
        };
        Some((choice, delta))
    }

    /// Execute a command on this machine (`cm_ExecCommand`), for the types
    /// that are ported. Returns whether it was handled.
    pub fn apply(&mut self, cmd: &Command) -> bool {
        match cmd.kind {
            kind::DRIVE_DELTA => {
                let Some(d) = DriveDelta::from_command(cmd) else { return false };
                if let Some(c) = self.characters.iter_mut().flatten().find(|c| c.object_id == d.character) {
                    d.apply(&mut c.drives);
                }
                true
            }
            _ => false,
        }
    }
}
