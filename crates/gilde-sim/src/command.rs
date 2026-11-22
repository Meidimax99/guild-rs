//! Network commands: the replicated changes to the world.
//!
//! Every change that must be identical on all machines is a command record
//! of 0x99 bytes: a client builds it (`cm_Request*`, sent with `cm_Send`
//! `0x4b1d40`), the server validates and orders it, and every client executes
//! it through the handler table at `0x635030` (`cm_ExecCommand` `0x4b2510`).
//! Single player loops the commands back locally. See `docs/architecture.md`
//! §4 and `docs/commands.tsv`.
//!
//! ```text
//! +0x00 u8   type (0x00..0x69)
//! +0x01 u16  length on the wire
//! +0x03 u8   connection index of the sender
//! +0x04 u32  request id (low 15 bits index the acknowledgement slots); -1 = none/broadcast
//! +0x08 u32  sequence count
//! +0x0c u32  ?
//! +0x10      payload (0x81 bytes; byte 0 is a sub-type for some types)
//! +0x91 u32  previous record (queue link, memory only)
//! +0x95 u32  next record (queue link, memory only)
//! ```

use crate::date::Date;

pub const RECORD_SIZE: usize = 0x99;
pub const PAYLOAD_OFFSET: usize = 0x10;
pub const PAYLOAD_SIZE: usize = 0x91 - PAYLOAD_OFFSET;
/// Number of command types (handler table size).
pub const COMMAND_TYPES: usize = 0x6a;
/// "No request id": the server sets this on records it broadcasts.
pub const NO_REQUEST: u32 = u32::MAX;

/// Command types whose meaning is known.
pub mod kind {
    /// Connection handshake; the server answers with the role.
    pub const HELLO: u8 = 0x03;
    /// Role / phase notification from the server.
    pub const ROLE: u8 = 0x04;
    /// Start of a transaction (records executed together).
    pub const TX_BEGIN: u8 = 0x05;
    /// End of a transaction.
    pub const TX_END: u8 = 0x06;
    /// File transfer: total size (payload u32).
    pub const FILE_SIZE: u8 = 0x08;
    /// File transfer: one 0x80-byte chunk.
    pub const FILE_CHUNK: u8 = 0x09;
    /// Sell objects (`cm_ExSellObjekt`).
    pub const SELL_OBJECT: u8 = 0x11;
    /// Add to byte/short/int fields of an object (`cm_ExFieldDelta`):
    /// object id, count, then (size, count, u16 offset, values).
    pub const FIELD_DELTA: u8 = 0x16;
    /// Add to a character's drive values (`cm_ExDriveDelta`), see
    /// [`crate::actions::DriveDelta`].
    pub const DRIVE_DELTA: u8 = 0x18;
    /// Game clock tick: set the date, run one simulation step.
    pub const SET_DATE: u8 = 0x1e;
    /// System message, sub-types in [`system`].
    pub const SYSTEM: u8 = 0x20;
    /// Building upgrade (`cm_ExGebUpgrade`).
    pub const BUILDING_UPGRADE: u8 = 0x3a;
    /// Construct a building (`cm_ExCreateGeb`).
    pub const CREATE_BUILDING: u8 = 0x4c;
    /// Lobby command (`lb_ExLobbyServerCommand`).
    pub const LOBBY: u8 = 0x5f;
}

/// Sub-types of [`kind::SYSTEM`] (payload byte 0).
pub mod system {
    /// Client acknowledges the clock (ready for the next round).
    pub const TICK_ACK: u8 = 0x01;
    /// End of the round (game day).
    pub const END_OF_ROUND: u8 = 0x02;
    /// The server tells a client its index (payload u32 at byte 1).
    pub const CLIENT_INDEX: u8 = 0x07;
    /// The client has a record for the server to relay.
    pub const RELAY: u8 = 0x0e;
    /// Set the game speed (payload u32 at byte 1, 0..=4).
    pub const SPEED: u8 = 0x12;
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Command {
    pub kind: u8,
    pub conn: u8,
    pub request: u32,
    pub seq: u32,
    pub field_0c: u32,
    pub payload: [u8; PAYLOAD_SIZE],
}

impl Command {
    pub fn new(kind: u8) -> Self {
        Command { kind, conn: 0xff, request: NO_REQUEST, seq: 0, field_0c: 0, payload: [0; PAYLOAD_SIZE] }
    }

    /// Command `0x1e` carrying a date (`cm_RequestSetDate` `0x4b30c0`).
    pub fn set_date(date: &Date) -> Self {
        let mut c = Command::new(kind::SET_DATE);
        c.payload[..14].copy_from_slice(&date.to_bytes());
        c
    }

    /// The date of a [`kind::SET_DATE`] command.
    pub fn date(&self) -> Option<Date> {
        (self.kind == kind::SET_DATE).then(|| Date::from_bytes(&self.payload).unwrap())
    }

    /// A system message with a sub-type and a u32 value.
    pub fn system(sub: u8, value: u32) -> Self {
        let mut c = Command::new(kind::SYSTEM);
        c.payload[0] = sub;
        c.payload[1..5].copy_from_slice(&value.to_le_bytes());
        c
    }

    /// Payload byte 0, the sub-type of system messages.
    pub fn sub_kind(&self) -> u8 {
        self.payload[0]
    }

    /// The u32 after the sub-type.
    pub fn value(&self) -> u32 {
        u32::from_le_bytes(self.payload[1..5].try_into().unwrap())
    }

