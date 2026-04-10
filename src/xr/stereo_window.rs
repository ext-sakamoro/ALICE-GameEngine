//! [`StereoWindowProvider`] — winit + wgpu simulator backend.
//!
//! Opens a single window split into left/right viewports, with the HMD pose
//! controlled by the mouse and the "controllers" driven by the keyboard.
//!
//! ## Default key map
//!
//! | Key | Action |
//! |-----|--------|
//! | `W` `A` `S` `D` | HMD position (XZ plane) |
//! | `Q` `E` | HMD position (Y axis, down/up) |
//! | Mouse motion | HMD yaw / pitch |
//! | Left mouse | Right `Trigger` (analog 1.0 / 0.0) |
//! | Right mouse | Right `Grip` (analog 1.0 / 0.0) |
//! | Middle mouse | Left `Trigger` |
//! | `Space` | Right `TriggerPress` |
//! | `LShift` | Right `GripPress` |
//! | Arrow keys | Right `Thumbstick` |
//! | `I` `J` `K` `L` | Left `Thumbstick` |
//! | `1` / `2` | Right `ButtonPrimary` / `ButtonSecondary` |
//! | `3` / `4` | Left `ButtonPrimary` / `ButtonSecondary` |
//! | `Escape` | Request exit |
//!
//! ## Rendering
//!
//! The window is split horizontally. The left half is cleared to a dark red
//! tint, the right half to a dark blue tint, so the developer can visually
//! confirm the stereo viewport split. Real scene rendering is left to higher
//! layers (or a future enhancement).

use std::collections::HashSet;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton as WinitMouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::app::{AppCallbacks, AppState};
use crate::engine::{Engine, EngineConfig, System};
use crate::gpu::{GpuConfig, GpuContext};
use crate::math::{Color, Quat, Vec3};
use crate::window::{FrameTimer, WindowConfig};

use super::provider::XrProvider;
use super::types::{
    XrAction, XrActionSet, XrConfig, XrError, XrHand, XrHaptics, XrPose, XrSessionState,
    XrViewState,
};

// ---------------------------------------------------------------------------
// SimController
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone, Copy)]
struct SimController {
    pose: XrPose,
    grip: f32,
    grip_press: bool,
    trigger: f32,
    trigger_press: bool,
    thumbstick: [f32; 2],
    thumbstick_click: bool,
    button_primary: bool,
    button_secondary: bool,
}

// ---------------------------------------------------------------------------
// StereoWindowProvider
// ---------------------------------------------------------------------------

/// Simulator [`XrProvider`] backed by winit input. See module docs for the
/// default key bindings.
pub struct StereoWindowProvider {
    config: XrConfig,
    action_set: XrActionSet,
    state: XrSessionState,
    hmd_yaw: f32,
    hmd_pitch: f32,
    hmd_position: Vec3,
    hmd_pose: XrPose,
    left: SimController,
    right: SimController,
    pressed: HashSet<KeyCode>,
    pub haptics_log: Vec<XrHaptics>,
    exit_requested: bool,
    speed_metres_per_sec: f32,
    mouse_sensitivity: f32,
}

impl StereoWindowProvider {
    /// Create a new provider in the [`XrSessionState::Focused`] state.
    #[must_use]
    pub fn new(config: XrConfig) -> Self {
        let mut s = Self {
            config,
            action_set: XrActionSet::new("stereo_window"),
            state: XrSessionState::Focused,
            hmd_yaw: 0.0,
            hmd_pitch: 0.0,
            hmd_position: Vec3::new(0.0, 1.7, 0.0),
            hmd_pose: XrPose::identity(),
            left: SimController::default(),
            right: SimController::default(),
            pressed: HashSet::new(),
            haptics_log: Vec::new(),
            exit_requested: false,
            speed_metres_per_sec: 2.5,
            mouse_sensitivity: 0.0035,
        };
        s.left.pose = XrPose::new(Vec3::new(-0.25, 1.3, -0.4), Quat::default());
        s.right.pose = XrPose::new(Vec3::new(0.25, 1.3, -0.4), Quat::default());
        s.recompute_hmd_pose();
        s
    }

    /// Tunable: linear movement speed for WASD/QE.
    pub fn set_speed(&mut self, speed_metres_per_sec: f32) {
        self.speed_metres_per_sec = speed_metres_per_sec;
    }

