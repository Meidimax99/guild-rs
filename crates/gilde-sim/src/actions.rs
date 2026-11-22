//! AI actions ("methods"): what a character decides to do once its turn in
//! the AI schedule comes (`ai_Decide` `0x46c7d0`).
//!
//! An action is a 0x94-byte record in the table at `0xfb0080` (73 slots).
//! The code registers each one (`am_RegisterMethod` `0x483990`, called by
//! `0x48fb60`, `0x47f080`, `0x47dfe0`, `0x49a330`) with three functions and a
//! flags word; `gamedata/ai/ai_data_2.dfn` then supplies the drive weights:
//!
//! ```text
//! +0x00 u8      id
//! +0x01 char[32] name
//! +0x24 fn      score(&short, &long, id, character, …) -> usable
//! +0x28 fn      target(mode, character, depth, out1, out2) -> action to carry out (0 = impossible)
//! +0x2c fn      execute(character, params1, params2) -> success
//! +0x30 4 × (u8 drive, f32 weight)  short-term desire
//! +0x50 4 × (u8 drive, f32 weight)  working copy of the short-term list
//! +0x70 4 × (u8 drive, f32 weight)  long-term desire
//! +0x90 u32     flags: low byte = which characters may use it (`& character +0x218`),
//!               0x200 = may be repeated
//! ```
//!
//! **Selection** (`ai_SelectAction` `0x483c40`): every usable action is
//! scored; the best short-term and the best long-term candidate are kept,
//! each confirmed by its target function, which returns the action actually
//! carried out (it can plan a different, preparatory action). The long-term
//! choice wins if the best short-term score is 0 or lower than the long-term
//! one.
//!
//! **Execution** (`ai_ExecuteAction` `0x484150`): on success the motivating
//! action's short-term drives drop by half their weight, the carried-out
//! action's as well, and fatigue rises by 500 (same action) or 250. The
//! changes are not applied directly but sent as command `0x18`
//! ([`DriveDelta`]), so every machine applies them.

use crate::drives::{DRIVES, Drives, MAX_VALUE};
use gilde_formats::ai::{AiActions, DriveWeight};

/// Table slots (ids `1..73`).
pub const SLOTS: usize = 0x49;
/// Flag: the action may be chosen again right after itself.
pub const FLAG_REPEATABLE: u32 = 0x200;
/// Drive index of fatigue in drive-delta commands (`+0x124`).
pub const FATIGUE: u8 = DRIVES as u8;

/// How an action is scored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scoring {
    /// `ai_ScoreAction` `0x492e10` on the drive weights.
    Generic,
    /// An action-specific score function (not ported yet).
    Custom(u32),
}

