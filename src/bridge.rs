//! ALICE ecosystem bridge: trait interfaces for integrating external
//! ALICE-xxx crates (ALICE-SDF, ALICE-Physics, ALICE-Audio, etc.)
//! without hard dependencies.
//!
//! Each bridge trait defines the contract that an external crate must
//! implement to plug into the engine. The engine calls through these
//! traits; the concrete implementation lives in the external crate.

use crate::math::{Color, Vec3};

// ---------------------------------------------------------------------------
// SDF Bridge — for ALICE-SDF integration
// ---------------------------------------------------------------------------

/// Trait for external SDF evaluators (e.g. ALICE-SDF's `CompiledSdf`).
pub trait SdfEvaluator: Send + Sync {
    /// Evaluates the signed distance at point `p`.
    fn eval(&self, p: Vec3) -> f32;

    /// Evaluates the gradient (normal) at point `p`.
    fn normal(&self, p: Vec3, eps: f32) -> Vec3 {
        let dx = self.eval(Vec3::new(p.x() + eps, p.y(), p.z()))
            - self.eval(Vec3::new(p.x() - eps, p.y(), p.z()));
        let dy = self.eval(Vec3::new(p.x(), p.y() + eps, p.z()))
            - self.eval(Vec3::new(p.x(), p.y() - eps, p.z()));
        let dz = self.eval(Vec3::new(p.x(), p.y(), p.z() + eps))
            - self.eval(Vec3::new(p.x(), p.y(), p.z() - eps));
        Vec3::new(dx, dy, dz).normalize()
    }

    /// Evaluates a batch of points. Default: sequential. ALICE-SDF overrides
    /// with SIMD 8-wide + Rayon parallel.
    fn eval_batch(&self, points: &[Vec3]) -> Vec<f32> {
        points.iter().map(|&p| self.eval(p)).collect()
    }
}

// ---------------------------------------------------------------------------
// Physics Bridge — for ALICE-Physics integration
// ---------------------------------------------------------------------------

/// Trait for external physics collision providers.
pub trait CollisionProvider: Send + Sync {
    /// Tests a sphere against the collision world.
    fn sphere_cast(
        &self,
        origin: Vec3,
        radius: f32,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<CollisionHit>;

    /// Tests an AABB against the collision world.
    fn aabb_overlap(&self, min: Vec3, max: Vec3) -> bool;
}

/// Hit result from a collision query.
#[derive(Debug, Clone, Copy)]
pub struct CollisionHit {
    pub point: Vec3,
    pub normal: Vec3,
    pub distance: f32,
}

// ---------------------------------------------------------------------------
// Audio Bridge — for ALICE-Audio integration
// ---------------------------------------------------------------------------

/// Trait for external audio sample providers (e.g. ALICE-Audio decoders).
pub trait AudioSampleProvider: Send + Sync {
    /// Reads mono samples into the buffer. Returns number of samples written.
    fn read_samples(&mut self, buffer: &mut [f32]) -> usize;

    /// Returns the sample rate.
    fn sample_rate(&self) -> u32;

    /// Returns total duration in seconds, or None if streaming.
    fn duration(&self) -> Option<f32>;

    /// Seeks to a position in seconds.
    fn seek(&mut self, position_seconds: f32);

    /// Returns true if the source has finished playback.
    fn is_finished(&self) -> bool;
}

// ---------------------------------------------------------------------------
// Render Bridge — for ALICE-Render / custom renderers
// ---------------------------------------------------------------------------

/// Trait for external mesh providers (e.g. ALICE-SDF's Marching Cubes output).
pub trait MeshProvider: Send + Sync {
    fn vertex_count(&self) -> usize;
    fn index_count(&self) -> usize;
    /// Returns vertex data as interleaved pos(3f) + normal(3f) + uv(2f) bytes.
    fn vertex_bytes(&self) -> &[u8];
    fn index_bytes(&self) -> &[u8];
}

/// Trait for external shader transpilers (e.g. ALICE-SDF HLSL/GLSL output).
pub trait ShaderTranspiler: Send + Sync {
    /// Transpiles WGSL to the target language.
    ///
    /// # Errors
    ///
    /// Returns an error message if the transpilation fails.
    fn transpile(&self, wgsl: &str, target: ShaderTarget) -> Result<String, String>;
}

/// Target shader language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderTarget {
    Hlsl,
    Glsl,
    Msl,
    SpirV,
}

// ---------------------------------------------------------------------------
// UI Bridge — for custom widget renderers
// ---------------------------------------------------------------------------

/// Trait for external UI renderers.
pub trait UiRenderer: Send + Sync {
    /// Draws a filled rectangle.
    fn draw_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color);

    /// Draws text at position.
    fn draw_text(&mut self, x: f32, y: f32, text: &str, size: f32, color: Color);
}