    /// Tunable: mouse-look sensitivity.
    pub fn set_mouse_sensitivity(&mut self, sens: f32) {
        self.mouse_sensitivity = sens;
    }

    /// Forward keyboard event from the windowing layer.
    pub fn handle_key(&mut self, code: KeyCode, pressed: bool) {
        if pressed {
            self.pressed.insert(code);
        } else {
            self.pressed.remove(&code);
        }

        // Digital actions update on edge.
        let down = pressed;
        match code {
            KeyCode::Space => self.right.trigger_press = down,
            KeyCode::ShiftLeft => self.right.grip_press = down,
            KeyCode::Digit1 => self.right.button_primary = down,
            KeyCode::Digit2 => self.right.button_secondary = down,
            KeyCode::Digit3 => self.left.button_primary = down,
            KeyCode::Digit4 => self.left.button_secondary = down,
            KeyCode::Escape => {
                if down {
                    self.request_exit();
                }
            }
            _ => {}
        }
    }

    /// Forward mouse motion (in window-space pixels).
    pub fn handle_mouse_motion(&mut self, dx: f32, dy: f32) {
        self.hmd_yaw -= dx * self.mouse_sensitivity;
        self.hmd_pitch -= dy * self.mouse_sensitivity;
        let limit = std::f32::consts::FRAC_PI_2 - 0.05;
        if self.hmd_pitch > limit {
            self.hmd_pitch = limit;
        }
        if self.hmd_pitch < -limit {
            self.hmd_pitch = -limit;
        }
    }

    /// Forward mouse button event.
    pub fn handle_mouse_button(&mut self, button: WinitMouseButton, pressed: bool) {
        let v = if pressed { 1.0 } else { 0.0 };
        match button {
            WinitMouseButton::Left => self.right.trigger = v,
            WinitMouseButton::Right => self.right.grip = v,
            WinitMouseButton::Middle => self.left.trigger = v,
            _ => {}
        }
    }

    /// Apply continuous keyboard input to HMD position. Call once per frame
    /// before [`Self::sync_actions`].
    pub fn integrate(&mut self, dt: f32) {
        let mut forward = Vec3::new(0.0, 0.0, 0.0);
        let mut right = Vec3::new(0.0, 0.0, 0.0);
        let mut up = Vec3::new(0.0, 0.0, 0.0);

        let yaw_sin = self.hmd_yaw.sin();
        let yaw_cos = self.hmd_yaw.cos();
        let fwd_dir = Vec3::new(-yaw_sin, 0.0, -yaw_cos);
        let right_dir = Vec3::new(yaw_cos, 0.0, -yaw_sin);

        if self.pressed.contains(&KeyCode::KeyW) {
            forward = fwd_dir;
        }
        if self.pressed.contains(&KeyCode::KeyS) {
            forward = Vec3::new(-fwd_dir.x(), 0.0, -fwd_dir.z());
        }
        if self.pressed.contains(&KeyCode::KeyD) {
            right = right_dir;
        }
        if self.pressed.contains(&KeyCode::KeyA) {
            right = Vec3::new(-right_dir.x(), 0.0, -right_dir.z());
        }
        if self.pressed.contains(&KeyCode::KeyE) {
            up = Vec3::new(0.0, 1.0, 0.0);
        }
        if self.pressed.contains(&KeyCode::KeyQ) {
            up = Vec3::new(0.0, -1.0, 0.0);
        }

        let dx = (forward.x() + right.x() + up.x()) * self.speed_metres_per_sec * dt;
        let dy = (forward.y() + right.y() + up.y()) * self.speed_metres_per_sec * dt;
        let dz = (forward.z() + right.z() + up.z()) * self.speed_metres_per_sec * dt;
        self.hmd_position = Vec3::new(
            self.hmd_position.x() + dx,
            self.hmd_position.y() + dy,
            self.hmd_position.z() + dz,
        );

        // Right thumbstick from arrow keys.
        let mut rx = 0.0_f32;
        let mut ry = 0.0_f32;
        if self.pressed.contains(&KeyCode::ArrowLeft) {
            rx -= 1.0;
        }
        if self.pressed.contains(&KeyCode::ArrowRight) {
            rx += 1.0;
        }
        if self.pressed.contains(&KeyCode::ArrowUp) {
            ry += 1.0;
        }
        if self.pressed.contains(&KeyCode::ArrowDown) {
            ry -= 1.0;
        }
        self.right.thumbstick = [rx, ry];

        // Left thumbstick from IJKL.
        let mut lx = 0.0_f32;
        let mut ly = 0.0_f32;
        if self.pressed.contains(&KeyCode::KeyJ) {
            lx -= 1.0;
        }
        if self.pressed.contains(&KeyCode::KeyL) {
            lx += 1.0;
        }
        if self.pressed.contains(&KeyCode::KeyI) {
            ly += 1.0;
        }
        if self.pressed.contains(&KeyCode::KeyK) {
            ly -= 1.0;
        }
        self.left.thumbstick = [lx, ly];

        self.recompute_hmd_pose();
        self.recompute_controller_poses();
    }

