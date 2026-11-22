//! The game server, transport-independent (`server/server.dll`, thread
//! `srv_Main` `0x10008d20`; see `docs/architecture.md` §6).
//!
//! The original runs inside the host's game process. Clients connect over
//! TCP; the server takes them through handshake and lobby, gives each its
//! client index, then orders, validates and broadcasts their commands and
//! drives the game clock. This type contains that logic without sockets:
//! the caller feeds received commands in and sends what [`Server::take_outbound`]
//! returns.
//!
//! Simplifications so far: no world upload/distribution for new games
//! (saved games load locally anyway), no lobby protocol beyond "ready", and
//! relayed records (system sub-type `0x0e`) are broadcast immediately.

use gilde_sim::clock::{ClockEvent, ServerClock};
use gilde_sim::command::{Command, NO_REQUEST, kind, system};
use gilde_sim::Date;
use std::collections::VecDeque;

/// Connection slots (`0x101db6e0`, 8 × 0x264264 bytes).
pub const MAX_CONNECTIONS: usize = 8;
/// `game.ini` `[Config] Port` / `[Network] Port` default.
pub const DEFAULT_PORT: u16 = 7531;
/// UDP port the lobby announcement is broadcast to.
pub const ANNOUNCE_PORT: u16 = 12345;
/// Lobby announcement interval.
pub const ANNOUNCE_INTERVAL_MS: u64 = 3000;
/// A connection without traffic for this long is dropped.
pub const IDLE_TIMEOUT_MS: u64 = 300_000;
/// Role sent in reply to the handshake: the host's own connection.
pub const ROLE_HOST: u32 = 1;
/// Role sent in reply to the handshake: a remote player.
pub const ROLE_REMOTE: u32 = 2;

/// The server's copy of the world: decides whether a command is applied.
/// The original validates about 45 command types (economy, buildings,
/// crimes, …) against its own world and accepts the rest unchanged.
pub trait Authority {
    fn validate(&mut self, cmd: &Command) -> bool;

    /// A transaction (records between types 5 and 6) is accepted as a whole.
    fn validate_transaction(&mut self, cmds: &[Command]) -> bool {
        cmds.iter().all(|c| self.validate(c))
    }
}

/// Accepts every command.
pub struct AcceptAll;

impl Authority for AcceptAll {
    fn validate(&mut self, _: &Command) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Lobby,
    Playing,
}

#[derive(Default)]
struct Connection {
    host: bool,
    greeted: bool,
    ready: bool,
    acked: bool,
    index: u8,
    last_seen: u64,
    outbox: VecDeque<Command>,
    transaction: Option<Vec<Command>>,
}

/// What the transport should do besides sending queued commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerEvent {
    /// Broadcast the lobby announcement on [`ANNOUNCE_PORT`].
    Announce { players: usize, expected: usize },
    /// All players are in; the game has started.
    Started,
    /// A connection was dropped (timeout).
    Dropped(usize),
}

pub struct Server<A: Authority> {
    conns: Vec<Option<Connection>>,
    expected: usize,
    phase: Phase,
    pub clock: ServerClock,
    pub authority: A,
    next_seq: u32,
    next_announce: u64,
    /// Commands rejected by the authority (for logging and tests).
    pub rejected: u32,
}

