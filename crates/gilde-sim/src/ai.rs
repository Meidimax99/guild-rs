//! AI scheduling (`ai_Schedule` `0x46c9d0`), run once per game minute.
//!
//! The 768 character slots are visited round-robin at 12.8 slots per elapsed
//! game minute (the fraction carries over), so every character is visited
//! once per game hour, between 06:00 and 23:00. A visited character gets its
//! hourly update (`FUN_004787f0`); between 07:00 and 22:00 it also gets an AI
//! decision (`FUN_0046c7d0`) — AI dynasty heads every hour, everyone else
//! every second hour (when `(slot ^ hour)` is even).

use crate::date::Date;

/// Character slots (`g_characters`).
pub const SLOTS: usize = 0x300;
/// Slots visited per game minute.
pub const SLOTS_PER_MINUTE: f32 = 12.8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visit {
    pub slot: usize,
    /// Run an AI decision for this character.
    pub decide: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AiScheduler {
    /// Next slot to visit (`0xfb5740`).
    pub next: usize,
    /// Date of the last run (`0xfb5744`).
    pub last: Date,
    /// Fractional slot carry (`0xfb5754`).
    pub carry: f32,
}

impl AiScheduler {
    /// Run at `now`. `kind(slot)` returns the character kind of an occupied
    /// slot (None = free). `special_session` is session flag `0x80`
    /// (tutorial), which disables decisions.
    pub fn step(&mut self, now: &Date, special_session: bool, kind: impl Fn(usize) -> Option<u8>) -> Vec<Visit> {
        let elapsed = self.last.minutes_until(now);
        if elapsed <= 0 || now.hour >= 23 || now.hour <= 5 {
            return Vec::new();
        }
        let previous = self.last;
        self.last = *now;
        if previous.round < now.round {
            // First run in a new round: only take the date.
            return Vec::new();
        }
        let count = if elapsed < 61 {
            let exact = elapsed as f32 * SLOTS_PER_MINUTE;
            let whole = exact as usize;
            self.carry += exact - whole as f32;
            if self.carry >= 1.0 {
                self.carry -= 1.0;
                whole + 1
            } else {
                whole
            }
        } else {
            // More than an hour behind: visit everyone, catch up one hour.
            self.last = Date { hour: previous.hour + 1, ..previous };
            SLOTS
        };
        let decide_hours = now.hour > 6 && now.hour < 22 && !special_session;
        let mut visits = Vec::new();
        let mut slot = self.next;
        for _ in 0..count {
            if let Some(k) = kind(slot) {
                let decide = decide_hours && (k == 5 || ((slot as u16 ^ now.hour) & 1) == 0);
                visits.push(Visit { slot, decide });
            }
            slot = (slot + 1) % SLOTS;
        }
        self.next = slot;
        visits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(hour: u16, minute: i32) -> Date {
        Date { round: 1, hour, minute, second: 0 }
    }

    #[test]
    fn every_slot_once_per_hour() {
        let mut s = AiScheduler { last: at(8, 0), ..Default::default() };
        let mut seen = vec![0u32; SLOTS];
        for m in 1..=60 {
            for v in s.step(&at(8 + (m / 60) as u16, m % 60), false, |_| Some(0)) {
                seen[v.slot] += 1;
            }
        }
        assert!(seen.iter().all(|&n| n == 1), "{:?}", seen.iter().filter(|&&n| n != 1).count());
    }

    #[test]
    fn decisions_by_kind_and_parity() {
        let mut s = AiScheduler { last: at(9, 0), ..Default::default() };
        let v = s.step(&at(9, 1), false, |slot| Some(if slot == 3 { 5 } else { 0 }));
        assert_eq!(v.len(), 12);
        for x in &v {
            assert_eq!(x.decide, x.slot == 3 || (x.slot as u16 ^ 9) & 1 == 0, "slot {}", x.slot);
        }
        // Outside 07:00–22:00 nobody decides; outside 06:00–23:00 nothing runs.
        let mut s = AiScheduler { last: at(22, 0), ..Default::default() };
        assert!(s.step(&at(22, 5), false, |_| Some(5)).iter().all(|x| !x.decide));
        let mut s = AiScheduler { last: at(23, 0), ..Default::default() };
        assert!(s.step(&at(23, 5), false, |_| Some(5)).is_empty());
    }

    #[test]
    fn free_slots_and_new_round() {
        let mut s = AiScheduler { last: at(9, 0), ..Default::default() };
        assert!(s.step(&at(9, 1), false, |_| None).is_empty());
        assert_eq!(s.next, 12);
        let next_day = Date { round: 2, hour: 6, minute: 1, second: 0 };
        assert!(s.step(&next_day, false, |_| Some(0)).is_empty(), "first run of a round only takes the date");
        assert_eq!(s.last, next_day);
    }
}