    pub fn to_record(&self) -> [u8; RECORD_SIZE] {
        let mut r = [0u8; RECORD_SIZE];
        r[0] = self.kind;
        // Deviation: the original computes a per-type wire length
        // (server FUN_1000b9d0); the full record length is always valid.
        r[1..3].copy_from_slice(&(RECORD_SIZE as u16).to_le_bytes());
        r[3] = self.conn;
        r[4..8].copy_from_slice(&self.request.to_le_bytes());
        r[8..12].copy_from_slice(&self.seq.to_le_bytes());
        r[12..16].copy_from_slice(&self.field_0c.to_le_bytes());
        r[PAYLOAD_OFFSET..PAYLOAD_OFFSET + PAYLOAD_SIZE].copy_from_slice(&self.payload);
        r
    }

    pub fn from_record(r: &[u8]) -> Option<Self> {
        if r.len() < PAYLOAD_OFFSET + PAYLOAD_SIZE {
            return None;
        }
        let u32_at = |o: usize| u32::from_le_bytes(r[o..o + 4].try_into().unwrap());
        Some(Command {
            kind: r[0],
            conn: r[3],
            request: u32_at(4),
            seq: u32_at(8),
            field_0c: u32_at(12),
            payload: r[PAYLOAD_OFFSET..PAYLOAD_OFFSET + PAYLOAD_SIZE].try_into().unwrap(),
        })
    }
}

/// Result of executing one command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// No handler for this type (the original logs "Invalid Commandtype").
    Unhandled,
}

pub type Handler<W> = fn(&mut W, &Command) -> Outcome;

/// Client side of `cm_ExecCommand`: executes commands in arrival order
/// through a handler table, keeps transactions together and counts gaps in
/// the sequence numbers.
pub struct Executor<W> {
    handlers: Vec<Option<Handler<W>>>,
    /// Sequence count of the last command that had a request id.
    pub last_seq: Option<u32>,
    /// Commands whose sequence number skipped ahead ("Lost a Command").
    pub lost: u32,
    transaction: Option<Vec<Command>>,
}

impl<W> Default for Executor<W> {
    fn default() -> Self {
        Executor { handlers: vec![None; COMMAND_TYPES], last_seq: None, lost: 0, transaction: None }
    }
}

impl<W> Executor<W> {
    pub fn register(&mut self, kind: u8, handler: Handler<W>) {
        self.handlers[kind as usize] = Some(handler);
    }

    /// Receive one command; returns the outcomes of the commands executed
    /// now (several when a transaction completes, none while it is open).
    pub fn receive(&mut self, world: &mut W, cmd: Command) -> Vec<(u8, Outcome)> {
        match cmd.kind {
            kind::TX_BEGIN => {
                self.transaction = Some(Vec::new());
                return Vec::new();
            }
            kind::TX_END => {
                let group = self.transaction.take().unwrap_or_default();
                return group.into_iter().map(|c| self.execute(world, c)).collect();
            }
            _ => {}
        }
        if let Some(t) = &mut self.transaction {
            t.push(cmd);
            return Vec::new();
        }
        vec![self.execute(world, cmd)]
    }

    fn execute(&mut self, world: &mut W, cmd: Command) -> (u8, Outcome) {
        if cmd.request != NO_REQUEST {
            if let Some(last) = self.last_seq
                && cmd.seq != last.wrapping_add(1)
                && cmd.seq != last
            {
                self.lost += 1;
            }
            self.last_seq = Some(cmd.seq);
        }
        let outcome = match self.handlers.get(cmd.kind as usize).copied().flatten() {
            Some(h) => h(world, &cmd),
            None => Outcome::Unhandled,
        };
        (cmd.kind, outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_round_trip() {
        let mut c = Command::set_date(&Date { round: 19, hour: 6, minute: 58, second: 0 });
        c.conn = 2;
        c.request = 0x1234;
        c.seq = 77;
        let r = c.to_record();
        assert_eq!(r.len(), 0x99);
        assert_eq!(r[0], 0x1e);
        assert_eq!(&r[0x10..0x1e], &Date { round: 19, hour: 6, minute: 58, second: 0 }.to_bytes());
        assert_eq!(Command::from_record(&r).unwrap(), c);
        assert_eq!(c.date().unwrap().minute, 58);
    }

    #[derive(Default)]
    struct World {
        log: Vec<u8>,
    }

    #[test]
    fn transactions_execute_together() {
        let mut ex = Executor::<World>::default();
        ex.register(0x11, |w, c| {
            w.log.push(c.kind);
            Outcome::Done
        });
        let mut w = World::default();
        assert!(ex.receive(&mut w, Command::new(kind::TX_BEGIN)).is_empty());
        assert!(ex.receive(&mut w, Command::new(0x11)).is_empty());
        assert!(ex.receive(&mut w, Command::new(0x11)).is_empty());
        assert!(w.log.is_empty());
        let out = ex.receive(&mut w, Command::new(kind::TX_END));
        assert_eq!(out, vec![(0x11, Outcome::Done), (0x11, Outcome::Done)]);
        assert_eq!(w.log.len(), 2);
        assert_eq!(ex.receive(&mut w, Command::new(0x12)), vec![(0x12, Outcome::Unhandled)]);
    }

    #[test]
    fn sequence_gaps_are_counted() {
        let mut ex = Executor::<World>::default();
        let mut w = World::default();
        for seq in [1, 2, 3, 5, 6, 6] {
            let mut c = Command::new(0x11);
            c.request = 1;
            c.seq = seq;
            ex.receive(&mut w, c);
        }
        assert_eq!(ex.lost, 1);
        assert_eq!(ex.last_seq, Some(6));
    }
}
