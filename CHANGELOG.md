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

### v0.7 wave 10 — GUI stack v2 (grid + animation + text input + notify)

Builds on the Wave 9 foundation so the engine's GUI surface
reaches Web / React parity.

- **`grid`** — CSS-Grid style layout solver alongside `flex`.
  `TrackSize::Fixed(px)` / `Fraction(weight)` (CSS `fr` analogue)
  / `Auto` + per-cell `column_span` / `row_span`. Useful for
  dashboards, photo galleries, header bars with named columns.
- **`ui_animation`** — `UiTransition` + 5 `Easing` curves (Linear /
  EaseIn / EaseOut / EaseInOut / Spring). `set_target(value, dur)`
  starts the animation, `tick(dt)` advances it. Mid-flight target
  changes restart the curve from the current value (responsive UI
  feel). Useful for hover / pressed / focused state interpolation.
- **`imgui::text_input`** — single-line text field. `UiInput` gains
  `typed: Vec<char>` + `backspace: bool` so any host input layer
  can forward keystrokes. Records `UiInteraction::TextInputEdited`
  / `TextInputFocused` for game-side handlers.
- **`notify`** — `NotifyCenter` toast / modal queue. `push` adds
  a `Notification` (Info / Success / Warning / Error severities);
  `tick(dt)` auto-dismisses toasts and leaves modals; `active()`
  returns modals first then toasts in arrival order. Capacity caps
  drop the oldest toast but never a modal.

### v0.7 wave 9 — Bevy-style GUI stack

Four new modules + theme bring the engine's UI surface up to
parity with the bevy_ui / bevy_egui / bevy_inspector_egui trio.

- **`flex`** — Flexbox layout solver. `FlexDirection` (Row /
  Column) × `JustifyContent` (Start / Centre / End / SpaceBetween /
  SpaceAround) × `AlignItems` (Start / Centre / End / Stretch) with
  `flex_grow` weight + uniform `gap`. `solve(container, layout,
  children) → Vec<ResolvedChild>` returns per-child rects.

- **`imgui`** — Immediate-mode GUI builder
  (egui / `bevy_egui` style). `UiContext::new(UiInput)` creates a
  per-frame builder, widgets `label` / `button` / `slider` /
  `checkbox` record [`UiCommand`]s for the renderer + interaction
  events for game logic. Auto vertical stacking, hit-testing
  against the cursor + primary press / release flags.

- **`inspector`** — Scene-graph inspector (`bevy_inspector_egui`
  style). `Inspector::rows(&scene)` returns a flat `Vec<InspectorRow>`
  with depth-indented names + per-kind label; `Inspector::detail(
  scene, id)` returns the `(field, value)` pairs for the right-side
  property panel. Read-only by design — mutations route through
  `Editor::apply` so undo/redo stays consistent.

- **`theme`** — `UiTheme` colour palette + spacing scale + font
  size scale, with `dark` / `light` presets (= IDE default / Material
  Design). Stateless, serde round-trippable.

### v0.7 wave 8 — ALICE-SDF feature port

Adopts six high-value features from `ALICE-SDF` so the engine's
in-tree SDF support catches up to the standalone crate without
adding it as a dependency.

- **Adaptive Marching Cubes** (`sdf::adaptive_marching_cubes`) —
  octree refinement on top of the existing MC: skips empty octants,
  subdivides only where corner-vs-centre error exceeds a threshold,
  falls back to the standard `marching_cubes` at the leaves.
- **Dual Contouring** (`sdf::dual_contouring`) — one vertex per cell
  placed at the average surface intersection plus stitched quads
  across sign-changing edges. Sharp edges survive better than MC;
  QEF refinement omitted for now (= still close to ALICE-SDF
  output for organic shapes).
- **`sdf::shell_offset`** — `(eval(p) + offset).abs() - thickness/2`
  turns any SDF into a hollow shell, with optional offset shift.
- **Volume / Surface Area Monte Carlo** (`sdf::volume_monte_carlo`,
  `sdf::surface_area_monte_carlo`) — deterministic PRNG samples a
  bounding box and returns `(estimate, standard_error)`. Validated
  against the unit sphere's known volume and area.
- **`sdf2d` module** — 5 primitives (`Circle`, `Box`, `RoundedBox`,
  `Segment`, `Triangle`) + 4 boolean ops (Union / Intersect /
  Subtract / SmoothUnion) + `sample_grid` for icon / font /
  UI-mask rasterisation.
