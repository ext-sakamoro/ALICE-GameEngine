# ALICE-GameEngine

Hybrid mesh + SDF game engine in Rust. 31 modules, 688 tests, wgpu deferred renderer (Vulkan/Metal/DX12/WebGPU).

## Quick Start (5 lines)

```rust
use alice_game_engine::easy::*;

let mut game = GameBuilder::new("My Game").build();
game.add_camera();
game.add_cube(0.0, 1.0, -5.0);
game.add_sphere_sdf(3.0, 0.0, 0.0, 1.0);
game.add_light(0.0, 10.0, 0.0);
game.run_headless(300);
```

Or use the prelude for full control:

```rust
use alice_game_engine::prelude::*;
```

## Windowed Example

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

## Usage Guide

### ECS — Entity Creation & Components

```rust
use alice_game_engine::*;

let mut world = World::new();
let entity = world.spawn();

// Add components
world.transform_store.insert(entity, Transform::new(10.0, 5.0));
world.velocity_store.insert(entity, Velocity::new(1.0, -0.5));
world.collider_store.insert(entity, Collider::new(AABB::new(-1.0, -1.0, 1.0, 1.0), 0));

// Game loop
let mut time = GameTime::new();
time.tick(1.0 / 60.0);
PhysicsSystem::update(&mut world, &time);
let collisions = PhysicsSystem::detect_collisions(&world);
```

### Scene Graph — Mesh + SDF Hybrid

```rust
use alice_game_engine::scene_graph::*;
use alice_game_engine::math::*;

let mut scene = SceneGraph::new("my_level");

// Camera
let cam = scene.add(Node::new("camera", NodeKind::Camera(CameraData::default())));

// Polygon mesh
let mut cube = Node::new("cube", NodeKind::Mesh(MeshData { mesh_id: 0, material_id: 0, cast_shadows: true }));
cube.local_transform.position = Vec3::new(0.0, 1.0, -5.0);
cube.local_transform.rotation = Quat::from_axis_angle(Vec3::Y, 0.5);
let cube_id = scene.add(cube);

// SDF volume in the same scene
let mut sphere = Node::new("terrain", NodeKind::Sdf(SdfData {
    sdf_json: r#"{"Primitive":{"Sphere":{"radius":2.0}}}"#.to_string(),
    half_extents: Vec3::new(2.0, 2.0, 2.0),
    generate_collider: true,
}));
sphere.local_transform.position = Vec3::new(5.0, 0.0, 0.0);
scene.add(sphere);

// Lights
scene.add(Node::new("sun", NodeKind::Light(LightData {
    variant: LightVariant::Directional,
    intensity: 1.5,
    ..LightData::default()
})));

// Hierarchy
let child = scene.add_child(cube_id, Node::new("child", NodeKind::Empty));

// Update transforms + frustum cull
scene.update_world_matrices();
let vp = Mat4::perspective(std::f32::consts::FRAC_PI_4, 16.0/9.0, 0.1, 100.0);
let frustum = scene_graph::Frustum::from_view_projection(vp);
let visible = scene.frustum_cull(&frustum);
```

### Physics — RigidBody & Collision

```rust
use alice_game_engine::physics3d::*;
use alice_game_engine::math::Vec3;

let mut world = PhysicsWorld::new();
world.gravity = Vec3::new(0.0, -9.81, 0.0);

// Dynamic body
let ball = world.add_body(RigidBody::new(Vec3::new(0.0, 10.0, 0.0), 1.0));
world.bodies[ball].restitution = 0.7;
world.bodies[ball].linear_damping = 0.02;

// Static ground
world.add_body(RigidBody::new_static(Vec3::new(0.0, 0.0, 0.0)));

// Apply forces
world.bodies[ball].apply_force(Vec3::new(5.0, 0.0, 0.0));
world.bodies[ball].apply_impulse(Vec3::new(0.0, 20.0, 0.0));

// Simulate (broadphase + narrowphase + resolve integrated)
for _ in 0..600 {
    world.step(1.0 / 60.0);
}

// Check contacts
for contact in &world.contacts {
    println!("Contact: body {} hit body {}", contact.body_a, contact.body_b);
}
```

### Audio — Sound Playback & Spatial

