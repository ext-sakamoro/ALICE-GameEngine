//! Application runner: winit event loop integration.
//!
//! Ties together winit window, wgpu GPU context, and the engine loop
//! into a single `App::run()` entry point.

use crate::engine::{Engine, EngineConfig, EngineContext, System};
use crate::input::{Key, MouseButton as EngineMouseButton};
use crate::window::FrameTimer;

// ---------------------------------------------------------------------------
// AppState — tracks window lifecycle
// ---------------------------------------------------------------------------

/// Application lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    WaitingForWindow,
    Running,
    Suspended,
    Exiting,
}

// ---------------------------------------------------------------------------
// AppCallbacks — user-defined game logic
// ---------------------------------------------------------------------------

/// User-defined callbacks for the application.
pub trait AppCallbacks {
    /// Called once after window + GPU are ready.
    fn init(&mut self, ctx: &mut EngineContext);

    /// Called every frame at variable rate.
    fn update(&mut self, ctx: &mut EngineContext, dt: f32);

    /// Called at fixed timestep for physics.
    fn fixed_update(&mut self, _ctx: &mut EngineContext, _fixed_dt: f32) {}
}

// ---------------------------------------------------------------------------
// BridgeSystem — adapts AppCallbacks to engine::System
// ---------------------------------------------------------------------------

struct BridgeSystem<'a> {
    callbacks: &'a mut dyn AppCallbacks,
}

impl System for BridgeSystem<'_> {
    fn init(&mut self, ctx: &mut EngineContext) {
        self.callbacks.init(ctx);
    }

    fn fixed_update(&mut self, ctx: &mut EngineContext, fixed_dt: f32) {
        self.callbacks.fixed_update(ctx, fixed_dt);
    }

    fn update(&mut self, ctx: &mut EngineContext, dt: f32) {
        self.callbacks.update(ctx, dt);
    }
}

// ---------------------------------------------------------------------------
// AppRunner — headless runner (no actual winit, for testing)
// ---------------------------------------------------------------------------

/// Headless application runner for testing or server environments.
pub struct HeadlessRunner {
    pub engine: Engine,
    pub timer: FrameTimer,
    pub state: AppState,
    pub input: crate::input::InputState,
}

impl HeadlessRunner {
    #[must_use]
    pub fn new(engine_config: EngineConfig) -> Self {
        Self {
            engine: Engine::new(engine_config),
            timer: FrameTimer::new(),
            state: AppState::WaitingForWindow,
            input: crate::input::InputState::new(),
        }
    }

    /// Initializes the engine with the given callbacks.
    pub fn init(&mut self, callbacks: &mut dyn AppCallbacks) {
        let mut bridge = BridgeSystem { callbacks };
        self.engine.init(&mut bridge);
        self.state = AppState::Running;
    }

    /// Runs a single frame. Returns false if should exit.
    pub fn frame(&mut self, dt: f32, callbacks: &mut dyn AppCallbacks) -> bool {
        if self.state != AppState::Running {
            return false;
        }
        self.input.begin_frame();
        self.timer
            .update((f64::from(self.timer.delta_seconds) + f64::from(dt)) * 1000.0);
        let mut bridge = BridgeSystem { callbacks };
        self.engine.frame(dt, &mut bridge)
    }

    /// Runs N frames at the given FPS.
    pub fn run_frames(&mut self, count: u32, fps: f32, callbacks: &mut dyn AppCallbacks) {
        let dt = 1.0 / fps;
        for _ in 0..count {
            if !self.frame(dt, callbacks) {
                break;
            }
        }
    }

    pub const fn stop(&mut self) {
        self.engine.stop();
        self.state = AppState::Exiting;
    }
}

// ---------------------------------------------------------------------------
// winit key mapping
// ---------------------------------------------------------------------------

/// Maps winit `KeyCode` string name to engine `Key`.
#[must_use]
pub fn map_winit_key(key_name: &str) -> Option<Key> {
    crate::window::map_key(key_name)
}

/// Maps winit mouse button index to engine `MouseButton`.
#[must_use]
pub const fn map_winit_mouse(index: u32) -> Option<EngineMouseButton> {
    crate::window::map_mouse_button(index)
}

// ---------------------------------------------------------------------------
// Windowed runner — real winit event loop + wgpu rendering
// ---------------------------------------------------------------------------

