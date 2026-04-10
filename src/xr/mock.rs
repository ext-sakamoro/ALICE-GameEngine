//! [`MockProvider`] — pure-Rust [`XrProvider`] for unit tests.

use std::collections::HashMap;

use super::provider::XrProvider;
use super::types::{
    XrAction, XrActionSet, XrConfig, XrError, XrHand, XrHaptics, XrPose, XrSessionState,
    XrViewState,
};

/// Mock backend with mutable canned values. No external dependencies.
///
/// Useful for headless unit testing of game logic that consumes
/// [`XrProvider`] without spinning up a real session.
pub struct MockProvider {
    config: XrConfig,
    action_set: XrActionSet,
    state: XrSessionState,
    floats: HashMap<(XrHand, XrAction), f32>,
    bools: HashMap<(XrHand, XrAction), bool>,
    vec2s: HashMap<(XrHand, XrAction), [f32; 2]>,
    poses: HashMap<XrHand, XrPose>,
    hmd: XrPose,
    views: [XrViewState; 2],
    pub haptics_log: Vec<XrHaptics>,
    exit_requested: bool,
}

impl MockProvider {
    /// New mock provider in the [`XrSessionState::Focused`] state.
    #[must_use]
    pub fn new(config: XrConfig) -> Self {
        Self {
            config,
            action_set: XrActionSet::new("mock"),
            state: XrSessionState::Focused,
            floats: HashMap::new(),
            bools: HashMap::new(),
            vec2s: HashMap::new(),
            poses: HashMap::new(),
            hmd: XrPose::identity(),
            views: [XrViewState::default(); 2],
            haptics_log: Vec::new(),
            exit_requested: false,
        }
    }

    /// Inject a float action value (e.g. trigger pull).
    pub fn set_float(&mut self, hand: XrHand, action: XrAction, value: f32) {
        self.floats.insert((hand, action), value);
    }

    /// Inject a digital action value (e.g. button press).
    pub fn set_bool(&mut self, hand: XrHand, action: XrAction, value: bool) {
        self.bools.insert((hand, action), value);
    }

    /// Inject a Vec2 action value (e.g. thumbstick).
    pub fn set_vec2(&mut self, hand: XrHand, action: XrAction, value: [f32; 2]) {
        self.vec2s.insert((hand, action), value);
    }

    /// Inject a controller pose.
    pub fn set_controller_pose(&mut self, hand: XrHand, pose: XrPose) {
        self.poses.insert(hand, pose);
    }

    /// Inject an HMD pose.
    pub fn set_hmd_pose(&mut self, pose: XrPose) {
        self.hmd = pose;
    }

    /// Force a session state for testing lifecycle transitions.
    pub fn set_session_state(&mut self, state: XrSessionState) {
        self.state = state;
    }

    /// Has [`request_exit`](Self::request_exit) been called?
    #[must_use]
    pub const fn exit_requested(&self) -> bool {
        self.exit_requested
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new(XrConfig::default())
    }
}

impl XrProvider for MockProvider {
    fn name(&self) -> &str {
        "mock"
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
        self.floats.get(&(hand, action)).copied().unwrap_or(0.0)
    }

    fn action_bool(&self, hand: XrHand, action: XrAction) -> bool {
        self.bools.get(&(hand, action)).copied().unwrap_or(false)
    }

    fn action_vec2(&self, hand: XrHand, action: XrAction) -> [f32; 2] {
        self.vec2s
            .get(&(hand, action))
            .copied()
            .unwrap_or([0.0, 0.0])
    }

    fn controller_pose(&self, hand: XrHand) -> Option<XrPose> {
        self.poses.get(&hand).copied()
    }

    fn hmd_pose(&self) -> Option<XrPose> {
        Some(self.hmd)
    }

    fn views(&self) -> [XrViewState; 2] {
        self.views
    }

    fn apply_haptics(&mut self, pulse: XrHaptics) -> Result<(), XrError> {
        self.haptics_log.push(pulse);
        Ok(())
    }

    fn request_exit(&mut self) {
        self.exit_requested = true;
        self.state = XrSessionState::Stopping;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{Quat, Vec3};

    #[test]
    fn mock_starts_focused() {
        let mock = MockProvider::default();
        assert_eq!(mock.session_state(), XrSessionState::Focused);
        assert!(mock.should_render());
    }

    #[test]
    fn mock_records_injected_floats() {
        let mut mock = MockProvider::default();
        mock.set_float(XrHand::Right, XrAction::Trigger, 0.7);
        assert!((mock.action_float(XrHand::Right, XrAction::Trigger) - 0.7).abs() < 1e-6);
        assert_eq!(mock.action_float(XrHand::Left, XrAction::Trigger), 0.0);
    }

    #[test]
    fn mock_records_injected_bools_and_vec2() {
        let mut mock = MockProvider::default();
        mock.set_bool(XrHand::Left, XrAction::ButtonPrimary, true);
        mock.set_vec2(XrHand::Right, XrAction::Thumbstick, [0.4, -0.8]);
        assert!(mock.action_bool(XrHand::Left, XrAction::ButtonPrimary));
        assert_eq!(
            mock.action_vec2(XrHand::Right, XrAction::Thumbstick),
            [0.4, -0.8]
        );
    }

    #[test]
    fn mock_records_haptics_calls() {
        let mut mock = MockProvider::default();
        mock.apply_haptics(XrHaptics::pulse(XrHand::Right, 0.05, 0.5))
            .unwrap();
        mock.apply_haptics(XrHaptics::pulse(XrHand::Right, 0.1, 1.0))
            .unwrap();
        assert_eq!(mock.haptics_log.len(), 2);
        assert!((mock.haptics_log[1].amplitude - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mock_pose_injection() {
        let mut mock = MockProvider::default();
        mock.set_controller_pose(
            XrHand::Right,
            XrPose::new(Vec3::new(0.5, 1.0, -0.3), Quat::default()),
        );
        let pose = mock.controller_pose(XrHand::Right).unwrap();
        assert!((pose.position.x() - 0.5).abs() < 1e-6);
        assert!(mock.controller_pose(XrHand::Left).is_none());
    }

    #[test]
    fn mock_request_exit_transitions_to_stopping() {
        let mut mock = MockProvider::default();
        assert!(!mock.exit_requested());
        mock.request_exit();
        assert!(mock.exit_requested());
        assert_eq!(mock.session_state(), XrSessionState::Stopping);
        assert!(!mock.should_render());
    }
}
