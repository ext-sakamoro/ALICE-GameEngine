# ALICE-GameEngine

**v0.6.0** — Hybrid mesh + SDF game engine in Rust. 44 modules,
**793 lib tests** (default) / **1,007** (full features), wgpu deferred
renderer (Vulkan / Metal / DX12 / WebGPU), turn-based RPG runtime, 3D
action combat, no-code event scripting, hierarchical pathfinding, HD-2D
post-process (naga-validated + offscreen draw verified on Mac Metal).

[日本語ドキュメント](README.ja.md) · [Changelog](CHANGELOG.md)

## Install

```toml
# Cargo.toml
[dependencies]
alice-game-engine = "0.6"

# Or for the whole renderer + windowing + everything:
alice-game-engine = { version = "0.6", features = ["full"] }
```

Feature flags pick what you compile:

| Feature | What you get |
|---------|--------------|
| (default) | ECS, scene graph, math, scripting, battle, action combat, animation, ability, navmesh stubs |
| `window` | winit window + wgpu render loop (implies `gpu`) |
| `gpu` | wgpu device + shader pipeline |
| `sdf` | SDF primitives + Marching Cubes |
| `audio` | HRTF + bus effects + MusicTrack / ReverbZone |
| `ui` | retained-mode widgets |
| `particles` | CPU emitter + curl noise + TrailEmitter |
| `navmesh` | A*, hierarchical A*, grid → NavMesh |
| `full` | everything above |
| `ffi` / `python` / `godot` | FFI bindings |

## What's in the box

- **Hybrid scene graph** — meshes and SDF volumes coexist in the same tree
- **Deferred wgpu renderer** with GBuffer, RenderGraph, debug overlay
- **Verlet physics** + sweep-and-prune broadphase + SDF CCD
- **HRTF audio** with bus effects, `MusicTrack` BGM cross-fade, `ReverbZone`
- **Turn-based RPG runtime** — `TurnBattleRunner` (speed-ordered, grid +
  attack-range) + 13 serializable `EventCommand`s + 5 advanced flow
  (Cutscene/Parallel/Repeat/LoopUntil/LlmDialogue) + `EventScriptDef` JSON
- **3D action combat** — `Hitbox` / `Hurtbox` (sphere + capsule),
  `ComboSystem` with input window, `LockOn` (cone), `HitStop` for weighty hits
- **Animation** — Keyframes, `StateMachine`, `BlendTree1D`, 2-bone analytic IK
- **Pathfinding** — A*, **hierarchical A*** with cluster planning,
  grid-to-NavMesh auto-generation
- **Particles** — CPU emitter + **curl-noise force field** + `TrailEmitter`
- **Deferred decals** — OBB projector node (`NodeKind::Decal`), depth-
  reconstruct WGSL, three blend modes (alpha / multiply / additive),
  `cargo run --example decal_demo`
- **HD-2D / post-process** — `Sprite3D` billboard + WGSL templates for
  pbr-sprite, **SSGI** (16 samples) and **SMAA** (all naga-validated)
- **Multiplayer scaffolding** — `LoopbackTransport` implements
  `bridge::NetworkTransport`, demoed in `examples/multiplayer_battle`
- **`bridge::*` traits** for plugging in ALICE-SDF, ALICE-Physics,
  ALICE-Audio, ALICE-Text and your own back-ends
- **XR layer** (pure-Rust, no OpenXR dep) with MockProvider + StereoWindow

## Quick Start

### Five-line headless game

```rust
use alice_game_engine::easy::*;

let mut game = GameBuilder::new("My Game").build();
game.add_camera();
game.add_cube(0.0, 1.0, -5.0);
game.add_sphere_sdf(3.0, 0.0, 0.0, 1.0);
game.add_light(0.0, 10.0, 0.0);
game.run_headless(300);
```

### Tiny RPG turn — copy / paste / run