- **`heatmap` module** — `heatmap_slice(node, axis, depth, ...)`
  samples an SDF slice and returns an RGBA buffer ready to upload
  to `Rgba8Unorm`. Four scientific colormaps (CoolWarm, Binary,
  Viridis, Magma).

### v0.7 wave 7 — end-to-end demos + Android app

- **virtual_shadow caster demo** —
  `templates/virtual_shadow_demo.rs` allocates one atlas page,
  runs a depth-only render pass through
  `VirtualShadowGpu::render_caster_to_page` writing a fixed depth
  (`0.42`), then reads back the centre + a foreign-page texel to
  prove the viewport restriction. Verified on Apple M3
  (= inside `0.420`, outside `1.000`).

- **Android Studio sample app** — new `android/` directory ships
  a complete minimum-viable Android Studio project:
  `settings.gradle.kts` / `build.gradle.kts` / `app/build.gradle.kts`
  + `AndroidManifest.xml` + `AliceGameEngine.java` (JNI wrapper,
  `AutoCloseable`, opaque `long` handle, phase constants) +
  `MainActivity.java` (Choreographer-driven tick + touch
  forwarding). Native build path documented in `android/README.md`
  (= `cargo-ndk` 一発で `libalice_game_engine.so` ⇒
  `jniLibs/arm64-v8a/`).

- **Cubemap sky GPU demo** —
  `templates/cubemap_sky_demo.rs` runs
  `render_sky_to_faces` on a real GPU, reads back all 6 faces of
  the `Rgba16Float` cube texture, decodes the centre half-float
  texel per face, and asserts the procedural sky filled the texture
  with the expected gradient. Verified on Apple M3 (= +Y zenith
  blue / -Y horizon warm / sides smooth gradient).

- **`virtual_shadow` atlas texture usage** — `COPY_SRC` added so
  shadow data can be read back to host memory (= unit tests, screen
  capture, debug overlays).

### v0.7 wave 6 — driver demos + JNI bridge

Closes the four residual items from Wave 5.

- **TLAS bottom-up dispatch demo** —
  `templates/gpu_bvh_refit_demo.rs` builds a small BVH, dispatches
  `gpu_bvh_interior_refit_compute` once per
  `Bvh::levels_bottom_up()` slot, reads back the root node, and
  asserts the GPU-refit AABB matches the CPU-built scene bounds.
  Verified on Apple M3 (= 3 levels, root bounds = scene bounds).

- **`virtual_shadow::render_caster_to_page`** — one-page depth
  render driver: opens a depth-only render pass scoped to the
  page's atlas viewport (= `set_viewport` with `page_uv_offset`),
  invokes the caller's `draw_callback`, and submits. The smallest
  driver an engine needs to fill exactly the dirty pages of a
  virtual shadow map.

- **`Humanoid::bind_from_vrm(&VrmExtract)`** — bridges the VRM
  camelCase naming (`leftUpperArm`) to the engine's snake_case
  [`HumanoidBone`] (`left_upper_arm`) and bulk-binds every known
  bone in one call. Unknown bones are skipped (= forward
  compatible).

- **Android JNI wrapper scaffold** — `mobile::AliceGameEngineHandle`
  + `alice_ge_create` / `alice_ge_tick` / `alice_ge_touch` /
  `alice_ge_destroy` (`#[unsafe(no_mangle)] extern "C"`) so the
  Java side can store an opaque pointer + dispatch frame ticks +
  touch events. ABI is now frozen; real method bodies land in a
  follow-up PR without breaking the Java wrapper.

### v0.7 wave 5 — production drivers (residual gap)

Closes the four "scaffold あり / driver 未着" items left over from
Wave 4.

- **TLAS interior refit driver** —
  `Bvh::levels_bottom_up()` returns node indices grouped by tree
  level (leaves first, root last) for the GPU dispatcher.
  `shader::GPU_BVH_INTERIOR_REFIT_COMPUTE_WGSL` unions each interior
  node's children bounds (one workgroup invocation per node in the
  current level). The bottom-up dispatch sequence is now a one-loop
  driver: iterate over `levels_bottom_up()` and dispatch
  `gpu_bvh_interior_refit_compute` per level.

