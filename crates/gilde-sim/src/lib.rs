//! Game simulation (in progress): calendar, game clock, replicated commands
//! and the authoritative server core.

pub mod clock;
pub mod command;
pub mod date;

pub use clock::{ClockEvent, LocalClock, ServerClock};
pub use command::{Command, Executor, Outcome};
pub use date::{Date, DayPhase};
