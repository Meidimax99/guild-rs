//! The game date (`g_date`, 14 bytes at `0x79cca0`).
//!
//! ```text
//! u32 round     one round = one in-game day, shown as a year (1400 + round);
//!               round & 3 is the season (date_Season, FUN_004ec530)
//! u16 hour      @4
//! u32 minute    @6
//! u32 second    @10
//! ```
//! Days start at 06:00 (`game_Session`). [`Date::add`] is `FUN_004ec320`,
//! ordering is `FUN_004ec3f0`, the calendar view is `FUN_004ec4a0`.

use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Date {
    pub round: i32,
    pub hour: u16,
    pub minute: i32,
    pub second: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
}

impl Date {
    pub fn start_of_round(round: i32) -> Self {
        Date { round, hour: 6, minute: 0, second: 0 }
    }

    pub fn season_index(&self) -> usize {
        (self.round & 3) as usize
    }

    pub fn season(&self) -> Season {
        [Season::Spring, Season::Summer, Season::Autumn, Season::Winter][self.season_index()]
    }

    /// Seconds since midnight.
    pub fn time_of_day(&self) -> i32 {
        (self.hour as i32 * 60 + self.minute) * 60 + self.second
    }

    /// `date_Add` (`0x4ec320`): add hours/minutes/seconds with carry into the
    /// round — exactly as the original, which divides with truncation (so
    /// negative inputs can leave negative minutes/seconds) and keeps the hour
    /// in a 16-bit field (a negative hour sum wraps). Checked against the
    /// original with recorded vectors (`tests/difftest.rs`).
    pub fn add(&mut self, hours: i32, minutes: i32, seconds: i32) {
        let s = self.second.wrapping_add(seconds);
        let m = self.minute.wrapping_add(s / 60);
        self.second = s % 60;
        let m = m.wrapping_add(minutes);
        self.hour = self.hour.wrapping_add((m / 60) as u16);
        self.minute = m % 60;
        let mut h = (self.hour as u32).wrapping_add(hours as u32);
        if (h as i32) > 23 {
            self.round = self.round.wrapping_add((h / 24) as i32);
            h %= 24;
        }
        if (h as i32) < 0 {
            let k = (h.wrapping_neg().wrapping_sub(1)) / 24 + 1;
            self.round = self.round.wrapping_sub(k as i32);
            self.hour = (h as u16).wrapping_add((k as u16).wrapping_mul(24));
        } else {
            self.hour = h as u16;
        }
    }

    /// Calendar display (`FUN_004ec4a0`): year 1400 + round, month by season.
    pub fn year(&self) -> i32 {
        1400 + self.round
    }

    pub fn month(&self) -> u32 {
        self.season_index() as u32 * 3 + 1
    }

    /// The 14-byte layout used in memory, savegames and command payloads.
    pub fn to_bytes(&self) -> [u8; 14] {
        let mut b = [0u8; 14];
        b[0..4].copy_from_slice(&self.round.to_le_bytes());
        b[4..6].copy_from_slice(&self.hour.to_le_bytes());
        b[6..10].copy_from_slice(&self.minute.to_le_bytes());
        b[10..14].copy_from_slice(&self.second.to_le_bytes());
        b
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let b: &[u8; 14] = b.get(..14)?.try_into().ok()?;
        Some(Date {
            round: i32::from_le_bytes(b[0..4].try_into().unwrap()),
            hour: u16::from_le_bytes(b[4..6].try_into().unwrap()),
            minute: i32::from_le_bytes(b[6..10].try_into().unwrap()),
            second: i32::from_le_bytes(b[10..14].try_into().unwrap()),
        })
    }

    /// Whole minutes from `self` to `later` (`FUN_004ec460`; seconds are
    /// ignored).
    pub fn minutes_until(&self, later: &Date) -> i32 {
        ((later.round - self.round) * 24 - self.hour as i32 + later.hour as i32) * 60 - self.minute + later.minute
    }

    /// Set the time of day, keeping the round (`date_SetTime`).
    pub fn set_time(&mut self, hour: u16, minute: i32, second: i32) {
        self.hour = hour;
        self.minute = minute;
        self.second = second;
    }
}

impl PartialOrd for Date {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Date {
    fn cmp(&self, o: &Self) -> Ordering {
        self.round.cmp(&o.round).then(self.time_of_day().cmp(&o.time_of_day()))
    }
}

/// Light-transition times per season in minutes (`0x618c40`, 4 × 6 ints).
pub const LIGHT_TIMES: [[i32; 6]; 4] = [
    [390, 510, 630, 1050, 1170, 1290],
    [300, 420, 540, 1140, 1260, 1380],
    [420, 540, 660, 1020, 1140, 1260],
    [480, 600, 690, 960, 1020, 1140],
];

/// Which of the scene's 7 light sets to use and how far to blend into the next.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DayPhase {
    pub set: usize,
    pub blend: f32,
}

impl DayPhase {
    /// `sky_UpdateLightByTime` (`FUN_00513b70`): map the time of day onto
    /// 0..600 piecewise between the season's transition times; the day span
    /// (t2..t3) covers 200 units, the others 100.
    pub fn at(date: &Date) -> Self {
        let t = date.time_of_day();
        let b = LIGHT_TIMES[date.season_index()].map(|m| m * 60);
        let seg = |lo: i32, hi: i32, span: i32, base: i32| base + (t - lo) * span / (hi - lo).max(1);
        let v = if t < b[0] {
            0
        } else if t < b[1] {
            seg(b[0], b[1], 100, 0)
        } else if t < b[2] {
            seg(b[1], b[2], 100, 100)
        } else if t < b[3] {
            seg(b[2], b[3], 200, 200)
        } else if t < b[4] {
            seg(b[3], b[4], 100, 400)
        } else if t < b[5] {
            seg(b[4], b[5], 100, 500)
        } else {
            600
        };
        let set = (v / 100) as usize;
        DayPhase { set: set % 7, blend: v as f32 * 0.01 - set as f32 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_carries() {
        let mut d = Date::start_of_round(3);
        d.add(17, 59, 61);
        assert_eq!(d, Date { round: 4, hour: 0, minute: 0, second: 1 });
        // Like the original: truncating division leaves a negative minute
        // instead of borrowing from the hour.
        d.add(0, -1, 0);
        assert_eq!((d.round, d.hour, d.minute), (4, 0, -1));
        // Negative hours do borrow from the round.
        d.add(-1, 1, 0);
        assert_eq!((d.round, d.hour, d.minute), (3, 23, 0));
    }

    #[test]
    fn phases() {
        let mut d = Date::start_of_round(0);
        d.hour = 3;
        assert_eq!(DayPhase::at(&d).set, 0);
        d.hour = 12;
        assert_eq!(DayPhase::at(&d).set, 2);
        d.hour = 23;
        assert_eq!(DayPhase::at(&d).set, 6);
    }
}