```rust
use alice_game_engine::ability::{Attribute, AttributeSet};
use alice_game_engine::battle::{
    BattleAction, BattleCommand, BattleResult, Battler, Party, RandomAi, Team,
    TurnBattleRunner,
};

fn battler(name: &str, hp: f32, atk: f32, spd: f32, team: Team) -> Battler {
    let mut a = AttributeSet::new();
    a.add(Attribute::new("hp", hp, 0.0, hp));
    a.add(Attribute::new("atk", atk, 0.0, 999.0));
    a.add(Attribute::new("def", 0.0, 0.0, 999.0));
    a.add(Attribute::new("speed", spd, 0.0, 999.0));
    Battler::new(name, a, team)
}

fn main() {
    let allies = Party::new(vec![battler("Hero", 60.0, 12.0, 10.0, Team::Ally)]);
    let enemies = Party::new(vec![battler("Slime", 25.0, 4.0, 4.0, Team::Enemy)]);
    let mut runner = TurnBattleRunner::new(allies, enemies);
    let mut ai = RandomAi::new(1);

    while runner.result() == BattleResult::Ongoing {
        let cmds = vec![BattleCommand {
            actor_idx: 0,
            action: BattleAction::Attack { target_idx: 0 },
        }];
        runner.run_turn(cmds, &mut ai);
    }
    assert_eq!(runner.result(), BattleResult::Win);
}
```

### Full control with the prelude

```rust
use alice_game_engine::prelude::*;
```

## Windowed Example

```bash
cargo run --example spinning_cube --features full
```

Opens a window with a rotating colored cube rendered via wgpu. Press Escape to exit.

## Examples — what runs out of the box

```bash
cargo run --example rpg                # turn-based: NPC choice → battle → reward
cargo run --example multiplayer_battle # two peers via LoopbackTransport
cargo run --example visual_novel       # EventScript-driven branching story
cargo run --example fps_combat         # LockOn cone + hitscan + HitStop
cargo run --example decal_demo         # 5 deferred decals on wall + floor (headless)
cargo run --example platformer_action --features particles
                                       # sword Hitbox + Curl-Noise dash trail
cargo run --example spinning_cube --features full
cargo run --example physics_sandbox --features full
```

Every example fits on one screen — copy `templates/<name>.rs` and rename
the example entry in `Cargo.toml` to start a new project.

## Turn-Based RPG

The full RPG starter (`templates/rpg.rs`):

```bash
cargo run --example rpg
```

Output (excerpt):

```
Elder: Welcome, traveler. A slime has made the cave its home.
CHOICE: Will you help us?
  (1) Accept  (2) Decline
> Accept
[switch quest_active = true]
[Battle begins: cave_slime]

=== Battle: Hero vs Slime ===
  Hero attacks Slime for 13 damage. (12 HP left)
  Slime attacks Hero for 2 damage. (58 HP left)
  Hero attacks Slime for 13 damage. (0 HP left)
  Slime is defeated!

Elder: Take this potion.
[has_item potion >= 2 ? true]
```

The template composes three pieces:

1. **`battle::TurnBattleRunner`** — speed-ordered turn loop with `Attack /
   UseAbility / Defend / Flee` actions, `BattleAi` trait, default `RandomAi`
2. **`scripting::EventScript`** — sequence of `EventCommand`s. 13 built-ins:
   `Message`, `ChangeAttr`, `Wait`, `Branch`, `GiveItem`, `BeginBattle`,
   `Choice`, `SetVar`, `IfVar`, `SetSwitch`, `HasItem`, `TakeItem`,
   `MapTransition`, plus the advanced flow controls `Cutscene`, `Parallel`,
   `Repeat`, `LoopUntil`, `LlmDialogue` (LLM-backed NPC dialogue)
3. **`ability::AbilitySystem`** — UE5 GAS-inspired attributes, gameplay
   effects, cooldowns

Hook a themed world into the engine via `EngineContext::set_world_provider`
and the `bridge::WorldProvider` trait — implement once and reuse the same
battle / event runtime across any setting.

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

## Cookbook — frequent recipes

### Run a no-code event script

```rust
use alice_game_engine::scripting::*;
use alice_game_engine::ability::AttributeSet;

let mut script = EventScript::new();
script.push(Box::new(MessageCommand::new("Elder", "Welcome.")));
script.push(Box::new(ChoiceCommand::pick(
    "Accept the quest?",
    vec!["Yes".into(), "No".into()],
    "answer", 0,
)));
script.push(Box::new(IfVarCommand::new(
    "answer", Comparison::Eq, 0,
    Box::new(GiveItemCommand::new("compass", 1)),
    Box::new(MessageCommand::new("Elder", "Some other time then.")),
)));

let mut vars = ScriptVars::new();
let mut attrs = AttributeSet::new();
let mut log = Vec::new();
while !script.is_done() {
    let mut ctx = EventContext { vars: &mut vars, attrs: Some(&mut attrs),
        log: &mut log, elapsed_ticks: 0 };
    script.step(&mut ctx);
}
```