- **Cubemap sky render driver** —
  `CubemapCaptureTargets::render_sky_to_faces(device, queue,
  atmosphere)` builds a full wgpu render pipeline against
  `CUBEMAP_SKY_FRAGMENT_WGSL`, allocates per-face uniform buffers
  (= inverse view-projection + sun + horizon / zenith colors), and
  submits a fullscreen-triangle draw into every face. The probe is
  now populated with the procedural sky in one call.

- **VRM full extract** — `parse_vrm_full(json) → VrmExtract` now
  returns the full humanoid bone binding list and expression preset
  weights, not just the meta block. Includes `VrmBoneBinding`
  (`bone_name`, `node_index`) + `VrmExpressionBinding` (`preset`,
  `weight`) and gracefully handles VRMs that omit either section.

- **`virtual_shadow` GPU pipeline** —
  `VirtualShadowGpu::new(device, atlas_pages_per_side, page_size)`
  allocates a `Depth32Float` atlas texture + view + comparison
  sampler (= the residency that the existing
  `VirtualShadowMap` page table indexes into). `page_uv_offset`
  converts a `PhysicalPageHandle` into the atlas UV offset shader
  code reads from.

### v0.7 wave 4 — evaluation report follow-through

Closes the 10 residual items from the v0.6 evaluation report (E.
弱み / 残課題). Each item now ships at least scaffold + tests + doc.

- **Cubemap GPU full render** —
  `shader::CUBEMAP_SKY_FRAGMENT_WGSL` (naga-validated) renders the
  procedural sky into a cubemap face; combine with
  `env_probe::CubemapCaptureTargets` for a real dynamic IBL probe.

- **iOS / Android library binaries** — `Cargo.toml` `[lib]
  crate-type = ["rlib", "staticlib", "cdylib"]` now produces
  `.rlib` + `.a` + `.dylib` from one `cargo build`. Verified locally
  on macOS aarch64 (= 3 artefacts in `target/release/`).

- **Editor websocket UI** — `templates/editor_ui/index.html`
  ships an embedded browser editor (connect / hello / snapshot /
  undo / redo / translate / rename / hide) and
  `editor_server_demo` serves it at `http://127.0.0.1:8088/`.

- **GPU compute light culling demo** —
  `templates/gpu_compute_light_culling_demo.rs` dispatches the
  tiled-light compute shader on a real GPU device. Verified on Apple
  M3 (4 lights, 240/240 tiles covered).

- **TLAS/BLAS GPU scaffold** — `shader::GPU_BVH_REFIT_COMPUTE_WGSL`
  (naga-validated) implements the leaf-refit pass for the existing
  CPU-built `Bvh`; the bottom-up interior refit driver lands in a
  follow-up PR.

- **Asset I/O extension** — `asset::parse_vrm_json` (VRM 1.x meta
  block), `asset::parse_fbx_header` (Kaydara FBX binary magic +
  version), `asset::is_usda` (`#usda` ASCII recogniser) — scaffold
  recognisers so importers can branch.

- **Multi-platform CI** — `.github/workflows/ci.yml` test job runs
  on a `[ubuntu-latest, macos-latest, windows-latest]` matrix.

- **Procedural animation** — new `cloth` module (mass-spring grid
  with Verlet + length constraint + wind) and `soft_body` module
  (3-axis lattice with pin support). 13 unit tests.

- **TUTORIAL.md** — five-step getting-started covering install /
  LLM NPC / SDF+mesh scene / RPG battle / editor websocket.

- **`virtual_shadow` scaffold** — `VirtualShadowMap` page allocator
  (UE5-style) with `allocate` / `release` / `lookup`. 6 unit tests.

### v0.7 wave 3 — 4-region production wiring

- **Editor websocket server demo** — new `editor_server` feature
  (axum 0.7 + tokio macros / rt-multi-thread) + `templates/
  editor_server_demo.rs` runs an axum server on `127.0.0.1:8088`
  with `/ws` handling JSON-encoded `EditorClientMessage` frames via
  `dispatch_client_message`. Shared scene + editor behind
  `tokio::sync::Mutex` for multi-client safety.

- **Cubemap GPU render driver** —
  `CubemapCaptureTargets::clear_all_faces(device, queue, color)`:
  smallest "render driver" that submits 6 clear-only passes against
  the 6 face views, proving the wiring before the engine plugs in
  its real deferred render. Production code swaps the clear for the
  6-face deferred render.

