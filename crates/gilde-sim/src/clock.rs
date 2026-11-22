//! The game clock. The simulation advances one step per command
//! [`kind::SET_DATE`](crate::command::kind::SET_DATE); this module decides
//! when those commands are produced (`docs/architecture.md` §5).
//!
//! * [`LocalClock`]: single player. `clk_Advance` (`0x51f960`) runs every 71
//!   timer ticks of 14 ms and adds `(speed × 0.00625 + 0.5) × 100` game
//!   seconds to a local date while the hour is below 23; `master_Step` sends
//!   that date to itself with `cm_RequestSetDate` whenever it differs from
//!   the game date.
//! * [`ServerClock`]: network game, run by the server (`srv_Main`). One game
//!   minute per interval (speed table `0x10024738`); at 23:00 the round ends
//!   and the next round starts at 06:00 once every client has acknowledged.

use crate::date::Date;

/// Multimedia timer period (`timer_Start(0xe, …)`).
pub const TIMER_MS: u32 = 14;
/// `timer_Register(clk_Advance, 0x47)`.
pub const LOCAL_CLOCK_TICKS: u32 = 71;
/// The hour at which a round ends (both clocks stop advancing).
pub const END_OF_DAY_HOUR: u16 = 23;
/// The hour a new round starts at.
pub const START_OF_DAY_HOUR: u16 = 6;
/// Server: milliseconds per game minute for speed 0..=4 (default 2).
pub const SERVER_MS_PER_MINUTE: [u32; 5] = [1155, 937, 770, 577, 385];
pub const SERVER_DEFAULT_SPEED: usize = 2;

/// Game seconds added per `clk_Advance` call for a `speed` setting (`game.ini`
/// `speed`, options slider in steps of 40, fast-forward 750).
///
/// The game runs with the x87 FPU in 24-bit precision (Direct3D 8 without
/// `D3DCREATE_FPU_PRESERVE`), so every intermediate result is rounded to
/// single precision; `f32` arithmetic reproduces that exactly (e.g. speed 296
/// gives 234, not 235). Checked against the original in `tests/difftest.rs`.
pub fn local_seconds_per_call(speed: i32) -> i32 {
    ((speed as f32 * 0.00625_f32 + 0.5) * 100.0) as i32
}

/// Single-player clock.
#[derive(Debug, Clone)]
pub struct LocalClock {
    /// The clock's own date (`0x78a2f0`); runs ahead of the game date until
    /// the game executes the tick command.
    pub date: Date,
    pub speed: i32,
    pub paused: bool,
    /// Keep running past 23:00 (session flag `0x80`, e.g. the tutorial).
    pub ignore_night: bool,
    elapsed_ms: u32,
}

impl LocalClock {
    pub fn new(date: Date, speed: i32) -> Self {
        LocalClock { date, speed, paused: false, ignore_night: false, elapsed_ms: 0 }
    }

    /// Advance by `real_ms` milliseconds. Returns the date to send as a tick
    /// command if it differs from `game_date` (the date the simulation has
    /// reached), like `master_Step` does.
    pub fn update(&mut self, real_ms: u32, game_date: &Date) -> Option<Date> {
        if !self.paused {
            self.elapsed_ms += real_ms;
            let period = LOCAL_CLOCK_TICKS * TIMER_MS;
            while self.elapsed_ms >= period {
                self.elapsed_ms -= period;
                self.advance_once(game_date);
            }
        }
        (self.date != *game_date).then_some(self.date)
    }

    /// One `clk_Advance` call.
    pub fn advance_once(&mut self, game_date: &Date) {
        if (self.date.hour < END_OF_DAY_HOUR && game_date.hour < END_OF_DAY_HOUR) || self.ignore_night {
            let s = local_seconds_per_call(self.speed);
            self.date.add(s / 3600, (s % 3600) / 60, s % 60);
        }
    }

    /// Start the next round (after the end-of-round processing).
    pub fn next_round(&mut self) {
        self.date.round += 1;
        self.date.set_time(START_OF_DAY_HOUR, 0, 0);
    }
}

/// What the server clock asks the server to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockEvent {
    /// Broadcast a tick command with this date.
    Tick(Date),
    /// Broadcast "end of round" (system message sub-type 2).
    EndOfRound,
}