### Serialize a quest to JSON for editor / hot reload

```rust
use alice_game_engine::scripting::{EventCommandDef, EventScriptDef};

let def = EventScriptDef { commands: vec![
    EventCommandDef::Message { speaker: "Lyra".into(), text: "Welcome.".into() },
    EventCommandDef::GiveItem { item: "compass".into(), count: 1 },
]};
let json = def.to_json().unwrap();
let parsed = EventScriptDef::from_json(&json).unwrap();
let mut script = parsed.build();   // ready to step()
```

### Wire a custom themed world

```rust
use alice_game_engine::bridge::{WorldProvider, WorldEnvironment, ZoneId,
                                 SpawnPose, TeleportResult};
use alice_game_engine::engine::EngineContext;
use alice_game_engine::math::Vec3;

struct MyWorld { pos: Vec3, day: f32 }
impl WorldProvider for MyWorld {
    fn step(&mut self, dt: f32) { self.day = (self.day + dt * 0.01).fract(); }
    fn camera_position(&self) -> Vec3 { self.pos }
    fn camera_yaw(&self) -> f32 { 0.0 }
    fn camera_pitch(&self) -> f32 { 0.0 }
    fn look_delta(&mut self, _: f32, _: f32) {}
    fn move_intent(&mut self, dir: Vec3, _: bool) { self.pos = self.pos + dir; }
    fn environment(&self) -> WorldEnvironment {
        WorldEnvironment { day_phase: self.day, ..Default::default() }
    }
    fn current_zone(&self) -> ZoneId { ZoneId(0) }
    fn zone_spawn(&self, _: ZoneId) -> Option<SpawnPose> { None }
    fn teleport_to(&mut self, _: ZoneId) -> TeleportResult { TeleportResult::Started }
}

let mut ctx = EngineContext::default();
ctx.set_world_provider(Box::new(MyWorld { pos: Vec3::ZERO, day: 0.0 }));
```

### Add 3D action-combat hit feedback

```rust
use alice_game_engine::action_combat::{
    resolve_hits, ColliderShape, HitStop, Hitbox, Hurtbox,
};
use alice_game_engine::math::Vec3;

let mut hits = vec![{
    let mut h = Hitbox::new(1, 100,
        ColliderShape::Sphere { center: Vec3::ZERO, radius: 1.0 },
        "heavy_swipe");
    h.damage = 22.0;
    h.hitstop_frames = 4;  // "weight" on impact
    h
}];
let mut hurts = vec![Hurtbox::new(2, 200,
    ColliderShape::Sphere { center: Vec3::new(0.5, 0.0, 0.0), radius: 1.0 })];

let events = resolve_hits(&mut hits, &mut hurts);
let mut hit_stop = HitStop::default();
hit_stop.trigger(events.iter().map(|e| e.hitstop_frames).max().unwrap_or(0));
// While hit_stop.is_active(), hit_stop.time_scale() == 0.0 → pause sim.
```

### Multiplayer via the LoopbackTransport (or any `NetworkTransport`)

```rust
use alice_game_engine::network::LoopbackTransport;
use alice_game_engine::bridge::NetworkTransport;

let (mut host, mut client) = LoopbackTransport::pair(1, 2);
client.send_to(1, b"hello host").unwrap();
let inbox = host.recv();
assert_eq!(inbox[0].1, b"hello host".to_vec());
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

### Battle — Turn-Based Runner

```rust
use alice_game_engine::ability::{Attribute, AttributeSet};
use alice_game_engine::battle::{
    BattleAction, BattleCommand, BattleResult, Battler, Party, RandomAi, Team,
    TurnBattleRunner,
};