- **Mobile CI smoke** — `.github/workflows/ci.yml` gains a new
  `mobile-build` job on `macos-latest` that runs `cargo check
  --target aarch64-apple-ios` + `--target aarch64-linux-android`
  with `--no-default-features` so iOS / Android cross builds are
  validated on every PR.

- **GPU compute headless demo** — new `templates/gpu_compute_demo.rs`
  drives the DDGI update compute shader end-to-end on a real GPU
  via pollster + wgpu Instance/Adapter/Device, dispatches via
  `dispatch_compute_once`-style flow, reads back the irradiance
  buffer, and verifies the mean matches the input samples (= 0.7).
  Verified on Apple M3 (Metal). Falls back to a graceful skip
  message when no compatible adapter is available.

### v0.7 wave 2 — 4-region depth

- **`editor`** undo/redo wired through — `Editor::undo` /
  `Editor::redo` mutate the scene, push/pop the inverse on the
  appropriate stack, and a new `apply` clears the redo stack
  per the standard editor convention. New `EditorCommand::RemoveNode`
  variant + post-execute `patch_inverse_for_add` so `AddNode` /
  `RemoveNode` round-trip with the correct node id.

- **`editor`** websocket / MCP protocol enums — `EditorClientMessage`
  (`Hello` / `Apply` / `Undo` / `Redo` / `Snapshot`),
  `EditorServerMessage` (`Welcome` / `Outcome` / `Snapshot` /
  `Error`), `EDITOR_PROTOCOL_VERSION = 1`, `dispatch_client_message`
  pure-function dispatcher. Ready for both axum websocket transports
  and MCP `tools/call` adapters in a follow-up PR.

- **`gpu`** compute helpers — `GpuContext::create_storage_buffer`,
  `create_empty_storage_buffer`, and `dispatch_compute_once`
  (one-shot compute submit + readback) so module-side `*Gpu`
  pipelines can be exercised from unit tests / offline tools without
  every caller duplicating the encoder + map_async boilerplate.

- **`env_probe`** GPU cubemap targets — `CubemapFaceCamera` +
  `cubemap_face_cameras(position, near, far)` + (under feature
  `gpu`) `CubemapCaptureTargets::new(device, ...)` which allocates a
  6-layer `Rgba16Float` cube texture, the cube-array view, and one
  face-level view per face so the existing deferred renderer can run
  6 face passes into the same target.

- **`mobile`** target descriptor — `MobileTarget` enum (Ios /
  Android / Other), `MobileTarget::current()` resolves at compile
  time from `cfg(target_os)`, plus `mobile_build_hints()` and stub
  `android` / `ios` submodules (= placeholders for platform glue).

### v0.7 wave 1 — 4-region expansion

- **GPU compute shaders** — `LIGHT_CULLING_COMPUTE_WGSL`
  + `DDGI_UPDATE_COMPUTE_WGSL` const (naga-validated, builtin cache
  10 → 12). Per-module wgpu pipeline scaffolds:
  `light_culling::TiledLightCullerGpu` (= 4-binding compute pipeline,
  `workgroup_count_x(light_count)` helper) and
  `ddgi::DdgiVolumeGpu` (= 3-binding compute pipeline, one workgroup
  per probe, 8×8 threads). CPU implementations stay as the reference
  / fallback.

- **`env_probe`** new helpers — `cubemap_face_views(position)` returns
  the six standard cube-face view matrices; `capture_sky_to_cubemap(
  resolution, atmosphere)` rasterises [`sky::sky_color`] into a fresh
  [`Cubemap`] so probes can be seeded without the full GPU 6-face
  capture pass landing yet.

- **`mobile`** new module — `TouchPhase` / `TouchEvent` / `ScreenMetrics`
  + `TouchCamera` (= single-finger orbit, two-finger pinch zoom,
  device-independent units). Renderer-agnostic so iOS / Android / web
  shells can forward platform touches with no extra glue.

- **`editor`** new module — `EditorCommand` (Add / Hide / Show /
  Translate / SetScale / SetRotation / Rename), `Editor::apply`
  mutates a [`SceneGraph`] and records the inverse on
  `EditorHistory` (capacity-bounded). JSON round-trip ready, designed
  to be the receiving end of the MCP server's `editor.apply` tool and
  a future websocket transport for browser editors.

