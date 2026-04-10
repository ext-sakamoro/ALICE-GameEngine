//! [`XrProvider`] trait — abstract input/output for `XR` backends.

use super::types::{
    XrAction, XrActionSet, XrConfig, XrError, XrHand, XrHaptics, XrPose, XrSessionState,
    XrViewState,
};

/// A backend that produces `XR` poses, input state, and consumes haptics.
///
/// Concrete backends include:
/// - [`super::MockProvider`] — canned data for unit tests
/// - [`super::stereo_window::StereoWindowProvider`] (feature `window`) — winit-driven simulator
///
/// Future backends (Quest `VrApi`, `OpenVR` `FFI`, network bridge) implement this
/// same trait without leaking external runtime types.
pub trait XrProvider: Send {
    /// Provider name (for logging / debugging).
    fn name(&self) -> &str;

    /// Configuration this provider was constructed with.
    fn config(&self) -> &XrConfig;

    /// Currently-attached action set.
    fn action_set(&self) -> &XrActionSet;

    /// Drain runtime events and update internal state.
    ///
    /// Should be called once per frame at the start of the loop.
    ///
    /// # Errors
    ///
    /// Backend-specific runtime errors.
    fn poll_events(&mut self) -> Result<(), XrError>;

    /// Synchronise input device state with the runtime / OS.
    ///
    /// Called once per frame after [`Self::poll_events`].
    ///
    /// # Errors
    ///
    /// Backend-specific runtime errors.
    fn sync_actions(&mut self) -> Result<(), XrError>;

    /// Current session lifecycle state.
    fn session_state(&self) -> XrSessionState;

    /// Should the application render this frame?
    fn should_render(&self) -> bool {
        self.session_state().should_render()
    }

    /// Read an analog action (`Trigger`, `Grip`, …).
    fn action_float(&self, hand: XrHand, action: XrAction) -> f32;

    /// Read a digital action (`TriggerPress`, `ButtonPrimary`, …).
    fn action_bool(&self, hand: XrHand, action: XrAction) -> bool;

    /// Read a `Vec2` action (`Thumbstick`).
    fn action_vec2(&self, hand: XrHand, action: XrAction) -> [f32; 2];

    /// Controller pose in tracking space, if known.
    fn controller_pose(&self, hand: XrHand) -> Option<XrPose>;

    /// HMD pose in tracking space, if known.
    fn hmd_pose(&self) -> Option<XrPose>;

    /// Per-eye view states for the current frame.
    fn views(&self) -> [XrViewState; 2];

    /// Apply a haptic pulse on a controller.
    ///
    /// # Errors
    ///
    /// Backend-specific runtime errors.
    fn apply_haptics(&mut self, pulse: XrHaptics) -> Result<(), XrError>;

    /// Request the session to begin shutdown (e.g. user presses Escape).
    ///
    /// Default implementation is a no-op; backends override as needed.
    fn request_exit(&mut self) {}
}