fn battler(name: &str, hp: f32, atk: f32, speed: f32, team: Team) -> Battler {
    let mut a = AttributeSet::new();
    a.add(Attribute::new("hp", hp, 0.0, hp));
    a.add(Attribute::new("atk", atk, 0.0, 999.0));
    a.add(Attribute::new("def", 0.0, 0.0, 999.0));
    a.add(Attribute::new("speed", speed, 0.0, 999.0));
    Battler::new(name, a, team)
}

let allies  = Party::new(vec![battler("Hero", 80.0, 12.0, 10.0, Team::Ally)]);
let enemies = Party::new(vec![battler("Slime", 25.0, 4.0, 4.0, Team::Enemy)]);
let mut runner = TurnBattleRunner::new(allies, enemies);
let mut ai = RandomAi::new(1);

loop {
    let cmds = vec![BattleCommand {
        actor_idx: 0,
        action: BattleAction::Attack { target_idx: 0 },
    }];
    match runner.run_turn(cmds, &mut ai) {
        BattleResult::Ongoing => continue,
        _ => break,
    }
}
assert_eq!(runner.result(), BattleResult::Win);
```

The runner applies `Defend` first (halving incoming damage), tries `Flee`
(succeeds if total ally speed > enemy), then resolves remaining actions in
descending `speed` order. Implement `BattleAi` to plug in your own enemy
strategy (`RandomAi` is provided out of the box).

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

### Scripting — Events, Timers, and no-code EventCommands

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

// no-code RPG events — drop commands into an EventScript
let mut script = EventScript::new();
script.push(Box::new(MessageCommand::new("Elder", "Hello, traveler.")));
script.push(Box::new(ChoiceCommand::pick(
    "Take the quest?",
    vec!["Accept".into(), "Decline".into()],
    "answer",
    0, // tests / starter templates: hard-code 'Accept'
)));
script.push(Box::new(IfVarCommand::new(
    "answer",
    Comparison::Eq,
    0,
    Box::new(SetSwitchCommand::new("quest_active", true)),
    Box::new(MessageCommand::new("Elder", "Maybe next time.")),
)));
script.push(Box::new(GiveItemCommand::new("potion", 2)));
script.push(Box::new(HasItemCommand::new("potion", 1, "has_potion")));
// CutsceneCommand / ParallelCommand / RepeatCommand / LoopUntilCommand /
// LlmDialogueCommand are also available for richer flows.

let mut vars = ScriptVars::new();
let mut log = Vec::new();
let mut ctx = EventContext { vars: &mut vars, attrs: None,
    log: &mut log, elapsed_ticks: 0 };
while !script.is_done() { script.step(&mut ctx); }
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
| ecs | 1,872 | 107 | SoA sparse-set ECS, spatial hash grid broadphase |
| scene_graph | 1,277 | 43 | Mesh+SDF hybrid node tree, AABB3, frustum culling, reparenting |
| sdf | 1,243 | 39 | 7 primitives, 6 boolean ops, Marching Cubes (256 tables), Rayon parallel MC, sphere trace, SDF collider |
| audio | 1,240 | 47 | Bus effects (ping-pong), HRTF, PCM playback, spatial panning, WAV export, **MusicTrack** (BGM cross-fade), **ReverbZone** (4 presets) |
| ui | 951 | 30 | Retained-mode widgets, vertical+horizontal layout, focus management, theme |
| physics3d | 815 | 36 | Verlet integration, sweep-and-prune broadphase, impulse solver, SDF CCD, damping, sleeping |
| math | 776 | 30 | Vec2/3/4, Mat4, Quat, Color, perspective+orthographic projection |
| renderer | 773 | 25 | Deferred GBuffer, RenderGraph (Kahn topo sort), DebugRenderer |
| app | 715 | 13 | `run_windowed()` (winit+wgpu), `HeadlessRunner`, WAV export |
| navmesh | 960 | 27 | NavMesh, A* pathfinding, SDF avoidance, crowd separation (RVO), **grid→NavMesh auto-generation**, **hierarchical A*** with cluster planning |
| animation | 950 | 42 | Keyframe (Linear/Step/Cubic), Track, Clip, Player, StateMachine, **2-bone IK solver**, **BlendTree1D** |
| input | 587 | 16 | Keyboard/Mouse/Gamepad, ActionMap, axis binding, just_pressed |
| scripting | 2,140 | 68 | EventBus (pub/sub), Timer/TimerManager, ScriptVars, 13 EventCommands + advanced flow (Cutscene/Parallel/Repeat/LoopUntil/LlmDialogue), EventScript runner, **EventScriptDef** serializable definition for no-code editors / hot reload |
| scene2d | 532 | 21 | Sprite2D, TileMap, Aabb2, Body2D, Physics2D, z-order |
| gpu | 521 | 10 | wgpu Device/Queue/Surface, render_mesh(), create_texture_rgba8() |
| ability | 501 | 16 | Gameplay Ability System: attributes, effects, cooldowns, modifiers |
| battle | 950 | 19 | Turn-based runner, Battler/Party/BattleAction, Attack/Defend/Flee/UseAbility/Move, BattleAi trait + RandomAi, speed-ordered execution, GridCell + Chebyshev attack range |
| action_combat | 600 | 14 | 3D action combat — Hitbox/Hurtbox (sphere+capsule), resolve_hits, ComboSystem with input window, LockOn (cone), HitStop |
| hd2d_postfx | 320 | 11 | Sprite3D billboard + WGSL templates (hd2d_sprite/ssgi/smaa), naga-validated |
| shader | 439 | 15 | ShaderCache, 5 built-in WGSL shaders |
| particle | 720 | 22 | CPU emitter, multi-shape (Point/Sphere/Box/Cone), gravity, **curl-noise force field**, **TrailEmitter** with max-len cap |
| import | 409 | 17 | Unity YAML scene parser, UE5 .uasset header parser, format detection |
| texture | 400 | 18 | TextureAsset, mipmap, checkerboard, GpuTextureDesc, SamplerDesc |
| fix128 | 353 | 19 | 128-bit fixed-point (i128, 40 frac bits), Fix128Vec3, long-duration precision |
| render_pipeline | 354 | 13 | FrameData extraction, MvpUniforms, MaterialUniforms, PipelineState |
| engine | 354 | 11 | Game loop, System trait, fixed timestep, interpolation alpha |
| asset | 336 | 13 | OBJ parser, glTF header, SDF JSON loader, asset type detection |
| collision | 333 | 10 | GJK convex intersection, SDF-mesh hybrid narrowphase |
| camera_controller | 322 | 19 | FPS camera (WASD+mouse), Orbit camera (rotate/zoom/pan) |
| resource | 309 | 12 | Async resource manager, ref counting, load state |
| easy | 295 | 9 | GameBuilder + Game high-level API (5-line game setup) |
| query | 293 | 11 | Typed ECS queries (query2/3), filter, SystemScheduler |
| gpu_mesh | 280 | 9 | GpuMeshDesc, VertexLayout, DrawCommand/DrawQueue |
| simd_eval | 268 | 8 | SIMD 8-wide SDF evaluation (wide f32x8), Vec3x8, batch eval |
| lod | 264 | 13 | LOD group selection, screen coverage, batch culling |
| window | 263 | 15 | WindowConfig, key mapping, FrameTimer |
| bridge | 642 | 12 | ALICE-xxx integration traits (`SdfEvaluator`, `CollisionProvider`, `AudioSampleProvider`, `WorldProvider`, `TextProcessor`, `AnimationProvider`, `NetworkTransport`, `SdfFontProvider`, ...), Plugin system |
| **Total** | **22,300** | **793** | |

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

## ALICE Eco-System Integration

The `bridge` module exposes trait surfaces for plugging in ALICE-xxx crates
or your own implementations:

| Trait | Hooked to (examples) |
|-------|----------------------|
| `SdfEvaluator` | ALICE-SDF `CompiledSdf` |
| `CollisionProvider` | ALICE-Physics |
| `AudioSampleProvider` | ALICE-Audio decoders |
| `MeshProvider` | ALICE-SDF Marching Cubes output |
| `ShaderTranspiler` | ALICE-SDF HLSL/GLSL transpiler |
| `WorldProvider` | themed-world back-end (ZoneId/Weather/Teleport) |
| `TextProcessor` | **ALICE-Text** (`AliceTextProcessor` adapter for log/dialogue compression) |
| `AnimationProvider` | ALICE-Animation |
| `SkeletonProvider` | skeletal animation backend |
| `SdfFontProvider` | ALICE-Font |
| `NetworkTransport` | ALICE-Sync or custom transports |
| `StreamingProtocol` | ALICE-Streaming-Protocol |
| `UiRenderer` | custom UI back-end |
| `Plugin` | per-frame extension hook |

The engine crate is dep-free of the ALICE-xxx stack — adapter
implementations live in downstream consumer crates so each game can pick
the back-ends it actually needs.

## Multiplayer

```bash
cargo run --example multiplayer_battle
```

A two-peer `LoopbackTransport` (in-memory) shows host + client coordinating
the same `TurnBattleRunner`. Swap the transport for an ALICE-Sync or
WebRTC back-end in production — `bridge::NetworkTransport` is the only
contract the runner sees.

## 3D Action Combat

For real-time character-action games (DMC / Souls / Sekiro style):

```rust
use alice_game_engine::action_combat::{
    ColliderShape, HitStop, Hitbox, Hurtbox, LockOn, LockOnCandidate, resolve_hits,
};
use alice_game_engine::math::Vec3;