```rust
use alice_game_engine::audio::*;
use alice_game_engine::math::Vec3;

let mut engine = AudioEngine::new();

// Add effect bus
let mut sfx_bus = AudioBus::new("sfx");
sfx_bus.effects.push(Effect::Reverb(Reverb::new(0.4, 4410)));
sfx_bus.effects.push(Effect::Attenuate(Attenuate { gain: 0.8 }));
engine.add_bus(sfx_bus);

// PCM source with spatial positioning
let mut src = AudioSource::new("gunshot", "sfx");
src.set_pcm(vec![0.9, 0.7, 0.3, 0.1, -0.2, -0.1]); // raw samples
src.spatial = true;
src.position = Vec3::new(5.0, 0.0, -3.0);
src.max_distance = 50.0;
src.playing = true;
engine.add_source(src);

// Render to stereo buffer (panned based on listener position)
engine.listener_position = Vec3::ZERO;
engine.listener_forward = Vec3::new(0.0, 0.0, -1.0);
let output = engine.render(1024, 44100);

// Export to WAV
let wav_bytes = alice_game_engine::app::export_wav(&output);
std::fs::write("output.wav", wav_bytes).unwrap();
```

### Animation — Keyframes & State Machine

```rust
use alice_game_engine::animation::*;

// Create animation clip
let mut walk = AnimationClip::new("walk");
walk.looping = true;
let mut track = Track::new("leg_angle");
track.add_keyframe(Keyframe::new(0.0, 0.0));
track.add_keyframe(Keyframe::with_interp(0.5, 1.0, Interpolation::CubicBezier));
track.add_keyframe(Keyframe::new(1.0, 0.0));
walk.tracks.push(track);

// Playback
let mut player = AnimationPlayer::new("walk");
player.speed = 1.5;
player.play();
player.update(0.3); // advance 0.3s
let values = walk.evaluate(player.time); // → [("leg_angle", 0.78)]

// State machine
let mut sm = StateMachine::new("idle");
sm.add_state("walk");
sm.add_state("run");
sm.add_transition("idle", "walk", "move", 0.2);  // 0.2s blend
sm.add_transition("walk", "run", "sprint", 0.3);
sm.add_transition("walk", "idle", "stop", 0.2);

sm.trigger("move");
sm.update(0.1); // mid-transition, blend_factor = 0.5
sm.update(0.2); // transition complete → state = "walk"
```

### Input — ActionMap & Gamepad

```rust
use alice_game_engine::input::*;

let mut input = InputState::new();
let mut actions = ActionMap::new();

// Bind multiple sources to one action
actions.bind_action("jump", InputSource::Key(Key::Space));
actions.bind_action("jump", InputSource::Gamepad(GamepadButton::South));

// Analog axes
actions.bind_axis("move_x", AxisSource::KeyPair { negative: Key::A, positive: Key::D });
actions.bind_axis("move_x", AxisSource::GamepadAxis(GamepadAxis::LeftStickX));
actions.bind_axis("look_x", AxisSource::MouseX);

// Per-frame usage
input.begin_frame();
input.key_press(Key::D);
input.gamepad_axis_update(GamepadAxis::LeftStickX, 0.7);

if actions.is_action_just_pressed("jump", &input) { /* jump */ }
let move_x = actions.axis_value("move_x", &input); // → 1.0 (D key)
```

### SDF — Primitives, Meshing & Collision

```rust
use alice_game_engine::sdf::*;
use alice_game_engine::math::Vec3;

// Build SDF tree
let scene = SdfNode::Operation {
    op: SdfOp::SmoothUnion,
    k: 0.5,
    children: vec![
        SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 }),
        SdfNode::Transform {
            translation: Vec3::new(1.5, 0.0, 0.0),
            child: Box::new(SdfNode::Primitive(SdfPrimitive::Box {
                half_extents: Vec3::new(0.8, 0.8, 0.8),
            })),
        },
    ],
};

// Evaluate distance at a point
let dist = scene.eval(Vec3::new(0.5, 0.0, 0.0));

// Sphere trace (raymarching)
if let Some(hit) = sphere_trace(&scene, Vec3::new(0.0, 0.0, 5.0), Vec3::new(0.0, 0.0, -1.0), 128, 100.0, 0.001) {
    println!("Hit at distance {:.2}, steps: {}", hit.distance, hit.steps);
}

// Generate triangle mesh (Marching Cubes)
let mesh = marching_cubes(&scene, Vec3::new(-3.0, -3.0, -3.0), Vec3::new(3.0, 3.0, 3.0), 32);
println!("{} vertices, {} triangles", mesh.vertex_count(), mesh.triangle_count());

// SDF collision test
if let Some(contact) = sdf_sphere_test(&scene, Vec3::new(0.5, 0.0, 0.0), 0.5) {
    println!("Penetration: {:.3}", contact.penetration);
}
```

### UI — Widgets & Layout