/// The registration data of every action: id, name, score function, flags
/// (dumped with `re/scripts/ai_methods.py`). Target and execute functions are
/// listed in `docs/ai_actions.tsv`.
pub const METHODS: [(u8, &str, Scoring, u32); 72] = {
    use Scoring::{Custom as C, Generic as G};
    [
        (1, "DummyUnversehrtheit", G, 0x1),
        (2, "DummyBeruf", G, 0x1),
        (3, "DummyVergnuegen", G, 0x1),
        (4, "Bildung", G, 0x1),
        (5, "Bildung (Beruf)", G, 0x1),
        (6, "DummyRechtschaffenheit", G, 0x1),
        (7, "DummySicherheit", G, 0x1),
        (8, "DummyFortpflanzung", G, 0x1),
        (9, "Bestechung", C(0x484f80), 0x19),
        (10, "Amtsbewerbung", C(0x485860), 0x1b),
        (11, "Objekt kaufen", G, 0x1),
        (12, "Objekt Kaufen (Markt)", G, 0xff),
        (13, "Objekt kaufen (AP)", G, 0x1),
        (14, "Raum kaufen", G, 0xff),
        (15, "Repair Building", G, 0xff),
        (16, "Wohnsitz kaufen", G, 0x3),
        (17, "Wohnsitz bauen", G, 0x3),
        (18, "Wohnsitz upgraden (Credits)", G, 0x3),
        (19, "Betrieb bauen", G, 0x3),
        (20, "Betrieb upgraden (AP)", G, 0x3),
        (21, "Betrieb upgraden (Credits)", G, 0x3),
        (22, "Medicus ausfsuchen", G, 0x3b),
        (23, "Sabotieren", C(0x487a40), 0x1c3),
        (24, "Verpruegeln", C(0x487ff0), 0x1c9),
        (25, "Buergerrecht", G, 0x1b),
        (26, "ApsSparen", G, 0x1),
        (27, "UseObject (Military)", G, 0x3),
        (28, "UseObject (Ansehen)", G, 0x19),
        (29, "UseObject (APS)", G, 0x39),
        (30, "UseObject (Bildung)", G, 0x3b),
        (31, "UseObject (Unversehrtheit)", G, 0x3f),
        (32, "UseObject (Dynastie)", G, 0x17),
        (33, "UseObject (Gemeinheit)", G, 0xc9),
        (34, "UseObject (Sicherheit)", G, 0x39),
        (35, "Use Parfuem", C(0x48a540), 0xff),
        (36, "Use Trank", C(0x48a790), 0xff),
        (37, "Spionage", G, 0x69),
        (38, "Ablass Kaufen", G, 0x39),
        (39, "UseAmt", G, 0xff),
        (40, "Drohen", C(0x48b240), 0x5d),
        (41, "Pamphlete", C(0x48b4c0), 0x5d),
        (42, "Anschwaerzen", G, 0x39),
        (43, "Beleidigen", C(0x48b920), 0xc9),
        (44, "Erpressen", C(0x48ba70), 0xc9),
        (45, "UsePrivs", G, 0xff),
        (46, "Buergermeister", C(0x48bc60), 0x39),
        (47, "Baumeister", G, 0x3b),
        (48, "Buergermeister upgrade", G, 0x39),
        (49, "Buergermeister Rathaus", G, 0x39),
        (50, "Dunkle Ecke", C(0x48d080), 0x69),
        (51, "Kredit Nehmen", G, 0x7),
        (52, "Change Profession", C(0x48d960), 0x7),
        (53, "Stammtisch", G, 0x19),
        (54, "Zunft Preismanipulation", G, 0x3),
        (55, "Zunft Aufruhr", G, 0x43),
        (56, "Zunft Level 3", G, 0x4b),
        (57, "Upgrade Zunft", C(0x48ee00), 0x1b),
        (58, "Berufsausbildung", C(0x48f470), 0x1f),
        (59, "Studium", G, 0x13),
        (60, "Buergermeister Kerker", G, 0x39),
        (61, "Graveyard Owner", G, 0x1c3),
        (62, "Handle Grave", G, 0x1),
        (63, "Use Seance", G, 0x1),
        (64, "Build Domizil", G, 0x1c3),
        (65, "UseSecretRoom", G, 0x1c3),
        (66, "UseCannonTower", G, 0x1c1),
        (67, "BuildSculpture", G, 0x3),
        (68, "Blutrache", G, 0x1c3),
        (69, "Verfluchen", G, 0x1c3),
        (70, "Theater", G, 0x1),
        (71, "Ehrerbietung", G, 0x3),
        (72, "UseAlchemistryItems", G, 0x1c3),
    ]
};

const UNUSED: DriveWeight = DriveWeight { drive: 0xff, weight: 0.0 };

#[derive(Debug, Clone)]
pub struct ActionDef {
    pub id: u8,
    pub name: &'static str,
    pub scoring: Scoring,
    pub flags: u32,
    pub short: [DriveWeight; 4],
    pub long: [DriveWeight; 4],
}

impl ActionDef {
    /// Whether a character with action mask `mask` (`+0x218`) may use it.
    pub fn usable_by(&self, mask: u8) -> bool {
        self.flags as u8 & mask != 0
    }
}

/// The action table (`0xfb0080`).
#[derive(Debug, Clone, Default)]
pub struct ActionTable {
    pub actions: Vec<Option<ActionDef>>,
}

impl ActionTable {
    /// Registered actions with the weights of `ai_data_2.dfn`
    /// (`ai_LoadActions` `0x4832e0`).
    pub fn new(weights: &AiActions) -> Self {
        let mut actions: Vec<Option<ActionDef>> = vec![None; SLOTS];
        for &(id, name, scoring, flags) in &METHODS {
            actions[id as usize] = Some(ActionDef { id, name, scoring, flags, short: [UNUSED; 4], long: [UNUSED; 4] });
        }
        for w in &weights.actions {
            if let Some(Some(a)) = actions.get_mut(w.id as usize) {
                a.short = w.short;
                a.long = w.long;
            }
        }
        ActionTable { actions }
    }

    pub fn get(&self, id: u8) -> Option<&ActionDef> {
        self.actions.get(id as usize).and_then(Option::as_ref)
    }

    pub fn is_empty(&self) -> bool {
        self.actions.iter().all(Option::is_none)
    }
}

/// Value of drive `d` as `ai_ScoreAction` reads it (`+0x90 + d·12`): the 13
/// drives, then fatigue; unused slots (`0xff`) carry weight 0.
fn drive_value(drives: &Drives, d: u8) -> f32 {
    match d as usize {
        i if i < DRIVES => drives.drives[i].value,
        DRIVES => drives.fatigue.value,
        _ => 0.0,
    }
}

