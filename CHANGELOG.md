# Changelog

## [Unreleased]

### New modules

- **`decal`** — Deferred decal projection (Wicked-inspired). `DecalData`
  (albedo / normal / color / opacity / layer_mask / blend_mode),
  `DecalBlendMode` (AlphaBlend / Multiply / Additive with stable shader
  IDs), `DecalDraw` with pre-computed inverse world matrix and OBB→AABB
  helper for frustum culling.

### Existing modules — additions

- **`joint`** — Three new `JointKind` variants (Wicked-inspired
  `ConstraintComponent`):
  - `Slider` (prismatic, 1-axis slide with min/max offset clamp)
  - `Fixed` (weld, holds `B - A == offset`)
  - `ConeTwist` (humanoid hip / shoulder, swing half-angle clamp;
    `twist_half_angle` stored but not enforced by the position-based
    solver)
  Constructors: `Joint::slider` / `Joint::fixed` / `Joint::cone_twist`.
  Example: `cargo run --example constraint_demo` (piston + weld + cone
  twist scenes).

- **`light_culling`** (feature `gpu`) — Tiled (Forward+ style) CPU-side
  light culler. `LightCullingConfig` (tile_size / max_lights_per_tile),
  `TiledLightCuller::cull(lights, view, proj) → TileLightList` returns
  per-tile point/spot light index lists + a separate directional list.
  Per-tile overflow drops the farthest lights (distance priority).
  Demo: `cargo run --example light_culling_demo --features gpu` (128
  point lights + 1 directional over 1920×1080, 16-px tiles).

- **`shader`** — `TILED_LIGHTING_FRAGMENT_WGSL` const added (storage-
  buffer-based per-tile light index lookup, naga-validated). Built-in
  shader cache now reports 9 entries.

- **`jobs`** — Fork-join job system (Wicked-inspired `wiJobSystem`).
  `JobContext` (Mutex+Condvar pending counter, optional parent for
  nesting), `JobArgs` (job_index / group_id / group_index), top-level
  `execute` / `dispatch` / `wait`. Dedicated `rayon::ThreadPool`
  (isolated from the global pool used by `sdf::marching_cubes_parallel`).
  Panic-safe via RAII `JobGuard` so `wait` always unblocks. Example:
  `cargo run --example job_system_demo` (256 fork-joined "particle" jobs
  + 2 "asset" jobs, single parent `wait` covers both stages).

### Scene graph + renderer integration

- `NodeKind::Decal(DecalData)` — decals are first-class scene graph
  nodes; their OBB extents come from `local_transform.scale`.
- `SceneGraph::decals()` collector + `local_aabb` arm for the unit OBB.
- `FrameData` gains `decal_draws: Vec<DecalDraw>`, collected per frame
  with pre-computed inverse world matrices for shader use.
- `RenderPass::Decal` inserted between `GBuffer` and `DeferredLighting`
  in `Renderer::active_passes()`; `DrawStats.decal_nodes`,
  `PipelineState.decal_pass_enabled` added.

### Shaders

- `DECAL_VERTEX_WGSL` + `DECAL_FRAGMENT_WGSL` — depth-reconstruct OBB
  projection with branchless blend mode dispatch matching
  `DecalBlendMode::shader_id`. Both naga-validated in unit tests. Built-
  in shader cache now reports 8 entries (was 6).

### Example

- `cargo run --example decal_demo` — five decals (bullet hole, blood,
  graffiti, sign, glowing rune) projected onto a wall mesh and SDF
  floor, headless.

## [0.6.0] - 2026-06-11

Major content release: turn-based RPG, 3D action combat, hierarchical
pathfinding, HD-2D post-process, multiplayer scaffolding, and 14 no-code
event commands. 793 lib tests / 1,007 with `full` features.

### New modules

- **`battle`** — `TurnBattleRunner` with speed-ordered execution,
  `Attack` / `Defend` / `Flee` / `UseAbility` / `Move`, `BattleAi` trait,
  `RandomAi`, `GridCell` + Chebyshev attack range.
- **`action_combat`** — `Hitbox` / `Hurtbox` (sphere + capsule),
  `resolve_hits`, `ComboSystem` with input window, `LockOn` cone,
  `HitStop`.