```rust
use alice_game_engine::ui::*;
use alice_game_engine::math::{Vec2, Color};

let mut ui = UiContext::new();

// Horizontal toolbar
let mut toolbar = Widget::new(WidgetKind::Panel);
toolbar.layout_direction = LayoutDirection::Horizontal;
toolbar.background = Color::new(0.2, 0.2, 0.2, 1.0);
let toolbar_id = ui.add(toolbar);

// Buttons in the toolbar
let mut btn1 = Widget::new(WidgetKind::Button { label: "File".to_string() });
btn1.desired_size = Vec2::new(60.0, 30.0);
ui.add_child(toolbar_id, btn1);

let mut btn2 = Widget::new(WidgetKind::Button { label: "Edit".to_string() });
btn2.desired_size = Vec2::new(60.0, 30.0);
ui.add_child(toolbar_id, btn2);

// Run layout
ui.layout(toolbar_id, Rect::new(0.0, 0.0, 800.0, 30.0));

// Hit testing
if let Some(hit) = ui.hit_test(25.0, 15.0) {
    println!("Clicked widget: {}", hit);
}

// Focus management
let mut focus = FocusManager::new();
focus.register(WidgetId(1));
focus.register(WidgetId(2));
focus.tab_next(); // focus → Widget(1)
focus.tab_next(); // focus → Widget(2)

// Message passing
ui.send(UiMessage::new(WidgetId(1), MessageDirection::FromWidget, MessagePayload::Click));
for msg in ui.drain_messages() {
    println!("Event: {:?} on {}", msg.payload, msg.target);
}
```

### NavMesh — Pathfinding & Crowd

```rust
use alice_game_engine::navmesh::*;
use alice_game_engine::math::Vec3;

// Build navmesh
let mesh = NavMesh {
    vertices: vec![
        NavVertex { position: Vec3::new(0.0, 0.0, 0.0) },
        NavVertex { position: Vec3::new(10.0, 0.0, 0.0) },
        NavVertex { position: Vec3::new(5.0, 0.0, 10.0) },
    ],
    triangles: vec![NavTriangle { indices: [0, 1, 2], neighbors: [u32::MAX; 3] }],
};

// A* pathfinding
if let Some(path) = a_star(&mesh, 0, 0) {
    println!("Path: {:?}", path);
}

// Agent following waypoints
let mut agent = NavAgent::new(Vec3::ZERO, 5.0, 0.5);
agent.set_path(NavPath { waypoints: vec![Vec3::new(5.0, 0.0, 3.0), Vec3::new(8.0, 0.0, 7.0)] });
for _ in 0..100 {
    agent.update(1.0 / 60.0);
    if agent.reached_goal { break; }
}

// SDF obstacle avoidance
let steered = sdf_steer(agent.position, Vec3::new(1.0, 0.0, 0.0), Vec3::new(3.0, 0.0, 0.0), 2.0, 1.5);

// Crowd separation
let mut agents = vec![
    NavAgent::new(Vec3::new(0.0, 0.0, 0.0), 3.0, 0.5),
    NavAgent::new(Vec3::new(0.3, 0.0, 0.0), 3.0, 0.5),
];
crowd_separation(&mut agents, 2.0, 1.0);
```

### Ability System (UE5 GAS)

```rust
use alice_game_engine::ability::*;

// Define attributes
let mut attrs = AttributeSet::new();
attrs.add(Attribute::new("health", 100.0, 0.0, 100.0));
attrs.add(Attribute::new("mana", 80.0, 0.0, 100.0));

// Create ability with cost and cooldown
let fireball = Ability::new("fireball", 60, "mana", 25.0,
    GameplayEffect::instant("fire_damage", vec![
        AttributeModifier::flat("health", -40.0),
    ])
);

let mut sys = AbilitySystem::new();
sys.add_ability(fireball);

// Activate
if sys.activate("fireball", &mut attrs) {
    println!("Mana after cast: {}", attrs.value("mana")); // 55.0
}

// Timed buff (heals 5 HP per tick for 10 ticks)
let regen = Ability::new("regen", 0, "mana", 10.0,
    GameplayEffect::timed("heal_over_time", 10, vec![
        AttributeModifier::flat("health", 5.0),
    ])
);
sys.add_ability(regen);
sys.activate("regen", &mut attrs);
for _ in 0..10 { sys.tick(&mut attrs); }
```

### Camera Controllers

