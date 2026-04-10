//! # `XR` (`OpenXR`) Integration
//!
//! Minimal `OpenXR` scaffolding for `VR`/`AR` rendering. Gated behind the `xr` feature.
//!
//! Provides:
//! - [`XrConfig`] — application metadata
//! - [`XrSession`] — instance + system + session handles
//! - [`XrViewState`] — per-eye view/projection state
//! - [`XrActionSet`] — controller input bindings
//! - [`XrHaptics`] — controller vibration
//!
//! ## Example
//!
//! ```no_run
//! # #[cfg(feature = "xr")] {
//! use alice_game_engine::xr::{XrConfig, init_openxr};
//!
//! let config = XrConfig::new("My VR Game", (1, 0, 0));
//! let session = init_openxr(&config).expect("OpenXR init failed");
//! # }
//! ```
//!
//! ## Status
//!
//! This module currently provides the public API surface and instance/session
//! creation only. Swapchain allocation, frame submission, and render-loop
//! integration are TODO and will be implemented alongside the first consumer
//! (e.g. `DoujinGameEngine::h_vr`).

use crate::math::{Mat4, Quat, Vec3};

/// Application metadata passed to the `OpenXR` runtime on instance creation.
#[derive(Debug, Clone)]
pub struct XrConfig {
    pub app_name: String,
    pub app_version: (u16, u16, u16),
    pub engine_name: String,
    pub engine_version: (u16, u16, u16),
    pub form_factor: XrFormFactor,
    pub view_configuration: XrViewConfiguration,
    pub blend_mode: XrBlendMode,
}

impl XrConfig {
    #[must_use]
    pub fn new(app_name: impl Into<String>, app_version: (u16, u16, u16)) -> Self {
        Self {
            app_name: app_name.into(),
            app_version,
            engine_name: "ALICE-GameEngine".to_string(),
            engine_version: (0, 5, 0),
            form_factor: XrFormFactor::HeadMountedDisplay,
            view_configuration: XrViewConfiguration::Stereo,
            blend_mode: XrBlendMode::Opaque,
        }
    }
}

impl Default for XrConfig {
    fn default() -> Self {
        Self::new("ALICE XR App", (0, 1, 0))
    }
}

/// Physical device class the runtime should address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrFormFactor {
    HeadMountedDisplay,
    HandheldDisplay,
}

/// View layout requested from the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrViewConfiguration {
    Mono,
    Stereo,
    Quad,
}

impl XrViewConfiguration {
    #[must_use]
    pub const fn view_count(self) -> usize {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Quad => 4,
        }
    }
}

/// Environment blend mode (VR = Opaque, AR = AlphaBlend/Additive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrBlendMode {
    Opaque,
    Additive,
    AlphaBlend,
}

/// Errors returned by the `XR` subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XrError {
    /// The `xr` feature is enabled but the underlying runtime could not be loaded.
    RuntimeUnavailable(String),
    /// Requested form factor is not available on this device.
    FormFactorUnavailable,
    /// System does not support the requested view configuration.
    ViewConfigurationUnsupported,
    /// `OpenXR` call returned a non-success result.
    OpenXrCall(String),
    /// Feature not yet implemented.
    NotImplemented(&'static str),
}

impl core::fmt::Display for XrError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::RuntimeUnavailable(s) => write!(f, "XR runtime unavailable: {s}"),
            Self::FormFactorUnavailable => write!(f, "XR form factor unavailable"),
            Self::ViewConfigurationUnsupported => write!(f, "XR view configuration unsupported"),
            Self::OpenXrCall(s) => write!(f, "OpenXR call failed: {s}"),
            Self::NotImplemented(s) => write!(f, "XR not implemented: {s}"),
        }
    }
}

impl std::error::Error for XrError {}

/// Per-eye view and projection computed from an `OpenXR` frame.
#[derive(Debug, Clone, Copy)]
pub struct XrViewState {
    pub position: Vec3,
    pub orientation: Quat,
    pub fov_angle_left: f32,
    pub fov_angle_right: f32,
    pub fov_angle_up: f32,
    pub fov_angle_down: f32,
}