// ---------------------------------------------------------------------------
// Network Bridge — for ALICE-Sync / external transport
// ---------------------------------------------------------------------------

/// Trait for network transport backends (ALICE-Sync, tokio, quinn, WebRTC).
pub trait NetworkTransport: Send + Sync {
    /// Sends raw bytes to a peer.
    ///
    /// # Errors
    /// Returns error on send failure.
    fn send_to(&mut self, peer_id: u32, data: &[u8]) -> Result<(), String>;

    /// Receives pending data. Returns (`peer_id`, data) pairs.
    fn recv(&mut self) -> Vec<(u32, Vec<u8>)>;

    /// Returns connected peer count.
    fn connected_peers(&self) -> usize;
}

// ---------------------------------------------------------------------------
// Skeleton Bridge — for external animation systems
// ---------------------------------------------------------------------------

/// Trait for external skeletal animation providers (e.g. ALICE-Animation).
pub trait SkeletonProvider: Send + Sync {
    fn bone_count(&self) -> usize;
    fn skin_matrices(&self) -> &[f32];
    fn apply_animation(&mut self, name: &str, time: f32);
}

// ---------------------------------------------------------------------------
// Animation Bridge — for ALICE-Animation
// ---------------------------------------------------------------------------

/// Trait for external animation systems (e.g. ALICE-Animation).
pub trait AnimationProvider: Send + Sync {
    /// Lists available animation clip names.
    fn clip_names(&self) -> Vec<String>;
    /// Evaluates a clip at time t, returns (`track_name`, value) pairs.
    fn evaluate(&self, clip_name: &str, time: f32) -> Vec<(String, f32)>;
    /// Returns clip duration.
    fn clip_duration(&self, clip_name: &str) -> f32;
}

// ---------------------------------------------------------------------------
// Protocol Bridge — for ALICE-Streaming-Protocol
// ---------------------------------------------------------------------------

/// Trait for streaming protocol backends (e.g. ALICE-Streaming-Protocol).
pub trait StreamingProtocol: Send + Sync {
    /// Sends a scene delta to a remote peer.
    ///
    /// # Errors
    /// Returns error on send failure.
    fn send_delta(&mut self, delta: &[u8]) -> Result<(), String>;
    /// Receives pending deltas.
    fn recv_deltas(&mut self) -> Vec<Vec<u8>>;
    /// Returns current bandwidth usage in bytes/sec.
    fn bandwidth_bytes_per_sec(&self) -> u64;
}

// ---------------------------------------------------------------------------
// Font Bridge — for ALICE-Font
// ---------------------------------------------------------------------------

/// Trait for SDF font glyph providers (e.g. ALICE-Font).
pub trait SdfFontProvider: Send + Sync {
    /// Returns SDF glyph data for a character.
    fn glyph_sdf(&self, char_code: u32, font_size: f32) -> Option<GlyphSdf>;
    /// Returns line height for the given font size.
    fn line_height(&self, font_size: f32) -> f32;
}

/// SDF glyph data from ALICE-Font.
#[derive(Debug, Clone)]
pub struct GlyphSdf {
    pub width: u32,
    pub height: u32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub advance: f32,
    pub sdf_data: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Text Bridge — for ALICE-Text
// ---------------------------------------------------------------------------

/// Trait for text processing (e.g. ALICE-Text compression/encoding).
pub trait TextProcessor: Send + Sync {
    /// Compresses text.
    fn compress(&self, input: &str) -> Vec<u8>;
    /// Decompresses text.
    ///
    /// # Errors
    /// Returns error on decode failure.
    fn decompress(&self, data: &[u8]) -> Result<String, String>;
}

// ---------------------------------------------------------------------------
// Image Decoder Bridge — for external PNG/JPG
// ---------------------------------------------------------------------------

/// Trait for image decoding (implement with `image` crate).
pub trait ImageDecoderBridge: Send + Sync {
    /// Decodes image bytes to RGBA8.
    ///
    /// # Errors
    /// Returns error on decode failure.
    fn decode_rgba8(&self, data: &[u8]) -> Result<(u32, u32, Vec<u8>), String>;
}

// ---------------------------------------------------------------------------
// World Bridge — for ALICE-Metaverse / external themed-world providers
// ---------------------------------------------------------------------------

/// Opaque zone identifier. Concrete providers choose their own zone taxonomy;
/// the engine treats this as a `u32` handle and does not interpret it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ZoneId(pub u32);

/// Time-of-day + weather snapshot used for lighting, particle, audio cues.
/// POD layout — directly copy-able to GPU uniforms.
#[derive(Debug, Clone, Copy, Default)]
pub struct WorldEnvironment {
    /// 0.0 = midnight, 0.25 = sunrise, 0.5 = noon, 0.75 = sunset.
    pub day_phase: f32,
    /// 0.0..=1.0 fog density.
    pub fog: f32,
    /// 0.0..=1.0 rain intensity.
    pub rain: f32,
    /// 0.0..=1.0 lightning strobe (decays fast).
    pub lightning: f32,
    /// Ambient temperature in Celsius. Default 20.
    pub temperature_c: f32,
}

/// Spawn pose for a zone.
#[derive(Debug, Clone, Copy, Default)]
pub struct SpawnPose {
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

/// Result of a teleport request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeleportResult {
    Started,
    Busy,
    Unknown,
}

/// Trait for stateful world providers (e.g. ALICE-Metaverse themed park).
///
/// A `WorldProvider` represents a *running* simulated world — it owns the
/// camera, time-of-day, weather, zone topology, and any internal dynamics.
/// The engine queries it each frame for environment cues and forwards camera
/// input through it.
///
/// This trait is scene-agnostic — concrete providers may implement zones via
/// SDF, voxel grids, polygon meshes, or any combination. SDF evaluation
/// remains the responsibility of [`SdfEvaluator`]; a provider may also
/// implement that trait, or expose neither.
///
/// # Threading
/// `Send + Sync`. The engine may call read-only methods from render threads
/// while a single owning thread drives [`Self::step`].
pub trait WorldProvider: Send + Sync {
    /// Advances the world by `dt` seconds. Called once per fixed step from
    /// `Engine::step()` before system updates.
    fn step(&mut self, dt: f32);