- **`hd2d_postfx`** — `Sprite3D` billboard + WGSL templates for
  `hd2d_sprite`, `ssgi`, `smaa`. naga-validated + GPU-loaded on Mac Metal.

### Scripting

- 13 serializable `EventCommand`s — `Message` / `ChangeAttr` / `Wait` /
  `Branch` / `GiveItem` / `BeginBattle` / `Choice` / `SetVar` / `IfVar` /
  `SetSwitch` / `HasItem` / `TakeItem` / `MapTransition`.
- 5 advanced flow commands — `Cutscene` (multi-line dialogue),
  `Parallel`, `Repeat`, `LoopUntil`, `LlmDialogue` (LLM-backed NPC).
- `EventScriptDef` JSON form for no-code editors / hot reload.

### Bridge

- New `bridge::WorldProvider` trait — themed-world plug-in (zones,
  weather, camera, teleport).
- `network::LoopbackTransport` implementing `bridge::NetworkTransport`,
  driving the new `multiplayer_battle` example.

### Existing modules — additions

- **`animation`** — 2-bone analytic IK solver, `BlendTree1D`.
- **`audio`** — `MusicTrack` cross-fade BGM controller, `ReverbZone` with
  4 presets (outdoor / room / cavern / underwater).
- **`navmesh`** — Grid → NavMesh auto-generation, hierarchical A* with
  cluster planning.
- **`particle`** — Curl-noise force field, `TrailEmitter` with `max_len`.

### Examples

- `rpg` — full opening / choice / battle / reward flow.
- `multiplayer_battle` — two peers over `LoopbackTransport` driving one
  `TurnBattleRunner`.
- `visual_novel` — branching dialogue + LLM line + heroine route, all
  via `EventScript`.
- `fps_combat` — `LockOn` + hitscan + `HitStop`.
- `platformer_action` — sword hitbox + dash trail (Curl Noise) (requires
  `--features particles`).

### Quality

- clippy (pedantic + nursery): 0 lib warnings (style allows justified at
  the `lib.rs` top, see comments).
- 793 unit tests / 1,007 with `full` features. CI 7-in-a-row green.
- 6 WGSL shaders / pipelines verified end-to-end on Mac Metal (offscreen
  triangle readback passes — green pixel confirmed).
- New `tokio` dependency only in downstream consumers needing
  `AliceSyncTransport`; the engine crate itself stays dep-free of
  the ALICE-xxx stack.

## [0.5.0] - 2026-03-28

Initial public release. 31 modules, 17,932 lines, 688 tests.
Dual licensed: MIT + Commercial.

### Core
- **ECS** (1,781 lines, 107 tests): Generational arena, sparse-set ComponentStore, World, Scene, spatial hash grid broadphase
- **Scene Graph** (1,277 lines, 43 tests): Mesh+SDF hybrid NodeKind, hierarchical TRS, AABB3, frustum culling, reparenting, descendants query
- **Engine** (354 lines, 11 tests): System trait, fixed timestep (60Hz), interpolation alpha, configurable max steps
- **Math** (776 lines, 30 tests): Vec2/3/4, Mat4, Quat, Color, perspective + orthographic projection, sRGB conversion

### Rendering
- **GPU** (521 lines, 10 tests): wgpu Device/Queue/Surface, render_mesh(), render_clear(), vertex/index/uniform buffer creation, RGBA8 texture upload
- **Renderer** (773 lines, 25 tests): Deferred GBuffer (5 attachments), RenderGraph (Kahn topological sort), DebugRenderer (wireframe/AABB)
- **Shader** (439 lines, 15 tests): ShaderCache, 5 built-in WGSL shaders (GBuffer vertex/fragment, SDF raymarch, fullscreen vertex, deferred lighting)
- **Render Pipeline** (354 lines, 13 tests): FrameData scene extraction, MvpUniforms, MaterialUniforms, PipelineState
- **GPU Mesh** (280 lines, 9 tests): GpuMeshDesc, VertexLayout (Position3F/Normal3F/Uv2F/Color4F/Tangent4F), DrawCommand/DrawQueue with material sort
- **Texture** (400 lines, 18 tests): TextureAsset, mipmap level calculation, checkerboard generator, GpuTextureDesc, SamplerDesc
- **LOD** (264 lines, 13 tests): LodGroup, screen coverage calculation, batch LOD selection, pixel-size culling