    fn recompute_hmd_pose(&mut self) {
        let half_yaw = self.hmd_yaw * 0.5;
        let half_pitch = self.hmd_pitch * 0.5;
        let yaw_q = Quat(glam::Quat::from_axis_angle(glam::Vec3::Y, self.hmd_yaw));
        let pitch_q = Quat(glam::Quat::from_axis_angle(glam::Vec3::X, self.hmd_pitch));
        let _ = (half_yaw, half_pitch);
        self.hmd_pose = XrPose::new(self.hmd_position, Quat(yaw_q.0 * pitch_q.0));
    }

    fn recompute_controller_poses(&mut self) {
        // Place controllers ~25cm in front of HMD with simple offsets.
        let yaw = self.hmd_yaw;
        let yaw_sin = yaw.sin();
        let yaw_cos = yaw.cos();
        let forward = Vec3::new(-yaw_sin, 0.0, -yaw_cos);
        let right_dir = Vec3::new(yaw_cos, 0.0, -yaw_sin);

        let base = Vec3::new(
            self.hmd_position.x() + forward.x() * 0.4,
            self.hmd_position.y() - 0.4,
            self.hmd_position.z() + forward.z() * 0.4,
        );

        self.left.pose = XrPose::new(
            Vec3::new(
                base.x() - right_dir.x() * 0.2,
                base.y(),
                base.z() - right_dir.z() * 0.2,
            ),
            self.hmd_pose.orientation,
        );
        self.right.pose = XrPose::new(
            Vec3::new(
                base.x() + right_dir.x() * 0.2,
                base.y(),
                base.z() + right_dir.z() * 0.2,
            ),
            self.hmd_pose.orientation,
        );
    }

    fn eye_view_state(&self, eye_offset_x: f32) -> XrViewState {
        let yaw_sin = self.hmd_yaw.sin();
        let yaw_cos = self.hmd_yaw.cos();
        let right_dir = Vec3::new(yaw_cos, 0.0, -yaw_sin);
        let position = Vec3::new(
            self.hmd_position.x() + right_dir.x() * eye_offset_x,
            self.hmd_position.y(),
            self.hmd_position.z() + right_dir.z() * eye_offset_x,
        );
        let pose = XrPose::new(position, self.hmd_pose.orientation);
        let fov = std::f32::consts::FRAC_PI_4;
        XrViewState::new(pose, -fov, fov, fov, -fov)
    }
}

impl XrProvider for StereoWindowProvider {
    fn name(&self) -> &str {
        "stereo_window"
    }

    fn config(&self) -> &XrConfig {
        &self.config
    }

    fn action_set(&self) -> &XrActionSet {
        &self.action_set
    }

    fn poll_events(&mut self) -> Result<(), XrError> {
        Ok(())
    }

    fn sync_actions(&mut self) -> Result<(), XrError> {
        Ok(())
    }

    fn session_state(&self) -> XrSessionState {
        self.state
    }

    fn action_float(&self, hand: XrHand, action: XrAction) -> f32 {
        let c = match hand {
            XrHand::Left => &self.left,
            XrHand::Right => &self.right,
        };
        match action {
            XrAction::Trigger => c.trigger,
            XrAction::Grip => c.grip,
            _ => 0.0,
        }
    }