    fn camera_position(&self) -> Vec3;
    fn camera_yaw(&self) -> f32;
    fn camera_pitch(&self) -> f32;

    /// Applies look delta (mouse / right-stick).
    fn look_delta(&mut self, dyaw: f32, dpitch: f32);

    /// Applies movement intent in camera-local axes
    /// (x=right, y=up, z=forward). `sprinting` modulates speed.
    fn move_intent(&mut self, local_dir: Vec3, sprinting: bool);

    fn environment(&self) -> WorldEnvironment;

    fn current_zone(&self) -> ZoneId;
    fn zone_spawn(&self, zone: ZoneId) -> Option<SpawnPose>;

    /// Lists all zones. Default: empty (anonymous zones).
    fn zones(&self) -> Vec<ZoneId> {
        Vec::new()
    }

    /// Display name for a zone in a given BCP-47 locale (`"ja"`, `"en"`).
    /// Default: None.
    fn zone_name(&self, _zone: ZoneId, _locale: &str) -> Option<String> {
        None
    }

    /// Initiates a teleport. Engine resets input deltas on `Started`.
    fn teleport_to(&mut self, zone: ZoneId) -> TeleportResult;

    /// True while a teleport animation is playing. Default: false.
    fn is_teleporting(&self) -> bool {
        false
    }

    /// Returns the WGSL fragment shader for this world (for wgpu).
    /// Default: None — engine renderer handles all rendering.
    fn shader_wgsl(&self) -> Option<String> {
        None
    }

    /// Returns the GLSL fragment shader (for WebGL / OpenGL paths).
    /// Default: None.
    fn shader_glsl(&self) -> Option<String> {
        None
    }
}

// ---------------------------------------------------------------------------
// Plugin system
// ---------------------------------------------------------------------------

/// A plugin that can be registered with the engine to extend functionality.
pub trait Plugin: Send + Sync {
    /// Plugin name for identification.
    fn name(&self) -> &str;

    /// Called once when the plugin is registered.
    fn on_register(&mut self) {}

    /// Called every frame.
    fn on_update(&mut self, _dt: f32) {}

    /// Called on shutdown.
    fn on_shutdown(&mut self) {}
}

/// Registry of plugins.
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
        }
    }

    /// Registers a plugin.
    pub fn register(&mut self, mut plugin: Box<dyn Plugin>) {
        plugin.on_register();
        self.plugins.push(plugin);
    }

    /// Updates all plugins.
    pub fn update(&mut self, dt: f32) {
        for plugin in &mut self.plugins {
            plugin.on_update(dt);
        }
    }

    /// Shuts down all plugins.
    pub fn shutdown(&mut self) {
        for plugin in &mut self.plugins {
            plugin.on_shutdown();
        }
    }

    /// Finds a plugin by name.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&dyn Plugin> {
        self.plugins.iter().find(|p| p.name() == name).map(|p| &**p)
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.plugins.len()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Wave 11: ALICE-xxx eco-system bridges
// ---------------------------------------------------------------------------

