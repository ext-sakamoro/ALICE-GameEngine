//! Keyframe animation system with blending and state machine.
//!
//! ```rust
//! use alice_game_engine::animation::*;
//!
//! let mut track = Track::new("x");
//! track.add_keyframe(Keyframe::new(0.0, 0.0));
//! track.add_keyframe(Keyframe::new(1.0, 10.0));
//! assert!((track.evaluate(0.5) - 5.0).abs() < 0.1);
//! ```
//!
//! Supports Linear, Step, and `CubicBezier` interpolation across named
//! tracks. An `AnimationPlayer` drives playback, and `StateMachine`
//! manages transitions between clips (Idle→Walk→Run).

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Interpolation
// ---------------------------------------------------------------------------

/// Interpolation mode between keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Interpolation {
    #[default]
    Linear,
    Step,
    CubicBezier,
}

// ---------------------------------------------------------------------------
// Keyframe
// ---------------------------------------------------------------------------

/// A single keyframe: time → value with interpolation mode.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    pub time: f32,
    pub value: f32,
    pub interpolation: Interpolation,
}

impl Keyframe {
    #[must_use]
    pub const fn new(time: f32, value: f32) -> Self {
        Self {
            time,
            value,
            interpolation: Interpolation::Linear,
        }
    }

    #[must_use]
    pub const fn with_interp(time: f32, value: f32, interpolation: Interpolation) -> Self {
        Self {
            time,
            value,
            interpolation,
        }
    }
}

// ---------------------------------------------------------------------------
// Track
// ---------------------------------------------------------------------------

/// A track animates a single named parameter over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub name: String,
    pub keyframes: Vec<Keyframe>,
}

impl Track {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            keyframes: Vec::new(),
        }
    }

    /// Adds a keyframe and keeps the list sorted by time.
    pub fn add_keyframe(&mut self, kf: Keyframe) {
        let pos = self
            .keyframes
            .binary_search_by(|k| k.time.total_cmp(&kf.time))
            .unwrap_or_else(|e| e);
        self.keyframes.insert(pos, kf);
    }

    /// Evaluates the track at the given time.
    #[must_use]
    pub fn evaluate(&self, t: f32) -> f32 {
        if self.keyframes.is_empty() {
            return 0.0;
        }
        if self.keyframes.len() == 1 || t <= self.keyframes[0].time {
            return self.keyframes[0].value;
        }
        let last = &self.keyframes[self.keyframes.len() - 1];
        if t >= last.time {
            return last.value;
        }

        let idx = self
            .keyframes
            .binary_search_by(|k| k.time.total_cmp(&t))
            .unwrap_or_else(|e| e.saturating_sub(1));

        let a = &self.keyframes[idx];
        let b = &self.keyframes[(idx + 1).min(self.keyframes.len() - 1)];

        if (b.time - a.time).abs() < 1e-10 {
            return a.value;
        }

        let frac = (t - a.time) / (b.time - a.time);

        match b.interpolation {
            Interpolation::Linear => (b.value - a.value).mul_add(frac, a.value),
            Interpolation::Step => a.value,
            Interpolation::CubicBezier => {
                // Hermite basis smoothstep
                let s = frac * frac * 2.0f32.mul_add(-frac, 3.0);
                (b.value - a.value).mul_add(s, a.value)
            }
        }
    }

    /// Returns the duration (time of last keyframe).
    #[must_use]
    pub fn duration(&self) -> f32 {
        self.keyframes.last().map_or(0.0, |k| k.time)
    }
}

// ---------------------------------------------------------------------------
// AnimationClip
// ---------------------------------------------------------------------------

/// A collection of tracks that animate together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationClip {
    pub name: String,
    pub tracks: Vec<Track>,
    pub looping: bool,
}

impl AnimationClip {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            tracks: Vec::new(),
            looping: false,
        }
    }

    /// Duration is the max track duration.
    #[must_use]
    pub fn duration(&self) -> f32 {
        self.tracks
            .iter()
            .map(Track::duration)
            .fold(0.0_f32, f32::max)
    }

    /// Evaluates all tracks at time `t`. Returns (name, value) pairs.
    #[must_use]
    pub fn evaluate(&self, t: f32) -> Vec<(&str, f32)> {
        let effective_t = if self.looping && self.duration() > 0.0 {
            t % self.duration()
        } else {
            t
        };
        self.tracks
            .iter()
            .map(|track| (track.name.as_str(), track.evaluate(effective_t)))
            .collect()
    }

    /// Finds a track by name.
    #[must_use]
    pub fn find_track(&self, name: &str) -> Option<&Track> {
        self.tracks.iter().find(|t| t.name == name)
    }
}

