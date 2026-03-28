# Changelog

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