/// Topic-based publish/subscribe + scene state mirror. Implementors
/// connect to ALICE-Sync (or any other multiplayer / cross-process
/// sync backend) and expose a uniform API the engine can call from
/// its frame loop.
pub trait SyncProvider: Send + Sync {
    fn publish(&mut self, topic: &str, payload: &[u8]);
    fn poll(&mut self) -> Vec<SyncMessage>;
    fn subscribed_topics(&self) -> Vec<String>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyncMessage {
    pub topic: String,
    pub payload: Vec<u8>,
}

/// AI / NPC decision-making backend. Implementors plug ALICE-Cognitive
/// (or any custom behaviour tree / utility-AI / LLM-driven brain)
/// into the engine's gameplay loop.
pub trait CognitiveProvider: Send + Sync {
    fn perceive(&mut self, observation: &str);
    fn decide(&mut self) -> CognitiveAction;
    fn learn(&mut self, reward: f32);
}

#[derive(Debug, Clone, PartialEq)]
pub enum CognitiveAction {
    Idle,
    Move {
        target_x: f32,
        target_y: f32,
        target_z: f32,
    },
    Speak {
        utterance: String,
    },
    UseAbility {
        ability_id: u32,
    },
    Custom {
        name: String,
        payload: String,
    },
}

/// Audio / video codec backend. Implementors expose ALICE-Codec
/// (or any custom encoder) so the engine can capture replays and
/// stream cinematics without statically depending on a specific
/// codec crate.
pub trait MediaCodec: Send + Sync {
    fn encode_frame(&mut self, pixels: &[u8], width: u32, height: u32) -> Vec<u8>;
    fn decode_frame(&mut self, encoded: &[u8]) -> Option<DecodedMediaFrame>;
    fn format_name(&self) -> &str;
}

#[derive(Debug, Clone, PartialEq)]
pub struct DecodedMediaFrame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// ALICE-LOL DSL compiler. Implementors translate LOL source code
/// (or compiled bytecode) into the engine's SDF JSON so authors can
/// drive scenes from natural-language-like declarations.
pub trait LolScriptProvider: Send + Sync {
    /// Compile `source` into a JSON string that the engine feeds into
    /// `sdf_assets::load_asdf` or `asset::load_sdf_json`.
    ///
    /// # Errors
    ///
    /// Returns an error string when the LOL source fails to compile.
    fn compile_lol(&mut self, source: &str) -> Result<String, String>;
}

/// VFX provider (ALICE-Visual). Lets the engine spawn higher-level
/// effects (impact bursts, weather, scripted cinematics) without
/// depending on a specific VFX implementation.
pub trait VfxProvider: Send + Sync {
    fn spawn_effect(&mut self, name: &str, position_x: f32, position_y: f32, position_z: f32);
    fn active_effect_count(&self) -> usize;
}

/// Asset cache provider (ALICE-Cache). The engine queries this for
/// streamed / network-loaded resources and offloads eviction to the
/// implementor.
pub trait AssetCacheProvider: Send + Sync {
    fn get(&self, key: &str) -> Option<Vec<u8>>;
    fn put(&mut self, key: &str, value: Vec<u8>);
    fn remove(&mut self, key: &str);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ---------------------------------------------------------------------------
// Wave 12: P2P bridge stack
// ---------------------------------------------------------------------------

/// Public-key-based peer identity. Implementors bridge real signing
/// keys (Ed25519, libp2p `Keypair`, etc.) to the engine without
/// exposing the private material.
pub trait PeerIdentity: Send + Sync {
    /// Stable peer id derived from the public key.
    fn peer_id(&self) -> String;
    /// Sign arbitrary bytes with the local private key.
    fn sign(&self, message: &[u8]) -> Vec<u8>;
    /// Verify a signature claimed to come from `peer_id`.
    fn verify(&self, peer_id: &str, message: &[u8], signature: &[u8]) -> bool;
}

/// Peer discovery backend (mDNS, Kademlia bootstrap, manual
/// configuration). The engine periodically calls
/// [`PeerDiscovery::known_peers`] to refresh its peer table.
pub trait PeerDiscovery: Send + Sync {
    fn announce(&mut self, self_addr: &str);
    fn known_peers(&self) -> Vec<PeerAddress>;
    fn forget(&mut self, peer_id: &str);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerAddress {
    pub peer_id: String,
    pub multiaddr: String,
}

/// NAT-traversal helper (STUN / TURN / ICE / hole-punching).
/// `gather_candidates` collects local + reflexive addresses;
/// `pair_with` records a remote candidate set for connectivity
/// checks.
pub trait NatTraversal: Send + Sync {
    fn gather_candidates(&mut self) -> Vec<IceCandidate>;
    fn pair_with(&mut self, remote_peer: &str, remote_candidates: &[IceCandidate]);
    fn punch_hole(&mut self, remote_peer: &str) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IceCandidate {
    /// `host` / `srflx` (server-reflexive) / `relay`.
    pub kind: String,
    pub address: String,
    pub port: u16,
    pub priority: u32,
}

/// Distributed hash table (Kademlia-style) provider. Engine code
/// stores small key-value pairs (= player session beacons, scene
/// hashes) without owning the network plumbing.
pub trait DhtProvider: Send + Sync {
    fn put(&mut self, key: &str, value: Vec<u8>);
    fn get(&self, key: &str) -> Option<Vec<u8>>;
    fn find_peers(&self, key: &str) -> Vec<String>;
}

/// Gossip / pub-sub overlay (libp2p `gossipsub` analogue). Topics
/// are arbitrary strings; published payloads fan out to all
/// subscribers.
pub trait GossipProvider: Send + Sync {
    fn subscribe(&mut self, topic: &str);
    fn unsubscribe(&mut self, topic: &str);
    fn publish(&mut self, topic: &str, payload: &[u8]);
    fn drain_inbox(&mut self) -> Vec<GossipMessage>;
    fn peer_count(&self) -> usize;
}

#[derive(Debug, Clone, PartialEq)]
pub struct GossipMessage {
    pub topic: String,
    pub payload: Vec<u8>,
    pub from_peer: String,
}

/// Conflict-free Replicated Data Type sync. Implementors expose a
/// CRDT (= last-writer-wins map, OR-set, RGA list, Automerge doc)
/// + the delta encoding required to converge peers.
pub trait CrdtSync: Send + Sync {
    fn local_update(&mut self, key: &str, value: &[u8]);
    fn encode_delta(&self) -> Vec<u8>;
    fn apply_delta(&mut self, delta: &[u8]);
    fn snapshot(&self) -> Vec<(String, Vec<u8>)>;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct TestSdf;
    impl SdfEvaluator for TestSdf {
        fn eval(&self, p: Vec3) -> f32 {
            p.length() - 1.0
        }
    }

