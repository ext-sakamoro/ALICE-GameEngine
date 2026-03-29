//! Simplified high-level API for users who don't want to learn internals.
//!
//! ```rust
//! use alice_game_engine::easy::*;
//!
//! let mut game = GameBuilder::new("My Game").build();
//! game.add_camera();
//! game.add_cube(0.0, 1.0, -5.0);
//! game.add_sphere_sdf(3.0, 0.0, 0.0, 1.0);
//! game.add_light(0.0, 10.0, 0.0);
//! game.run_headless(300);
//! ```

#[cfg(feature = "window")]
use crate::engine::EngineContext;
use crate::engine::{Engine, EngineConfig, System};
use crate::math::{Quat, Vec3};
use crate::scene_graph::{
    CameraData, LightData, LightVariant, MeshData, Node, NodeId, NodeKind, SdfData,
};
use crate::window::WindowConfig;

// ---------------------------------------------------------------------------
// GameBuilder
// ---------------------------------------------------------------------------

/// Builder for quick game setup.
pub struct GameBuilder {
    title: String,
    width: u32,
    height: u32,
}

impl GameBuilder {
    #[must_use]
    pub fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            width: 1280,
            height: 720,
        }
    }

    #[must_use]
    pub const fn size(mut self, width: u32, height: u32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    #[must_use]
    pub fn build(self) -> Game {
        let config = EngineConfig {
            window_title: self.title.clone(),
            window_width: self.width,
            window_height: self.height,
            ..EngineConfig::default()
        };
        let mut engine = Engine::new(config);
        let mut noop = NoopSystem;
        engine.init(&mut noop);
        Game {
            engine,
            window_config: WindowConfig {
                title: self.title,
                width: self.width,
                height: self.height,
                ..WindowConfig::default()
            },
        }
    }
}

struct NoopSystem;
impl System for NoopSystem {}

// ---------------------------------------------------------------------------
// Game — simplified interface
// ---------------------------------------------------------------------------

/// High-level game wrapper. Hides the complexity of scene graph, ECS, etc.
pub struct Game {
    pub engine: Engine,
    pub window_config: WindowConfig,
}

impl Game {
    /// Adds a default perspective camera at the origin.
    pub fn add_camera(&mut self) -> NodeId {
        self.engine
            .context
            .scene
            .add(Node::new("camera", NodeKind::Camera(CameraData::default())))
    }

    /// Adds a camera at the given position looking at the target.
    pub fn add_camera_at(&mut self, x: f32, y: f32, z: f32) -> NodeId {
        let mut node = Node::new("camera", NodeKind::Camera(CameraData::default()));
        node.local_transform.position = Vec3::new(x, y, z);
        self.engine.context.scene.add(node)
    }

    /// Adds a mesh cube at the given position.
    pub fn add_cube(&mut self, x: f32, y: f32, z: f32) -> NodeId {
        let mut node = Node::new("cube", NodeKind::Mesh(MeshData::default()));
        node.local_transform.position = Vec3::new(x, y, z);
        self.engine.context.scene.add(node)
    }

    /// Adds an SDF sphere at the given position with the given radius.
    pub fn add_sphere_sdf(&mut self, x: f32, y: f32, z: f32, radius: f32) -> NodeId {
        let mut node = Node::new(
            "sdf_sphere",
            NodeKind::Sdf(SdfData {
                sdf_json: format!(r#"{{"Primitive":{{"Sphere":{{"radius":{radius}}}}}}}"#),
                half_extents: Vec3::new(radius, radius, radius),
                generate_collider: false,
            }),
        );
        node.local_transform.position = Vec3::new(x, y, z);
        self.engine.context.scene.add(node)
    }

    /// Adds a directional light.
    pub fn add_light(&mut self, x: f32, y: f32, z: f32) -> NodeId {
        let mut node = Node::new(
            "light",
            NodeKind::Light(LightData {
                variant: LightVariant::Directional,
                intensity: 1.0,
                ..LightData::default()
            }),
        );
        node.local_transform.position = Vec3::new(x, y, z);
        self.engine.context.scene.add(node)
    }

    /// Adds a point light with radius.
    pub fn add_point_light(&mut self, x: f32, y: f32, z: f32, radius: f32) -> NodeId {
        let mut node = Node::new(
            "point_light",
            NodeKind::Light(LightData {
                variant: LightVariant::Point { radius },
                intensity: 2.0,
                ..LightData::default()
            }),
        );
        node.local_transform.position = Vec3::new(x, y, z);
        self.engine.context.scene.add(node)
    }

    /// Adds an empty node (group/pivot).
    pub fn add_empty(&mut self, name: &str, x: f32, y: f32, z: f32) -> NodeId {
        let mut node = Node::new(name, NodeKind::Empty);
        node.local_transform.position = Vec3::new(x, y, z);
        self.engine.context.scene.add(node)
    }

    /// Sets the position of a node.
    pub fn set_position(&mut self, id: NodeId, x: f32, y: f32, z: f32) {
        if let Some(node) = self.engine.context.scene.get_mut(id) {
            node.local_transform.position = Vec3::new(x, y, z);
        }
    }

    /// Rotates a node around Y axis.
    pub fn rotate_y(&mut self, id: NodeId, radians: f32) {
        if let Some(node) = self.engine.context.scene.get_mut(id) {
            node.local_transform.rotation = Quat::from_axis_angle(Vec3::Y, radians);
        }
    }

    /// Returns the number of nodes in the scene.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.engine.context.scene.node_count()
    }

    /// Returns current engine time in seconds.
    #[must_use]
    pub const fn time(&self) -> f64 {
        self.engine.context.time.total_seconds
    }