let mut hits = vec![{
    let mut h = Hitbox::new(1, 100, ColliderShape::Sphere {
        center: Vec3::new(0.0, 0.0, 0.0), radius: 1.0,
    }, "heavy_swipe");
    h.damage = 22.0;
    h.hitstop_frames = 4;
    h
}];
let mut hurts = vec![Hurtbox::new(2, 200, ColliderShape::Sphere {
    center: Vec3::new(0.5, 0.0, 0.0), radius: 1.0,
})];

let events = resolve_hits(&mut hits, &mut hurts);
for e in &events {
    println!("{} hit {} for {} dmg from {}", e.attacker, e.target, e.damage, e.source);
}

// HitStop gives weighty impact:
let mut hs = HitStop::default();
hs.trigger(events.iter().map(|e| e.hitstop_frames).max().unwrap_or(0));
// While hs.is_active(), use hs.time_scale() (== 0.0) to freeze updates.

// LockOn picks a target inside a cone:
let mut lock = LockOn::new(15.0, 0.6 /* ~34° half-angle */);
let cands = vec![LockOnCandidate { entity: 200, position: Vec3::new(0.0, 0.0, 5.0) }];
let target = lock.acquire(Vec3::ZERO, Vec3::new(0.0, 0.0, 1.0), &cands);
```

## Quality

```bash
cargo test --features full        # 1,007 lib tests
cargo clippy --lib -- -W clippy::all -W clippy::pedantic -W clippy::nursery
                                  # 0 lib warnings (see lib.rs allow list)