    #[test]
    fn sdf_evaluator_eval() {
        let sdf = TestSdf;
        assert!(sdf.eval(Vec3::ZERO) < 0.0);
        assert!(sdf.eval(Vec3::new(2.0, 0.0, 0.0)) > 0.0);
    }

    // --- Wave 11 bridge tests ---

    struct MockSync {
        outbox: Vec<SyncMessage>,
    }
    impl SyncProvider for MockSync {
        fn publish(&mut self, topic: &str, payload: &[u8]) {
            self.outbox.push(SyncMessage {
                topic: topic.to_string(),
                payload: payload.to_vec(),
            });
        }
        fn poll(&mut self) -> Vec<SyncMessage> {
            std::mem::take(&mut self.outbox)
        }
        fn subscribed_topics(&self) -> Vec<String> {
            vec!["scene".into()]
        }
    }

    #[test]
    fn sync_provider_round_trip() {
        let mut s = MockSync { outbox: Vec::new() };
        s.publish("scene", b"hi");
        let polled = s.poll();
        assert_eq!(polled.len(), 1);
        assert_eq!(polled[0].topic, "scene");
        assert_eq!(s.subscribed_topics(), vec!["scene".to_string()]);
    }

    struct MockBrain {
        memory: Vec<String>,
        reward_sum: f32,
    }
    impl CognitiveProvider for MockBrain {
        fn perceive(&mut self, o: &str) {
            self.memory.push(o.to_string());
        }
        fn decide(&mut self) -> CognitiveAction {
            if self.memory.iter().any(|m| m.contains("enemy")) {
                CognitiveAction::UseAbility { ability_id: 7 }
            } else {
                CognitiveAction::Idle
            }
        }
        fn learn(&mut self, r: f32) {
            self.reward_sum += r;
        }
    }

    #[test]
    fn cognitive_provider_branches_on_observation() {
        let mut b = MockBrain {
            memory: Vec::new(),
            reward_sum: 0.0,
        };
        b.perceive("enemy spotted");
        assert!(matches!(
            b.decide(),
            CognitiveAction::UseAbility { ability_id: 7 }
        ));
        b.learn(1.0);
        assert!((b.reward_sum - 1.0).abs() < 1e-6);
    }

    struct PassThroughCodec;
    impl MediaCodec for PassThroughCodec {
        fn encode_frame(&mut self, p: &[u8], _w: u32, _h: u32) -> Vec<u8> {
            p.to_vec()
        }
        fn decode_frame(&mut self, e: &[u8]) -> Option<DecodedMediaFrame> {
            Some(DecodedMediaFrame {
                pixels: e.to_vec(),
                width: 1,
                height: 1,
            })
        }
        fn format_name(&self) -> &str {
            "raw"
        }
    }

    #[test]
    fn codec_round_trip_returns_input() {
        let mut c = PassThroughCodec;
        let encoded = c.encode_frame(&[1, 2, 3], 1, 3);
        let decoded = c.decode_frame(&encoded).unwrap();
        assert_eq!(decoded.pixels, vec![1, 2, 3]);
        assert_eq!(c.format_name(), "raw");
    }