/// Server clock (`srv_Main` game loop).
#[derive(Debug, Clone)]
pub struct ServerClock {
    pub date: Date,
    ms_per_minute: u32,
    running: bool,
    next_due: u64,
}

impl ServerClock {
    pub fn new(date: Date) -> Self {
        ServerClock { date, ms_per_minute: SERVER_MS_PER_MINUTE[SERVER_DEFAULT_SPEED], running: false, next_due: 0 }
    }

    /// System message `0x12`: values above 3 select the fastest speed.
    pub fn set_speed(&mut self, speed: u32) {
        self.ms_per_minute = SERVER_MS_PER_MINUTE[speed.min(4) as usize];
    }

    pub fn ms_per_minute(&self) -> u32 {
        self.ms_per_minute
    }

    /// Whether the clock waits for all clients to acknowledge (start of the
    /// game and of every round).
    pub fn waiting_for_acks(&self) -> bool {
        !self.running
    }

    /// Run the clock at time `now_ms`. `all_acked`: every connected client
    /// has acknowledged since the last end of round (the caller clears the
    /// acknowledgements when this returns ticks after waiting). `relay_pending`:
    /// records are waiting to be relayed, which holds the clock.
    pub fn poll(&mut self, now_ms: u64, all_acked: bool, relay_pending: bool) -> Vec<ClockEvent> {
        if relay_pending {
            return Vec::new();
        }
        if !self.running {
            if !all_acked {
                return Vec::new();
            }
            self.running = true;
            self.next_due = now_ms;
        } else if self.next_due > now_ms {
            return Vec::new();
        }
        let mut events = Vec::new();
        self.date.add(0, 1, 0);
        events.push(ClockEvent::Tick(self.date));
        if self.date.hour >= END_OF_DAY_HOUR {
            events.push(ClockEvent::EndOfRound);
            self.running = false;
            self.date.round += 1;
            self.date.set_time(START_OF_DAY_HOUR, 0, 0);
        }
        self.next_due += self.ms_per_minute as u64;
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_rate() {
        assert_eq!(local_seconds_per_call(0), 50);
        assert_eq!(local_seconds_per_call(40), 75);
        assert_eq!(local_seconds_per_call(750), 518);
    }

    #[test]
    fn local_clock_stops_at_night() {
        let start = Date::start_of_round(3);
        let mut c = LocalClock::new(start, 40);
        let mut game = start;
        let mut calls = 0;
        while c.date.hour < END_OF_DAY_HOUR {
            if let Some(d) = c.update(LOCAL_CLOCK_TICKS * TIMER_MS, &game) {
                game = d;
            }
            calls += 1;
        }
        // 17 hours at 75 s per call.
        assert_eq!(calls, (17 * 3600 + 74) / 75);
        let stuck = c.date;
        for _ in 0..10 {
            c.update(LOCAL_CLOCK_TICKS * TIMER_MS, &game);
        }
        assert_eq!(c.date, stuck);
        c.next_round();
        assert_eq!(c.date, Date::start_of_round(4));
    }

    #[test]
    fn server_day() {
        let mut c = ServerClock::new(Date::start_of_round(0));
        assert!(c.poll(0, false, false).is_empty(), "waits for acknowledgements");
        let mut now = 0;
        let mut ticks = 0;
        loop {
            let ev = c.poll(now, true, false);
            ticks += ev.iter().filter(|e| matches!(e, ClockEvent::Tick(_))).count();
            if ev.contains(&ClockEvent::EndOfRound) {
                break;
            }
            now += 770;
        }
        assert_eq!(ticks, 17 * 60);
        assert_eq!(c.date, Date::start_of_round(1));
        assert!(c.waiting_for_acks());
        assert!(c.poll(now + 10_000, false, false).is_empty());
    }

    #[test]
    fn server_interval_and_speed() {
        let mut c = ServerClock::new(Date::start_of_round(0));
        assert_eq!(c.poll(0, true, false).len(), 1);
        assert!(c.poll(769, true, false).is_empty());
        assert_eq!(c.poll(770, true, false).len(), 1);
        c.set_speed(9);
        assert_eq!(c.ms_per_minute(), 385);
        assert!(c.poll(2000, true, true).is_empty(), "relays hold the clock");
    }
}