cargo fmt -- --check              # 0 diffs
# Optional, with a working GPU adapter:
cargo test --features gpu -- --ignored gpu
                                  # 3 WGSL shaders load + offscreen draw verified
```

## FAQ

### Why two `bridge::` audio providers?

`AudioSampleProvider` is the engine's trait. Downstream consumers (e.g. a
private themed-world crate) can implement it with **ALICE-Audio**
(procedural BGM via `gen_sine` / `gen_noise`) or **ALICE-Voice**
(formant-synthesised dialogue). The engine itself stays dep-free; only
the consumer crate pulls those backends in.

### Is `bridge::WorldProvider` a public extension point?

Yes. Implement it once for your world's camera / weather / zones /
teleport, inject with `EngineContext::set_world_provider(Box::new(...))`,
and reuse the same battle / event / animation runtime across any
setting.

### How do I add a new EventCommand?

Implement `EventCommand` (a sync `execute(&mut self, ctx)
-> CommandStatus`) and push it into your `EventScript`. For editor /
hot-reload support, also add a variant to `EventCommandDef` and wire
`into_command()`.

### How do I run multiplayer over a real network?

Replace `LoopbackTransport::pair` with any other implementation of
`bridge::NetworkTransport`. Async transports can be wrapped behind a
private tokio runtime in a downstream adapter crate (see ALICE-Sync
integration sketches in `templates/multiplayer_battle.rs`).

## License

Dual licensed under **MIT** and **Commercial**.

- **MIT** — Free for open source and commercial use under $100K/year with attribution. See [LICENSE](LICENSE).
- **Commercial** — Required for proprietary SaaS or high-revenue products. See [LICENSE-COMMERCIAL](LICENSE-COMMERCIAL).

Contact: sakamoro@alicelaw.net