// ---------------------------------------------------------------------------
// AnimationPlayer
// ---------------------------------------------------------------------------

/// Playback state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

/// Plays an animation clip.
#[derive(Debug, Clone)]
pub struct AnimationPlayer {
    pub clip_name: String,
    pub state: PlaybackState,
    pub time: f32,
    pub speed: f32,
    pub blend_weight: f32,
}

impl AnimationPlayer {
    #[must_use]
    pub fn new(clip_name: &str) -> Self {
        Self {
            clip_name: clip_name.to_string(),
            state: PlaybackState::Stopped,
            time: 0.0,
            speed: 1.0,
            blend_weight: 1.0,
        }
    }

    pub const fn play(&mut self) {
        self.state = PlaybackState::Playing;
    }

    pub const fn pause(&mut self) {
        self.state = PlaybackState::Paused;
    }

    pub const fn stop(&mut self) {
        self.state = PlaybackState::Stopped;
        self.time = 0.0;
    }

    /// Advances playback by `dt` seconds.
    pub fn update(&mut self, dt: f32) {
        if self.state == PlaybackState::Playing {
            self.time += dt * self.speed;
        }
    }

    /// Returns whether the animation has finished (non-looping only).
    #[must_use]
    pub fn is_finished(&self, clip_duration: f32) -> bool {
        self.state == PlaybackState::Playing && self.time >= clip_duration
    }
}

// ---------------------------------------------------------------------------
// StateMachine
// ---------------------------------------------------------------------------

/// A transition between states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transition {
    pub from: String,
    pub to: String,
    pub condition: String,
    pub duration: f32,
}

/// Animation state machine.
#[derive(Debug, Clone)]
pub struct StateMachine {
    pub states: Vec<String>,
    pub transitions: Vec<Transition>,
    pub current_state: String,
    pub transition_progress: f32,
    pub transitioning_to: Option<String>,
}

impl StateMachine {
    #[must_use]
    pub fn new(initial_state: &str) -> Self {
        Self {
            states: vec![initial_state.to_string()],
            transitions: Vec::new(),
            current_state: initial_state.to_string(),
            transition_progress: 0.0,
            transitioning_to: None,
        }
    }

    /// Adds a state.
    pub fn add_state(&mut self, name: &str) {
        if !self.states.iter().any(|s| s == name) {
            self.states.push(name.to_string());
        }
    }

    /// Adds a transition rule.
    pub fn add_transition(&mut self, from: &str, to: &str, condition: &str, duration: f32) {
        self.transitions.push(Transition {
            from: from.to_string(),
            to: to.to_string(),
            condition: condition.to_string(),
            duration,
        });
    }

    /// Triggers a condition and starts a transition if applicable.
    pub fn trigger(&mut self, condition: &str) {
        if self.transitioning_to.is_some() {
            return;
        }
        let target = self
            .transitions
            .iter()
            .find(|t| t.from == self.current_state && t.condition == condition)
            .map(|t| t.to.clone());
        if let Some(to) = target {
            self.transitioning_to = Some(to);
            self.transition_progress = 0.0;
        }
    }

    /// Advances transition by `dt`.
    pub fn update(&mut self, dt: f32) {
        if let Some(ref to) = self.transitioning_to.clone() {
            let duration = self
                .transitions
                .iter()
                .find(|t| t.from == self.current_state && t.to == *to)
                .map_or(0.1, |t| t.duration);
            self.transition_progress += dt / duration;
            if self.transition_progress >= 1.0 {
                self.current_state.clone_from(to);
                self.transitioning_to = None;
                self.transition_progress = 0.0;
            }
        }
    }