/// Runs the application with a real window and GPU rendering.
///
/// This creates a winit event loop, opens a window, initializes wgpu,
/// and drives the engine loop until the window is closed.
///
/// # Errors
///
/// Returns an error string if window or GPU creation fails.
#[cfg(feature = "window")]
pub fn run_windowed(
    config: crate::window::WindowConfig,
    engine_config: EngineConfig,
    callbacks: Box<dyn AppCallbacks>,
) -> Result<(), String> {
    use winit::event_loop::EventLoop;

    let event_loop = EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = WindowedApp {
        config,
        engine_config,
        callbacks,
        state: AppState::WaitingForWindow,
        window: None,
        gpu: None,
        surface: None,
        timer: FrameTimer::new(),
        engine: None,
        input: crate::input::InputState::new(),
        pipeline: None,
        vertex_buffer: None,
        index_buffer: None,
        uniform_buffer: None,
        bind_group: None,
        index_count: 0,
    };

    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("Event loop error: {e}"))
}

#[cfg(feature = "window")]
struct WindowedApp {
    config: crate::window::WindowConfig,
    engine_config: EngineConfig,
    callbacks: Box<dyn AppCallbacks>,
    state: AppState,
    window: Option<std::sync::Arc<winit::window::Window>>,
    gpu: Option<crate::gpu::GpuContext>,
    surface: Option<wgpu::Surface<'static>>,
    timer: FrameTimer,
    engine: Option<Engine>,
    input: crate::input::InputState,
    pipeline: Option<wgpu::RenderPipeline>,
    vertex_buffer: Option<wgpu::Buffer>,
    index_buffer: Option<wgpu::Buffer>,
    uniform_buffer: Option<wgpu::Buffer>,
    bind_group: Option<wgpu::BindGroup>,
    index_count: u32,
}

#[cfg(feature = "window")]
impl WindowedApp {
    #[allow(clippy::too_many_lines)]
    fn init_gpu_resources(&mut self) {
        let gpu = self.gpu.as_ref().unwrap();

        // Cube vertices: position (3f) + color (3f), 8 corners × 3 triangles per face = 36 verts
        #[rustfmt::skip]
        let vertices: &[f32] = &[
            // Front face (z = -0.5)
            -0.5, -0.5, -0.5,  1.0, 0.0, 0.0,
             0.5, -0.5, -0.5,  0.0, 1.0, 0.0,
             0.5,  0.5, -0.5,  0.0, 0.0, 1.0,
            -0.5,  0.5, -0.5,  1.0, 1.0, 0.0,
            // Back face (z = 0.5)
            -0.5, -0.5,  0.5,  1.0, 0.0, 1.0,
             0.5, -0.5,  0.5,  0.0, 1.0, 1.0,
             0.5,  0.5,  0.5,  1.0, 1.0, 1.0,
            -0.5,  0.5,  0.5,  0.5, 0.5, 0.5,
        ];

        #[rustfmt::skip]
        let indices: &[u16] = &[
            0, 1, 2,  2, 3, 0, // front
            1, 5, 6,  6, 2, 1, // right
            5, 4, 7,  7, 6, 5, // back
            4, 0, 3,  3, 7, 4, // left
            3, 2, 6,  6, 7, 3, // top
            4, 5, 1,  1, 0, 4, // bottom
        ];
        self.index_count = indices.len() as u32;

        let vb_bytes = bytemuck::cast_slice(vertices);
        let ib_bytes = bytemuck::cast_slice(indices);
        let (vb, ib) = gpu.create_mesh_buffers(vb_bytes, ib_bytes);
        self.vertex_buffer = Some(vb);
        self.index_buffer = Some(ib);

        // MVP uniform: identity for now, updated each frame
        let mvp_data = [0.0_f32; 48]; // 3 × mat4x4
        let ub = gpu.create_uniform_buffer(bytemuck::cast_slice(&mvp_data));

        let bind_group_layout =
            gpu.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("mvp_layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mvp_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: ub.as_entire_binding(),
            }],
        });

        let pipeline_layout = gpu
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("pipeline_layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let shader_src = r"
struct Uniforms {
    mvp: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(@location(0) position: vec3<f32>, @location(1) color: vec3<f32>) -> VsOut {
    var out: VsOut;
    out.pos = u.mvp * vec4<f32>(position, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(@location(0) color: vec3<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(color, 1.0);
}
";
        let shader = gpu.create_shader("cube_shader", shader_src);

        let pipeline = gpu
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("cube_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 24,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                offset: 0,
                                shader_location: 0,
                                format: wgpu::VertexFormat::Float32x3,
                            },
                            wgpu::VertexAttribute {
                                offset: 12,
                                shader_location: 1,
                                format: wgpu::VertexFormat::Float32x3,
                            },
                        ],
                    }],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: gpu.surface_format(),
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        self.pipeline = Some(pipeline);
        self.uniform_buffer = Some(ub);
        self.bind_group = Some(bind_group);
    }
}