/// Result of `ai_ScoreAction`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Score {
    pub short: f32,
    pub long: f32,
    pub usable: bool,
}

/// `ai_ScoreAction` (`0x492e10`): `Σ weight / k · drive value` over both
/// lists, with `k` = 40 for the drive the character is acting on and 50
/// otherwise (100/50 if the action's flags exclude the character, which
/// then only counts as usable if its current activity is one of the
/// action's positively weighted drives). Computed in `f32` like the game
/// (24-bit FPU precision).
pub fn score(action: &ActionDef, drives: &Drives, mask: u8) -> Score {
    let activity = drives.activity;
    let allowed = action.usable_by(mask);
    let (k_active, k_other) = if allowed { (40.0f32, 50.0f32) } else { (50.0, 100.0) };
    let mut usable = allowed;
    if !usable {
        let motivated = |list: &[DriveWeight; 4]| list.iter().any(|w| w.drive == activity && w.weight > 0.0);
        usable = motivated(&action.short) || motivated(&action.long);
    }
    let sum = |list: &[DriveWeight; 4]| {
        let mut s = 0.0f32;
        for w in list {
            let k = if w.drive == activity { k_active } else { k_other };
            s += w.weight / k * drive_value(drives, w.drive);
        }
        s
    };
    Score { short: sum(&action.short), long: sum(&action.long), usable }
}

/// What the AI needs from the rest of the game for one character. The
/// default methods cover what is ported: generic scoring, and target and
/// execute functions only for the drive-satisfying `Dummy*` actions.
pub trait Planner {
    /// The score function. `Custom` scores are not ported: `None` (unusable).
    fn score(&mut self, action: &ActionDef, drives: &Drives, mask: u8) -> Option<Score> {
        match action.scoring {
            Scoring::Generic => Some(score(action, drives, mask)),
            Scoring::Custom(_) => None,
        }
    }

    /// The target function in planning mode (mode 0, depth 0): the action to
    /// carry out, 0 if impossible.
    fn target(&mut self, action: &ActionDef) -> u8 {
        match action.id {
            // FUN_004844a0 … FUN_00484470: always possible, carry out itself.
            1 | 2 | 3 | 6 | 7 | 8 => action.id,
            _ => 0,
        }
    }

    /// The execute function; true on success.
    fn execute(&mut self, action: &ActionDef) -> bool {
        matches!(action.id, 1 | 2 | 3 | 6 | 7 | 8)
    }
}

/// The planner with only the ported parts.
#[derive(Debug, Default, Clone, Copy)]
pub struct BasicPlanner;
impl Planner for BasicPlanner {}

/// Result of a selection: character `+0x131` and `+0x132`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Choice {
    /// The action whose desire won.
    pub motive: u8,
    /// The action carried out (returned by the target function); 0 = none.
    pub action: u8,
}

/// `ai_SelectAction` (`0x483c40`). `last` is the previously carried-out
/// action (`+0x132`), which is not considered again.
pub fn select(table: &ActionTable, drives: &Drives, mask: u8, last: u8, planner: &mut impl Planner) -> Choice {
    let (mut best_short, mut best_long) = (-1e30f32, -1e30f32);
    let (mut short_pick, mut long_pick) = (Choice::default(), Choice::default());
    for a in table.actions.iter().flatten() {
        if a.id == last || !a.usable_by(mask) {
            continue;
        }
        let Some(s) = planner.score(a, drives, mask) else { continue };
        if !s.usable || !(best_short <= s.short || best_long <= s.long) {
            continue;
        }
        let target = planner.target(a);
        if target == 0 {
            continue;
        }
        if best_short <= s.short {
            best_short = s.short;
            short_pick = Choice { motive: a.id, action: target };
        }
        if best_long <= s.long {
            best_long = s.long;
            long_pick = Choice { motive: a.id, action: target };
        }
    }
    if best_short == 0.0 || best_short < best_long { long_pick } else { short_pick }
}

/// Command `0x18` (`cm_RequestDriveDelta` `0x4b2da0`, executed by
/// `cm_ExDriveDelta` `0x4b63d0`): add deltas to a character's drive values
/// (index 13 = fatigue), each clamped to `0..=1000`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DriveDelta {
    /// Character object id (`+0x04`).
    pub character: u32,
    pub deltas: Vec<(u8, f32)>,
}

impl DriveDelta {
    /// At most 23 entries fit (`cm_AddDriveDelta` `0x4b2230`).
    pub const MAX: usize = 23;

