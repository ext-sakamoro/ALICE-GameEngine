//! Engine integration: ties all subsystems into a unified game loop.
//!
//! ## Pipeline
//!
//! ```text
//! init → [fixed_update (physics 60Hz)] → update → render → audio
//! ```

use crate::ecs::{GameTime, World};
use crate::math::Vec3;
use crate::resource::ResourceManager;
use crate::scene_graph::SceneGraph;

#[cfg(feature = "audio")]
use crate::audio::AudioEngine;

#[cfg(feature = "ui")]
use crate::ui::UiContext;

// ---------------------------------------------------------------------------
// System trait
// ---------------------------------------------------------------------------

/// User-defined system that receives engine context each frame.
pub trait System {
    /// Called once when the engine starts.
    fn init(&mut self, _ctx: &mut EngineContext) {}

    /// Called at a fixed rate (default 60Hz) for physics/logic.
    fn fixed_update(&mut self, _ctx: &mut EngineContext, _fixed_dt: f32) {}

    /// Called every frame with variable dt.
    fn update(&mut self, _ctx: &mut EngineContext, _dt: f32) {}
}

// ---------------------------------------------------------------------------
// EngineConfig
// ---------------------------------------------------------------------------

/// Configuration for the engine.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub fixed_timestep: f32,
    pub max_fixed_steps_per_frame: u32,
    pub gravity: Vec3,
    pub window_title: String,
    pub window_width: u32,
    pub window_height: u32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            fixed_timestep: 1.0 / 60.0,
            max_fixed_steps_per_frame: 5,
            gravity: Vec3::new(0.0, -9.81, 0.0),
            window_title: "ALICE Engine".to_string(),
            window_width: 1280,
            window_height: 720,
        }
    }
}

// ---------------------------------------------------------------------------
// EngineContext — mutable access to all subsystems
// ---------------------------------------------------------------------------

/// Provides mutable access to all engine subsystems during callbacks.
pub struct EngineContext {
    pub world: World,
    pub scene: SceneGraph,
    pub resources: ResourceManager,
    pub time: GameTime,
    pub input: crate::input::InputState,
    pub action_map: crate::input::ActionMap,
    pub plugins: crate::bridge::PluginRegistry,
    pub coroutines: crate::verse::TickExecutor,
    /// Registered mesh assets — index is `MeshData::mesh_id`.
    pub mesh_assets: Vec<crate::asset::MeshAsset>,

    /// External SDF evaluator (e.g. ALICE-SDF `CompiledSdf`).
    pub sdf_evaluator: Option<Box<dyn crate::bridge::SdfEvaluator>>,
    /// External collision provider (e.g. ALICE-Physics).
    pub collision_provider: Option<Box<dyn crate::bridge::CollisionProvider>>,

    #[cfg(feature = "audio")]
    pub audio: AudioEngine,

    #[cfg(feature = "ui")]
    pub ui: UiContext,
}

impl EngineContext {
    #[must_use]
    pub fn new() -> Self {
        Self {
            world: World::new(),
            scene: SceneGraph::new("default"),
            resources: ResourceManager::new(),
            time: GameTime::new(),
            input: crate::input::InputState::new(),
            action_map: crate::input::ActionMap::new(),
            plugins: crate::bridge::PluginRegistry::new(),
            mesh_assets: Vec::new(),
            coroutines: crate::verse::TickExecutor::new(),
            sdf_evaluator: None,
            collision_provider: None,

            #[cfg(feature = "audio")]
            audio: AudioEngine::new(),

            #[cfg(feature = "ui")]
            ui: UiContext::new(),
        }
    }

    /// Registers a mesh asset and returns its `mesh_id` (index).
    pub fn register_mesh_asset(&mut self, asset: crate::asset::MeshAsset) -> u32 {
        let id = self.mesh_assets.len() as u32;
        self.mesh_assets.push(asset);
        id
    }

    /// Injects an external SDF evaluator (e.g. ALICE-SDF).
    pub fn set_sdf_evaluator(&mut self, evaluator: Box<dyn crate::bridge::SdfEvaluator>) {
        self.sdf_evaluator = Some(evaluator);
    }

    /// Injects an external collision provider (e.g. ALICE-Physics).
    pub fn set_collision_provider(&mut self, provider: Box<dyn crate::bridge::CollisionProvider>) {
        self.collision_provider = Some(provider);
    }

    /// Evaluates SDF at a point using the external evaluator (if set) or returns None.
    #[must_use]
    pub fn eval_sdf(&self, p: Vec3) -> Option<f32> {
        self.sdf_evaluator.as_ref().map(|e| e.eval(p))
    }
}

impl Default for EngineContext {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Engine
// ---------------------------------------------------------------------------

/// The main engine struct that drives the game loop.
pub struct Engine {
    pub config: EngineConfig,
    pub context: EngineContext,
    fixed_accumulator: f32,
    running: bool,
    frame_count: u64,
}

impl Engine {
    #[must_use]
    pub fn new(config: EngineConfig) -> Self {
        Self {
            config,
            context: EngineContext::new(),
            fixed_accumulator: 0.0,
            running: false,
            frame_count: 0,
        }
    }

    /// Initializes the engine and calls `System::init`.
    pub fn init(&mut self, system: &mut dyn System) {
        self.running = true;
        system.init(&mut self.context);
    }

