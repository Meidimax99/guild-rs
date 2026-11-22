//! Character drives (`chr_UpdateDrives` `0x46bef0`, per character
//! `chr_UpdateDrivesOne` `0x46bf20`), run once per game minute.
//!
//! Every character has 13 drives at `+0x88` (12 bytes each: rate, decay,
//! value 0..=1000) and a "fatigue" triple at `+0x124`, followed by the current
//! activity byte at `+0x130` — the index of the drive being acted on. The
//! whole block (0xa8 bytes from `+0x88`) is stored in city files and saves.
//!
//! Per minute: fatigue and every drive except the current activity move
//! towards `rate / decay` (`value + rate − decay·value`, clamped); some
//! drives are suppressed by the character's situation. The strongest drive
//! becomes the activity; the current one is favoured while fatigue is low
//! (`(2 − fatigue/500) · value`).

pub const DRIVES: usize = 13;

/// Drive meanings, from the action catalogue `ai_data_2.dfn` (its `Dummy*`
/// and `UseObject (…)` entries) and the suppression rules below.
pub const DRIVE_NAMES: [&str; DRIVES] = [
    "action points",     // 0: grows while the character has < 7 action points (+0x194)
    "health",            // 1: Unversehrtheit; reset unless active
    "residence",         // 2: Wohnsitz; suppressed once a residence is owned (+0x170)
    "money",             // 3: rate scaled by the greed factor (+0x1e4)
    "profession",        // 4: Beruf
    "pleasure",          // 5: Vergnügen
    "reputation",        // 6: Ansehen
    "power",             // 7: offices (Amtsbewerbung, UseAmt)
    "education",         // 8: Bildung
    "righteousness",     // 9: Rechtschaffenheit
    "villainy",          // 10: Gemeinheit
    "security",          // 11: Sicherheit
    "family",            // 12: Fortpflanzung / Dynastie
];
/// Size of the stored block (`+0x88` … `+0x12f`).
pub const BLOCK_SIZE: usize = 0xa8;
pub const MAX_VALUE: f32 = 1000.0;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Drive {
    pub rate: f32,
    pub decay: f32,
    pub value: f32,
}

impl Drive {
    fn step(&self, rate: f32) -> f32 {
        (rate + self.value - self.decay * self.value).clamp(0.0, MAX_VALUE)
    }
}

/// The parts of the character the update looks at besides the drives.
#[derive(Debug, Clone, Copy, Default)]
pub struct DriveContext {
    /// Character `+0x194`, action points: drive 0 only grows below 7.
    pub action_points: i32,
    /// Character `+0x170`, residence: drive 2 only grows without one …
    pub residence: i32,
    /// … and if the character is not a politician (kind 3).
    pub kind: u8,
    /// Character `+0x1e4`, greed: multiplies the rate of drive 3 (money).
    pub greed: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Drives {
    pub drives: [Drive; DRIVES],
    pub fatigue: Drive,
    /// Current activity (`+0x130`).
    pub activity: u8,
}

impl Drives {
    /// From the stored block and the activity byte.
    pub fn from_bytes(block: &[u8], activity: u8) -> Option<Self> {
        if block.len() < BLOCK_SIZE {
            return None;
        }
        let f = |o: usize| f32::from_le_bytes(block[o..o + 4].try_into().unwrap());
        let triple = |o: usize| Drive { rate: f(o), decay: f(o + 4), value: f(o + 8) };
        let mut d = Drives { activity, fatigue: triple(DRIVES * 12), ..Default::default() };
        for (i, x) in d.drives.iter_mut().enumerate() {
            *x = triple(i * 12);
        }
        Some(d)
    }

    pub fn to_bytes(&self) -> [u8; BLOCK_SIZE] {
        let mut b = [0u8; BLOCK_SIZE];
        let mut put = |o: usize, d: &Drive| {
            b[o..o + 4].copy_from_slice(&d.rate.to_le_bytes());
            b[o + 4..o + 8].copy_from_slice(&d.decay.to_le_bytes());
            b[o + 8..o + 12].copy_from_slice(&d.value.to_le_bytes());
        };
        for (i, d) in self.drives.iter().enumerate() {
            put(i * 12, d);
        }
        put(DRIVES * 12, &self.fatigue);
        b
    }

    /// One game minute. Returns the new activity if it changed.
    pub fn update(&mut self, ctx: &DriveContext) -> Option<u8> {
        self.fatigue.value = self.fatigue.step(self.fatigue.rate);
        let current = self.activity as usize;
        let mut best = 0usize;
        let mut best_score = 0.0f32;
        for i in 0..DRIVES {
            if i != current {
                let d = &mut self.drives[i];
                match i {
                    1 => d.value = 0.0,
                    0 if ctx.action_points >= 7 => d.value = 0.0,
                    2 if ctx.residence != 0 || ctx.kind == 3 => d.value = 0.0,
                    3 => d.value = d.step(d.rate * ctx.greed),
                    _ => d.value = d.step(d.rate),
                }
            }
            let v = self.drives[i].value;
            let score = if i == current { (2.0 - self.fatigue.value * 0.002) * v } else { v };
            if best_score <= score {
                best_score = score;
                best = i;
            }
        }
        if best == current {
            return None;
        }
        let old = &mut self.drives[current];
        if old.value >= MAX_VALUE {
            old.value -= 50.0;
        }
        self.activity = best as u8;
        self.fatigue.value = 0.0;
        Some(self.activity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> DriveContext {
        DriveContext { action_points: 0, residence: 0, kind: 0, greed: 1.0 }
    }

    #[test]
    fn drive_converges_to_rate_over_decay() {
        let mut d = Drives { activity: 12, ..Default::default() };
        d.drives[5] = Drive { rate: 10.0, decay: 0.02, value: 0.0 };
        // A dominant current activity keeps drive 5 from taking over.
        d.drives[12].value = 1000.0;
        for _ in 0..2000 {
            d.update(&ctx());
        }
        assert!((d.drives[5].value - 500.0).abs() < 1.0, "{}", d.drives[5].value);
    }

    #[test]
    fn strongest_drive_wins_and_current_is_favoured() {
        let mut d = Drives { activity: 4, ..Default::default() };
        d.drives[4].value = 300.0;
        d.drives[6].value = 500.0;
        d.drives[6].decay = 0.0;
        // Fresh activity: score 2 · 300 = 600 beats 500.
        assert_eq!(d.update(&ctx()), None);
        // With full fatigue the bonus is gone (factor 0): switch.
        d.fatigue.value = 1000.0;
        assert_eq!(d.update(&ctx()), Some(6));
        assert_eq!(d.fatigue.value, 0.0);
    }

    #[test]
    fn situational_suppression() {
        let mut d = Drives { activity: 12, ..Default::default() };
        for i in 0..4 {
            d.drives[i] = Drive { rate: 5.0, decay: 0.0, value: 100.0 };
        }
        d.update(&DriveContext { action_points: 7, residence: 1, kind: 0, greed: 2.0 });
        assert_eq!(d.drives[0].value, 0.0);
        assert_eq!(d.drives[1].value, 0.0);
        assert_eq!(d.drives[2].value, 0.0);
        assert_eq!(d.drives[3].value, 110.0);
    }

    #[test]
    fn byte_round_trip() {
        let mut d = Drives::default();
        d.drives[7] = Drive { rate: 1.5, decay: 0.25, value: 42.0 };
        d.fatigue = Drive { rate: 2.0, decay: 0.1, value: 3.0 };
        assert_eq!(Drives::from_bytes(&d.to_bytes(), 0).unwrap(), d);
    }
}