impl<A: Authority> Server<A> {
    /// `expected`: number of players to wait for (config block; from the
    /// save header's human count when a saved game is loaded).
    pub fn new(expected: usize, date: Date, authority: A) -> Self {
        Server {
            conns: (0..MAX_CONNECTIONS).map(|_| None).collect(),
            expected: expected.clamp(1, MAX_CONNECTIONS),
            phase: Phase::Lobby,
            clock: ServerClock::new(date),
            authority,
            next_seq: 1,
            next_announce: 0,
            rejected: 0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn connected(&self) -> usize {
        self.conns.iter().flatten().count()
    }

    /// Accept a new connection (`srv_Accept`); `None` when all slots are used
    /// or the game has started.
    pub fn connect(&mut self, now: u64, host: bool) -> Option<usize> {
        if self.phase != Phase::Lobby {
            return None;
        }
        let slot = self.conns.iter().position(Option::is_none)?;
        self.conns[slot] = Some(Connection { host, last_seen: now, ..Default::default() });
        Some(slot)
    }

    pub fn disconnect(&mut self, id: usize) {
        if let Some(c) = self.conns.get_mut(id) {
            *c = None;
        }
    }

    /// Commands queued for a connection.
    pub fn take_outbound(&mut self, id: usize) -> Vec<Command> {
        match self.conns.get_mut(id).and_then(Option::as_mut) {
            Some(c) => c.outbox.drain(..).collect(),
            None => Vec::new(),
        }
    }

    fn send(&mut self, id: usize, cmd: Command) {
        if let Some(Some(c)) = self.conns.get_mut(id) {
            c.outbox.push_back(cmd);
        }
    }

    fn broadcast(&mut self, cmd: &Command) {
        for c in self.conns.iter_mut().flatten() {
            c.outbox.push_back(cmd.clone());
        }
    }

    /// A command arrived from connection `id` (`srv_ProcessInput`).
    pub fn receive(&mut self, now: u64, id: usize, mut cmd: Command) {
        let Some(Some(conn)) = self.conns.get_mut(id) else { return };
        conn.last_seen = now;
        cmd.conn = id as u8;
        match (self.phase, cmd.kind) {
            (_, kind::HELLO) => {
                conn.greeted = true;
                let role = if conn.host { ROLE_HOST } else { ROLE_REMOTE };
                let mut reply = Command::new(kind::HELLO);
                reply.payload[..4].copy_from_slice(&role.to_le_bytes());
                reply.conn = id as u8;
                self.send(id, reply);
            }
            (Phase::Lobby, kind::LOBBY) => conn.ready = true,
            (Phase::Lobby, _) => {}
            (Phase::Playing, kind::TX_BEGIN) => conn.transaction = Some(Vec::new()),
            (Phase::Playing, kind::TX_END) => {
                let group = conn.transaction.take().unwrap_or_default();
                if self.authority.validate_transaction(&group) {
                    self.apply(id, Command::new(kind::TX_BEGIN));
                    for c in group {
                        self.apply(id, c);
                    }
                    self.apply(id, Command::new(kind::TX_END));
                } else {
                    self.rejected += 1;
                }
            }
            (Phase::Playing, _) => {
                if let Some(t) = &mut conn.transaction {
                    t.push(cmd);
                    return;
                }
                self.execute(id, cmd);
            }
        }
    }

    /// `srv_ExecuteCommands` for one command: server-side handling of system
    /// messages, validation, then echo and broadcast.
    fn execute(&mut self, id: usize, cmd: Command) {
        if cmd.kind == kind::SYSTEM {
            match cmd.sub_kind() {
                system::TICK_ACK => {
                    if let Some(Some(c)) = self.conns.get_mut(id) {
                        c.acked = true;
                    }
                }
                system::SPEED => self.clock.set_speed(cmd.value()),
                _ => {}
            }
        }
        if !self.authority.validate(&cmd) {
            self.rejected += 1;
            return;
        }
        self.apply(id, cmd);
    }

    /// Number an accepted command, echo it to its sender and broadcast it to
    /// everyone else without a request id.
    fn apply(&mut self, id: usize, mut cmd: Command) {
        cmd.seq = self.next_seq;
        self.next_seq = self.next_seq.wrapping_add(1);
        cmd.conn = id as u8;
        self.send(id, cmd.clone());
        let mut others = cmd;
        others.request = NO_REQUEST;
        for (i, c) in self.conns.iter_mut().enumerate() {
            if i != id
                && let Some(c) = c
            {
                c.outbox.push_back(others.clone());
            }
        }
    }

    /// Run the server at time `now` (one iteration of the `srv_Main` loop).
    pub fn step(&mut self, now: u64) -> Vec<ServerEvent> {
        let mut events = Vec::new();
        // Drop silent connections.
        for (i, c) in self.conns.iter_mut().enumerate() {
            if c.as_ref().is_some_and(|c| now.saturating_sub(c.last_seen) > IDLE_TIMEOUT_MS) {
                *c = None;
                events.push(ServerEvent::Dropped(i));
            }
        }
        match self.phase {
            Phase::Lobby => {
                let players = self.connected();
                let all_ready = self.conns.iter().flatten().all(|c| c.greeted && c.ready);
                if players == self.expected && all_ready {
                    self.start();
                    events.push(ServerEvent::Started);
                } else if now >= self.next_announce {
                    self.next_announce = now + ANNOUNCE_INTERVAL_MS;
                    events.push(ServerEvent::Announce { players, expected: self.expected });
                }
            }
            Phase::Playing => self.run_clock(now),
        }
        events
    }

    /// Client setup: role and client index for every connection in slot order.
    fn start(&mut self) {
        self.phase = Phase::Playing;
        let ids: Vec<usize> = (0..MAX_CONNECTIONS).filter(|&i| self.conns[i].is_some()).collect();
        for (n, &id) in ids.iter().enumerate() {
            let mut role = Command::new(kind::ROLE);
            role.payload[0] = 2;
            self.send(id, role);
            self.send(id, Command::system(system::CLIENT_INDEX, n as u32));
            if let Some(Some(c)) = self.conns.get_mut(id) {
                c.index = n as u8;
            }
        }
    }

    fn run_clock(&mut self, now: u64) {
        let waiting = self.clock.waiting_for_acks();
        let all_acked = self.conns.iter().flatten().all(|c| c.acked);
        let events = self.clock.poll(now, all_acked, false);
        if waiting && !events.is_empty() {
            for c in self.conns.iter_mut().flatten() {
                c.acked = false;
            }
        }
        for e in events {
            match e {
                ClockEvent::Tick(date) => self.broadcast(&Command::set_date(&date)),
                ClockEvent::EndOfRound => {
                    let ids: Vec<usize> = (0..MAX_CONNECTIONS).filter(|&i| self.conns[i].is_some()).collect();
                    for (n, id) in ids.into_iter().enumerate() {
                        self.send(id, Command::system(system::END_OF_ROUND, (n == 0) as u32));
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(cmds: &[Command]) -> Vec<(u8, u8)> {
        cmds.iter().map(|c| (c.kind, c.sub_kind())).collect()
    }

    fn started_server(players: usize) -> Server<AcceptAll> {
        let mut s = Server::new(players, Date::start_of_round(0), AcceptAll);
        for i in 0..players {
            let id = s.connect(0, i == 0).unwrap();
            s.receive(0, id, Command::new(kind::HELLO));
            s.receive(0, id, Command::new(kind::LOBBY));
        }
        assert_eq!(s.step(15), vec![ServerEvent::Started]);
        s
    }

    #[test]
    fn lobby_handshake_and_start() {
        let mut s = Server::new(2, Date::start_of_round(0), AcceptAll);
        let a = s.connect(0, true).unwrap();
        s.receive(0, a, Command::new(kind::HELLO));
        let reply = s.take_outbound(a);
        assert_eq!(reply[0].kind, kind::HELLO);
        assert_eq!(&reply[0].payload[..4], &ROLE_HOST.to_le_bytes());
        assert!(matches!(s.step(0)[0], ServerEvent::Announce { players: 1, expected: 2 }));
        assert!(s.step(100).is_empty(), "announcements every 3 s");
        let b = s.connect(100, false).unwrap();
        s.receive(100, b, Command::new(kind::HELLO));
        s.receive(100, a, Command::new(kind::LOBBY));
        assert!(!s.step(200).contains(&ServerEvent::Started), "b is not ready");
        s.receive(200, b, Command::new(kind::LOBBY));
        assert_eq!(s.step(215), vec![ServerEvent::Started]);
        let out_b = s.take_outbound(b);
        assert_eq!(kinds(&out_b[1..]), vec![(kind::ROLE, 2), (kind::SYSTEM, system::CLIENT_INDEX)]);
        assert_eq!(out_b.last().unwrap().value(), 1);
        assert_eq!(s.take_outbound(a).last().unwrap().value(), 0);
        assert!(s.connect(300, false).is_none(), "no joining after the start");
    }

    #[test]
    fn clock_waits_for_all_acks() {
        let mut s = started_server(2);
        s.take_outbound(0);
        s.take_outbound(1);
        s.step(100);
        assert!(s.take_outbound(0).is_empty());
        s.receive(100, 0, Command::system(system::TICK_ACK, 0));
        s.step(115);
        assert!(s.take_outbound(1).iter().all(|c| c.kind != kind::SET_DATE));
        s.receive(120, 1, Command::system(system::TICK_ACK, 0));
        s.take_outbound(0);
        s.take_outbound(1);
        s.step(130);
        let ticks: Vec<Date> = s.take_outbound(1).iter().filter_map(Command::date).collect();
        assert_eq!(ticks, vec![Date { round: 0, hour: 6, minute: 1, second: 0 }]);
        s.take_outbound(0);
        s.step(900);
        assert_eq!(s.take_outbound(0).iter().filter_map(Command::date).count(), 1);
    }

    #[test]
    fn commands_are_echoed_numbered_and_broadcast() {
        let mut s = started_server(3);
        for i in 0..3 {
            s.take_outbound(i);
        }
        let mut c = Command::new(kind::SELL_OBJECT);
        c.request = 42;
        s.receive(10, 1, c.clone());
        s.receive(11, 2, c);
        let to0 = s.take_outbound(0);
        let to1 = s.take_outbound(1);
        assert_eq!(to1[0].request, 42, "echo keeps the request id");
        assert_eq!(to0[0].request, NO_REQUEST, "broadcast drops it");
        assert_eq!((to0[0].seq, to0[1].seq), (1, 2));
        assert_eq!((to0[0].conn, to0[1].conn), (1, 2));
    }

    struct NoSales;
    impl Authority for NoSales {
        fn validate(&mut self, c: &Command) -> bool {
            c.kind != kind::SELL_OBJECT
        }
    }

    #[test]
    fn rejected_commands_are_not_broadcast() {
        let mut s = Server::new(1, Date::start_of_round(0), NoSales);
        let a = s.connect(0, true).unwrap();
        s.receive(0, a, Command::new(kind::HELLO));
        s.receive(0, a, Command::new(kind::LOBBY));
        s.step(0);
        s.take_outbound(a);
        s.receive(1, a, Command::new(kind::SELL_OBJECT));
        assert!(s.take_outbound(a).is_empty());
        assert_eq!(s.rejected, 1);
        s.receive(2, a, Command::new(kind::TX_BEGIN));
        s.receive(2, a, Command::new(kind::CREATE_BUILDING));
        s.receive(2, a, Command::new(kind::SELL_OBJECT));
        s.receive(2, a, Command::new(kind::TX_END));
        assert!(s.take_outbound(a).is_empty(), "a transaction is rejected as a whole");
    }

    #[test]
    fn end_of_round_and_idle_timeout() {
        let mut s = started_server(2);
        s.receive(0, 0, Command::system(system::TICK_ACK, 0));
        s.receive(0, 1, Command::system(system::TICK_ACK, 0));
        let mut now = 20;
        loop {
            s.step(now);
            // Keep the connections alive.
            s.receive(now, 0, Command::new(0x7f));
            s.receive(now, 1, Command::new(0x7f));
            let out = s.take_outbound(0);
            if out.iter().any(|c| c.kind == kind::SYSTEM && c.sub_kind() == system::END_OF_ROUND) {
                assert_eq!(out.iter().find(|c| c.sub_kind() == system::END_OF_ROUND).unwrap().value(), 1);
                break;
            }
            now += 770;
        }
        assert_eq!(s.clock.date, Date::start_of_round(1));
        assert_eq!(s.step(now + IDLE_TIMEOUT_MS + 1), vec![ServerEvent::Dropped(0), ServerEvent::Dropped(1)]);
    }
}
