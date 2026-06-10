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
}