    struct MockLol;
    impl LolScriptProvider for MockLol {
        fn compile_lol(&mut self, source: &str) -> Result<String, String> {
            if source.contains("sphere") {
                Ok(r#"{"kind":"sphere","radius":1.0}"#.to_string())
            } else {
                Err("unsupported LOL primitive".into())
            }
        }
    }

    #[test]
    fn lol_compile_translates_primitive() {
        let mut l = MockLol;
        let j = l.compile_lol("sphere 1").unwrap();
        assert!(j.contains("\"sphere\""));
        assert!(l.compile_lol("blob").is_err());
    }

    struct MockVfx {
        active: usize,
    }
    impl VfxProvider for MockVfx {
        fn spawn_effect(&mut self, _n: &str, _x: f32, _y: f32, _z: f32) {
            self.active += 1;
        }
        fn active_effect_count(&self) -> usize {
            self.active
        }
    }

    #[test]
    fn vfx_provider_counts_spawns() {
        let mut v = MockVfx { active: 0 };
        v.spawn_effect("explosion", 0.0, 0.0, 0.0);
        v.spawn_effect("smoke", 1.0, 0.0, 0.0);
        assert_eq!(v.active_effect_count(), 2);
    }

    struct MockCache {
        map: std::collections::HashMap<String, Vec<u8>>,
    }
    impl AssetCacheProvider for MockCache {
        fn get(&self, k: &str) -> Option<Vec<u8>> {
            self.map.get(k).cloned()
        }
        fn put(&mut self, k: &str, v: Vec<u8>) {
            self.map.insert(k.to_string(), v);
        }
        fn remove(&mut self, k: &str) {
            self.map.remove(k);
        }
        fn len(&self) -> usize {
            self.map.len()
        }
    }

    #[test]
    fn asset_cache_put_get_remove() {
        let mut c = MockCache {
            map: std::collections::HashMap::new(),
        };
        c.put("texture", vec![1, 2, 3]);
        assert_eq!(c.get("texture"), Some(vec![1, 2, 3]));
        assert_eq!(c.len(), 1);
        c.remove("texture");
        assert!(c.is_empty());
    }

    // --- Wave 12 P2P bridge tests ---

    struct MockIdentity {
        id: String,
    }
    impl PeerIdentity for MockIdentity {
        fn peer_id(&self) -> String {
            self.id.clone()
        }
        fn sign(&self, m: &[u8]) -> Vec<u8> {
            // Trivial signature = id ++ message hash.
            let mut out = self.id.as_bytes().to_vec();
            out.extend_from_slice(&m.iter().map(|b| b.wrapping_add(1)).collect::<Vec<_>>());
            out
        }
        fn verify(&self, peer_id: &str, m: &[u8], sig: &[u8]) -> bool {
            let prefix = peer_id.as_bytes();
            sig.starts_with(prefix)
                && sig[prefix.len()..] == m.iter().map(|b| b.wrapping_add(1)).collect::<Vec<_>>()
        }
    }

    #[test]
    fn peer_identity_sign_verify_round_trip() {
        let id = MockIdentity {
            id: "peer-A".into(),
        };
        let sig = id.sign(b"hello");
        assert!(id.verify("peer-A", b"hello", &sig));
        assert!(!id.verify("peer-B", b"hello", &sig));
    }

    struct MockDiscovery {
        table: Vec<PeerAddress>,
    }
    impl PeerDiscovery for MockDiscovery {
        fn announce(&mut self, _addr: &str) {}
        fn known_peers(&self) -> Vec<PeerAddress> {
            self.table.clone()
        }
        fn forget(&mut self, id: &str) {
            self.table.retain(|p| p.peer_id != id);
        }
    }

    #[test]
    fn peer_discovery_forget_removes_entry() {
        let mut d = MockDiscovery {
            table: vec![
                PeerAddress {
                    peer_id: "a".into(),
                    multiaddr: "/ip4/127.0.0.1/tcp/1".into(),
                },
                PeerAddress {
                    peer_id: "b".into(),
                    multiaddr: "/ip4/127.0.0.1/tcp/2".into(),
                },
            ],
        };
        assert_eq!(d.known_peers().len(), 2);
        d.forget("a");
        assert_eq!(d.known_peers().len(), 1);
    }

    struct MockNat {
        local: Vec<IceCandidate>,
        pairs: std::collections::HashMap<String, Vec<IceCandidate>>,
    }
    impl NatTraversal for MockNat {
        fn gather_candidates(&mut self) -> Vec<IceCandidate> {
            self.local.clone()
        }
        fn pair_with(&mut self, peer: &str, remote: &[IceCandidate]) {
            self.pairs.insert(peer.to_string(), remote.to_vec());
        }
        fn punch_hole(&mut self, peer: &str) -> bool {
            self.pairs.contains_key(peer)
        }
    }

