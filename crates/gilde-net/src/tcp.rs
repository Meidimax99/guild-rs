//! TCP transport for [`Server`] and a matching client (`nt_*`).
//!
//! The original server thread (`Init`/`Exit` exports of `server.dll`) loops
//! every 15 ms: accept, read records, execute, write, run the clock. Clients
//! connect with a non-blocking socket (`nt_Init` `0x41dc80`: receive buffer
//! 256 KiB, send buffer 128 KiB) and start with the handshake (type 3).
//!
//! Deviation: the lobby announcement on UDP 12345 carries a short text
//! (`GILDE <players>/<expected> <port>`) instead of the original config block,
//! whose layout is not decoded yet.

use crate::frame::{self, Decoder};
use crate::server::{ANNOUNCE_PORT, Authority, Server, ServerEvent};
use gilde_sim::command::Command;
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Loop period of the server thread.
pub const STEP: Duration = Duration::from_millis(15);

struct Peer {
    stream: TcpStream,
    decoder: Decoder,
    pending: Vec<u8>,
}

/// A running server thread (`Init` … `Exit`).
pub struct ServerHandle {
    pub addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ServerHandle {
    /// Start a server on `port` (0 = any free port) waiting for `expected`
    /// players.
    #[allow(clippy::needless_range_loop)] // slots index both the peers and the server
    pub fn spawn<A: Authority + Send + 'static>(port: u16, mut server: Server<A>) -> std::io::Result<Self> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let announce = UdpSocket::bind(("0.0.0.0", 0)).ok().filter(|s| s.set_broadcast(true).is_ok());
        let thread = std::thread::spawn(move || {
            let start = Instant::now();
            let mut peers: Vec<Option<Peer>> = (0..crate::server::MAX_CONNECTIONS).map(|_| None).collect();
            let mut host_taken = false;
            while !flag.load(Ordering::Relaxed) {
                let now = start.elapsed().as_millis() as u64;
                // Accept.
                while let Ok((stream, peer_addr)) = listener.accept() {
                    let _ = stream.set_nonblocking(true);
                    let _ = stream.set_nodelay(true);
                    let host = !host_taken && peer_addr.ip().is_loopback();
                    match server.connect(now, host) {
                        Some(id) => {
                            host_taken |= host;
                            peers[id] = Some(Peer { stream, decoder: Decoder::default(), pending: Vec::new() });
                            log::info!("New Connection from {peer_addr} (slot {id})");
                        }
                        None => drop(stream),
                    }
                }
                // Read.
                for id in 0..peers.len() {
                    let Some(p) = &mut peers[id] else { continue };
                    let mut buf = [0u8; 4096];
                    let mut closed = false;
                    loop {
                        match p.stream.read(&mut buf) {
                            Ok(0) => {
                                closed = true;
                                break;
                            }
                            Ok(n) => p.decoder.push(&buf[..n]),
                            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                            Err(_) => {
                                closed = true;
                                break;
                            }
                        }
                    }
                    let mut cmds = Vec::new();
                    loop {
                        match p.decoder.next_command() {
                            Ok(Some(c)) => cmds.push(c),
                            Ok(None) => break,
                            Err(_) => {
                                closed = true;
                                break;
                            }
                        }
                    }
                    for c in cmds {
                        server.receive(now, id, c);
                    }
                    if closed {
                        peers[id] = None;
                        server.disconnect(id);
                    }
                }
                // Run.
                for e in server.step(now) {
                    match e {
                        ServerEvent::Announce { players, expected } => {
                            if let Some(s) = &announce {
                                let msg = format!("GILDE {players}/{expected} {}", addr.port());
                                let _ = s.send_to(msg.as_bytes(), ("255.255.255.255", ANNOUNCE_PORT));
                            }
                        }
                        ServerEvent::Dropped(id) => peers[id] = None,
                        ServerEvent::Started => log::info!("all players connected, game started"),
                    }
                }
                // Write.
                for id in 0..peers.len() {
                    let out = server.take_outbound(id);
                    let Some(p) = &mut peers[id] else { continue };
                    for c in out {
                        p.pending.extend(frame::encode(&c));
                    }
                    while !p.pending.is_empty() {
                        match p.stream.write(&p.pending) {
                            Ok(0) => break,
                            Ok(n) => {
                                p.pending.drain(..n);
                            }
                            Err(_) => break,
                        }
                    }
                }
                std::thread::sleep(STEP);
            }
        });
        Ok(ServerHandle { addr, stop, thread: Some(thread) })
    }

    /// `Exit`: stop the thread and wait for it.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Client connection (`nt_Init`, `nt_Recv`).
pub struct Client {
    stream: TcpStream,
    decoder: Decoder,
}

impl Client {
    pub fn connect(addr: SocketAddr) -> std::io::Result<Self> {
        let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500))?;
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Client { stream, decoder: Decoder::default() })
    }

    pub fn send(&mut self, cmd: &Command) -> std::io::Result<()> {
        self.stream.set_nonblocking(false)?;
        let r = self.stream.write_all(&frame::encode(cmd));
        self.stream.set_nonblocking(true)?;
        r
    }

    /// Commands received so far.
    pub fn poll(&mut self) -> std::io::Result<Vec<Command>> {
        let mut buf = [0u8; 4096];
        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => return Err(ErrorKind::ConnectionAborted.into()),
                Ok(n) => self.decoder.push(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e) => return Err(e),
            }
        }
        let mut out = Vec::new();
        while let Some(c) = self.decoder.next_command().map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))? {
            out.push(c);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::AcceptAll;
    use gilde_sim::Date;
    use gilde_sim::command::{kind, system};

    fn wait_for(c: &mut Client, mut pred: impl FnMut(&Command) -> bool) -> Vec<Command> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = Vec::new();
        while Instant::now() < deadline {
            for cmd in c.poll().unwrap() {
                let done = pred(&cmd);
                seen.push(cmd);
                if done {
                    return seen;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("timed out; received {seen:?}");
    }

    #[test]
    fn two_clients_over_tcp() {
        let mut server = Server::new(2, Date::start_of_round(0), AcceptAll);
        server.clock.set_speed(4);
        let handle = ServerHandle::spawn(0, server).unwrap();
        let addr = SocketAddr::from(([127, 0, 0, 1], handle.addr.port()));
        let mut a = Client::connect(addr).unwrap();
        let mut b = Client::connect(addr).unwrap();
        for c in [&mut a, &mut b] {
            c.send(&Command::new(kind::HELLO)).unwrap();
            c.send(&Command::new(kind::LOBBY)).unwrap();
        }
        let got = wait_for(&mut b, |c| c.kind == kind::SYSTEM && c.sub_kind() == system::CLIENT_INDEX);
        assert_eq!(got.last().unwrap().value(), 1);
        wait_for(&mut a, |c| c.kind == kind::SYSTEM && c.sub_kind() == system::CLIENT_INDEX);
        for c in [&mut a, &mut b] {
            c.send(&Command::system(system::TICK_ACK, 0)).unwrap();
        }
        let ticks = wait_for(&mut a, |c| c.date().is_some_and(|d| d.minute == 2));
        let dates: Vec<Date> = ticks.iter().filter_map(Command::date).collect();
        assert_eq!(dates[0], Date { round: 0, hour: 6, minute: 1, second: 0 });
        // A command from b reaches a.
        let mut sell = Command::new(kind::SELL_OBJECT);
        sell.request = 5;
        b.send(&sell).unwrap();
        let got = wait_for(&mut a, |c| c.kind == kind::SELL_OBJECT);
        assert_eq!(got.last().unwrap().conn, 1);
        handle.stop();
    }
}
