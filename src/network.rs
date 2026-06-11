//! Network / multiplayer: peer management, message protocol, state sync.
//!
//! Transport-agnostic via traits. Plug in tokio/quinn/WebRTC via
//! `NetTransport`. Compatible with ALICE-Sync for deterministic replication.
//!
//! ```rust
//! use alice_game_engine::network::*;
//!
//! let mut host = GameHost::new(0);
//! host.accept_peer(PeerId(1), "player2");
//! host.broadcast(&NetMessage::new(MsgKind::StateUpdate, b"pos:1,2,3"));
//! assert_eq!(host.peer_count(), 1);
//! ```

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Peer
// ---------------------------------------------------------------------------

/// Unique peer identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub u32);

/// Connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerState {
    Connecting,
    Connected,
    Disconnected,
}

/// A connected peer.
#[derive(Debug, Clone)]
pub struct NetPeer {
    pub id: PeerId,
    pub name: String,
    pub state: PeerState,
    pub latency_ms: f32,
    pub packets_sent: u64,
    pub packets_received: u64,
}

impl NetPeer {
    #[must_use]
    pub fn new(id: PeerId, name: &str) -> Self {
        Self {
            id,
            name: name.to_string(),
            state: PeerState::Connected,
            latency_ms: 0.0,
            packets_sent: 0,
            packets_received: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

/// Message kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MsgKind {
    Ping,
    Pong,
    StateUpdate,
    Input,
    Rpc,
    Chat,
    JoinRequest,
    JoinAccept,
    Disconnect,
}

/// A network message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetMessage {
    pub kind: MsgKind,
    pub sender: PeerId,
    pub payload: Vec<u8>,
    pub sequence: u64,
    pub reliable: bool,
}

impl NetMessage {
    #[must_use]
    pub fn new(kind: MsgKind, payload: &[u8]) -> Self {
        Self {
            kind,
            sender: PeerId(0),
            payload: payload.to_vec(),
            sequence: 0,
            reliable: true,
        }
    }