    pub fn apply(&self, drives: &mut Drives) {
        for &(d, delta) in &self.deltas {
            let v = match d as usize {
                i if i < DRIVES => &mut drives.drives[i].value,
                DRIVES => &mut drives.fatigue.value,
                _ => continue,
            };
            let x = *v + delta;
            *v = if x < 0.0 || x < MAX_VALUE { if x >= 0.0 { x } else { 0.0 } } else { MAX_VALUE };
        }
    }

    pub fn to_command(&self) -> crate::Command {
        let mut c = crate::Command::new(crate::command::kind::DRIVE_DELTA);
        c.payload[0..4].copy_from_slice(&self.character.to_le_bytes());
        c.payload[4] = self.deltas.len() as u8;
        for (i, &(d, v)) in self.deltas.iter().enumerate() {
            let o = 5 + i * 5;
            c.payload[o] = d;
            c.payload[o + 1..o + 5].copy_from_slice(&v.to_le_bytes());
        }
        c
    }

    pub fn from_command(c: &crate::Command) -> Option<Self> {
        if c.kind != crate::command::kind::DRIVE_DELTA {
            return None;
        }
        let p = &c.payload;
        let n = (p[4] as usize).min(Self::MAX);
        let deltas = (0..n).map(|i| (p[5 + i * 5], f32::from_le_bytes(p[6 + i * 5..10 + i * 5].try_into().unwrap()))).collect();
        Some(DriveDelta { character: u32::from_le_bytes(p[0..4].try_into().unwrap()), deltas })
    }
}

/// `ai_ExecuteAction` (`0x484150`) after a successful execute function: the
/// drive changes to send. `None` if there is no action or it failed.
pub fn execute(table: &ActionTable, choice: Choice, character: u32, planner: &mut impl Planner) -> Option<DriveDelta> {
    let action = table.get(choice.action)?;
    if !planner.execute(action) {
        return None;
    }
    let mut deltas = Vec::new();
    if let Some(m) = table.get(choice.motive) {
        deltas.extend(m.short.iter().filter(|w| w.drive < 0xe).map(|w| (w.drive, w.weight * -0.5)));
    }
    // The working copy at +0x50, equal to the short-term list.
    deltas.extend(action.short.iter().filter(|w| w.drive < 0xe).map(|w| (w.drive, w.weight * -0.5)));
    deltas.push((FATIGUE, if choice.motive == choice.action { 500.0 } else { 250.0 }));
    deltas.truncate(DriveDelta::MAX);
    Some(DriveDelta { character, deltas })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gilde_formats::ai::AiAction;

    fn w(drive: u8, weight: f32) -> DriveWeight {
        DriveWeight { drive, weight }
    }

    fn table() -> ActionTable {
        let mut list = [UNUSED; 4];
        let mut actions = Vec::new();
        for (id, d) in [(1u8, 1u8), (3, 5), (7, 11)] {
            list[0] = w(d, 10.0);
            list[1] = w(3, -10.0);
            actions.push(AiAction { id, name: String::new(), short: list, long: [UNUSED; 4] });
        }
        ActionTable::new(&AiActions { actions })
    }

    #[test]
    fn methods_cover_the_table() {
        let ids: Vec<u8> = METHODS.iter().map(|m| m.0).collect();
        assert_eq!(ids, (1..SLOTS as u8).collect::<Vec<_>>());
    }

    #[test]
    fn picks_the_strongest_satisfiable_drive() {
        let t = table();
        let mut d = Drives::default();
        d.drives[1].value = 100.0;
        d.drives[5].value = 900.0;
        d.drives[11].value = 300.0;
        d.activity = 5;
        let c = select(&t, &d, 1, 0, &mut BasicPlanner);
        assert_eq!(c, Choice { motive: 3, action: 3 });
        // The previous action is skipped.
        let c = select(&t, &d, 1, 3, &mut BasicPlanner);
        assert_eq!(c, Choice { motive: 7, action: 7 });
        // A mask that excludes everything.
        assert_eq!(select(&t, &d, 2, 0, &mut BasicPlanner), Choice::default());
    }

    #[test]
    fn execution_lowers_the_drive_and_adds_fatigue() {
        let t = table();
        let delta = execute(&t, Choice { motive: 3, action: 3 }, 42, &mut BasicPlanner).unwrap();
        assert_eq!(delta.deltas, vec![(5, -5.0), (3, 5.0), (5, -5.0), (3, 5.0), (FATIGUE, 500.0)]);
        let back = DriveDelta::from_command(&delta.to_command()).unwrap();
        assert_eq!(back, delta);
        let mut d = Drives::default();
        d.drives[5].value = 3.0;
        d.fatigue.value = 900.0;
        back.apply(&mut d);
        assert_eq!(d.drives[5].value, 0.0);
        assert_eq!(d.drives[3].value, 10.0);
        assert_eq!(d.fatigue.value, 1000.0);
    }
}
