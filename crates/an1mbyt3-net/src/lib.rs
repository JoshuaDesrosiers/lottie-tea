//! Port-based multiplayer for an1mbyt3 games: a tiny **WebSocket relay**.
//!
//! One machine hosts — the editor's "Host" (`ctx.net.host(port)`, through a
//! Tauri command), or the standalone `an1mbyt3-server` binary — and every
//! player, the host's own game included, connects to it as a WebSocket
//! client (`ctx.net.join("192.168.1.20:7777")`). Browsers can open
//! WebSockets but can't listen on a port, which is why the listening half
//! lives here, natively.
//!
//! The server doesn't know anything about games: it gives each connection
//! an id, tracks names, picks a **host player** (the first to join; the
//! next-oldest takes over if they leave) and forwards JSON messages. Game
//! logic — who is authoritative for what — lives in the game scripts; the
//! usual pattern is "the host player runs the world, everyone sends it
//! their inputs".
//!
//! **Protocol** (one JSON object per text frame):
//! - client → server: `{"t":"hello","name":"Ada"}` (first), then
//!   `{"t":"send","data":<any JSON>,"to":<id, optional>}` (`to` omitted =
//!   everyone else).
//! - server → client: `{"t":"welcome","id":2,"host":1,"players":[{"id":1,"name":"Ada"},…]}`,
//!   `{"t":"join","id":3,"name":"Bo"}`, `{"t":"leave","id":3}`,
//!   `{"t":"host","id":2}` (host changed), `{"t":"msg","from":1,"data":<any JSON>}`.
//!
//! Plain threads and blocking sockets (one thread per player, a short read
//! timeout so each thread can also flush what others sent it) — LAN-party
//! scale, no async runtime needed.

use std::collections::BTreeMap;
use std::io;
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tungstenite::{Message, WebSocket};

/// The port games use when none is given.
pub const DEFAULT_PORT: u16 = 7777;
/// How long a new connection may take to say hello.
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
/// How often each player's thread wakes to flush outgoing messages.
const POLL: Duration = Duration::from_millis(5);

struct Player {
    name: String,
    outbox: Sender<String>,
}

/// Everyone connected, and who the host player is.
#[derive(Default)]
struct Room {
    next_id: u32,
    host: Option<u32>,
    players: BTreeMap<u32, Player>,
}

impl Room {
    fn roster(&self) -> Value {
        Value::Array(self.players.iter().map(|(id, p)| json!({ "id": id, "name": p.name })).collect())
    }

    /// Sends `text` to everyone but `except`.
    fn broadcast(&self, text: &str, except: Option<u32>) {
        for (id, p) in &self.players {
            if Some(*id) != except {
                let _ = p.outbox.send(text.to_string());
            }
        }
    }
}

/// A running relay server; stops when dropped (or on [`RelayServer::stop`]).
pub struct RelayServer {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    room: Arc<Mutex<Room>>,
}

impl RelayServer {
    /// Listens on every interface at `port` (`0` = any free port).
    pub fn start(port: u16) -> io::Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], port)))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        let stop = Arc::new(AtomicBool::new(false));
        let room = Arc::new(Mutex::new(Room { next_id: 1, ..Default::default() }));
        let thread = {
            let (stop, room) = (Arc::clone(&stop), Arc::clone(&room));
            std::thread::Builder::new().name("an1mbyt3-net accept".into()).spawn(move || accept_loop(listener, stop, room))?
        };
        Ok(Self { port, stop, thread: Some(thread), room })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// How many players are connected right now.
    pub fn player_count(&self) -> usize {
        self.room.lock().map(|r| r.players.len()).unwrap_or(0)
    }

    /// Stops accepting and disconnects everyone.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for RelayServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn accept_loop(listener: TcpListener, stop: Arc<AtomicBool>, room: Arc<Mutex<Room>>) {
    let mut players = Vec::new();
    while !stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                let (stop, room) = (Arc::clone(&stop), Arc::clone(&room));
                if let Ok(handle) = std::thread::Builder::new().name("an1mbyt3-net player".into()).spawn(move || serve_player(stream, stop, room)) {
                    players.push(handle);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
        players.retain(|h: &JoinHandle<()>| !h.is_finished());
    }
    for handle in players {
        let _ = handle.join();
    }
}

/// Reads one text frame, or `None` if nothing arrived within the read
/// timeout. `Err` means the connection is gone.
fn read_text(ws: &mut WebSocket<TcpStream>) -> Result<Option<String>, ()> {
    match ws.read() {
        Ok(Message::Text(text)) => Ok(Some(text.as_str().to_string())),
        Ok(Message::Close(_)) => Err(()),
        Ok(_) => Ok(None),
        Err(tungstenite::Error::Io(e)) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => Ok(None),
        Err(_) => Err(()),
    }
}