    #[test]
    fn nat_traversal_pair_then_punch() {
        let mut n = MockNat {
            local: vec![IceCandidate {
                kind: "host".into(),
                address: "192.168.0.5".into(),
                port: 41234,
                priority: 100,
            }],
            pairs: std::collections::HashMap::new(),
        };
        assert_eq!(n.gather_candidates().len(), 1);
        assert!(!n.punch_hole("remote"));
        n.pair_with(
            "remote",
            &[IceCandidate {
                kind: "srflx".into(),
                address: "203.0.113.4".into(),
                port: 50000,
                priority: 50,
            }],
        );
        assert!(n.punch_hole("remote"));
    }

    struct MockDht {
        kv: std::collections::HashMap<String, Vec<u8>>,
    }
    impl DhtProvider for MockDht {
        fn put(&mut self, k: &str, v: Vec<u8>) {
            self.kv.insert(k.to_string(), v);
        }
        fn get(&self, k: &str) -> Option<Vec<u8>> {
            self.kv.get(k).cloned()
        }
        fn find_peers(&self, _k: &str) -> Vec<String> {
            vec!["peer-x".into(), "peer-y".into()]
        }
    }

    #[test]
    fn dht_put_get_find() {
        let mut d = MockDht {
            kv: std::collections::HashMap::new(),
        };
        d.put("session/abc", vec![1, 2, 3]);
        assert_eq!(d.get("session/abc"), Some(vec![1, 2, 3]));
        assert_eq!(d.find_peers("session/abc").len(), 2);
    }

    struct MockGossip {
        topics: std::collections::HashSet<String>,
        inbox: Vec<GossipMessage>,
    }
    impl GossipProvider for MockGossip {
        fn subscribe(&mut self, t: &str) {
            self.topics.insert(t.to_string());
        }
        fn unsubscribe(&mut self, t: &str) {
            self.topics.remove(t);
        }
        fn publish(&mut self, t: &str, p: &[u8]) {
            if self.topics.contains(t) {
                self.inbox.push(GossipMessage {
                    topic: t.to_string(),
                    payload: p.to_vec(),
                    from_peer: "self".into(),
                });
            }
        }
        fn drain_inbox(&mut self) -> Vec<GossipMessage> {
            std::mem::take(&mut self.inbox)
        }
        fn peer_count(&self) -> usize {
            3
        }
    }

    #[test]
    fn gossip_subscribe_publish_drain() {
        let mut g = MockGossip {
            topics: std::collections::HashSet::new(),
            inbox: Vec::new(),
        };
        g.subscribe("chat");
        g.publish("chat", b"hello");
        g.publish("private", b"ignored");
        let msgs = g.drain_inbox();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].topic, "chat");
        assert!(g.drain_inbox().is_empty());
        assert_eq!(g.peer_count(), 3);
    }

    struct MockCrdt {
        kv: std::collections::HashMap<String, Vec<u8>>,
        pending: Vec<u8>,
    }
    impl CrdtSync for MockCrdt {
        fn local_update(&mut self, k: &str, v: &[u8]) {
            self.kv.insert(k.to_string(), v.to_vec());
            // Trivial delta encoding: "key|value_len|bytes".
            self.pending
                .extend_from_slice(format!("{k}|{}|", v.len()).as_bytes());
            self.pending.extend_from_slice(v);
        }
        fn encode_delta(&self) -> Vec<u8> {
            self.pending.clone()
        }
        fn apply_delta(&mut self, delta: &[u8]) {
            // For the mock we just append the raw bytes — production
            // CRDT crates parse + merge.
            self.pending.extend_from_slice(delta);
        }
        fn snapshot(&self) -> Vec<(String, Vec<u8>)> {
            self.kv
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        }
    }

    #[test]
    fn crdt_local_update_emits_delta_and_snapshots() {
        let mut c = MockCrdt {
            kv: std::collections::HashMap::new(),
            pending: Vec::new(),
        };
        c.local_update("hp", &[60]);
        let delta = c.encode_delta();
        assert!(!delta.is_empty());
        let snap = c.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].0, "hp");
    }

    #[test]
    fn sdf_evaluator_normal() {
        let sdf = TestSdf;
        let n = sdf.normal(Vec3::new(1.0, 0.0, 0.0), 0.001);
        assert!((n.x() - 1.0).abs() < 0.05);
    }

    #[test]
    fn sdf_evaluator_batch() {
        let sdf = TestSdf;
        let points = vec![Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0)];
        let results = sdf.eval_batch(&points);
        assert_eq!(results.len(), 2);
        assert!(results[0] < 0.0);
        assert!(results[1] > 0.0);
    }

    struct TestPlugin {
        registered: bool,
        updates: u32,
    }

    impl TestPlugin {
        fn new() -> Self {
            Self {
                registered: false,
                updates: 0,
            }
        }
    }

    impl Plugin for TestPlugin {
        fn name(&self) -> &str {
            "test_plugin"
        }
        fn on_register(&mut self) {
            self.registered = true;
        }
        fn on_update(&mut self, _dt: f32) {
            self.updates += 1;
        }
    }

