//! Mobile (iOS / Android) scaffold — touch input, DPI scaling, and a
//! pinch-and-pan camera controller tuned for handheld devices.
//!
//! The actual integration with iOS UIKit (`UITouch` → [`TouchEvent`])
//! and Android `MotionEvent` lives in the platform crate that owns the
//! window (e.g. `winit` with `android-activity` feature). This module
//! provides the device-agnostic data types + a camera controller so
//! gameplay code stays portable.
//!
//! ```rust
//! use alice_game_engine::mobile::{TouchCamera, TouchEvent, TouchPhase};
//! use alice_game_engine::math::Vec3;
//!
//! let mut cam = TouchCamera::new(Vec3::new(0.0, 2.0, 5.0));
//! cam.handle_touch(&TouchEvent {
//!     id: 0,
//!     phase: TouchPhase::Began,
//!     position: (100.0, 200.0),
//! });
//! cam.handle_touch(&TouchEvent {
//!     id: 0,
//!     phase: TouchPhase::Moved,
//!     position: (110.0, 215.0),
//! });
//! ```

use crate::math::Vec3;

// ---------------------------------------------------------------------------
// Touch input types
// ---------------------------------------------------------------------------

/// Touch lifecycle phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Began,
    Moved,
    Ended,
    Cancelled,
}

/// One touch sample. `position` is in screen pixels with origin
/// top-left (consistent with iOS / Android / wgpu).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchEvent {
    /// Stable touch id (= finger identifier across moves).
    pub id: u32,
    pub phase: TouchPhase,
    pub position: (f32, f32),
}

/// Screen metrics for DPI scaling. `scale_factor` follows the platform
/// convention (= iOS `UIScreen.nativeScale`, Android `densityDpi /
/// 160.0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenMetrics {
    pub width_px: u32,
    pub height_px: u32,
    pub scale_factor: f32,
}

impl ScreenMetrics {
    #[must_use]
    pub const fn new(width_px: u32, height_px: u32, scale_factor: f32) -> Self {
        Self {
            width_px,
            height_px,
            scale_factor,
        }
    }

    /// Logical (= device-independent) pixel width.
    #[must_use]
    pub fn logical_width(self) -> f32 {
        self.width_px as f32 / self.scale_factor
    }

    /// Logical height.
    #[must_use]
    pub fn logical_height(self) -> f32 {
        self.height_px as f32 / self.scale_factor
    }
}

// ---------------------------------------------------------------------------
// TouchCamera
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct ActiveTouch {
    id: u32,
    last_position: (f32, f32),
}

/// Pinch-and-pan orbit camera for handheld devices.
///
/// - Single-finger drag → orbit around `target` (yaw + pitch).
/// - Two-finger pinch → zoom (= move along the current view direction).
#[derive(Debug, Clone)]
pub struct TouchCamera {
    pub target: Vec3,
    pub distance: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub min_pitch: f32,
    pub max_pitch: f32,
    pub orbit_sensitivity: f32,
    pub pinch_sensitivity: f32,
    active_touches: Vec<ActiveTouch>,
    pinch_reference: Option<f32>,
}

impl TouchCamera {
    /// Build a camera looking at `target` from 5 units behind.
    #[must_use]
    pub fn new(target: Vec3) -> Self {
        Self {
            target,
            distance: 5.0,
            yaw: 0.0,
            pitch: 0.3,
            min_distance: 1.0,
            max_distance: 50.0,
            min_pitch: -1.4,
            max_pitch: 1.4,
            orbit_sensitivity: 0.005,
            pinch_sensitivity: 0.01,
            active_touches: Vec::with_capacity(2),
            pinch_reference: None,
        }
    }

    /// Current camera position in world space.
    #[must_use]
    pub fn position(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + Vec3::new(sy * cp, sp, cy * cp) * self.distance
    }

