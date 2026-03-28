# ALICE-GameEngine

Hybrid mesh + SDF game engine in Rust. 31 modules, 688 tests, wgpu deferred renderer (Vulkan/Metal/DX12/WebGPU).

## Quick Start

```bash
cargo run --example spinning_cube --features full
```

Opens a window with a rotating colored cube rendered via wgpu. Press Escape to exit.

```rust
use alice_game_engine::app::{run_windowed, AppCallbacks};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::{Quat, Vec3};
use alice_game_engine::scene_graph::*;
use alice_game_engine::window::WindowConfig;

struct MyGame;

impl AppCallbacks for MyGame {
    fn init(&mut self, ctx: &mut EngineContext) {
        // Camera
        ctx.scene.add(Node::new("camera", NodeKind::Camera(CameraData::default())));

        // Polygon mesh
        ctx.scene.add(Node::new("cube", NodeKind::Mesh(MeshData::default())));

        // SDF volume (coexists with mesh in the same scene graph)
        ctx.scene.add(Node::new("sphere", NodeKind::Sdf(SdfData {
            sdf_json: r#"{"Primitive":{"Sphere":{"radius":1.0}}}"#.to_string(),
            half_extents: Vec3::ONE,
            generate_collider: true,
        })));
    }

    fn update(&mut self, ctx: &mut EngineContext, _dt: f32) {
        let t = ctx.time.total_seconds as f32;
        if let Some(node) = ctx.scene.get_mut(NodeId(1)) {
            node.local_transform.rotation = Quat::from_axis_angle(Vec3::Y, t);
        }
    }
}

fn main() {
    run_windowed(WindowConfig::default(), EngineConfig::default(), Box::new(MyGame)).unwrap();
}
```

## Architecture

```
                    +-----------+
                    |  app.rs   |  winit event loop + wgpu present
                    +-----+-----+
                          |
                    +-----+-----+
                    | engine.rs |  System trait, fixed timestep (60Hz physics)
                    +-----+-----+
                          |
        +---------+-------+-------+---------+
        |         |       |       |         |
   scene_graph  ecs   physics3d  audio   input
   (mesh+SDF)  (ECS)  (impulse) (HRTF)  (action map)
        |                 |
   +----+----+      broadphase
   |         |      (sweep-and-prune)
 renderer   sdf
 (wgpu)   (marching cubes)
```

## Modules

| Module | Lines | Tests | Description |
|--------|------:|------:|-------------|
| ecs | 1,781 | 107 | Generational arena ECS, spatial hash grid broadphase |
| scene_graph | 1,277 | 43 | Mesh+SDF hybrid node tree, AABB3, frustum culling, reparenting |
| sdf | 1,112 | 37 | 7 primitives, 6 boolean ops, Marching Cubes (256 tables), sphere trace, SDF collider |
| audio | 975 | 39 | Bus effects (ping-pong), HRTF, PCM playback, spatial panning, WAV export |
| ui | 951 | 30 | Retained-mode widgets, vertical+horizontal layout, focus management, theme |
| math | 776 | 30 | Vec2/3/4, Mat4, Quat, Color, perspective+orthographic projection |
| renderer | 773 | 25 | Deferred GBuffer, RenderGraph (Kahn topo sort), DebugRenderer |
| app | 715 | 13 | `run_windowed()` (winit+wgpu), `HeadlessRunner`, WAV export |
| physics3d | 696 | 32 | RigidBody, sweep-and-prune broadphase, impulse solver, damping, sleeping |
| navmesh | 654 | 21 | NavMesh, A* pathfinding, SDF avoidance, crowd separation (RVO) |
| animation | 650 | 32 | Keyframe (Linear/Step/Cubic), Track, Clip, Player, StateMachine |
| input | 587 | 16 | Keyboard/Mouse/Gamepad, ActionMap, axis binding, just_pressed |
| scripting | 549 | 24 | EventBus (pub/sub), Timer/TimerManager, ScriptVars |
| scene2d | 532 | 21 | Sprite2D, TileMap, Aabb2, Body2D, Physics2D, z-order |
| gpu | 521 | 10 | wgpu Device/Queue/Surface, render_mesh(), create_texture_rgba8() |
| ability | 501 | 16 | Gameplay Ability System: attributes, effects, cooldowns, modifiers |
| shader | 439 | 15 | ShaderCache, 5 built-in WGSL shaders |
| particle | 432 | 16 | CPU emitter, multi-shape (Point/Sphere/Box/Cone), gravity |
| import | 409 | 17 | Unity YAML scene parser, UE5 .uasset header parser, format detection |
| texture | 400 | 18 | TextureAsset, mipmap, checkerboard, GpuTextureDesc, SamplerDesc |
| render_pipeline | 354 | 13 | FrameData extraction, MvpUniforms, MaterialUniforms, PipelineState |
| engine | 354 | 11 | Game loop, System trait, fixed timestep, interpolation alpha |
| collision | 333 | 10 | GJK convex intersection, SDF-mesh hybrid narrowphase |
| asset | 333 | 13 | OBJ parser, glTF header, SDF JSON loader, asset type detection |
| camera_controller | 322 | 19 | FPS camera (WASD+mouse), Orbit camera (rotate/zoom/pan) |
| resource | 309 | 12 | Async resource manager, ref counting, load state |
| query | 293 | 11 | Typed ECS queries (query2/3), filter, SystemScheduler |
| gpu_mesh | 280 | 9 | GpuMeshDesc, VertexLayout, DrawCommand/DrawQueue |
| lod | 264 | 13 | LOD group selection, screen coverage, batch culling |
| window | 263 | 15 | WindowConfig, key mapping, FrameTimer |
| **Total** | **17,932** | **688** | |

## Feature Flags

| Flag | Description |
|------|-------------|
| `gpu` | wgpu deferred renderer (Vulkan/Metal/DX12/WebGPU) |
| `window` | winit window + GPU (implies `gpu`) |
| `sdf` | SDF evaluation, Marching Cubes, sphere tracing |
| `audio` | Spatial audio with HRTF, bus routing, effects |
| `ui` | Retained-mode UI widget system |
| `particles` | Particle emitter system |
| `navmesh` | Navigation mesh + A* + crowd |
| `full` | All features |

## Key Design Decisions

**Mesh + SDF hybrid scene graph** — Polygon meshes and SDF volumes coexist as first-class `NodeKind` variants in the same tree. No conversion required.

**Enum dispatch over trait objects** — Audio effects, UI widgets, and SDF nodes use enum dispatch for zero vtable overhead in hot paths.

**Sweep-and-prune broadphase** — Physics uses X-axis sorted sweep-and-prune (O(n log n)) instead of O(n^2) brute force. ECS 2D collision uses spatial hash grid.

**wgpu over OpenGL** — Targets Vulkan, Metal, DX12, and WebGPU instead of legacy OpenGL. Future-proof for WASM.

**Self-contained physics** — No Rapier dependency. Impulse solver with damping, sleeping, angular velocity, and collision events.

## Examples

```bash
# Windowed GPU rendering (spinning cube)
cargo run --example spinning_cube --features full

# Headless engine loop (no window, 300 frames)
cargo run --example hello_engine --features full
```

## Quality

```bash
cargo test --features full        # 688 tests, 0 failures
cargo clippy --features full -- -W clippy::all  # 0 warnings
cargo fmt -- --check              # 0 diffs
cargo doc --no-deps --features full  # 0 warnings
```

## License

MIT