- **`gpu_bvh`** — CPU-side BVH + Morton-code radix sort. `Aabb`
  (union / centre / surface_area), `morton3_unit(xyz)` (30-bit
  Z-curve code in unit cube), `radix_sort_u32_pairs` (stable LSD
  3-pass × 11-bit, payload-preserving), `Bvh::build(aabbs, leaf_size)`
  (Morton-sorted, median-split top-down build). Same radix sort
  feeds Gaussian splat tile bins and particle / collision broadphase.
  Demo: `cargo run --example gpu_bvh_demo`.

- **`ddgi`** — Dynamic Diffuse Global Illumination (Majercik et al.
  2019). `DdgiConfig` (grid / spacing / resolution / hysteresis),
  `DdgiVolume::update_probe_irradiance` (low-pass filter blending),
  `sample_irradiance(world, dir)` (trilinear 8-probe blend, octahedral
  direction lookup, world-volume bounds check). `dir_to_oct` /
  `oct_to_dir` octahedral encoding utilities. Demo:
  `cargo run --example ddgi_demo` (warm-interior / cool-exterior
  probe volume queried at four world positions).

- **`gaussian_splat`** — 3D Gaussian Splatting (Kerbl et al. 2023)
  CPU data + per-frame projection. `Splat` (position / anisotropic
  scale / rotation / colour / opacity / SH band-1 coefficients),
  `Splat::isotropic` constructor, `GaussianCloud::prepare_frame(view,
  projection)` does frustum culling + screen-space projection +
  back-to-front depth sort. `evaluate_sh_band1(coeffs, view_dir)` for
  view-dependent colour. Tile-blending shader lives in a future PR.
  Demo: `cargo run --example gaussian_splat_demo` (200 random splats
  projected and depth-sorted in <1 ms).

- **`humanoid`** — VRM 1.0-style humanoid skeleton mapping +
  expression channels. `HumanoidBone` enum (25 canonical VRM bones),
  `Humanoid::bind` / `get` / `meets_required` / `missing_required`,
  `ExpressionChannel` (5 visemes + 7 emotion presets + `Custom`),
  `ExpressionSet::set` / `weight` / `remove` / `reset` /
  `set_visemes` lip-sync helper. Renderer-agnostic — owns mappings
  and weights, not transforms. Demo:
  `cargo run --example humanoid_demo` (4-frame lip-sync + blink +
  happy overlay).

- **`volumetric_clouds`** — Raymarched volumetric cloud module
  (Horizon Zero Dawn / Wicked Engine style). `VolumetricCloudConfig`
  (coverage / density / wind / altitude band / step count / sun),
  `cloud_density(world_pos, time, config)` (FBM base + detail erosion
  + height window), `march_cloud_ray(origin, dir, time, config) →
  CloudRayResult` (Beer's-law transmittance + HG scattering). Demo:
  `cargo run --example volumetric_clouds_demo` (ASCII sky at three
  coverage levels + wind advection).

- **`hair`** — CPU hair / grass strand simulator. `HairConfig`
  (segments / length / wind_strength / gravity / stiffness / LOD),
  `HairSystem::add_strand` + `simulate(dt, wind)` does Verlet
  integration with a length constraint, per-strand seed jitter, and
  LOD cutoff. Demo: `cargo run --example hair_demo` (64 grass blades
  swayed by a constant wind).

- **`env_probe`** — Environment probe / IBL data + prefilter math.
  `EnvProbeData` (scene-graph payload), `Cubemap` (6-face RGBA32F,
  direction sampling, bilinear), `prefilter_irradiance` (cosine
  convolution → diffuse IBL), `prefilter_radiance` (Phong-style mip
  chain → specular IBL split-sum), `PrefilteredEnvProbe` bundle.
  `NodeKind::EnvProbe` + `SceneGraph::env_probes()` collector. WGSL
  `IBL_LOOKUP_WGSL` helper for shader-side cubemap sampling, naga-
  validated. Built-in shader cache now reports 10 entries.

- **`ocean`** — Tessendorf-style FFT ocean simulator (Wicked-inspired
  `wiOcean`). `OceanConfig` (grid_size / patch_size / wind / amplitude /
  gravity), `OceanSimulator::simulate(time) → OceanFrame` with height
  field + central-difference normals. **No external dependencies**:
  ships a self-contained Cooley-Tukey radix-2 IFFT, Box-Muller
  Gaussian noise from a deterministic 64-bit hash. 32×32 grid runs in
  ~0.5 ms / frame on Apple Silicon. Demo:
  `cargo run --example ocean_demo` (ASCII heightmap at four time steps).

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