impl XrViewState {
    #[must_use]
    pub const fn new(
        position: Vec3,
        orientation: Quat,
        fov_angle_left: f32,
        fov_angle_right: f32,
        fov_angle_up: f32,
        fov_angle_down: f32,
    ) -> Self {
        Self {
            position,
            orientation,
            fov_angle_left,
            fov_angle_right,
            fov_angle_up,
            fov_angle_down,
        }
    }

    /// Compute an asymmetric projection matrix from the view's FOV half-angles.
    ///
    /// Returns a right-handed, reverse-Z-free projection suitable for rendering
    /// a single eye into an `OpenXR` swapchain image.
    #[must_use]
    pub fn projection_matrix(&self, near: f32, far: f32) -> Mat4 {
        let tan_left = self.fov_angle_left.tan();
        let tan_right = self.fov_angle_right.tan();
        let tan_up = self.fov_angle_up.tan();
        let tan_down = self.fov_angle_down.tan();

        let width_recip = (tan_right - tan_left).recip();
        let height_recip = (tan_up - tan_down).recip();
        let depth_recip = (far - near).recip();

        let cols = [
            2.0 * width_recip,
            0.0,
            0.0,
            0.0,
            //
            0.0,
            2.0 * height_recip,
            0.0,
            0.0,
            //
            (tan_right + tan_left) * width_recip,
            (tan_up + tan_down) * height_recip,
            -(far + near) * depth_recip,
            -1.0,
            //
            0.0,
            0.0,
            -2.0 * far * near * depth_recip,
            0.0,
        ];
        Mat4(glam::Mat4::from_cols_array(&cols))
    }
}

/// Controller / hand identifier for action bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XrHand {
    Left,
    Right,
}

/// Standard controller actions mapped from the interaction profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XrAction {
    /// Grip squeeze (analog).
    Grip,
    /// Grip button press (digital).
    GripPress,
    /// Trigger pull (analog).
    Trigger,
    /// Trigger button press (digital).
    TriggerPress,
    /// Thumbstick 2D.
    Thumbstick,
    /// Thumbstick press.
    ThumbstickClick,
    /// Primary face button (A/X).
    ButtonPrimary,
    /// Secondary face button (B/Y).
    ButtonSecondary,
    /// Menu button.
    Menu,
}

/// Action set holding the standard bindings for both hands.
#[derive(Debug, Default, Clone)]
pub struct XrActionSet {
    pub name: String,
    pub left_actions: Vec<XrAction>,
    pub right_actions: Vec<XrAction>,
}

impl XrActionSet {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let standard = vec![
            XrAction::Grip,
            XrAction::GripPress,
            XrAction::Trigger,
            XrAction::TriggerPress,
            XrAction::Thumbstick,
            XrAction::ThumbstickClick,
        ];
        Self {
            name: name.into(),
            left_actions: standard.clone(),
            right_actions: standard,
        }
    }

    pub fn bind(&mut self, hand: XrHand, action: XrAction) {
        let list = match hand {
            XrHand::Left => &mut self.left_actions,
            XrHand::Right => &mut self.right_actions,
        };
        if !list.contains(&action) {
            list.push(action);
        }
    }

    #[must_use]
    pub fn is_bound(&self, hand: XrHand, action: XrAction) -> bool {
        let list = match hand {
            XrHand::Left => &self.left_actions,
            XrHand::Right => &self.right_actions,
        };
        list.contains(&action)
    }
}

/// Haptic pulse request to send to a controller.
#[derive(Debug, Clone, Copy)]
pub struct XrHaptics {
    pub hand: XrHand,
    /// Duration in seconds.
    pub duration: f32,
    /// Frequency in Hz (0.0 = runtime default).
    pub frequency: f32,
    /// Amplitude 0.0..=1.0.
    pub amplitude: f32,
}

impl XrHaptics {
    #[must_use]
    pub const fn pulse(hand: XrHand, duration: f32, amplitude: f32) -> Self {
        Self {
            hand,
            duration,
            frequency: 0.0,
            amplitude,
        }
    }
}