    /// Runs N frames in headless mode (no window).
    pub fn run_headless(&mut self, frames: u32) {
        let dt = 1.0 / 60.0;
        let mut noop = NoopSystem;
        for _ in 0..frames {
            self.engine.frame(dt, &mut noop);
        }
    }

    /// Opens a window and runs until closed.
    ///
    /// # Errors
    ///
    /// Returns an error if window or GPU initialization fails.
    #[cfg(feature = "window")]
    pub fn run_windowed_simple(self) -> Result<(), String> {
        struct SimpleSystem;
        impl crate::app::AppCallbacks for SimpleSystem {
            fn init(&mut self, _ctx: &mut EngineContext) {}
            fn update(&mut self, _ctx: &mut EngineContext, _dt: f32) {}
        }
        crate::app::run_windowed(
            self.window_config,
            self.engine.config,
            Box::new(SimpleSystem),
        )
    }

    /// Adds an SDF box at the given position with half-extents.
    pub fn add_box_sdf(&mut self, x: f32, y: f32, z: f32, hx: f32, hy: f32, hz: f32) -> NodeId {
        let mut node = Node::new(
            "sdf_box",
            NodeKind::Sdf(SdfData {
                sdf_json: format!(
                    r#"{{"Primitive":{{"Box":{{"half_extents":[{hx},{hy},{hz}]}}}}}}"#
                ),
                half_extents: Vec3::new(hx, hy, hz),
                generate_collider: false,
            }),
        );
        node.local_transform.position = Vec3::new(x, y, z);
        self.engine.context.scene.add(node)
    }

    /// Gets a node's position.
    #[must_use]
    pub fn get_position(&self, id: NodeId) -> Option<(f32, f32, f32)> {
        self.engine.context.scene.get(id).map(|n| {
            let p = n.local_transform.position;
            (p.x(), p.y(), p.z())
        })
    }

    /// Checks if a key is currently pressed (requires `InputState`).
    #[must_use]
    pub const fn is_key_pressed(&self, _key: crate::input::Key) -> bool {
        false // Headless mode has no input; windowed mode handles via AppCallbacks
    }

    /// Adds a physics body at the given position and returns its index.
    pub fn add_physics_body(&mut self, x: f32, y: f32, z: f32, mass: f32) -> usize {
        let body = crate::physics3d::RigidBody::new(Vec3::new(x, y, z), mass);
        let mut world = crate::physics3d::PhysicsWorld::new();
        world.add_body(body)
    }

    /// Returns scene node count.
    #[must_use]
    pub fn scene_summary(&self) -> String {
        let s = &self.engine.context.scene;
        format!(
            "nodes:{} meshes:{} sdfs:{} lights:{} cameras:{}",
            s.node_count(),
            s.meshes().len(),
            s.sdf_volumes().len(),
            s.lights().len(),
            s.cameras().len(),
        )
    }

    /// Sends an MCP request to the engine and returns the response JSON.
    #[must_use]
    pub fn mcp_call(&mut self, method: &str, params: serde_json::Value) -> String {
        let request = crate::mcp::McpRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: method.to_string(),
            params,
        };
        let response = crate::mcp::McpHandler::handle(&request, &mut self.engine.context);
        serde_json::to_string(&response).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_builder() {
        let game = GameBuilder::new("Test").size(800, 600).build();
        assert_eq!(game.window_config.title, "Test");
        assert_eq!(game.window_config.width, 800);
    }

    #[test]
    fn game_add_camera() {
        let mut game = GameBuilder::new("Test").build();
        game.add_camera();
        assert_eq!(game.node_count(), 1);
    }

    #[test]
    fn game_add_cube() {
        let mut game = GameBuilder::new("Test").build();
        game.add_camera();
        let cube = game.add_cube(1.0, 2.0, 3.0);
        assert_eq!(game.node_count(), 2);
        let node = game.engine.context.scene.get(cube).unwrap();
        assert!((node.local_transform.position.x() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn game_add_sphere_sdf() {
        let mut game = GameBuilder::new("Test").build();
        let id = game.add_sphere_sdf(5.0, 0.0, 0.0, 2.0);
        let node = game.engine.context.scene.get(id).unwrap();
        assert!(matches!(node.kind, NodeKind::Sdf(_)));
    }

    #[test]
    fn game_add_light() {
        let mut game = GameBuilder::new("Test").build();
        game.add_light(0.0, 10.0, 0.0);
        game.add_point_light(3.0, 2.0, 0.0, 15.0);
        assert_eq!(game.engine.context.scene.lights().len(), 2);
    }

    #[test]
    fn game_set_position() {
        let mut game = GameBuilder::new("Test").build();
        let id = game.add_cube(0.0, 0.0, 0.0);
        game.set_position(id, 10.0, 20.0, 30.0);
        let node = game.engine.context.scene.get(id).unwrap();
        assert!((node.local_transform.position.x() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn game_rotate() {
        let mut game = GameBuilder::new("Test").build();
        let id = game.add_cube(0.0, 0.0, 0.0);
        game.rotate_y(id, 1.5);
        let node = game.engine.context.scene.get(id).unwrap();
        assert_ne!(node.local_transform.rotation, Quat::IDENTITY);
    }

    #[test]
    fn game_run_headless() {
        let mut game = GameBuilder::new("Test").build();
        game.add_camera();
        game.run_headless(60);
        assert!(game.time() > 0.0);
    }

    #[test]
    fn game_add_empty() {
        let mut game = GameBuilder::new("Test").build();
        let id = game.add_empty("pivot", 0.0, 0.0, 0.0);
        let node = game.engine.context.scene.get(id).unwrap();
        assert!(matches!(node.kind, NodeKind::Empty));
    }
}
