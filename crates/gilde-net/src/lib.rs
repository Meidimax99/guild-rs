//! Networking: the game server and the wire format (`server/server.dll`,
//! `nt_*`; see `docs/architecture.md` §6).

pub mod frame;
pub mod server;

pub use server::{AcceptAll, Authority, Server, ServerEvent};