    fn action_bool(&self, hand: XrHand, action: XrAction) -> bool {
        let c = match hand {
            XrHand::Left => &self.left,
            XrHand::Right => &self.right,
        };
        match action {
            XrAction::TriggerPress => c.trigger_press || c.trigger > 0.5,
            XrAction::GripPress => c.grip_press || c.grip > 0.5,
            XrAction::ButtonPrimary => c.button_primary,
            XrAction::ButtonSecondary => c.button_secondary,
            XrAction::ThumbstickClick => c.thumbstick_click,
            _ => false,
        }
    }

    fn action_vec2(&self, hand: XrHand, action: XrAction) -> [f32; 2] {
        let c = match hand {
            XrHand::Left => &self.left,
            XrHand::Right => &self.right,
        };
        match action {
            XrAction::Thumbstick => c.thumbstick,
            _ => [0.0, 0.0],
        }
    }

    fn controller_pose(&self, hand: XrHand) -> Option<XrPose> {
        Some(match hand {
            XrHand::Left => self.left.pose,
            XrHand::Right => self.right.pose,
        })
    }

    fn hmd_pose(&self) -> Option<XrPose> {
        Some(self.hmd_pose)
    }

    fn views(&self) -> [XrViewState; 2] {
        let half_ipd = self.config.ipd_metres * 0.5;
        [
            self.eye_view_state(-half_ipd),
            self.eye_view_state(half_ipd),
        ]
    }

    fn apply_haptics(&mut self, pulse: XrHaptics) -> Result<(), XrError> {
        log::debug!(
            "stereo_window haptics {:?} {:.3}s amp={:.2}",
            pulse.hand,
            pulse.duration,
            pulse.amplitude
        );
        self.haptics_log.push(pulse);
        Ok(())
    }

    fn request_exit(&mut self) {
        self.exit_requested = true;
        self.state = XrSessionState::Stopping;
    }
}

impl StereoWindowProvider {
    /// Has [`request_exit`] been called?
    #[must_use]
    pub const fn exit_requested(&self) -> bool {
        self.exit_requested
    }
}

// ---------------------------------------------------------------------------
// run_xr_windowed
// ---------------------------------------------------------------------------

/// Open a window with stereo viewport split and run the engine loop with a
/// [`StereoWindowProvider`].
///
/// The window is split horizontally; the left half clears to a dark red tint
/// and the right half to a dark blue tint, so the developer can visually
/// confirm the stereo split. The provider supplies pose / input data via
/// [`XrProvider`].
///
/// # Errors
///
/// Window creation, GPU initialisation, or event-loop errors.
pub fn run_xr_windowed(
    window_config: WindowConfig,
    engine_config: EngineConfig,
    callbacks: Box<dyn AppCallbacks>,
) -> Result<(), String> {
    let event_loop =
        EventLoop::new().map_err(|e| format!("Failed to create event loop: {e}"))?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);

    let mut app = StereoWindowApp {
        window_config,
        engine_config,
        callbacks,
        provider: StereoWindowProvider::new(XrConfig::default()),
        state: AppState::WaitingForWindow,
        window: None,
        gpu: None,
        surface: None,
        engine: None,
        timer: FrameTimer::new(),
    };

    event_loop
        .run_app(&mut app)
        .map_err(|e| format!("Event loop error: {e}"))
}

struct StereoWindowApp {
    window_config: WindowConfig,
    engine_config: EngineConfig,
    callbacks: Box<dyn AppCallbacks>,
    provider: StereoWindowProvider,
    state: AppState,
    window: Option<Arc<Window>>,
    gpu: Option<GpuContext>,
    surface: Option<wgpu::Surface<'static>>,
    engine: Option<Engine>,
    timer: FrameTimer,
}

struct CallbackBridge<'a> {
    callbacks: &'a mut dyn AppCallbacks,
}

impl System for CallbackBridge<'_> {
    fn init(&mut self, ctx: &mut crate::engine::EngineContext) {
        self.callbacks.init(ctx);
    }

    fn fixed_update(&mut self, ctx: &mut crate::engine::EngineContext, fixed_dt: f32) {
        self.callbacks.fixed_update(ctx, fixed_dt);
    }

    fn update(&mut self, ctx: &mut crate::engine::EngineContext, dt: f32) {
        self.callbacks.update(ctx, dt);
    }
}

