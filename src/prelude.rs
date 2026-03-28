//! Convenience prelude: import everything needed for a typical game in one line.
//!
//! ```rust
//! use alice_game_engine::prelude::*;
//! ```

pub use crate::app::{AppCallbacks, HeadlessRunner};
pub use crate::engine::{Engine, EngineConfig, EngineContext, System};
pub use crate::math::{Color, Mat4, Quat, Vec2, Vec3, Vec4};
pub use crate::scene_graph::{
    CameraData, LightData, LightVariant, LocalTransform, MeshData, Node, NodeId, NodeKind,
    SceneGraph, SdfData,
};
pub use crate::window::WindowConfig;

#[cfg(feature = "window")]
pub use crate::app::run_windowed;

pub use crate::ability::{Ability, AbilitySystem, AttributeSet, GameplayEffect};
pub use crate::animation::{AnimationClip, AnimationPlayer, Keyframe, Track};
pub use crate::bridge::{Plugin, PluginRegistry, SdfEvaluator};
pub use crate::camera_controller::{FpsCamera, OrbitCamera};
pub use crate::ecs::{EntityId, GameTime, World};
pub use crate::input::{ActionMap, InputState, Key, MouseButton};
pub use crate::physics3d::{PhysicsWorld, RigidBody};
pub use crate::scripting::{Event, EventBus};
