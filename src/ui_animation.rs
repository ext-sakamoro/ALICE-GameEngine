//! UI animation primitives: state transitions + easing curves.
//!
//! Stateful counterpart to [`crate::imgui`] for widgets whose
//! visual properties (colour, scale, opacity) should smoothly
//! follow input state (hover, pressed, focused) instead of
//! snapping. Wrap a value in [`UiTransition`], call
//! [`UiTransition::set_target`] from widget interaction code, and
//! sample [`UiTransition::current`] every frame.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// 1 - exp(-k·t) — exponential settle.
    Spring,
}

impl Easing {
    /// Apply the curve to a parameter `t ∈ [0, 1]`.
    #[must_use]
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t,
            Self::EaseOut => 1.0 - (1.0 - t).powi(2),
            Self::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - 2.0 * (1.0 - t).powi(2)
                }
            }
            Self::Spring => 1.0 - (-6.0 * t).exp(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiTransition {
    current: f32,
    target: f32,
    start: f32,
    elapsed: f32,
    duration: f32,
    easing: Easing,
}

impl UiTransition {
    /// Constant value, no animation in flight.
    #[must_use]
    pub const fn new(value: f32) -> Self {
        Self {
            current: value,
            target: value,
            start: value,
            elapsed: 0.0,
            duration: 0.0,
            easing: Easing::EaseOut,
        }
    }

    /// Change easing curve. Default is `EaseOut` (= UI hover snap).
    #[must_use]
    pub const fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    /// Begin animating toward `target` over `duration` seconds.
    /// Calling this with a different `target` mid-flight restarts
    /// the curve from the current value (= responsive UI feel).
    pub fn set_target(&mut self, target: f32, duration: f32) {
        if (self.target - target).abs() < 1e-6 && self.elapsed >= self.duration {
            return;
        }
        self.start = self.current;
        self.target = target;
        self.elapsed = 0.0;
        self.duration = duration.max(0.0);
        if self.duration <= 1e-6 {
            self.current = target;
        }
    }

    /// Advance the animation by `dt` seconds and return the new value.
    pub fn tick(&mut self, dt: f32) -> f32 {
        if self.duration <= 1e-6 || (self.target - self.start).abs() < 1e-9 {
            self.current = self.target;
            return self.current;
        }
        self.elapsed += dt.max(0.0);
        let t = (self.elapsed / self.duration).min(1.0);
        let eased = self.easing.apply(t);
        self.current = self.start + (self.target - self.start) * eased;
        self.current
    }

    #[must_use]
    pub const fn current(&self) -> f32 {
        self.current
    }

    #[must_use]
    pub fn settled(&self) -> bool {
        (self.current - self.target).abs() < 1e-4
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_easing_returns_input() {
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert!((Easing::Linear.apply(t) - t).abs() < 1e-6);
        }
    }

    #[test]
    fn ease_out_overshoots_linear() {
        // EaseOut at t=0.5 should be above 0.5 (faster start).
        let e = Easing::EaseOut.apply(0.5);
        assert!(e > 0.5);
        assert!(e < 1.0);
    }

    #[test]
    fn spring_settles_near_one() {
        let v = Easing::Spring.apply(1.0);
        assert!(v > 0.99);
    }

    #[test]
    fn transition_tick_progresses_toward_target() {
        let mut t = UiTransition::new(0.0);
        t.set_target(1.0, 1.0);
        let a = t.tick(0.5);
        assert!(a > 0.0 && a < 1.0);
        let b = t.tick(0.6);
        assert!((b - 1.0).abs() < 1e-3);
        assert!(t.settled());
    }

    #[test]
    fn set_target_zero_duration_snaps_immediately() {
        let mut t = UiTransition::new(0.0);
        t.set_target(1.0, 0.0);
        assert!((t.current() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn set_target_during_animation_restarts_from_current() {
        let mut t = UiTransition::new(0.0).with_easing(Easing::Linear);
        t.set_target(1.0, 1.0);
        t.tick(0.5); // current ≈ 0.5
        t.set_target(0.0, 1.0); // reverse direction
        t.tick(0.5); // halfway back from 0.5 → 0.25
        let v = t.current();
        assert!(v > 0.2 && v < 0.3, "expected ~0.25, got {v}");
    }

    #[test]
    fn ease_in_out_is_symmetric_around_half() {
        let early = Easing::EaseInOut.apply(0.25);
        let late = Easing::EaseInOut.apply(0.75);
        assert!((early + late - 1.0).abs() < 0.1);
    }
}