#[cfg(feature = "window")]
impl winit::application::ApplicationHandler for WindowedApp {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = winit::window::WindowAttributes::default()
            .with_title(&self.config.title)
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.width,
                self.config.height,
            ))
            .with_resizable(self.config.resizable);

        let window = std::sync::Arc::new(
            event_loop
                .create_window(attrs)
                .expect("Failed to create window"),
        );

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .expect("Failed to create surface");

        let size = window.inner_size();
        let gpu_config = crate::gpu::GpuConfig::default();
        let gpu_ctx = pollster::block_on(crate::gpu::GpuContext::from_surface(
            &instance,
            &surface,
            size.width.max(1),
            size.height.max(1),
            &gpu_config,
        ))
        .expect("Failed to init GPU");

        self.gpu = Some(gpu_ctx);

        // SAFETY: surface outlives gpu_ctx because both are owned by WindowedApp
        // and dropped together. We transmute the lifetime to 'static.
        let surface: wgpu::Surface<'static> = unsafe { std::mem::transmute(surface) };
        self.surface = Some(surface);

        let mut engine = Engine::new(self.engine_config.clone());
        let mut bridge = BridgeSystem {
            callbacks: &mut *self.callbacks,
        };
        engine.init(&mut bridge);
        self.engine = Some(engine);

        self.init_gpu_resources();
        self.state = AppState::Running;
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        use winit::event::{ElementState, WindowEvent};

        match event {
            WindowEvent::CloseRequested => {
                self.state = AppState::Exiting;
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let (Some(gpu), Some(surface)) = (&mut self.gpu, &self.surface) {
                    gpu.resize(surface, size.width.max(1), size.height.max(1));
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let key_str = format!("{:?}", event.physical_key);
                let key_name = key_str
                    .strip_prefix("Code(")
                    .and_then(|s| s.strip_suffix(')'))
                    .unwrap_or(&key_str);
                if let Some(key) = crate::window::map_key(key_name) {
                    match event.state {
                        ElementState::Pressed => self.input.key_press(key),
                        ElementState::Released => self.input.key_release(key),
                    }
                }
                if event.physical_key
                    == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape)
                {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                if self.state != AppState::Running {
                    return;
                }

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
                    * 1000.0;
                self.timer.update(now);
                let dt = self.timer.delta_seconds;

                self.input.begin_frame();
                if let Some(engine) = &mut self.engine {
                    let mut bridge = BridgeSystem {
                        callbacks: &mut *self.callbacks,
                    };
                    engine.frame(dt, &mut bridge);
                }

                // Update MVP uniform: compute rotation from engine time
                if let (Some(gpu), Some(ub), Some(engine)) =
                    (&self.gpu, &self.uniform_buffer, &self.engine)
                {
                    let t = engine.context.time.total_seconds as f32;
                    let aspect =
                        gpu.surface_config.width as f32 / gpu.surface_config.height.max(1) as f32;
                    let proj =
                        glam::Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
                    let view = glam::Mat4::look_at_rh(
                        glam::Vec3::new(0.0, 1.5, 3.0),
                        glam::Vec3::ZERO,
                        glam::Vec3::Y,
                    );
                    let model = glam::Mat4::from_rotation_y(t);
                    let mvp = proj * view * model;
                    let mvp_array = mvp.to_cols_array();
                    let mvp_bytes: &[u8] = bytemuck::cast_slice(&mvp_array);
                    gpu.write_buffer(ub, mvp_bytes);
                }

                // Render
                if let (Some(gpu), Some(surface), Some(pipeline), Some(vb), Some(ib), Some(bg)) = (
                    &self.gpu,
                    &self.surface,
                    &self.pipeline,
                    &self.vertex_buffer,
                    &self.index_buffer,
                    &self.bind_group,
                ) {
                    let _ = gpu.render_mesh(
                        surface,
                        pipeline,
                        vb,
                        ib,
                        self.index_count,
                        bg,
                        crate::math::Color::new(0.05, 0.05, 0.08, 1.0),
                    );
                }

                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Audio WAV export
// ---------------------------------------------------------------------------

/// Exports a `SampleBuffer` to WAV format bytes (16-bit PCM).
#[must_use]
#[cfg(feature = "audio")]
pub fn export_wav(buf: &crate::audio::SampleBuffer) -> Vec<u8> {
    let num_samples = buf.samples.len();
    let byte_rate = u32::from(buf.channels) * buf.sample_rate * 2;
    let block_align = buf.channels * 2;
    let data_size = (num_samples * 2) as u32;
    let file_size = 36 + data_size;

    let mut out = Vec::with_capacity(44 + num_samples * 2);
    // RIFF header
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&file_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");
    // fmt chunk
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&buf.channels.to_le_bytes());
    out.extend_from_slice(&buf.sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
                                                 // data chunk
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());
    for &s in &buf.samples {
        let clamped = s.clamp(-1.0, 1.0);
        let i16_val = (clamped * 32767.0) as i16;
        out.extend_from_slice(&i16_val.to_le_bytes());
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCallbacks {
        init_called: bool,
        update_count: u32,
        fixed_count: u32,
    }

    impl TestCallbacks {
        fn new() -> Self {
            Self {
                init_called: false,
                update_count: 0,
                fixed_count: 0,
            }
        }
    }

    impl AppCallbacks for TestCallbacks {
        fn init(&mut self, _ctx: &mut EngineContext) {
            self.init_called = true;
        }
        fn update(&mut self, _ctx: &mut EngineContext, _dt: f32) {
            self.update_count += 1;
        }
        fn fixed_update(&mut self, _ctx: &mut EngineContext, _fixed_dt: f32) {
            self.fixed_count += 1;
        }
    }

    #[test]
    fn headless_runner_init() {
        let mut runner = HeadlessRunner::new(EngineConfig::default());
        let mut cb = TestCallbacks::new();
        runner.init(&mut cb);
        assert!(cb.init_called);
        assert_eq!(runner.state, AppState::Running);
    }

    #[test]
    fn headless_runner_frame() {
        let mut runner = HeadlessRunner::new(EngineConfig::default());
        let mut cb = TestCallbacks::new();
        runner.init(&mut cb);
        let ok = runner.frame(1.0 / 60.0, &mut cb);
        assert!(ok);
        assert_eq!(cb.update_count, 1);
    }

    #[test]
    fn headless_runner_run_frames() {
        let mut runner = HeadlessRunner::new(EngineConfig::default());
        let mut cb = TestCallbacks::new();
        runner.init(&mut cb);
        runner.run_frames(10, 60.0, &mut cb);
        assert_eq!(cb.update_count, 10);
    }

    #[test]
    fn headless_runner_stop() {
        let mut runner = HeadlessRunner::new(EngineConfig::default());
        let mut cb = TestCallbacks::new();
        runner.init(&mut cb);
        runner.stop();
        let ok = runner.frame(1.0 / 60.0, &mut cb);
        assert!(!ok);
        assert_eq!(runner.state, AppState::Exiting);
    }

    #[test]
    fn headless_runner_fixed_update() {
        let mut runner = HeadlessRunner::new(EngineConfig::default());
        let mut cb = TestCallbacks::new();
        runner.init(&mut cb);
        // 1/30 = 2 fixed steps at 1/60
        runner.frame(1.0 / 30.0, &mut cb);
        assert_eq!(cb.fixed_count, 2);
    }

    #[test]
    fn app_state_variants() {
        assert_ne!(AppState::Running, AppState::Exiting);
        assert_ne!(AppState::WaitingForWindow, AppState::Suspended);
    }

    #[test]
    fn headless_not_running_before_init() {
        let runner = HeadlessRunner::new(EngineConfig::default());
        assert_eq!(runner.state, AppState::WaitingForWindow);
    }

    #[test]
    fn map_winit_key_works() {
        assert_eq!(map_winit_key("Space"), Some(Key::Space));
        assert_eq!(map_winit_key("???"), None);
    }

    #[test]
    fn map_winit_mouse_works() {
        assert_eq!(map_winit_mouse(0), Some(EngineMouseButton::Left));
        assert_eq!(map_winit_mouse(99), None);
    }

    struct SceneCallbacks;
    impl AppCallbacks for SceneCallbacks {
        fn init(&mut self, ctx: &mut EngineContext) {
            ctx.scene.add(crate::scene_graph::Node::new(
                "test",
                crate::scene_graph::NodeKind::Empty,
            ));
        }
        fn update(&mut self, _ctx: &mut EngineContext, _dt: f32) {}
    }

    #[test]
    fn headless_scene_access() {
        let mut runner = HeadlessRunner::new(EngineConfig::default());
        let mut cb = SceneCallbacks;
        runner.init(&mut cb);
        assert_eq!(runner.engine.context.scene.node_count(), 1);
    }

    #[test]
    fn headless_300_frames() {
        let mut runner = HeadlessRunner::new(EngineConfig::default());
        let mut cb = TestCallbacks::new();
        runner.init(&mut cb);
        runner.run_frames(300, 60.0, &mut cb);
        assert_eq!(cb.update_count, 300);
        assert_eq!(runner.engine.frame_count(), 300);
    }

    #[cfg(feature = "audio")]
    #[test]
    fn wav_export_header() {
        let buf = crate::audio::SampleBuffer::zeroed(44100, 2, 100);
        let wav = export_wav(&buf);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
    }

    #[cfg(feature = "audio")]
    #[test]
    fn wav_export_size() {
        let buf = crate::audio::SampleBuffer::zeroed(44100, 1, 10);
        let wav = export_wav(&buf);
        // 44 header + 10 samples * 2 bytes = 64
        assert_eq!(wav.len(), 44 + 10 * 2);
    }
}