/// Lifetime state of an `OpenXR` session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrSessionState {
    Idle,
    Ready,
    Synchronized,
    Visible,
    Focused,
    Stopping,
    LossPending,
    Exiting,
}

/// An initialized `OpenXR` session (instance + system + session handles).
///
/// With the `xr` feature disabled this type still exists as a configuration
/// carrier so that downstream crates can compile against the API without
/// pulling in `openxr`.
pub struct XrSession {
    pub config: XrConfig,
    pub state: XrSessionState,
    pub action_set: XrActionSet,
    #[cfg(feature = "xr")]
    #[allow(dead_code)]
    pub(crate) inner: XrSessionInner,
}

#[cfg(feature = "xr")]
#[allow(dead_code)]
pub(crate) struct XrSessionInner {
    pub instance: openxr::Instance,
    pub system: openxr::SystemId,
}

impl XrSession {
    /// Latest session lifecycle state.
    #[must_use]
    pub const fn state(&self) -> XrSessionState {
        self.state
    }

    /// Is the session in a state where the app should render frames?
    #[must_use]
    pub const fn should_render(&self) -> bool {
        matches!(
            self.state,
            XrSessionState::Visible | XrSessionState::Focused
        )
    }

    /// Poll runtime events and update `self.state`.
    ///
    /// Currently a stub — real implementation will drain
    /// `Instance::poll_event()` and handle session state changes.
    ///
    /// # Errors
    ///
    /// Returns [`XrError::NotImplemented`] until the real event loop lands.
    pub const fn poll_events(&mut self) -> Result<(), XrError> {
        Err(XrError::NotImplemented("XrSession::poll_events"))
    }

    /// Request a haptic pulse on a controller.
    ///
    /// Stub — real implementation will call
    /// `openxr::Session::apply_haptic_feedback()`.
    ///
    /// # Errors
    ///
    /// Returns [`XrError::NotImplemented`] until the real haptics path lands.
    pub const fn apply_haptics(&mut self, _pulse: XrHaptics) -> Result<(), XrError> {
        Err(XrError::NotImplemented("XrSession::apply_haptics"))
    }
}

/// Initialize an `OpenXR` instance + system + session.
///
/// With the `xr` feature disabled this returns a configuration-only session
/// (no runtime handles) so that library consumers can still construct type
/// definitions and compile on non-`XR` targets.
///
/// # Errors
///
/// - [`XrError::RuntimeUnavailable`] if the platform `OpenXR` loader library
///   cannot be found or fails to load.
/// - [`XrError::OpenXrCall`] if `xrCreateInstance` fails.
/// - [`XrError::FormFactorUnavailable`] if the requested form factor is not
///   provided by the active runtime.
#[cfg(feature = "xr")]
pub fn init_openxr(config: &XrConfig) -> Result<XrSession, XrError> {
    use openxr as xr;

    // SAFETY: `Entry::load()` opens the platform OpenXR loader shared library.
    // The loader is expected to be installed by the user or runtime (e.g.
    // Monado, SteamVR, Oculus Runtime). Failure is returned as `RuntimeUnavailable`.
    let entry = unsafe { xr::Entry::load() }
        .map_err(|e| XrError::RuntimeUnavailable(format!("loader: {e}")))?;

    let app_info = xr::ApplicationInfo {
        application_name: &config.app_name,
        application_version: (u32::from(config.app_version.0) << 16)
            | (u32::from(config.app_version.1) << 8)
            | u32::from(config.app_version.2),
        engine_name: &config.engine_name,
        engine_version: (u32::from(config.engine_version.0) << 16)
            | (u32::from(config.engine_version.1) << 8)
            | u32::from(config.engine_version.2),
        api_version: xr::Version::new(1, 0, 0),
    };

    let extensions = xr::ExtensionSet::default();

    let instance = entry
        .create_instance(&app_info, &extensions, &[])
        .map_err(|e| XrError::OpenXrCall(format!("create_instance: {e}")))?;

    let form_factor = match config.form_factor {
        XrFormFactor::HeadMountedDisplay => xr::FormFactor::HEAD_MOUNTED_DISPLAY,
        XrFormFactor::HandheldDisplay => xr::FormFactor::HANDHELD_DISPLAY,
    };

    let system = instance
        .system(form_factor)
        .map_err(|_| XrError::FormFactorUnavailable)?;

    Ok(XrSession {
        config: config.clone(),
        state: XrSessionState::Idle,
        action_set: XrActionSet::new("default"),
        inner: XrSessionInner { instance, system },
    })
}

