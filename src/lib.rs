#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(
    clippy::module_name_repetitions,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    // Intentional in math / SDF code where short names match shader / paper
    // notation.
    clippy::similar_names,
    clippy::many_single_char_names,
    // Bounded-non-negative cast patterns (e.g. usize from validated f32).
    clippy::cast_sign_loss,
    // Boxed closure types are intentional in scripting / network adapters
    // (deferred closure dispatch with `Send` bounds).
    clippy::type_complexity,
    // Nursery-only style preference; if-let-else reads better in our flow.
    clippy::option_if_let_else,
    clippy::struct_excessive_bools,
    clippy::missing_panics_doc,
    clippy::too_long_first_doc_paragraph,
    clippy::result_unit_err
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
pub mod action_combat;
pub mod animation;
pub mod app;
pub mod asset;
#[cfg(feature = "audio_output")]
pub mod audio_output;
pub mod battle;
pub mod bridge;
pub mod camera_controller;
pub mod cloth;
pub mod collision;
pub mod ddgi;
pub mod decal;
pub mod easy;
pub mod ecs;
pub mod editor;
pub mod engine;
pub mod env_probe;
pub mod environment_trajectory;
pub mod fix128;
pub mod flex;
pub mod gaussian_splat;
pub mod gpu_bvh;
pub mod gpu_mesh;
pub mod grid;
pub mod hair;
pub mod hd2d_postfx;
#[cfg(feature = "sdf")]
pub mod heatmap;
pub mod humanoid;
pub mod image_decode;
pub mod imgui;
pub mod import;
pub mod input;
pub mod inspector;
pub mod jobs;
pub mod joint;
pub mod llm;
pub mod lod;
pub mod lut_postprocess;
pub mod math;
pub mod mcp;
pub mod mobile;
pub mod network;
pub mod notify;
pub mod ocean;
pub mod physics3d;
pub mod prelude;
pub mod query;
pub mod render_pipeline;
pub mod resource;
pub mod scene2d;
pub mod scene_graph;
pub mod scene_io;
pub mod scripting;
#[cfg(feature = "sdf")]
pub mod sdf2d;
pub mod sdf_assets;
pub mod shader;
pub mod simd_eval;
pub mod skeleton;
pub mod sky;
pub mod soft_body;
#[cfg(feature = "gpu")]
pub mod sprite_render;
pub mod text;
pub mod texture;
pub mod theme;
pub mod ui_animation;
pub mod verse;
pub mod virtual_shadow;
pub mod volumetric_clouds;

// ---------------------------------------------------------------------------
// Feature-gated modules
// ---------------------------------------------------------------------------

#[cfg(feature = "gpu")]
pub mod gpu;

#[cfg(feature = "gpu")]
pub mod renderer;

#[cfg(feature = "gpu")]
pub mod light_culling;

#[cfg(feature = "window")]
pub mod ui_renderer;

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

pub mod xr;

// ---------------------------------------------------------------------------
// Re-exports
// ---------------------------------------------------------------------------