    #[test]
    fn plugin_registry() {
        let mut reg = PluginRegistry::new();
        reg.register(Box::new(TestPlugin::new()));
        assert_eq!(reg.count(), 1);
        assert!(reg.find("test_plugin").is_some());
    }

    #[test]
    fn plugin_lifecycle() {
        let mut reg = PluginRegistry::new();
        reg.register(Box::new(TestPlugin::new()));
        reg.update(0.016);
        reg.update(0.016);
        reg.shutdown();
        assert_eq!(reg.count(), 1);
    }

    #[test]
    fn collision_hit_struct() {
        let hit = CollisionHit {
            point: Vec3::ZERO,
            normal: Vec3::Y,
            distance: 1.5,
        };
        assert_eq!(hit.distance, 1.5);
    }

    #[test]
    fn shader_target_variants() {
        assert_ne!(ShaderTarget::Hlsl, ShaderTarget::Glsl);
        assert_ne!(ShaderTarget::Msl, ShaderTarget::SpirV);
    }

    #[test]
    fn plugin_not_found() {
        let reg = PluginRegistry::new();
        assert!(reg.find("nonexistent").is_none());
    }

    // -----------------------------------------------------------------------
    // WorldProvider tests
    // -----------------------------------------------------------------------

    struct TestWorld {
        pos: Vec3,
        yaw: f32,
        pitch: f32,
        zone: ZoneId,
        env: WorldEnvironment,
        teleporting: bool,
    }

    impl TestWorld {
        fn new() -> Self {
            Self {
                pos: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
                zone: ZoneId(0),
                env: WorldEnvironment::default(),
                teleporting: false,
            }
        }
    }

    impl WorldProvider for TestWorld {
        fn step(&mut self, _dt: f32) {
            self.teleporting = false;
        }
        fn camera_position(&self) -> Vec3 {
            self.pos
        }
        fn camera_yaw(&self) -> f32 {
            self.yaw
        }
        fn camera_pitch(&self) -> f32 {
            self.pitch
        }
        fn look_delta(&mut self, dyaw: f32, dpitch: f32) {
            self.yaw += dyaw;
            self.pitch = (self.pitch + dpitch).clamp(-1.5, 1.5);
        }
        fn move_intent(&mut self, dir: Vec3, _sprint: bool) {
            self.pos = self.pos + dir;
        }
        fn environment(&self) -> WorldEnvironment {
            self.env
        }
        fn current_zone(&self) -> ZoneId {
            self.zone
        }
        fn zone_spawn(&self, zone: ZoneId) -> Option<SpawnPose> {
            (zone.0 < 6).then_some(SpawnPose {
                position: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
            })
        }
        fn teleport_to(&mut self, zone: ZoneId) -> TeleportResult {
            if self.teleporting {
                return TeleportResult::Busy;
            }
            if zone.0 >= 6 {
                return TeleportResult::Unknown;
            }
            self.teleporting = true;
            self.zone = zone;
            TeleportResult::Started
        }
        fn is_teleporting(&self) -> bool {
            self.teleporting
        }
    }

    #[test]
    fn world_provider_teleport_lifecycle() {
        let mut w = TestWorld::new();
        assert_eq!(w.teleport_to(ZoneId(2)), TeleportResult::Started);
        assert!(w.is_teleporting());
        assert_eq!(w.current_zone(), ZoneId(2));
        assert_eq!(w.teleport_to(ZoneId(3)), TeleportResult::Busy);
        w.step(0.016);
        assert!(!w.is_teleporting());
        assert_eq!(w.teleport_to(ZoneId(99)), TeleportResult::Unknown);
    }

    #[test]
    fn world_environment_default_is_clear_midnight() {
        let env = WorldEnvironment::default();
        assert!(env.day_phase.abs() < f32::EPSILON);
        assert!(env.fog.abs() < f32::EPSILON);
        assert!(env.rain.abs() < f32::EPSILON);
        assert!(env.lightning.abs() < f32::EPSILON);
        assert!(env.temperature_c.abs() < f32::EPSILON);
    }

    #[test]
    fn zone_id_is_opaque() {
        assert_ne!(ZoneId(0), ZoneId(1));
        assert_eq!(ZoneId(5), ZoneId(5));
        let w = TestWorld::new();
        assert!(w.zone_spawn(ZoneId(0)).is_some());
        assert!(w.zone_spawn(ZoneId(99)).is_none());
    }

    #[test]
    fn world_provider_default_shaders_none() {
        let w = TestWorld::new();
        assert!(w.shader_wgsl().is_none());
        assert!(w.shader_glsl().is_none());
        assert!(w.zones().is_empty());
        assert!(w.zone_name(ZoneId(0), "ja").is_none());
    }
}