### SDF
- **SDF** (1,112 lines, 37 tests): 7 primitives (Sphere/Box/Capsule/Cylinder/Torus/Plane/Cone), 6 boolean ops (Union/Intersection/Subtraction + smooth variants), full TRS transform, standard Marching Cubes (256-entry edge+triangle tables), sphere tracing, SDF sphere collider, normal estimation

### Physics
- **Physics3D** (696 lines, 32 tests): RigidBody with mass/restitution/friction, semi-implicit Euler integration, sweep-and-prune broadphase O(n log n), impulse-based contact resolution, linear/angular damping, sleeping/wake, torque, gravity
- **Collision** (333 lines, 10 tests): GJK convex intersection (ConvexHull/ConvexSphere), Indeterminate result for non-convergence, SDF-mesh hybrid narrowphase

### Audio
- **Audio** (975 lines, 39 tests): AudioBus with ping-pong buffer effects chain, static dispatch Effect enum (Attenuate/LowPass/HighPass/BandPass/Reverb), HRTF processor (ITD+ILD), AudioSource with PCM buffer playback + looping, spatial equal-power panning, AudioEngine.render() full pipeline, WAV export

### Animation
- **Animation** (650 lines, 32 tests): Keyframe (Linear/Step/CubicBezier interpolation), Track with binary search evaluation, AnimationClip with looping, AnimationPlayer (play/pause/stop/speed), StateMachine with timed transitions and blend factor

### Input
- **Input** (587 lines, 16 tests): Keyboard/Mouse/Gamepad with just_pressed/just_released tracking, ActionMap name-based binding, AxisSource (KeyPair/GamepadAxis/MouseX/Y/Scroll)
- **Window** (263 lines, 15 tests): WindowConfig, winit KeyCode mapping, FrameTimer with 60-sample smoothed FPS

### UI
- **UI** (951 lines, 30 tests): Retained-mode widget tree (Panel/Button/Label/TextInput/Checkbox/Slider/Image/ScrollArea/DropdownList/ProgressBar), Measure/Arrange layout (Vertical+Horizontal), UiTheme, FocusManager (tab order), message passing (BubbleUp/Direct routing), hit testing

### Navigation
- **NavMesh** (654 lines, 21 tests): NavMesh (vertex/triangle/neighbor), A* pathfinding, NavAgent waypoint following, SDF dynamic obstacle avoidance, crowd separation (RVO-style)

### Gameplay
- **Ability** (501 lines, 16 tests): Gameplay Ability System (UE5 GAS inspired) — Attribute/AttributeSet, GameplayEffect (Instant/Duration/Infinite), AttributeModifier (flat/multiply), Ability with cooldown+cost, AbilitySystem tick/activate
- **Scripting** (549 lines, 24 tests): EventBus (publish/subscribe), Timer/TimerManager (OneShot/Repeating), ScriptVars (typed variable storage)

### 2D
- **Scene2D** (532 lines, 21 tests): Sprite2D, TileMap (grid + world coordinate conversion), Aabb2, Body2D, 2D AABB collision detection, SDF2D circle test, Scene2D z-order rendering

### Asset & Import
- **Asset** (333 lines, 13 tests): OBJ parser (v/f with fan triangulation), glTF binary header parser, SDF JSON loader, asset type detection by extension
- **Import** (409 lines, 17 tests): Unity YAML scene parser (GameObject/Transform/MeshRenderer/Camera/Light), UE5 .uasset header parser, automatic format detection, unity_to_node() ALICE conversion
- **Resource** (309 lines, 12 tests): Async resource manager with Pending/Ready/Failed states, ref counting, path-based lookup

### Infrastructure
- **App** (715 lines, 13 tests): `run_windowed()` with winit ApplicationHandler (keyboard/mouse/resize), HeadlessRunner for testing, WAV audio export
- **Camera Controller** (322 lines, 19 tests): FPS camera (WASD+mouse look, pitch clamp), Orbit camera (rotate/zoom/pan)
- **Query** (293 lines, 11 tests): Typed ECS queries (query2/query3), filter_with/filter_without, SystemScheduler (priority-ordered)
- **Particle** (432 lines, 16 tests): ParticleEmitter (Point/Sphere/Box/Cone shapes), gravity, color/size interpolation, LCG RNG
