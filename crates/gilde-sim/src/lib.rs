//! Game simulation (in progress): calendar, game clock, replicated commands,
//! character drives, AI scheduling and the runtime world.

pub mod ai;
pub mod clock;
pub mod command;
pub mod date;
pub mod drives;
pub mod notices;
pub mod world;

pub use clock::{ClockEvent, LocalClock, ServerClock};
pub use command::{Command, Executor, Outcome};
pub use date::{Date, DayPhase};
