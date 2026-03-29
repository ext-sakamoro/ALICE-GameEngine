# ALICE-GameEngine — AI Agent Reference

This file helps AI code assistants (Claude, Gemini, Cursor, Copilot) work with this engine.

## What is this?

Rust game engine. Mesh + SDF hybrid. 40 modules, 787 tests. wgpu renderer.
Verse (UE6) compatible gameplay primitives. MCP server for AI agent control.
991 free SDF assets from [Open-Source-SDF-Assets](https://github.com/ext-sakamoro/Open-Source-SDF-Assets).

## How to build

```bash
cargo test --features full    # full build + test
cargo test                    # minimal build (no GPU/audio/sdf)
cargo run --example spinning_cube --features full  # window demo
cargo run --bin mcp_server    # MCP server (stdio JSON-RPC)
```

## Routing: "I want to..." → module

| Goal | Use |
|------|-----|
| Open a window | `app::run_windowed(WindowConfig, EngineConfig, Box<dyn AppCallbacks>)` |
| No window (headless) | `easy::GameBuilder::new("name").build()` → `game.run_headless(frames)` |
| Add 3D objects | `scene_graph::SceneGraph::add(Node::new("name", NodeKind::Mesh(...)))` |
| Add SDF volume | `NodeKind::Sdf(SdfData { sdf_json: "...", ... })` |
| Load free SDF asset | `sdf_assets::load_asdf_file("path.asdf.json")` → `add_asdf_to_scene()` |
| SDF → triangle mesh | `sdf::marching_cubes(node, min, max, res)` or `marching_cubes_parallel()` |
| Physics simulation | `physics3d::PhysicsWorld` — `.add_body()`, `.step(dt)` (Verlet + CCD) |
| Sound playback | `audio::AudioSource::set_pcm(samples)` or `.set_sample_provider()` |
| Key/mouse/gamepad | `input::ActionMap::bind_action("jump", InputSource::Key(Key::Space))` |
| Animate | `animation::AnimationClip` + `Track` + `AnimationPlayer` |
| State machine | `animation::StateMachine::new("idle")` → `.trigger("move")` |
| NPC dialogue (LLM) | `llm::NpcContext::respond("input", &llm_provider)` |
| Procedural content | `llm::ContentGenRequest::new(ContentType::QuestDescription, "desc")` |
| Ability/buff system | `ability::AbilitySystem::activate("fireball", &mut attrs)` |
| A* pathfinding | `navmesh::a_star(&mesh, start_tri, goal_tri)` |
| Crowd separation | `navmesh::crowd_separation(&mut agents, radius, strength)` |
| 2D sprites/tiles | `scene2d::Sprite2D`, `TileMap`, `detect_2d_collisions()` |
| MCP remote control | `mcp::McpHandler::handle(&request, &mut ctx)` |
| MCP binary | `cargo run --bin mcp_server` (stdio JSON-RPC) |
| Import Unity scene | `import::parse_unity_yaml(&yaml)` → `unity_scene_to_nodes()` |
| Import UE5 asset | `import::parse_uasset_header(&bytes)` |
| SIMD batch eval | `simd_eval::eval_sphere_batch(&points, radius)` (8-wide f32x8) |
| 128-bit position | `fix128::Fix128Vec3::accumulate_f32(dx, dy, dz)` |
| Plug in ALICE-SDF | `ctx.set_sdf_evaluator(Box::new(impl SdfEvaluator))` |
| Plug in ALICE-Physics | `ctx.set_collision_provider(Box::new(impl CollisionProvider))` |
| Plug in ALICE-Voice | `source.set_sample_provider(Box::new(impl AudioSampleProvider))` |
| Plugin system | `ctx.plugins.register(Box::new(impl Plugin))` |
| Events/timers | `scripting::EventBus`, `Timer` |
| Verse failure context | `verse::decide(condition)` → `Failable<T>` |
| Verse transaction | `verse::Transaction::execute(&mut state, \|s\| { ... })` |
| Verse reactive var | `verse::LiveVar::new(value)` → `.set()`, `.is_dirty()` |
| Verse coroutine | `verse::Coroutine::new("name")` → `TickExecutor::spawn()` |
| Verse events | `verse::StickyEvent<T>`, `SubscribableEvent<T>` |
| LOD selection | `lod::select_lods(&positions, &radii, &groups, cam, fov, h, threshold)` |
| Frustum culling | `scene_graph::SceneGraph::frustum_cull(&frustum)` |
| Camera (FPS) | `camera_controller::FpsCamera::new(pos)` → `.look()`, `.move_local()` |
| Camera (Orbit) | `camera_controller::OrbitCamera::new(target, dist)` → `.orbit()`, `.zoom()` |
| WAV export | `app::export_wav(&sample_buffer)` |

## 5-line game

```rust
use alice_game_engine::easy::*;
let mut game = GameBuilder::new("Demo").build();
game.add_camera();
game.add_cube(0.0, 1.0, -5.0);
game.add_light(0.0, 10.0, 0.0);
game.run_headless(300);
```

## Load SDF asset (991 free models)

```rust
use alice_game_engine::sdf_assets::*;
let asdf = load_asdf_file("collections/pm-momuspark/Bench_01_Art.asdf.json").unwrap();
add_asdf_to_scene(&mut scene, "bench", &asdf, Vec3::new(5.0, 0.0, 0.0));
```

Assets: <https://github.com/ext-sakamoro/Open-Source-SDF-Assets> (991 .asdf.json files)

## Verse gameplay (UE6 compatible)

```rust
use alice_game_engine::verse::*;

// Failure context (Verse <decides>)
let result: Failable<()> = decide(gold >= 50);

// Transaction with rollback (Verse <transacts>)
Transaction::execute(&mut gold, |g| { *g -= 50; decide(*g >= 0) });

// Reactive variable
let mut hp = LiveVar::new(100);
hp.set(80);
assert!(hp.is_dirty());

// Coroutine
let mut exec = TickExecutor::new();
exec.spawn(Coroutine::new("patrol"));
exec.tick(); // advances one frame
```

## MCP tools (for AI agents)

```bash
# Register with Claude Code
claude mcp add --transport stdio alice-engine -- cargo run --bin mcp_server
```

| Tool | Action |
|------|--------|
| `scene_list` | List all scene nodes |
| `scene_add_node` | Add mesh/sdf/light/camera/empty |
| `scene_remove_node` | Remove node by ID |
| `scene_set_transform` | Move a node |
| `engine_status` | Frame count, time, node count |
| `physics_step` | Step N physics frames |

## NPC AI (local LLM)

```rust
use alice_game_engine::llm::*;
let llm = MockLlm::new("Welcome to the village.");
let mut npc = NpcContext::new("Guard", "a stern castle guard");
let reply = npc.respond("Hello!", &llm).unwrap();
```

Replace `MockLlm` with a real `LlmProvider` impl (llama.cpp FFI, ONNX, ALICE-Train).

## Key types

```
GameBuilder → Game                       # easy.rs
Engine + EngineConfig + EngineContext     # engine.rs
SceneGraph + Node + NodeId + NodeKind    # scene_graph.rs
PhysicsWorld + RigidBody + Contact3D     # physics3d.rs
AudioEngine + AudioSource + AudioBus     # audio.rs
ActionMap + InputState + Key             # input.rs
AnimationClip + AnimationPlayer          # animation.rs
StateMachine + Transition                # animation.rs
AbilitySystem + Ability + GameplayEffect # ability.rs
NpcContext + LlmProvider + LlmRequest    # llm.rs
McpHandler + McpRequest + McpResponse    # mcp.rs
Failable<T> + Transaction + LiveVar      # verse.rs
Coroutine + TickExecutor                 # verse.rs
StickyEvent<T> + SubscribableEvent<T>    # verse.rs
AsdfFile + load_asdf + add_asdf_to_scene # sdf_assets.rs
Vec3 + Mat4 + Quat + Color              # math.rs
Fix128 + Fix128Vec3                      # fix128.rs
Vec3x8 + eval_sphere_batch              # simd_eval.rs
```

## Examples

| Example | Features | Description |
|---------|----------|-------------|
| `hello_engine` | full | Headless 300 frames |
| `spinning_cube` | full | Windowed GPU cube |
| `pong` | full | 2D pong with AI paddle |
| `physics_sandbox` | full | 10 balls, damping, sleeping |
| `npc_chat` | (none) | LLM NPC dialogue |
| `mcp_controlled` | (none) | MCP-driven scene setup |

## Templates (copy to start a game)

| Template | Genre | Key modules |
|----------|-------|-------------|
| `templates/platformer.rs` | 2D action | scene2d, gravity, coin collect |
| `templates/rpg.rs` | RPG | ability (GAS), llm (NPC), verse (transaction), tilemap |
| `templates/fps.rs` | FPS shooter | scene_graph, physics3d, camera_controller |
| `templates/puzzle.rs` | Puzzle (Sokoban) | verse (Transaction undo) |
| `templates/sandbox.rs` | Physics sandbox | physics3d, SDF objects |
| `templates/visual_novel.rs` | Visual novel | llm (NPC), verse (branching), scripting |
| `templates/strategy.rs` | Turn-based strategy | ability (units), scene2d (tilemap) |
| `templates/tower_defense.rs` | Tower defense | navmesh (enemy path), ability (towers), timer (waves) |
| `templates/racing.rs` | Racing | physics3d (car), camera_controller (orbit) |
| `templates/space_trader.rs` | Space trader | ability (cargo), llm (NPC trade), scripting |
| `templates/rhythm.rs` | Rhythm game | audio, scripting (BPM timer), combo system |

Usage: `cp templates/rpg.rs examples/my_game.rs && cargo run --example my_game --features full`

## Feature flags

- `full` = window + sdf + audio + ui + particles + navmesh
- `window` = winit + wgpu
- `gpu` = wgpu only (no window)
- `ffi` / `python` / `godot` = future binding targets
- Default (no features) = core modules only, no GPU

## Notes for users

- `ComponentStore<T>` requires `T: Clone`
- Physics uses Verlet integration (set `prev_position` for initial velocity)
- `dense_slice()` on `ComponentStore` for batch/SIMD access