fn serve_player(stream: TcpStream, stop: Arc<AtomicBool>, room: Arc<Mutex<Room>>) {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(HELLO_TIMEOUT));
    let Ok(mut ws) = tungstenite::accept(stream) else { return };
    let _ = ws.get_mut().set_read_timeout(Some(POLL));

    // Wait for the hello (its name), then join the room.
    let deadline = Instant::now() + HELLO_TIMEOUT;
    let name = loop {
        if Instant::now() > deadline || stop.load(Ordering::SeqCst) {
            return;
        }
        match read_text(&mut ws) {
            Ok(Some(text)) => {
                let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                if v["t"] == "hello" {
                    let name: String = v["name"].as_str().unwrap_or("Player").chars().take(24).collect();
                    break if name.trim().is_empty() { "Player".to_string() } else { name };
                }
            }
            Ok(None) => {}
            Err(()) => return,
        }
    };
    let (outbox, inbox): (Sender<String>, Receiver<String>) = mpsc::channel();
    let id = {
        let Ok(mut r) = room.lock() else { return };
        let id = r.next_id;
        r.next_id += 1;
        if r.host.is_none() {
            r.host = Some(id);
        }
        r.broadcast(&json!({ "t": "join", "id": id, "name": name }).to_string(), None);
        r.players.insert(id, Player { name: name.clone(), outbox });
        let welcome = json!({ "t": "welcome", "id": id, "host": r.host, "players": r.roster() });
        if ws.send(Message::text(welcome.to_string())).is_err() {
            r.players.remove(&id);
            return;
        }
        id
    };

    // Relay until either side closes.
    'conn: while !stop.load(Ordering::SeqCst) {
        match read_text(&mut ws) {
            Ok(Some(text)) => {
                let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
                if v["t"] == "send" {
                    let out = json!({ "t": "msg", "from": id, "data": v["data"] }).to_string();
                    let Ok(r) = room.lock() else { break };
                    match v["to"].as_u64() {
                        Some(to) => {
                            if let Some(p) = r.players.get(&(to as u32)) {
                                let _ = p.outbox.send(out);
                            }
                        }
                        None => r.broadcast(&out, Some(id)),
                    }
                }
            }
            Ok(None) => {}
            Err(()) => break,
        }
        while let Ok(text) = inbox.try_recv() {
            if ws.send(Message::text(text)).is_err() {
                break 'conn;
            }
        }
    }
    let _ = ws.close(None);
    let _ = ws.flush();

    // Gone: tell the others, and hand the host role on if it was theirs.
    if let Ok(mut r) = room.lock() {
        r.players.remove(&id);
        r.broadcast(&json!({ "t": "leave", "id": id }).to_string(), None);
        if r.host == Some(id) {
            r.host = r.players.keys().next().copied();
            if let Some(new_host) = r.host {
                r.broadcast(&json!({ "t": "host", "id": new_host }).to_string(), None);
            }
        }
    }
}

/// This machine's address(es) other players on the network should type
/// to join: the LAN address the OS would route through (found without
/// sending anything), and loopback for players on the same machine.
pub fn lan_addresses() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        // `connect` on UDP only picks a route; no packet is sent.
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                out.push(addr.ip().to_string());
            }
        }
    }
    out.push("127.0.0.1".into());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    type Client = WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

    fn connect(port: u16, name: &str) -> (Client, Value) {
        let (mut ws, _) = tungstenite::connect(format!("ws://127.0.0.1:{port}")).expect("connect");
        ws.send(Message::text(json!({ "t": "hello", "name": name }).to_string())).unwrap();
        let welcome = next(&mut ws);
        (ws, welcome)
    }

    fn next(ws: &mut Client) -> Value {
        loop {
            if let Message::Text(t) = ws.read().expect("read") {
                return serde_json::from_str(t.as_str()).unwrap();
            }
        }
    }

    #[test]
    fn relays_between_players_and_hands_over_the_host_role() {
        let server = RelayServer::start(0).expect("start");
        let port = server.port();
        let (mut ada, welcome) = connect(port, "Ada");
        assert_eq!(welcome["t"], "welcome");
        assert_eq!(welcome["id"], 1);
        assert_eq!(welcome["host"], 1);

        let (mut bo, welcome) = connect(port, "Bo");
        assert_eq!(welcome["id"], 2);
        assert_eq!(welcome["host"], 1);
        assert_eq!(welcome["players"].as_array().unwrap().len(), 2);
        let joined = next(&mut ada);
        assert_eq!((joined["t"].as_str(), joined["id"].as_u64(), joined["name"].as_str()), (Some("join"), Some(2), Some("Bo")));

        // Broadcast: Bo → everyone else.
        bo.send(Message::text(json!({ "t": "send", "data": { "x": 5 } }).to_string())).unwrap();
        let msg = next(&mut ada);
        assert_eq!((msg["t"].as_str(), msg["from"].as_u64(), msg["data"]["x"].as_u64()), (Some("msg"), Some(2), Some(5)));
        // Direct: Ada → Bo.
        ada.send(Message::text(json!({ "t": "send", "data": "hi", "to": 2 }).to_string())).unwrap();
        let msg = next(&mut bo);
        assert_eq!((msg["from"].as_u64(), msg["data"].as_str()), (Some(1), Some("hi")));

        // The host leaves: Bo is told, and becomes host.
        ada.close(None).unwrap();
        let _ = ada.flush();
        let left = next(&mut bo);
        assert_eq!((left["t"].as_str(), left["id"].as_u64()), (Some("leave"), Some(1)));
        let host = next(&mut bo);
        assert_eq!((host["t"].as_str(), host["id"].as_u64()), (Some("host"), Some(2)));
        server.stop();
    }

    #[test]
    fn lists_a_loopback_address() {
        assert!(lan_addresses().iter().any(|a| a == "127.0.0.1"));
    }
}
