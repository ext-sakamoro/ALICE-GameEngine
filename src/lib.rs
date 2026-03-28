#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::struct_excessive_bools,
    clippy::missing_panics_doc,
    clippy::too_long_first_doc_paragraph
)]

//! # ALICE-GameEngine
//!
//! Hybrid mesh + SDF game engine in Rust. 34 modules, 700+ tests,
//! wgpu deferred renderer (Vulkan/Metal/DX12/WebGPU).
//!
//! ## Quick Start
//!
//! ```rust
//! use alice_game_engine::easy::*;
//!
//! let mut game = GameBuilder::new("Demo").build();
//! game.add_camera();
//! game.add_cube(0.0, 1.0, -5.0);
//! game.add_light(0.0, 10.0, 0.0);
//! game.run_headless(60);
//! assert!(game.time() > 0.0);
//! ```
//!
//! ## Modules
//!
//! | Category | Modules |
//! |----------|---------|
//! | Core | [`ecs`], [`scene_graph`], [`math`], [`engine`], [`resource`] |
//! | Rendering | [`renderer`], [`gpu`], [`gpu_mesh`], [`shader`], [`texture`], [`render_pipeline`], [`lod`] |
//! | Physics | [`physics3d`], [`collision`] |
//! | Audio | [`audio`] |
//! | Animation | [`animation`] |
//! | Input | [`input`], [`window`], [`camera_controller`] |
//! | UI | [`ui`] |
//! | Gameplay | [`ability`], [`scripting`], [`navmesh`], [`particle`] |
//! | 2D | [`scene2d`] |
//! | Asset | [`asset`], [`import`] |
//! | Integration | [`bridge`], [`easy`], [`prelude`], [`query`], [`app`] |

// ---------------------------------------------------------------------------
// Core modules (always available)
// ---------------------------------------------------------------------------

pub mod ability;
pub mod animation;
pub mod app;
pub mod asset;
pub mod bridge;
pub mod camera_controller;
pub mod collision;
pub mod easy;
pub mod ecs;
pub mod engine;
pub mod gpu_mesh;
pub mod import;
pub mod input;
pub mod lod;
pub mod math;
pub mod physics3d;
pub mod prelude;
pub mod query;
pub mod render_pipeline;
pub mod resource;
pub mod scene2d;
pub mod scene_graph;
pub mod scripting;
pub mod shader;
pub mod texture;

// ---------------------------------------------------------------------------
// Feature-gated modules
// ---------------------------------------------------------------------------

#[cfg(feature = "gpu")]
pub mod gpu;

#[cfg(feature = "gpu")]
pub mod renderer;

pub mod window;

#[cfg(feature = "sdf")]
pub mod sdf;

#[cfg(feature = "audio")]
pub mod audio;

#[cfg(feature = "ui")]
pub mod ui;

#[cfg(feature = "particles")]
pub mod particle;

#[cfg(feature = "navmesh")]
pub mod navmesh;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use ability::{Ability, AbilitySystem, AttributeSet, GameplayEffect};
pub use animation::{AnimationClip, AnimationPlayer, Keyframe, StateMachine, Track};
pub use app::{AppCallbacks, HeadlessRunner};
pub use bridge::{AudioSampleProvider, CollisionProvider, Plugin, PluginRegistry, SdfEvaluator};
pub use camera_controller::{FpsCamera, OrbitCamera};
pub use collision::{gjk, ConvexHull, ConvexSphere, GjkResult};
pub use easy::{Game, GameBuilder};
pub use ecs::{
    Collider, CollisionPair, ComponentStore, EntityId, EntityManager, GameEngineError, GameTime,
    Input, PhysicsSystem, Scene, Sprite, Transform, Velocity, World, AABB,
};
pub use engine::{Engine, EngineConfig, EngineContext, System};
pub use gpu_mesh::{DrawCommand, DrawQueue, GpuMeshDesc, VertexLayout};
pub use import::{detect_format, ProjectFormat};
pub use input::{ActionMap, InputState, Key, MouseButton};
pub use lod::{LodGroup, LodLevel, LodSelection};
pub use math::{Color, Mat4, Quat, Vec2, Vec3, Vec4};
pub use physics3d::{Contact3D, PhysicsWorld, RigidBody};
pub use render_pipeline::{FrameData, MaterialUniforms, MvpUniforms, RenderStats};
pub use scene2d::{Scene2D, Sprite2D, TileMap};
pub use scene_graph::{Node, NodeId, NodeKind, SceneGraph};
pub use scripting::{Event, EventBus, Timer, TimerManager};
pub use shader::{ShaderCache, ShaderSource, ShaderStage};
pub use texture::{GpuTextureDesc, TextureAsset};