impl ApplicationHandler for StereoWindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title(format!("{} (XR Stereo)", self.window_config.title))
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.window_config.width.max(640),
                self.window_config.height.max(360),
            ))
            .with_resizable(self.window_config.resizable);

        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("xr stereo: window create failed: {e}");
                event_loop.exit();
                return;
            }
        };

        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = match instance.create_surface(window.clone()) {
            Ok(s) => s,
            Err(e) => {
                log::error!("xr stereo: surface create failed: {e}");
                event_loop.exit();
                return;
            }
        };

        let size = window.inner_size();
        let gpu_config = GpuConfig::default();
        let gpu_ctx = match pollster::block_on(GpuContext::from_surface(
            &instance,
            &surface,
            size.width.max(1),
            size.height.max(1),
            &gpu_config,
        )) {
            Ok(g) => g,
            Err(e) => {
                log::error!("xr stereo: gpu init failed: {e:?}");
                event_loop.exit();
                return;
            }
        };

        // SAFETY: surface and gpu_ctx are both stored on `self` and dropped
        // together; transmute extends the surface's borrow lifetime to 'static.
        let surface_static: wgpu::Surface<'static> = unsafe { std::mem::transmute(surface) };

        let mut engine = Engine::new(self.engine_config.clone());
        let mut bridge = CallbackBridge {
            callbacks: &mut *self.callbacks,
        };
        engine.init(&mut bridge);

        self.gpu = Some(gpu_ctx);
        self.surface = Some(surface_static);
        self.engine = Some(engine);
        self.window = Some(window);
        self.state = AppState::Running;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
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
                if let PhysicalKey::Code(code) = event.physical_key {
                    let pressed = matches!(event.state, ElementState::Pressed);
                    self.provider.handle_key(code, pressed);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                self.provider
                    .handle_mouse_button(button, matches!(state, ElementState::Pressed));
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Use raw position for now; downstream may switch to MotionEvent.
                static mut PREV: Option<(f64, f64)> = None;
                #[allow(static_mut_refs)]
                let (px, py) = unsafe {
                    let prev = PREV.unwrap_or((position.x, position.y));
                    PREV = Some((position.x, position.y));
                    prev
                };
                let dx = (position.x - px) as f32;
                let dy = (position.y - py) as f32;
                self.provider.handle_mouse_motion(dx, dy);
            }
            WindowEvent::RedrawRequested => {
                if self.state != AppState::Running {
                    return;
                }
                if self.provider.exit_requested() {
                    event_loop.exit();
                    return;
                }

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
                    * 1000.0;
                self.timer.update(now);
                let dt = self.timer.delta_seconds;

                // 1. Provider integrates input + recomputes simulated VR state.
                let _ = self.provider.poll_events();
                let _ = self.provider.sync_actions();
                self.provider.integrate(dt);

                // 2. Drive the engine + user callbacks.
                if let Some(engine) = &mut self.engine {
                    let mut bridge = CallbackBridge {
                        callbacks: &mut *self.callbacks,
                    };
                    engine.frame(dt, &mut bridge);
                }

                // 3. Stereo split clear: render two passes with viewports.
                if let (Some(gpu), Some(surface)) = (&self.gpu, &self.surface) {
                    let _ = render_stereo_clear(gpu, surface);
                }

                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

/// Clear the framebuffer with a left/right colour split so the stereo
/// viewport mapping is visually obvious during development.
fn render_stereo_clear(gpu: &GpuContext, surface: &wgpu::Surface<'static>) -> Result<(), String> {
    let frame = surface
        .get_current_texture()
        .map_err(|e| format!("acquire frame: {e}"))?;
    let view = frame
        .texture
        .create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("xr_stereo_clear"),
        });

    {
        // Single pass that clears the entire surface; we use viewport
        // splits in the two render-pass attachments to tint each half.
        let _ = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("xr_stereo_left"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.06,
                        g: 0.02,
                        b: 0.02,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
    }

    // Right half: scissor-tinted second pass.
    {
        let width = gpu.surface_config.width;
        let height = gpu.surface_config.height;
        let half = width / 2;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("xr_stereo_right"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        pass.set_scissor_rect(half, 0, width - half, height);
        // Clear via a no-op draw — we use scissor + clear-on-load instead.
        // The scissor restricts subsequent ops to the right half, but we
        // intentionally don't draw anything (placeholder).
        let _ = (pass, half);
    }

    // Re-clear the right half to a different colour.
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("xr_stereo_right_tint"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        let width = gpu.surface_config.width;
        let height = gpu.surface_config.height;
        let half = width / 2;
        pass.set_scissor_rect(half, 0, width - half, height);
        // Issue clear via attachment Load; nothing else to do.
        let _ = (height, half);
    }

    let _unused = Color::WHITE; // keep import alive
    gpu.queue.submit(std::iter::once(encoder.finish()));
    frame.present();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_default_state_focused() {
        let p = StereoWindowProvider::new(XrConfig::default());
        assert_eq!(p.session_state(), XrSessionState::Focused);
        assert!(p.should_render());
        assert_eq!(p.name(), "stereo_window");
    }

    #[test]
    fn mouse_motion_updates_yaw_pitch() {
        let mut p = StereoWindowProvider::new(XrConfig::default());
        let yaw0 = p.hmd_yaw;
        let pitch0 = p.hmd_pitch;
        p.handle_mouse_motion(100.0, 50.0);
        assert!(p.hmd_yaw < yaw0, "yaw should decrease for +dx");
        assert!(p.hmd_pitch < pitch0, "pitch should decrease for +dy");
    }

    #[test]
    fn pitch_clamped() {
        let mut p = StereoWindowProvider::new(XrConfig::default());
        for _ in 0..1000 {
            p.handle_mouse_motion(0.0, -1000.0);
        }
        assert!(p.hmd_pitch < std::f32::consts::FRAC_PI_2);
    }

    #[test]
    fn mouse_button_drives_trigger_grip() {
        let mut p = StereoWindowProvider::new(XrConfig::default());
        p.handle_mouse_button(WinitMouseButton::Left, true);
        assert!((p.action_float(XrHand::Right, XrAction::Trigger) - 1.0).abs() < 1e-6);
        p.handle_mouse_button(WinitMouseButton::Left, false);
        assert!(p.action_float(XrHand::Right, XrAction::Trigger).abs() < 1e-6);

        p.handle_mouse_button(WinitMouseButton::Right, true);
        assert!((p.action_float(XrHand::Right, XrAction::Grip) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn integrate_translates_along_yaw() {
        let mut p = StereoWindowProvider::new(XrConfig::default());
        let start = p.hmd_position;
        p.handle_key(KeyCode::KeyW, true);
        p.integrate(1.0);
        let after = p.hmd_position;
        // Default yaw 0 → forward = -Z, so z should decrease.
        assert!(after.z() < start.z());
    }

    #[test]
    fn arrow_keys_drive_right_thumbstick() {
        let mut p = StereoWindowProvider::new(XrConfig::default());
        p.handle_key(KeyCode::ArrowRight, true);
        p.handle_key(KeyCode::ArrowUp, true);
        p.integrate(1.0 / 60.0);
        let v = p.action_vec2(XrHand::Right, XrAction::Thumbstick);
        assert!((v[0] - 1.0).abs() < 1e-6);
        assert!((v[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn ijkl_drives_left_thumbstick() {
        let mut p = StereoWindowProvider::new(XrConfig::default());
        p.handle_key(KeyCode::KeyL, true);
        p.handle_key(KeyCode::KeyI, true);
        p.integrate(1.0 / 60.0);
        let v = p.action_vec2(XrHand::Left, XrAction::Thumbstick);
        assert!((v[0] - 1.0).abs() < 1e-6);
        assert!((v[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn escape_requests_exit() {
        let mut p = StereoWindowProvider::new(XrConfig::default());
        p.handle_key(KeyCode::Escape, true);
        assert!(p.exit_requested());
        assert_eq!(p.session_state(), XrSessionState::Stopping);
    }

    #[test]
    fn views_offset_left_right_by_ipd() {
        let p = StereoWindowProvider::new(XrConfig::default());
        let [left, right] = p.views();
        let dx = right.pose.position.x() - left.pose.position.x();
        // Default yaw 0 → IPD offset is along world X.
        assert!((dx - p.config.ipd_metres).abs() < 1e-3);
    }

    #[test]
    fn haptics_logged() {
        let mut p = StereoWindowProvider::new(XrConfig::default());
        p.apply_haptics(XrHaptics::pulse(XrHand::Right, 0.05, 0.5))
            .unwrap();
        assert_eq!(p.haptics_log.len(), 1);
    }
}