    /// Returns the blend weight for the transition (0.0 = current, 1.0 = next).
    #[must_use]
    pub const fn blend_factor(&self) -> f32 {
        if self.transitioning_to.is_some() {
            self.transition_progress.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Returns the current state name.
    #[must_use]
    pub fn current(&self) -> &str {
        &self.current_state
    }

    /// Returns the target state if transitioning.
    #[must_use]
    pub fn target(&self) -> Option<&str> {
        self.transitioning_to.as_deref()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_track() -> Track {
        let mut t = Track::new("x");
        t.add_keyframe(Keyframe::new(0.0, 0.0));
        t.add_keyframe(Keyframe::new(1.0, 10.0));
        t.add_keyframe(Keyframe::new(2.0, 5.0));
        t
    }

    #[test]
    fn track_evaluate_start() {
        let t = make_track();
        assert!((t.evaluate(0.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn track_evaluate_mid() {
        let t = make_track();
        assert!((t.evaluate(0.5) - 5.0).abs() < 1e-4);
    }

    #[test]
    fn track_evaluate_end() {
        let t = make_track();
        assert!((t.evaluate(2.0) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn track_evaluate_beyond() {
        let t = make_track();
        assert!((t.evaluate(10.0) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn track_evaluate_before() {
        let t = make_track();
        assert!((t.evaluate(-1.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn track_empty() {
        let t = Track::new("empty");
        assert_eq!(t.evaluate(1.0), 0.0);
    }

    #[test]
    fn track_single_keyframe() {
        let mut t = Track::new("x");
        t.add_keyframe(Keyframe::new(0.0, 42.0));
        assert_eq!(t.evaluate(5.0), 42.0);
    }

    #[test]
    fn track_step_interpolation() {
        let mut t = Track::new("x");
        t.add_keyframe(Keyframe::with_interp(0.0, 0.0, Interpolation::Step));
        t.add_keyframe(Keyframe::with_interp(1.0, 10.0, Interpolation::Step));
        assert!((t.evaluate(0.5) - 0.0).abs() < 1e-6);
        assert!((t.evaluate(1.0) - 10.0).abs() < 1e-6);
    }

    #[test]
    fn track_cubic_bezier() {
        let mut t = Track::new("x");
        t.add_keyframe(Keyframe::with_interp(0.0, 0.0, Interpolation::CubicBezier));
        t.add_keyframe(Keyframe::with_interp(1.0, 1.0, Interpolation::CubicBezier));
        let mid = t.evaluate(0.5);
        assert!((mid - 0.5).abs() < 0.1); // smoothstep at 0.5 ≈ 0.5
    }

    #[test]
    fn track_duration() {
        let t = make_track();
        assert!((t.duration() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn track_sorted_insert() {
        let mut t = Track::new("x");
        t.add_keyframe(Keyframe::new(2.0, 20.0));
        t.add_keyframe(Keyframe::new(0.0, 0.0));
        t.add_keyframe(Keyframe::new(1.0, 10.0));
        assert!((t.keyframes[0].time).abs() < 1e-6);
        assert!((t.keyframes[1].time - 1.0).abs() < 1e-6);
    }

    #[test]
    fn clip_duration() {
        let mut clip = AnimationClip::new("walk");
        let mut t1 = Track::new("x");
        t1.add_keyframe(Keyframe::new(0.0, 0.0));
        t1.add_keyframe(Keyframe::new(3.0, 1.0));
        let mut t2 = Track::new("y");
        t2.add_keyframe(Keyframe::new(0.0, 0.0));
        t2.add_keyframe(Keyframe::new(2.0, 1.0));
        clip.tracks.push(t1);
        clip.tracks.push(t2);
        assert!((clip.duration() - 3.0).abs() < 1e-6);
    }

    #[test]
    fn clip_evaluate() {
        let mut clip = AnimationClip::new("test");
        let mut t = Track::new("x");
        t.add_keyframe(Keyframe::new(0.0, 0.0));
        t.add_keyframe(Keyframe::new(1.0, 10.0));
        clip.tracks.push(t);
        let vals = clip.evaluate(0.5);
        assert_eq!(vals.len(), 1);
        assert_eq!(vals[0].0, "x");
        assert!((vals[0].1 - 5.0).abs() < 1e-4);
    }

    #[test]
    fn clip_looping() {
        let mut clip = AnimationClip::new("loop");
        clip.looping = true;
        let mut t = Track::new("x");
        t.add_keyframe(Keyframe::new(0.0, 0.0));
        t.add_keyframe(Keyframe::new(1.0, 1.0));
        clip.tracks.push(t);
        let vals = clip.evaluate(1.5);
        assert!((vals[0].1 - 0.5).abs() < 0.1);
    }

    #[test]
    fn clip_find_track() {
        let mut clip = AnimationClip::new("test");
        clip.tracks.push(Track::new("x"));
        clip.tracks.push(Track::new("y"));
        assert!(clip.find_track("x").is_some());
        assert!(clip.find_track("z").is_none());
    }

    #[test]
    fn player_play_stop() {
        let mut p = AnimationPlayer::new("walk");
        assert_eq!(p.state, PlaybackState::Stopped);
        p.play();
        assert_eq!(p.state, PlaybackState::Playing);
        p.update(0.5);
        assert!((p.time - 0.5).abs() < 1e-6);
        p.stop();
        assert_eq!(p.time, 0.0);
    }

    #[test]
    fn player_pause() {
        let mut p = AnimationPlayer::new("walk");
        p.play();
        p.update(0.5);
        p.pause();
        p.update(0.5);
        assert!((p.time - 0.5).abs() < 1e-6);
    }

    #[test]
    fn player_speed() {
        let mut p = AnimationPlayer::new("run");
        p.speed = 2.0;
        p.play();
        p.update(1.0);
        assert!((p.time - 2.0).abs() < 1e-6);
    }

    #[test]
    fn player_is_finished() {
        let mut p = AnimationPlayer::new("attack");
        p.play();
        p.update(2.0);
        assert!(p.is_finished(1.5));
        assert!(!p.is_finished(3.0));
    }

    #[test]
    fn state_machine_basic() {
        let mut sm = StateMachine::new("idle");
        sm.add_state("walk");
        sm.add_state("run");
        sm.add_transition("idle", "walk", "move", 0.2);
        sm.add_transition("walk", "run", "sprint", 0.3);
        assert_eq!(sm.current(), "idle");
    }

    #[test]
    fn state_machine_trigger() {
        let mut sm = StateMachine::new("idle");
        sm.add_state("walk");
        sm.add_transition("idle", "walk", "move", 0.2);
        sm.trigger("move");
        assert_eq!(sm.target(), Some("walk"));
    }

    #[test]
    fn state_machine_transition_complete() {
        let mut sm = StateMachine::new("idle");
        sm.add_state("walk");
        sm.add_transition("idle", "walk", "move", 0.2);
        sm.trigger("move");
        sm.update(0.3);
        assert_eq!(sm.current(), "walk");
        assert_eq!(sm.target(), None);
    }

    #[test]
    fn state_machine_blend_factor() {
        let mut sm = StateMachine::new("idle");
        sm.add_state("walk");
        sm.add_transition("idle", "walk", "move", 1.0);
        sm.trigger("move");
        sm.update(0.5);
        assert!((sm.blend_factor() - 0.5).abs() < 1e-4);
    }

    #[test]
    fn state_machine_no_transition_for_wrong_condition() {
        let mut sm = StateMachine::new("idle");
        sm.add_state("walk");
        sm.add_transition("idle", "walk", "move", 0.2);
        sm.trigger("jump");
        assert_eq!(sm.target(), None);
    }

    #[test]
    fn state_machine_chain_transitions() {
        let mut sm = StateMachine::new("idle");
        sm.add_state("walk");
        sm.add_state("run");
        sm.add_transition("idle", "walk", "move", 0.1);
        sm.add_transition("walk", "run", "sprint", 0.1);
        sm.trigger("move");
        sm.update(0.2);
        assert_eq!(sm.current(), "walk");
        sm.trigger("sprint");
        sm.update(0.2);
        assert_eq!(sm.current(), "run");
    }

    #[test]
    fn state_machine_duplicate_state() {
        let mut sm = StateMachine::new("idle");
        sm.add_state("idle");
        assert_eq!(sm.states.len(), 1);
    }

    #[test]
    fn interpolation_default() {
        assert_eq!(Interpolation::default(), Interpolation::Linear);
    }

    #[test]
    fn state_machine_self_transition() {
        let mut sm = StateMachine::new("idle");
        sm.add_transition("idle", "idle", "reset", 0.1);
        sm.trigger("reset");
        assert_eq!(sm.target(), Some("idle"));
        sm.update(0.2);
        assert_eq!(sm.current(), "idle");
        assert_eq!(sm.target(), None);
    }

    #[test]
    fn state_machine_rapid_triggers() {
        let mut sm = StateMachine::new("idle");
        sm.add_state("walk");
        sm.add_state("run");
        sm.add_transition("idle", "walk", "move", 0.5);
        sm.add_transition("walk", "run", "sprint", 0.5);
        sm.trigger("move");
        sm.trigger("sprint"); // Should be ignored — already transitioning
        assert_eq!(sm.target(), Some("walk"));
    }

    #[test]
    fn state_machine_no_outgoing_transition() {
        let mut sm = StateMachine::new("dead");
        sm.trigger("anything");
        assert_eq!(sm.target(), None);
        assert_eq!(sm.current(), "dead");
    }

    #[test]
    fn player_blend_weight() {
        let p = AnimationPlayer::new("walk");
        assert_eq!(p.blend_weight, 1.0);
    }

    #[test]
    fn clip_empty_evaluate() {
        let clip = AnimationClip::new("empty");
        let vals = clip.evaluate(1.0);
        assert!(vals.is_empty());
    }
}