    /// Simulates one frame. Call this in your main loop.
    /// Returns false if the engine should stop.
    pub fn frame(&mut self, dt: f32, system: &mut dyn System) -> bool {
        if !self.running {
            return false;
        }

        self.context.time.tick(f64::from(dt));

        // Fixed timestep for physics
        self.fixed_accumulator += dt;
        let mut fixed_steps = 0u32;
        while self.fixed_accumulator >= self.config.fixed_timestep
            && fixed_steps < self.config.max_fixed_steps_per_frame
        {
            system.fixed_update(&mut self.context, self.config.fixed_timestep);
            self.fixed_accumulator -= self.config.fixed_timestep;
            fixed_steps += 1;
        }

        // Variable update
        system.update(&mut self.context, dt);

        // Update plugins and coroutines
        self.context.plugins.update(dt);
        self.context.coroutines.tick();

        // Update scene transforms
        self.context.scene.update_world_matrices();

        self.frame_count += 1;
        true
    }

    /// Stops the engine.
    pub const fn stop(&mut self) {
        self.running = false;
    }

    /// Returns whether the engine is running.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }

    /// Returns the total frame count.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Returns the fixed timestep accumulator (for interpolation).
    #[must_use]
    pub const fn fixed_accumulator(&self) -> f32 {
        self.fixed_accumulator
    }

    /// Returns the interpolation alpha for rendering between physics steps.
    #[must_use]
    pub fn interpolation_alpha(&self) -> f32 {
        if self.config.fixed_timestep > 0.0 {
            self.fixed_accumulator / self.config.fixed_timestep
        } else {
            1.0
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct CounterSystem {
        init_called: bool,
        fixed_count: u32,
        update_count: u32,
    }

    impl CounterSystem {
        fn new() -> Self {
            Self {
                init_called: false,
                fixed_count: 0,
                update_count: 0,
            }
        }
    }

    impl System for CounterSystem {
        fn init(&mut self, _ctx: &mut EngineContext) {
            self.init_called = true;
        }

        fn fixed_update(&mut self, _ctx: &mut EngineContext, _fixed_dt: f32) {
            self.fixed_count += 1;
        }

        fn update(&mut self, _ctx: &mut EngineContext, _dt: f32) {
            self.update_count += 1;
        }
    }

    #[test]
    fn engine_config_default() {
        let cfg = EngineConfig::default();
        assert!((cfg.fixed_timestep - 1.0 / 60.0).abs() < 1e-6);
        assert_eq!(cfg.window_width, 1280);
    }

    #[test]
    fn engine_init() {
        let mut engine = Engine::new(EngineConfig::default());
        let mut sys = CounterSystem::new();
        engine.init(&mut sys);
        assert!(sys.init_called);
        assert!(engine.is_running());
    }

    #[test]
    fn engine_frame_updates_time() {
        let mut engine = Engine::new(EngineConfig::default());
        let mut sys = CounterSystem::new();
        engine.init(&mut sys);
        engine.frame(1.0 / 60.0, &mut sys);
        assert!(engine.context.time.total_seconds > 0.0);
        assert_eq!(engine.frame_count(), 1);
    }

    #[test]
    fn engine_fixed_update_rate() {
        let mut engine = Engine::new(EngineConfig::default());
        let mut sys = CounterSystem::new();
        engine.init(&mut sys);
        // One frame at 1/30 should trigger ~2 fixed updates (1/60 each)
        engine.frame(1.0 / 30.0, &mut sys);
        assert_eq!(sys.fixed_count, 2);
        assert_eq!(sys.update_count, 1);
    }

    #[test]
    fn engine_max_fixed_steps() {
        let mut cfg = EngineConfig::default();
        cfg.max_fixed_steps_per_frame = 3;
        let mut engine = Engine::new(cfg);
        let mut sys = CounterSystem::new();
        engine.init(&mut sys);
        // Large dt would cause many fixed steps, but capped at 3
        engine.frame(1.0, &mut sys);
        assert_eq!(sys.fixed_count, 3);
    }

    #[test]
    fn engine_stop() {
        let mut engine = Engine::new(EngineConfig::default());
        let mut sys = CounterSystem::new();
        engine.init(&mut sys);
        engine.stop();
        let running = engine.frame(1.0 / 60.0, &mut sys);
        assert!(!running);
    }

    #[test]
    fn engine_interpolation_alpha() {
        let mut engine = Engine::new(EngineConfig::default());
        let mut sys = CounterSystem::new();
        engine.init(&mut sys);
        // Frame slightly longer than one fixed step
        engine.frame(1.0 / 60.0 + 0.005, &mut sys);
        let alpha = engine.interpolation_alpha();
        assert!(alpha >= 0.0 && alpha <= 1.0);
    }

    #[test]
    fn engine_multiple_frames() {
        let mut engine = Engine::new(EngineConfig::default());
        let mut sys = CounterSystem::new();
        engine.init(&mut sys);
        for _ in 0..10 {
            engine.frame(1.0 / 60.0, &mut sys);
        }
        assert_eq!(engine.frame_count(), 10);
        assert_eq!(sys.update_count, 10);
    }

    #[test]
    fn engine_context_default() {
        let ctx = EngineContext::new();
        assert_eq!(ctx.scene.node_count(), 0);
    }

    #[test]
    fn engine_scene_access() {
        let mut engine = Engine::new(EngineConfig::default());
        let mut sys = CounterSystem::new();
        engine.init(&mut sys);
        engine.context.scene.add(crate::scene_graph::Node::new(
            "test",
            crate::scene_graph::NodeKind::Empty,
        ));
        assert_eq!(engine.context.scene.node_count(), 1);
    }

    struct SpawnSystem;
    impl System for SpawnSystem {
        fn init(&mut self, ctx: &mut EngineContext) {
            ctx.world.spawn();
            ctx.world.spawn();
        }
    }

    #[test]
    fn engine_system_accesses_world() {
        let mut engine = Engine::new(EngineConfig::default());
        let mut sys = SpawnSystem;
        engine.init(&mut sys);
        assert_eq!(engine.context.world.entity_manager.living_count(), 2);
    }
}