/// Initialize an `OpenXR` instance + system + session (stub variant).
///
/// Without the `xr` feature this returns [`XrError::RuntimeUnavailable`].
///
/// # Errors
///
/// Always returns [`XrError::RuntimeUnavailable`] when the `xr` feature is
/// disabled.
#[cfg(not(feature = "xr"))]
pub fn init_openxr(_config: &XrConfig) -> Result<XrSession, XrError> {
    Err(XrError::RuntimeUnavailable(
        "xr feature is not enabled".to_string(),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xr_config_defaults() {
        let cfg = XrConfig::default();
        assert_eq!(cfg.form_factor, XrFormFactor::HeadMountedDisplay);
        assert_eq!(cfg.view_configuration, XrViewConfiguration::Stereo);
        assert_eq!(cfg.blend_mode, XrBlendMode::Opaque);
        assert_eq!(cfg.engine_name, "ALICE-GameEngine");
    }

    #[test]
    fn view_count_matches_configuration() {
        assert_eq!(XrViewConfiguration::Mono.view_count(), 1);
        assert_eq!(XrViewConfiguration::Stereo.view_count(), 2);
        assert_eq!(XrViewConfiguration::Quad.view_count(), 4);
    }

    #[test]
    fn action_set_has_standard_bindings() {
        let set = XrActionSet::new("gameplay");
        assert!(set.is_bound(XrHand::Left, XrAction::Trigger));
        assert!(set.is_bound(XrHand::Right, XrAction::Grip));
        assert!(set.is_bound(XrHand::Right, XrAction::Thumbstick));
    }

    #[test]
    fn action_set_bind_is_idempotent() {
        let mut set = XrActionSet::new("gameplay");
        let before = set.left_actions.len();
        set.bind(XrHand::Left, XrAction::Trigger);
        assert_eq!(set.left_actions.len(), before);
        set.bind(XrHand::Left, XrAction::Menu);
        assert_eq!(set.left_actions.len(), before + 1);
    }

    #[test]
    fn haptics_pulse_constructor() {
        let p = XrHaptics::pulse(XrHand::Right, 0.1, 0.5);
        assert_eq!(p.hand, XrHand::Right);
        assert!((p.duration - 0.1).abs() < 1e-6);
        assert!((p.amplitude - 0.5).abs() < 1e-6);
    }

    #[test]
    fn projection_matrix_is_finite() {
        let view = XrViewState::new(
            Vec3::new(0.0, 1.7, 0.0),
            Quat::default(),
            -0.8,
            0.8,
            0.9,
            -0.9,
        );
        let proj = view.projection_matrix(0.05, 100.0);
        for v in proj.0.to_cols_array() {
            assert!(v.is_finite(), "projection matrix has non-finite value");
        }
    }

    #[cfg(not(feature = "xr"))]
    #[test]
    fn init_without_feature_returns_runtime_unavailable() {
        let cfg = XrConfig::default();
        let result = init_openxr(&cfg);
        assert!(matches!(result, Err(XrError::RuntimeUnavailable(_))));
    }

    #[test]
    fn session_state_render_predicate() {
        // Construct a bare session by hand (without feature to avoid openxr).
        #[cfg(not(feature = "xr"))]
        {
            let session = XrSession {
                config: XrConfig::default(),
                state: XrSessionState::Focused,
                action_set: XrActionSet::new("default"),
            };
            assert!(session.should_render());
        }
    }
}
