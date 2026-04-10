//! Common data types for the `XR` layer.

use crate::math::{Mat4, Quat, Vec3};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Application metadata supplied to the `XR` provider on initialization.
#[derive(Debug, Clone)]
pub struct XrConfig {
    pub app_name: String,
    pub app_version: (u16, u16, u16),
    pub engine_name: String,
    pub engine_version: (u16, u16, u16),
    pub form_factor: XrFormFactor,
    pub view_configuration: XrViewConfiguration,
    pub blend_mode: XrBlendMode,
    /// Inter-pupillary distance in metres. Used by the stereo window
    /// simulator and any provider that needs an explicit IPD.
    pub ipd_metres: f32,
}

impl XrConfig {
    /// Construct a config for the given app.
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
            ipd_metres: 0.064,
        }
    }
}

impl Default for XrConfig {
    fn default() -> Self {
        Self::new("ALICE XR App", (0, 1, 0))
    }
}

/// Physical device class addressed by the runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrFormFactor {
    HeadMountedDisplay,
    HandheldDisplay,
}

/// View layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrViewConfiguration {
    Mono,
    Stereo,
    Quad,
}

impl XrViewConfiguration {
    /// Number of views the runtime is expected to produce per frame.
    #[must_use]
    pub const fn view_count(self) -> usize {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
            Self::Quad => 4,
        }
    }
}

/// Environment blend mode (`VR` = Opaque, `AR` = `AlphaBlend`/Additive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrBlendMode {
    Opaque,
    Additive,
    AlphaBlend,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors returned by the `XR` subsystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum XrError {
    /// Provider could not be initialised (no display, no runtime, etc).
    ProviderUnavailable(String),
    /// Requested form factor is not supported.
    FormFactorUnavailable,
    /// Requested view configuration is not supported.
    ViewConfigurationUnsupported,
    /// Provider-defined runtime error.
    Backend(String),
    /// Feature not yet implemented in this provider.
    NotImplemented(&'static str),
}

impl core::fmt::Display for XrError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ProviderUnavailable(s) => write!(f, "XR provider unavailable: {s}"),
            Self::FormFactorUnavailable => write!(f, "XR form factor unavailable"),
            Self::ViewConfigurationUnsupported => write!(f, "XR view configuration unsupported"),
            Self::Backend(s) => write!(f, "XR backend error: {s}"),
            Self::NotImplemented(s) => write!(f, "XR not implemented: {s}"),
        }
    }
}

impl std::error::Error for XrError {}

// ---------------------------------------------------------------------------
// Pose / view state
// ---------------------------------------------------------------------------

/// Position + orientation in tracking space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct XrPose {
    pub position: Vec3,
    pub orientation: Quat,
}

impl XrPose {
    #[must_use]
    pub const fn new(position: Vec3, orientation: Quat) -> Self {
        Self {
            position,
            orientation,
        }
    }

    /// Identity pose at the origin.
    #[must_use]
    pub fn identity() -> Self {
        Self {
            position: Vec3::default(),
            orientation: Quat::default(),
        }
    }

    /// View matrix (world → eye) corresponding to this pose.
    #[must_use]
    pub fn view_matrix(&self) -> Mat4 {
        let rot = Mat4::from_rotation(self.orientation);
        let trans = Mat4::from_translation(self.position);
        (trans * rot).inverse()
    }
}

impl Default for XrPose {
    fn default() -> Self {
        Self::identity()
    }
}

/// Per-eye view + projection state computed from a single frame.
#[derive(Debug, Clone, Copy)]
pub struct XrViewState {
    pub pose: XrPose,
    /// Half-angle, radians.
    pub fov_angle_left: f32,
    /// Half-angle, radians.
    pub fov_angle_right: f32,
    /// Half-angle, radians.
    pub fov_angle_up: f32,
    /// Half-angle, radians.
    pub fov_angle_down: f32,
}

impl XrViewState {
    #[must_use]
    pub const fn new(
        pose: XrPose,
        fov_angle_left: f32,
        fov_angle_right: f32,
        fov_angle_up: f32,
        fov_angle_down: f32,
    ) -> Self {
        Self {
            pose,
            fov_angle_left,
            fov_angle_right,
            fov_angle_up,
            fov_angle_down,
        }
    }

    /// Asymmetric perspective projection from the view's `FOV` half-angles.
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

impl Default for XrViewState {
    fn default() -> Self {
        let fov = std::f32::consts::FRAC_PI_4;
        Self::new(XrPose::identity(), -fov, fov, fov, -fov)
    }
}

// ---------------------------------------------------------------------------
// Hand / action
// ---------------------------------------------------------------------------

/// Controller / hand identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum XrHand {
    Left,
    Right,
}

/// Standard controller actions.
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
    /// Thumbstick X/Y (analog).
    Thumbstick,
    /// Thumbstick click.
    ThumbstickClick,
    /// Primary face button (A/X).
    ButtonPrimary,
    /// Secondary face button (B/Y).
    ButtonSecondary,
    /// Menu / system button.
    Menu,
}

/// Action set: which actions are bound on which hand.
#[derive(Debug, Default, Clone)]
pub struct XrActionSet {
    pub name: String,
    pub left_actions: Vec<XrAction>,
    pub right_actions: Vec<XrAction>,
}

impl XrActionSet {
    /// New action set with the standard gameplay bindings on both hands.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let standard = vec![
            XrAction::Grip,
            XrAction::GripPress,
            XrAction::Trigger,
            XrAction::TriggerPress,
            XrAction::Thumbstick,
            XrAction::ThumbstickClick,
            XrAction::ButtonPrimary,
            XrAction::ButtonSecondary,
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

// ---------------------------------------------------------------------------
// Haptics
// ---------------------------------------------------------------------------

/// Haptic pulse description.
#[derive(Debug, Clone, Copy)]
pub struct XrHaptics {
    pub hand: XrHand,
    /// Duration in seconds.
    pub duration: f32,
    /// Frequency in Hz (0.0 = backend default).
    pub frequency: f32,
    /// Amplitude in `[0.0, 1.0]`.
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

// ---------------------------------------------------------------------------
// Session lifecycle
// ---------------------------------------------------------------------------

/// Lifetime state of an `XR` session.
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

impl XrSessionState {
    /// Should the application render frames in this state?
    #[must_use]
    pub const fn should_render(self) -> bool {
        matches!(self, Self::Visible | Self::Focused)
    }
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
        assert!((cfg.ipd_metres - 0.064).abs() < 1e-6);
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
        assert!(set.is_bound(XrHand::Left, XrAction::ButtonPrimary));
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
        let view = XrViewState::default();
        let proj = view.projection_matrix(0.05, 100.0);
        for v in proj.0.to_cols_array() {
            assert!(v.is_finite(), "projection matrix has non-finite value");
        }
    }

    #[test]
    fn session_state_render_predicate() {
        assert!(XrSessionState::Focused.should_render());
        assert!(XrSessionState::Visible.should_render());
        assert!(!XrSessionState::Idle.should_render());
        assert!(!XrSessionState::Stopping.should_render());
    }

    #[test]
    fn pose_view_matrix_inverts_translation() {
        let pose = XrPose::new(Vec3::new(1.0, 2.0, 3.0), Quat::default());
        let view = pose.view_matrix();
        let world_origin = view.transform_point3(Vec3::new(1.0, 2.0, 3.0));
        assert!(world_origin.x().abs() < 1e-4);
        assert!(world_origin.y().abs() < 1e-4);
        assert!(world_origin.z().abs() < 1e-4);
    }
}