pub use ability::{Ability, AbilitySystem, AttributeSet, GameplayEffect};
pub use animation::{AnimationClip, AnimationPlayer, Keyframe, StateMachine, Track};
pub use app::{AppCallbacks, HeadlessRunner};
pub use asset::{
    parse_fbx_header, parse_vrm_full, parse_vrm_json, FbxHeader, VrmBoneBinding,
    VrmExpressionBinding, VrmExtract, VrmHeader,
};
pub use bridge::{
    AssetCacheProvider, AudioSampleProvider, CognitiveAction, CognitiveProvider, CollisionProvider,
    CrdtSync, DecodedMediaFrame, DhtProvider, GossipMessage, GossipProvider, IceCandidate,
    LolScriptProvider, MediaCodec, NatTraversal, PeerAddress, PeerDiscovery, PeerIdentity, Plugin,
    PluginRegistry, SdfEvaluator, SyncMessage, SyncProvider, VfxProvider,
};
pub use camera_controller::{FpsCamera, OrbitCamera};
pub use cloth::{ClothConfig, ClothParticle, ClothSim};
pub use collision::{gjk, ConvexHull, ConvexSphere, GjkResult};
pub use ddgi::{dir_to_oct, oct_to_dir, DdgiConfig, DdgiProbe, DdgiVolume};
pub use decal::{DecalBlendMode, DecalData, DecalDraw};
pub use easy::{Game, GameBuilder};
pub use ecs::{
    Collider, CollisionPair, ComponentStore, EntityId, EntityManager, GameEngineError, GameTime,
    Input, PhysicsSystem, Scene, Sprite, Transform, Velocity, World, AABB,
};
pub use editor::{
    dispatch_client_message, Editor, EditorClientMessage, EditorCommand, EditorError,
    EditorHistory, EditorOutcome, EditorServerMessage, EDITOR_PROTOCOL_VERSION,
};
pub use engine::{Engine, EngineConfig, EngineContext, System};
pub use env_probe::{
    capture_sky_to_cubemap, cubemap_face_cameras, cubemap_face_views, prefilter_irradiance,
    prefilter_radiance, Cubemap, CubemapFaceCamera, EnvProbeData, PrefilteredEnvProbe,
};
pub use fix128::{Fix128, Fix128Vec3};
pub use flex::{
    solve as flex_solve, AlignItems, FlexDirection, FlexLayout, FlexNode, JustifyContent,
    ResolvedChild,
};
pub use gaussian_splat::{evaluate_sh_band1, GaussianCloud, ProjectedSplat, Splat};
pub use gpu_bvh::{morton3_unit, radix_sort_u32_pairs, Aabb as BvhAabb, Bvh, BvhNode};
pub use gpu_mesh::{DrawCommand, DrawQueue, GpuMeshDesc, VertexLayout};
pub use grid::{solve as grid_solve, GridCell, GridLayout, ResolvedCell, TrackSize};
pub use hair::{HairConfig, HairStrand, HairSystem};
#[cfg(feature = "sdf")]
pub use heatmap::{heatmap_slice, Axis as SdfAxis, Colormap as SdfColormap};
pub use humanoid::{ExpressionChannel, ExpressionSet, Humanoid, HumanoidBone};
pub use image_decode::{decode_bmp, DecodedImage, ImageDecoder};
pub use imgui::{UiCommand, UiContext, UiInput, UiInteraction};
pub use import::{detect_format, ProjectFormat};
pub use input::{ActionMap, InputState, Key, MouseButton};
pub use inspector::{Inspector, InspectorRow};
pub use jobs::{dispatch, execute, wait, JobArgs, JobContext};
pub use joint::{solve_joints, Joint, JointKind, RagdollDef};
#[cfg(feature = "gpu")]
pub use light_culling::{LightCullingConfig, TileLightList, TiledLightCuller};
pub use llm::{LlmProvider, LlmRequest, LlmResponse, MockLlm, NpcContext};
pub use lod::{LodGroup, LodLevel, LodSelection};
pub use lut_postprocess::{load_cube_file, Lut3DData, LutPostProcess};
pub use math::{Color, Mat4, Quat, Vec2, Vec3, Vec4};
pub use mcp::{McpHandler, McpRequest, McpResponse};
pub use mobile::{
    mobile_build_hints, MobileTarget, ScreenMetrics, TouchCamera, TouchEvent, TouchPhase,
};
pub use network::{GameClient, GameHost, NetMessage, NetPeer, PeerId};
pub use notify::{Notification, NotifyCenter, Severity};
pub use ocean::{OceanConfig, OceanFrame, OceanSimulator};
pub use physics3d::{Contact3D, PhysicsWorld, RigidBody};
pub use render_pipeline::{FrameData, MaterialUniforms, MvpUniforms, RenderStats};
pub use scene2d::{Scene2D, Sprite2D, TileMap};
pub use scene_graph::{Node, NodeId, NodeKind, SceneGraph};
pub use scene_io::{load_scene, save_scene, scene_from_json, scene_to_json};
pub use scripting::{Event, EventBus, Timer, TimerManager};
pub use sdf_assets::{load_asdf, AsdfFile};
pub use shader::{ShaderCache, ShaderSource, ShaderStage};
pub use simd_eval::{eval_sphere_batch, Vec3x8};
pub use skeleton::{Bone, BoneTrack, SkeletalAnimation, Skeleton, SkinData};
pub use soft_body::{SoftBodyConfig, SoftBodySim, SoftParticle};
pub use text::{BitmapFont, TextLayout};
pub use texture::{GpuTextureDesc, TextureAsset};
pub use theme::UiTheme;
pub use ui_animation::{Easing, UiTransition};
pub use verse::{
    Coroutine, Failable, LiveVar, StickyEvent, SubscribableEvent, TickExecutor, Transaction,
};
#[cfg(feature = "gpu")]
pub use virtual_shadow::VirtualShadowGpu;
pub use virtual_shadow::{
    PhysicalPageHandle, VirtualPageId, VirtualShadowConfig, VirtualShadowMap,
};
pub use volumetric_clouds::{
    cloud_density, march_cloud_ray, CloudRayResult, VolumetricCloudConfig,
};
pub use xr::{
    MockProvider, XrAction, XrActionSet, XrAppCallbacks, XrBlendMode, XrConfig, XrError,
    XrFormFactor, XrHand, XrHaptics, XrPose, XrProvider, XrSessionState, XrViewConfiguration,
    XrViewState,
};