    #[must_use]
    pub fn unreliable(kind: MsgKind, payload: &[u8]) -> Self {
        Self {
            kind,
            sender: PeerId(0),
            payload: payload.to_vec(),
            sequence: 0,
            reliable: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Transport trait — for external crate injection
// ---------------------------------------------------------------------------

/// Transport layer abstraction (implement with tokio, quinn, WebRTC, etc.)
pub trait NetTransport: Send + Sync {
    /// Sends a message to a specific peer.
    ///
    /// # Errors
    /// Returns error string on send failure.
    fn send(&mut self, peer: PeerId, msg: &NetMessage) -> Result<(), String>;

    /// Receives pending messages. Non-blocking.
    fn recv(&mut self) -> Vec<(PeerId, NetMessage)>;

    /// Returns connected peer count.
    fn peer_count(&self) -> usize;
}

// ---------------------------------------------------------------------------
// GameHost — authoritative server
// ---------------------------------------------------------------------------

/// Game host (server) that manages peers and message routing.
pub struct GameHost {
    pub host_id: PeerId,
    pub peers: Vec<NetPeer>,
    pub outbox: Vec<(PeerId, NetMessage)>,
    pub inbox: Vec<(PeerId, NetMessage)>,
    next_sequence: u64,
}

impl GameHost {
    #[must_use]
    pub const fn new(host_id: u32) -> Self {
        Self {
            host_id: PeerId(host_id),
            peers: Vec::new(),
            outbox: Vec::new(),
            inbox: Vec::new(),
            next_sequence: 0,
        }
    }

    /// Accepts a new peer.
    pub fn accept_peer(&mut self, id: PeerId, name: &str) {
        self.peers.push(NetPeer::new(id, name));
    }

    /// Disconnects a peer.
    pub fn disconnect_peer(&mut self, id: PeerId) {
        if let Some(peer) = self.peers.iter_mut().find(|p| p.id == id) {
            peer.state = PeerState::Disconnected;
        }
    }

    /// Queues a message to a specific peer.
    pub fn send_to(&mut self, peer: PeerId, mut msg: NetMessage) {
        msg.sender = self.host_id;
        msg.sequence = self.next_sequence;
        self.next_sequence += 1;
        self.outbox.push((peer, msg));
    }

    /// Queues a message to all connected peers.
    pub fn broadcast(&mut self, msg: &NetMessage) {
        let peers: Vec<PeerId> = self
            .peers
            .iter()
            .filter(|p| p.state == PeerState::Connected)
            .map(|p| p.id)
            .collect();
        for pid in peers {
            self.send_to(pid, msg.clone());
        }
    }

    /// Processes incoming messages (call each frame).
    pub fn receive(&mut self, msg: NetMessage, from: PeerId) {
        if let Some(peer) = self.peers.iter_mut().find(|p| p.id == from) {
            peer.packets_received += 1;
        }
        self.inbox.push((from, msg));
    }

    /// Drains the inbox.
    pub fn drain_inbox(&mut self) -> Vec<(PeerId, NetMessage)> {
        std::mem::take(&mut self.inbox)
    }

    /// Drains the outbox.
    pub fn drain_outbox(&mut self) -> Vec<(PeerId, NetMessage)> {
        std::mem::take(&mut self.outbox)
    }

    #[must_use]
    pub fn peer_count(&self) -> usize {
        self.peers
            .iter()
            .filter(|p| p.state == PeerState::Connected)
            .count()
    }
}

// ---------------------------------------------------------------------------
// GameClient
// ---------------------------------------------------------------------------

/// Game client that connects to a host.
pub struct GameClient {
    pub local_id: PeerId,
    pub outbox: Vec<NetMessage>,
    pub inbox: Vec<NetMessage>,
    pub connected: bool,
    pub server_latency_ms: f32,
}

impl GameClient {
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self {
            local_id: PeerId(id),
            outbox: Vec::new(),
            inbox: Vec::new(),
            connected: false,
            server_latency_ms: 0.0,
        }
    }

    /// Sends a message to the server.
    pub fn send(&mut self, msg: NetMessage) {
        self.outbox.push(msg);
    }

    /// Receives a message from the server.
    pub fn receive(&mut self, msg: NetMessage) {
        self.inbox.push(msg);
    }

    /// Drains received messages.
    pub fn drain_inbox(&mut self) -> Vec<NetMessage> {
        std::mem::take(&mut self.inbox)
    }
}

// ---------------------------------------------------------------------------
// State Sync — delta compression for ECS
// ---------------------------------------------------------------------------

/// Entity state snapshot for network sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub entity_id: u32,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub velocity: [f32; 3],
}

/// Computes delta between two snapshots. Returns only changed entities.
#[must_use]
#[allow(clippy::float_cmp)]
pub fn compute_delta(prev: &[EntitySnapshot], current: &[EntitySnapshot]) -> Vec<EntitySnapshot> {
    current
        .iter()
        .filter(|c| {
            !prev.iter().any(|p| {
                p.entity_id == c.entity_id
                    && p.position == c.position
                    && p.rotation == c.rotation
                    && p.velocity == c.velocity
            })
        })
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// LoopbackTransport — in-memory pair, implements `bridge::NetworkTransport`
// ---------------------------------------------------------------------------

/// In-memory bidirectional transport pair — useful for local multi-peer
/// simulation, deterministic tests, and bots-vs-host scenarios where no
/// real network exists yet.
///
/// Use [`LoopbackTransport::pair`] to construct two linked endpoints; what
/// one sends, the other receives on its next `recv()`.
pub struct LoopbackTransport {
    pub local_peer: u32,
    inbox: std::sync::Arc<std::sync::Mutex<Vec<(u32, Vec<u8>)>>>,
    peer_inbox: std::sync::Arc<std::sync::Mutex<Vec<(u32, Vec<u8>)>>>,
    peer_id: u32,
}

impl LoopbackTransport {
    /// Build two endpoints that talk to each other. `a` thinks the remote
    /// peer is `b_id`, and vice versa.
    #[must_use]
    pub fn pair(a_id: u32, b_id: u32) -> (Self, Self) {
        let inbox_a = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let inbox_b = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let a = Self {
            local_peer: a_id,
            inbox: std::sync::Arc::clone(&inbox_a),
            peer_inbox: std::sync::Arc::clone(&inbox_b),
            peer_id: b_id,
        };
        let b = Self {
            local_peer: b_id,
            inbox: inbox_b,
            peer_inbox: inbox_a,
            peer_id: a_id,
        };
        (a, b)
    }
}

impl crate::bridge::NetworkTransport for LoopbackTransport {
    fn send_to(&mut self, peer_id: u32, data: &[u8]) -> Result<(), String> {
        if peer_id != self.peer_id {
            return Err(format!(
                "peer {peer_id} not connected (loopback peer is {})",
                self.peer_id
            ));
        }
        self.peer_inbox
            .lock()
            .map_err(|e| format!("loopback lock: {e}"))?
            .push((self.local_peer, data.to_vec()));
        Ok(())
    }

    fn recv(&mut self) -> Vec<(u32, Vec<u8>)> {
        match self.inbox.lock() {
            Ok(mut g) => std::mem::take(&mut *g),
            Err(_) => Vec::new(),
        }
    }

    fn connected_peers(&self) -> usize {
        1
    }
}

// ---------------------------------------------------------------------------
// Real UDP transport — implements `bridge::NetworkTransport` via tokio
// `UdpSocket`. Each peer binds to its own local port; remote addresses
// are configured up-front so `send_to(peer_id, ..)` can route to the
// right socket. Bytes that arrive on the bound socket are returned by
// `recv()` along with the originating peer id (reverse-mapped from
// `SocketAddr`).
// ---------------------------------------------------------------------------

#[cfg(feature = "network_udp")]
pub use udp_transport::UdpTransport;

#[cfg(feature = "network_udp")]
mod udp_transport {
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::Arc;

    use tokio::net::UdpSocket;
    use tokio::sync::Mutex;

    /// Configurable UDP transport. Spawns a background task on the
    /// embedded tokio runtime that reads incoming datagrams into an
    /// inbox; calls to [`UdpTransport::recv`] drain that inbox.
    pub struct UdpTransport {
        pub local_peer: u32,
        runtime: tokio::runtime::Runtime,
        socket: Arc<UdpSocket>,
        peer_addrs: HashMap<u32, SocketAddr>,
        addr_peers: HashMap<SocketAddr, u32>,
        inbox: Arc<Mutex<Vec<(u32, Vec<u8>)>>>,
    }

    impl UdpTransport {
        /// Bind a UDP socket and start reading incoming datagrams into
        /// the inbox.
        ///
        /// `bind_addr` is what the socket binds to (e.g. `0.0.0.0:0`
        /// for an ephemeral port). `peers` is the static peer table:
        /// `peer_id → remote_addr`. Senders consult that table.
        ///
        /// # Errors
        /// Returns the underlying I/O error if binding fails.
        pub fn bind(
            local_peer: u32,
            bind_addr: SocketAddr,
            peers: HashMap<u32, SocketAddr>,
        ) -> Result<Self, String> {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|e| format!("tokio runtime: {e}"))?;
            let socket = runtime
                .block_on(UdpSocket::bind(bind_addr))
                .map_err(|e| format!("UDP bind {bind_addr}: {e}"))?;
            let socket = Arc::new(socket);
            let addr_peers: HashMap<SocketAddr, u32> =
                peers.iter().map(|(&p, &a)| (a, p)).collect();
            let inbox = Arc::new(Mutex::new(Vec::<(u32, Vec<u8>)>::new()));

            let inbox_bg = Arc::clone(&inbox);
            let addr_peers_bg = addr_peers.clone();
            let socket_bg = Arc::clone(&socket);
            runtime.spawn(async move {
                let mut buf = vec![0u8; 1500];
                loop {
                    match socket_bg.recv_from(&mut buf).await {
                        Ok((n, from)) => {
                            let peer_id = addr_peers_bg.get(&from).copied().unwrap_or(0);
                            inbox_bg.lock().await.push((peer_id, buf[..n].to_vec()));
                        }
                        Err(_) => continue,
                    }
                }
            });

            Ok(Self {
                local_peer,
                runtime,
                socket,
                peer_addrs: peers,
                addr_peers,
                inbox,
            })
        }
    }

    impl crate::bridge::NetworkTransport for UdpTransport {
        fn send_to(&mut self, peer_id: u32, data: &[u8]) -> Result<(), String> {
            let Some(&addr) = self.peer_addrs.get(&peer_id) else {
                return Err(format!("unknown peer {peer_id}"));
            };
            let socket = Arc::clone(&self.socket);
            let payload = data.to_vec();
            self.runtime
                .block_on(async move { socket.send_to(&payload, addr).await })
                .map_err(|e| format!("send_to: {e}"))?;
            Ok(())
        }

        fn recv(&mut self) -> Vec<(u32, Vec<u8>)> {
            self.runtime
                .block_on(async { std::mem::take(&mut *self.inbox.lock().await) })
        }

        fn connected_peers(&self) -> usize {
            self.peer_addrs.len()
        }
    }

    // Keep the address-table accessible — the engine driver might want to
    // log peer status.
    impl UdpTransport {
        #[must_use]
        pub fn peer_addrs(&self) -> &HashMap<u32, SocketAddr> {
            &self.peer_addrs
        }

        #[must_use]
        pub fn addr_peers(&self) -> &HashMap<SocketAddr, u32> {
            &self.addr_peers
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_accept_peer() {
        let mut host = GameHost::new(0);
        host.accept_peer(PeerId(1), "alice");
        assert_eq!(host.peer_count(), 1);
    }

    #[test]
    fn host_disconnect() {
        let mut host = GameHost::new(0);
        host.accept_peer(PeerId(1), "alice");
        host.disconnect_peer(PeerId(1));
        assert_eq!(host.peer_count(), 0);
    }

    #[test]
    fn host_broadcast() {
        let mut host = GameHost::new(0);
        host.accept_peer(PeerId(1), "a");
        host.accept_peer(PeerId(2), "b");
        host.broadcast(&NetMessage::new(MsgKind::StateUpdate, b"hello"));
        let out = host.drain_outbox();
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn host_receive() {
        let mut host = GameHost::new(0);
        host.accept_peer(PeerId(1), "a");
        host.receive(NetMessage::new(MsgKind::Input, b"w"), PeerId(1));
        let msgs = host.drain_inbox();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn client_send_receive() {
        let mut client = GameClient::new(1);
        client.send(NetMessage::new(MsgKind::Input, b"jump"));
        assert_eq!(client.outbox.len(), 1);
        client.receive(NetMessage::new(MsgKind::StateUpdate, b"pos"));
        let msgs = client.drain_inbox();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn delta_sync_changed() {
        let prev = vec![EntitySnapshot {
            entity_id: 0,
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0; 3],
        }];
        let curr = vec![EntitySnapshot {
            entity_id: 0,
            position: [1.0, 0.0, 0.0],
            rotation: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0; 3],
        }];
        let delta = compute_delta(&prev, &curr);
        assert_eq!(delta.len(), 1);
    }

    #[test]
    fn delta_sync_unchanged() {
        let snap = vec![EntitySnapshot {
            entity_id: 0,
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            velocity: [0.0; 3],
        }];
        let delta = compute_delta(&snap, &snap);
        assert!(delta.is_empty());
    }

    #[test]
    fn net_message_reliable() {
        let msg = NetMessage::new(MsgKind::Rpc, b"call");
        assert!(msg.reliable);
    }

    #[test]
    fn net_message_unreliable() {
        let msg = NetMessage::unreliable(MsgKind::StateUpdate, b"pos");
        assert!(!msg.reliable);
    }

    #[test]
    fn peer_state() {
        let peer = NetPeer::new(PeerId(5), "test");
        assert_eq!(peer.state, PeerState::Connected);
    }

    #[test]
    fn host_sequence() {
        let mut host = GameHost::new(0);
        host.accept_peer(PeerId(1), "a");
        host.send_to(PeerId(1), NetMessage::new(MsgKind::Ping, b""));
        host.send_to(PeerId(1), NetMessage::new(MsgKind::Ping, b""));
        let out = host.drain_outbox();
        assert_eq!(out[0].1.sequence, 0);
        assert_eq!(out[1].1.sequence, 1);
    }

    // -----------------------------------------------------------------------
    // LoopbackTransport tests
    // -----------------------------------------------------------------------

    use crate::bridge::NetworkTransport;

    #[test]
    fn loopback_send_receives_on_peer() {
        let (mut a, mut b) = LoopbackTransport::pair(1, 2);
        a.send_to(2, b"hello").expect("send");
        let inbox = b.recv();
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox[0].0, 1);
        assert_eq!(inbox[0].1, b"hello".to_vec());
    }

    #[test]
    fn loopback_bidirectional() {
        let (mut a, mut b) = LoopbackTransport::pair(1, 2);
        a.send_to(2, b"ping").expect("send a→b");
        b.send_to(1, b"pong").expect("send b→a");
        let to_a = a.recv();
        let to_b = b.recv();
        assert_eq!(to_a[0].1, b"pong".to_vec());
        assert_eq!(to_b[0].1, b"ping".to_vec());
    }

    #[test]
    fn loopback_rejects_unknown_peer() {
        let (mut a, _b) = LoopbackTransport::pair(1, 2);
        let r = a.send_to(99, b"x");
        assert!(r.is_err());
    }

    #[test]
    fn loopback_drains_inbox() {
        let (mut a, mut b) = LoopbackTransport::pair(1, 2);
        a.send_to(2, b"x").unwrap();
        a.send_to(2, b"y").unwrap();
        a.send_to(2, b"z").unwrap();
        let inbox = b.recv();
        assert_eq!(inbox.len(), 3);
        // Second recv is empty (we drained)
        assert!(b.recv().is_empty());
    }

    #[test]
    fn loopback_reports_one_connected_peer() {
        let (a, _b) = LoopbackTransport::pair(1, 2);
        assert_eq!(a.connected_peers(), 1);
    }

    // -----------------------------------------------------------------------
    // UDP transport tests
    // -----------------------------------------------------------------------

    #[cfg(feature = "network_udp")]
    #[test]
    fn udp_two_peers_roundtrip() {
        use crate::bridge::NetworkTransport;
        use std::collections::HashMap;
        use std::net::{SocketAddr, UdpSocket as StdUdpSocket};

        // Probe two free ephemeral ports via std (the OS picks them) so
        // both peers can be configured with each other's addresses
        // up-front. Drop the probes immediately; tokio will rebind the
        // same ports a moment later.
        let a_probe = StdUdpSocket::bind("127.0.0.1:0").unwrap();
        let b_probe = StdUdpSocket::bind("127.0.0.1:0").unwrap();
        let a_addr: SocketAddr = a_probe.local_addr().unwrap();
        let b_addr: SocketAddr = b_probe.local_addr().unwrap();
        drop(a_probe);
        drop(b_probe);

        let mut a_peers = HashMap::new();
        a_peers.insert(2, b_addr);
        let mut b_peers = HashMap::new();
        b_peers.insert(1, a_addr);
        let mut a = UdpTransport::bind(1, a_addr, a_peers).expect("a bind");
        let mut b = UdpTransport::bind(2, b_addr, b_peers).expect("b bind");

        a.send_to(2, b"hello b").expect("a → b");
        std::thread::sleep(std::time::Duration::from_millis(80));
        let inbox = b.recv();
        assert!(!inbox.is_empty(), "no inbox on b");
        assert_eq!(inbox[0].1, b"hello b".to_vec());
        assert_eq!(inbox[0].0, 1);
    }
}