    /// Update internal state from a single touch event. Apps will
    /// typically forward every `TouchEvent` from their windowing layer.
    pub fn handle_touch(&mut self, event: &TouchEvent) {
        match event.phase {
            TouchPhase::Began => {
                // Replace any stale entry with the same id.
                self.active_touches.retain(|t| t.id != event.id);
                if self.active_touches.len() < 2 {
                    self.active_touches.push(ActiveTouch {
                        id: event.id,
                        last_position: event.position,
                    });
                }
                if self.active_touches.len() == 2 {
                    let a = self.active_touches[0].last_position;
                    let b = self.active_touches[1].last_position;
                    self.pinch_reference = Some(((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt());
                }
            }
            TouchPhase::Moved => {
                if self.active_touches.len() == 1 {
                    // Single-finger orbit.
                    if let Some(touch) = self.active_touches.iter_mut().find(|t| t.id == event.id) {
                        let dx = event.position.0 - touch.last_position.0;
                        let dy = event.position.1 - touch.last_position.1;
                        self.yaw -= dx * self.orbit_sensitivity;
                        self.pitch = (self.pitch + dy * self.orbit_sensitivity)
                            .clamp(self.min_pitch, self.max_pitch);
                        touch.last_position = event.position;
                    }
                } else if self.active_touches.len() == 2 {
                    if let Some(idx) = self.active_touches.iter().position(|t| t.id == event.id) {
                        self.active_touches[idx].last_position = event.position;
                    }
                    let a = self.active_touches[0].last_position;
                    let b = self.active_touches[1].last_position;
                    let current = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
                    if let Some(prev) = self.pinch_reference {
                        let delta = current - prev;
                        // Fingers closing → delta < 0 → distance shrinks
                        // (`distance + delta * sensitivity`).
                        self.distance = self
                            .distance
                            .mul_add(1.0, delta * self.pinch_sensitivity)
                            .clamp(self.min_distance, self.max_distance);
                    }
                    self.pinch_reference = Some(current);
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.active_touches.retain(|t| t.id != event.id);
                self.pinch_reference = None;
            }
        }
    }

    /// Number of currently-tracked fingers.
    #[must_use]
    pub fn active_touch_count(&self) -> usize {
        self.active_touches.len()
    }
}

// ---------------------------------------------------------------------------
// Platform target info + build guidance
// ---------------------------------------------------------------------------

/// Compile-time platform target descriptor. Useful for runtime
/// branching (e.g. "load this codec on iOS but not on Android").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobileTarget {
    Ios,
    Android,
    Other,
}

impl MobileTarget {
    /// Resolved at compile time from `cfg(target_os = ...)`.
    #[must_use]
    pub const fn current() -> Self {
        #[cfg(target_os = "ios")]
        {
            Self::Ios
        }
        #[cfg(target_os = "android")]
        {
            Self::Android
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        {
            Self::Other
        }
    }

    #[must_use]
    pub const fn is_mobile(self) -> bool {
        matches!(self, Self::Ios | Self::Android)
    }
}

/// Build configuration hints. Returned from `mobile_build_hints()` so
/// downstream apps can print a startup banner.
///
/// To ship the engine as an iOS / Android library, the **app's**
/// `Cargo.toml` (not the engine's) should add the dynamic / static
/// library kinds:
///
/// ```toml
/// [lib]
/// crate-type = ["cdylib", "staticlib", "rlib"]
/// ```
///
/// On Android the recommended companion crate is
/// `android-activity` together with `winit` (`android-native-activity`
/// feature). On iOS use `winit` with the default backends and link
/// the produced `.a` from Xcode.
#[must_use]
pub const fn mobile_build_hints() -> &'static str {
    "Add `crate-type = [\"cdylib\", \"staticlib\", \"rlib\"]` to the app \
     crate's [lib] table. Pair with `winit + android-activity` on Android, \
     or link the `.a` from Xcode on iOS."
}

#[cfg(target_os = "android")]
pub mod android {
    //! Android-specific glue. Currently a placeholder — winit's
    //! `android_main` macro is the canonical entry point and lives in
    //! the application crate.
    pub const PLATFORM_NAME: &str = "android";
}

#[cfg(target_os = "ios")]
pub mod ios {
    //! iOS-specific glue. Currently a placeholder; the app crate
    //! provides the UIApplicationDelegate via `winit`'s iOS support.
    pub const PLATFORM_NAME: &str = "ios";
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_metrics_logical_size_divides_by_scale() {
        let m = ScreenMetrics::new(2436, 1125, 3.0);
        assert!((m.logical_width() - 812.0).abs() < 1e-3);
        assert!((m.logical_height() - 375.0).abs() < 1e-3);
    }

    #[test]
    fn camera_position_starts_behind_target() {
        let cam = TouchCamera::new(Vec3::ZERO);
        let p = cam.position();
        assert!(p.z() > 0.0);
        assert!(p.length() > 0.0);
    }

    #[test]
    fn single_finger_drag_orbits_camera() {
        let mut cam = TouchCamera::new(Vec3::ZERO);
        let initial_yaw = cam.yaw;
        cam.handle_touch(&TouchEvent {
            id: 0,
            phase: TouchPhase::Began,
            position: (100.0, 100.0),
        });
        cam.handle_touch(&TouchEvent {
            id: 0,
            phase: TouchPhase::Moved,
            position: (200.0, 100.0),
        });
        assert!((cam.yaw - initial_yaw).abs() > 1e-6);
    }

    #[test]
    fn pinch_close_zooms_in() {
        let mut cam = TouchCamera::new(Vec3::ZERO);
        let initial_distance = cam.distance;
        cam.handle_touch(&TouchEvent {
            id: 0,
            phase: TouchPhase::Began,
            position: (100.0, 100.0),
        });
        cam.handle_touch(&TouchEvent {
            id: 1,
            phase: TouchPhase::Began,
            position: (300.0, 100.0),
        });
        // Move finger 1 closer → fingers closing → zoom in.
        cam.handle_touch(&TouchEvent {
            id: 1,
            phase: TouchPhase::Moved,
            position: (150.0, 100.0),
        });
        assert!(
            cam.distance < initial_distance,
            "pinch close should reduce distance ({} → {})",
            initial_distance,
            cam.distance,
        );
    }

    #[test]
    fn ended_touch_is_dropped() {
        let mut cam = TouchCamera::new(Vec3::ZERO);
        cam.handle_touch(&TouchEvent {
            id: 7,
            phase: TouchPhase::Began,
            position: (10.0, 10.0),
        });
        assert_eq!(cam.active_touch_count(), 1);
        cam.handle_touch(&TouchEvent {
            id: 7,
            phase: TouchPhase::Ended,
            position: (10.0, 10.0),
        });
        assert_eq!(cam.active_touch_count(), 0);
    }

    #[test]
    fn cancelled_touch_clears_pinch_reference() {
        let mut cam = TouchCamera::new(Vec3::ZERO);
        cam.handle_touch(&TouchEvent {
            id: 0,
            phase: TouchPhase::Began,
            position: (0.0, 0.0),
        });
        cam.handle_touch(&TouchEvent {
            id: 1,
            phase: TouchPhase::Began,
            position: (100.0, 0.0),
        });
        assert!(cam.pinch_reference.is_some());
        cam.handle_touch(&TouchEvent {
            id: 1,
            phase: TouchPhase::Cancelled,
            position: (100.0, 0.0),
        });
        assert!(cam.pinch_reference.is_none());
    }

    #[test]
    fn pitch_clamps_to_configured_range() {
        let mut cam = TouchCamera::new(Vec3::ZERO);
        cam.min_pitch = -0.5;
        cam.max_pitch = 0.5;
        cam.handle_touch(&TouchEvent {
            id: 0,
            phase: TouchPhase::Began,
            position: (0.0, 0.0),
        });
        // Drag downward thousands of pixels.
        for i in 0..200 {
            cam.handle_touch(&TouchEvent {
                id: 0,
                phase: TouchPhase::Moved,
                position: (0.0, (i + 1) as f32 * 50.0),
            });
        }
        assert!(cam.pitch <= 0.5 + 1e-3);
    }

    #[test]
    fn mobile_target_current_resolves_at_compile_time() {
        let t = MobileTarget::current();
        // On a host build (Mac / Linux / Windows dev box) the target is
        // Other; iOS / Android cross-builds will return their variant.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        assert_eq!(t, MobileTarget::Other);
        assert!(!MobileTarget::Other.is_mobile());
        assert!(MobileTarget::Ios.is_mobile());
        assert!(MobileTarget::Android.is_mobile());
        let _ = t;
    }

    #[test]
    fn mobile_build_hints_returns_configuration_string() {
        let h = mobile_build_hints();
        assert!(h.contains("cdylib"));
        assert!(h.contains("staticlib"));
    }

    #[test]
    fn at_most_two_touches_tracked() {
        let mut cam = TouchCamera::new(Vec3::ZERO);
        for id in 0..5 {
            cam.handle_touch(&TouchEvent {
                id,
                phase: TouchPhase::Began,
                position: (id as f32 * 10.0, 0.0),
            });
        }
        assert_eq!(cam.active_touch_count(), 2);
    }
}
