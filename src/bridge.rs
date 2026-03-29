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

/// Trait for external skeletal animation providers.
pub trait SkeletonProvider: Send + Sync {
    /// Returns bone count.
    fn bone_count(&self) -> usize;

    /// Returns skinning matrices (`bone_count` × mat4x4 as f32 slice).
    fn skin_matrices(&self) -> &[f32];

    /// Applies an animation at the given time.
    fn apply_animation(&mut self, name: &str, time: f32);
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
}