```rust
use alice_game_engine::camera_controller::*;
use alice_game_engine::math::Vec3;

// FPS camera
let mut fps = FpsCamera::new(Vec3::new(0.0, 1.8, 0.0));
fps.move_speed = 8.0;
fps.look(mouse_dx, mouse_dy);            // mouse look
fps.move_local(forward, strafe, 0.0, dt); // WASD
let view = fps.view_matrix();

// Orbit camera (editor-style)
let mut orbit = OrbitCamera::new(Vec3::ZERO, 10.0);
orbit.orbit(mouse_dx, mouse_dy);  // drag to rotate
orbit.zoom(scroll_delta);          // scroll to zoom
orbit.pan(dx, dy);                 // middle-drag to pan
let view = orbit.view_matrix();
```

### 2D — Sprites & TileMap

```rust
use alice_game_engine::scene2d::*;
use alice_game_engine::math::{Vec2, Color};

// Sprite
let mut player = Sprite2D::new(0); // texture_id = 0
player.position = Vec2::new(100.0, 200.0);
player.z_order = 10;

// TileMap
let mut tilemap = TileMap::new(16, 16, 32.0);
tilemap.set(3, 4, TileDef { id: 1, solid: true });
let (tx, ty) = tilemap.world_to_tile(Vec2::new(110.0, 140.0));
let solid = tilemap.is_solid(3, 4);

// 2D collision
let bodies = vec![
    Body2D::new(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0), 1.0),
    Body2D::new(Vec2::new(1.5, 0.0), Vec2::new(1.0, 1.0), 1.0),
];
let contacts = detect_2d_collisions(&bodies);

// Scene with z-order rendering
let mut scene = Scene2D::new();
scene.add(player);
let order = scene.render_order(); // sorted by z_order
```

### Asset Loading & Import

```rust
use alice_game_engine::asset::*;
use alice_game_engine::import::*;

// OBJ mesh loading
let obj_text = std::fs::read_to_string("model.obj").unwrap();
let mesh = parse_obj("my_model", &obj_text);
println!("{} triangles", mesh.triangle_count());

// Detect file format
assert_eq!(detect_format("level.unity"), ProjectFormat::UnityScene);
assert_eq!(detect_format("mesh.uasset"), ProjectFormat::UnrealAsset);

// Unity scene import
let yaml = std::fs::read_to_string("scene.unity").unwrap();
let objects = parse_unity_yaml(&yaml);
let nodes = unity_scene_to_nodes(&objects); // → Vec<Node> for scene graph

// SDF from JSON
let sdf_node = load_sdf_json(r#"{"Primitive":{"Sphere":{"radius":1.5}}}"#).unwrap();
```

### Scripting — Events & Timers

```rust
use alice_game_engine::scripting::*;

// Event bus
let mut bus = EventBus::new();
let sub_id = bus.subscribe("player_died");
bus.publish(Event::with_int("player_died", 42));
for event in bus.drain() {
    println!("{}: {:?}", event.name, event.payload);
}

// Timers
let mut timers = TimerManager::new();
timers.add(Timer::new("respawn", 3.0, TimerMode::OneShot));
timers.add(Timer::new("tick", 0.5, TimerMode::Repeating));
let fired = timers.update(0.6); // → ["tick"]
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
| `ffi` | C/C++/C# FFI bindings |
| `python` | Python (PyO3) bindings |
| `godot` | Godot GDExtension bindings |
| `full` | All runtime features (excludes ffi/python/godot) |

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

## ALICE Quality Standard (KARIKARI)

This crate follows the ALICE-KARIKARI optimization methodology:

- **Division exorcism** — No `/` in hot paths; use `mul_add` or reciprocal multiply
- **Branchless** — `mask.blend()` over `if/else` in SIMD paths
- **FMA** — `a.mul_add(b, c)` over `a * b + c`
- **SoA layout** — Struct-of-arrays over array-of-structs for cache efficiency
- **Sweep-and-prune** — O(n log n) broadphase, never O(n^2)
- **Test density** — 20+ tests per KLOC (current: 38.4/KLOC)
- **Release profile** — `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`

## Quality

```bash
cargo test --features full        # 688 tests, 0 failures
cargo clippy --features full -- -W clippy::all  # 0 warnings
cargo fmt -- --check              # 0 diffs
cargo doc --no-deps --features full  # 0 warnings
```

## License

Dual licensed under **MIT** and **Commercial**.

- **MIT** — Free for open source and commercial use under $100K/year with attribution. See [LICENSE](LICENSE).
- **Commercial** — Required for proprietary SaaS or high-revenue products. See [LICENSE-COMMERCIAL](LICENSE-COMMERCIAL).

Contact: sakamoro@alicelaw.net
