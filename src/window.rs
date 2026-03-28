//! Window management and application runner via winit.
//!
//! Provides `WindowConfig` and `AppRunner` to tie together
//! winit event loop, wgpu GPU context, and the engine loop.

use crate::input::{Key, MouseButton};

// ---------------------------------------------------------------------------
// WindowConfig
// ---------------------------------------------------------------------------

/// Configuration for creating a window.
#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub resizable: bool,
    pub vsync: bool,
    pub fullscreen: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "ALICE Engine".to_string(),
            width: 1280,
            height: 720,
            resizable: true,
            vsync: true,
            fullscreen: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Key mapping (winit → engine)
// ---------------------------------------------------------------------------

/// Maps a winit `KeyCode` debug name to the engine `Key` enum.
/// Accepts both `winit::keyboard::KeyCode` variant names and short forms.
#[must_use]
pub fn map_key(key_name: &str) -> Option<Key> {
    match key_name {
        "KeyA" | "A" => Some(Key::A),
        "KeyB" | "B" => Some(Key::B),
        "KeyC" | "C" => Some(Key::C),
        "KeyD" | "D" => Some(Key::D),
        "KeyE" | "E" => Some(Key::E),
        "KeyF" | "F" => Some(Key::F),
        "KeyG" | "G" => Some(Key::G),
        "KeyH" | "H" => Some(Key::H),
        "KeyI" | "I" => Some(Key::I),
        "KeyJ" | "J" => Some(Key::J),
        "KeyK" | "K" => Some(Key::K),
        "KeyL" | "L" => Some(Key::L),
        "KeyM" | "M" => Some(Key::M),
        "KeyN" | "N" => Some(Key::N),
        "KeyO" | "O" => Some(Key::O),
        "KeyP" | "P" => Some(Key::P),
        "KeyQ" | "Q" => Some(Key::Q),
        "KeyR" | "R" => Some(Key::R),
        "KeyS" | "S" => Some(Key::S),
        "KeyT" | "T" => Some(Key::T),
        "KeyU" | "U" => Some(Key::U),
        "KeyV" | "V" => Some(Key::V),
        "KeyW" | "W" => Some(Key::W),
        "KeyX" | "X" => Some(Key::X),
        "KeyY" | "Y" => Some(Key::Y),
        "KeyZ" | "Z" => Some(Key::Z),
        "Space" => Some(Key::Space),
        "Enter" | "Return" => Some(Key::Enter),
        "Escape" => Some(Key::Escape),
        "Tab" => Some(Key::Tab),
        "Backspace" => Some(Key::Backspace),
        "Delete" => Some(Key::Delete),
        "ArrowLeft" | "Left" => Some(Key::Left),
        "ArrowRight" | "Right" => Some(Key::Right),
        "ArrowUp" | "Up" => Some(Key::Up),
        "ArrowDown" | "Down" => Some(Key::Down),
        "ShiftLeft" => Some(Key::LShift),
        "ShiftRight" => Some(Key::RShift),
        "ControlLeft" => Some(Key::LCtrl),
        "ControlRight" => Some(Key::RCtrl),
        _ => None,
    }
}

/// Maps a mouse button index to our enum.
#[must_use]
pub const fn map_mouse_button(index: u32) -> Option<MouseButton> {
    match index {
        0 => Some(MouseButton::Left),
        1 => Some(MouseButton::Right),
        2 => Some(MouseButton::Middle),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// FrameTimer
// ---------------------------------------------------------------------------

/// Simple frame timer for delta time calculation.
#[derive(Debug, Clone)]
pub struct FrameTimer {
    last_time_ms: f64,
    pub delta_seconds: f32,
    pub fps: f32,
    frame_times: Vec<f32>,
    frame_index: usize,
}

impl FrameTimer {
    #[must_use]
    pub fn new() -> Self {
        Self {
            last_time_ms: 0.0,
            delta_seconds: 1.0 / 60.0,
            fps: 60.0,
            frame_times: vec![1.0 / 60.0; 60],
            frame_index: 0,
        }
    }

    /// Updates the timer. `current_time_ms` is wall-clock milliseconds.
    pub fn update(&mut self, current_time_ms: f64) {
        if self.last_time_ms > 0.0 {
            self.delta_seconds = ((current_time_ms - self.last_time_ms) * 0.001) as f32;
            self.delta_seconds = self.delta_seconds.clamp(0.0001, 0.1);
        }
        self.last_time_ms = current_time_ms;

        self.frame_times[self.frame_index] = self.delta_seconds;
        self.frame_index = (self.frame_index + 1) % self.frame_times.len();

        let avg: f32 = self.frame_times.iter().sum::<f32>() / self.frame_times.len() as f32;
        self.fps = if avg > 0.0 { 1.0 / avg } else { 0.0 };
    }

    /// Returns smoothed FPS.
    #[must_use]
    pub const fn smoothed_fps(&self) -> f32 {
        self.fps
    }
}

impl Default for FrameTimer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_config_default() {
        let cfg = WindowConfig::default();
        assert_eq!(cfg.width, 1280);
        assert_eq!(cfg.height, 720);
        assert!(cfg.resizable);
    }

    #[test]
    fn map_key_letters() {
        assert_eq!(map_key("KeyW"), Some(Key::W));
        assert_eq!(map_key("W"), Some(Key::W));
        assert_eq!(map_key("A"), Some(Key::A));
    }

    #[test]
    fn map_key_special() {
        assert_eq!(map_key("Space"), Some(Key::Space));
        assert_eq!(map_key("Escape"), Some(Key::Escape));
        assert_eq!(map_key("Enter"), Some(Key::Enter));
        assert_eq!(map_key("Tab"), Some(Key::Tab));
    }

    #[test]
    fn map_key_arrows() {
        assert_eq!(map_key("ArrowUp"), Some(Key::Up));
        assert_eq!(map_key("ArrowDown"), Some(Key::Down));
        assert_eq!(map_key("ArrowLeft"), Some(Key::Left));
    }

    #[test]
    fn map_key_unknown() {
        assert_eq!(map_key("FooBar"), None);
    }

    #[test]
    fn map_mouse_button_left() {
        assert_eq!(map_mouse_button(0), Some(MouseButton::Left));
    }

    #[test]
    fn map_mouse_button_right() {
        assert_eq!(map_mouse_button(1), Some(MouseButton::Right));
    }

    #[test]
    fn map_mouse_button_middle() {
        assert_eq!(map_mouse_button(2), Some(MouseButton::Middle));
    }

    #[test]
    fn map_mouse_button_unknown() {
        assert_eq!(map_mouse_button(99), None);
    }

    #[test]
    fn frame_timer_initial() {
        let ft = FrameTimer::new();
        assert!((ft.delta_seconds - 1.0 / 60.0).abs() < 1e-4);
    }

    #[test]
    fn frame_timer_update() {
        let mut ft = FrameTimer::new();
        ft.update(0.0);
        ft.update(16.666);
        assert!((ft.delta_seconds - 0.016666).abs() < 0.01);
    }

    #[test]
    fn frame_timer_clamp_large_dt() {
        let mut ft = FrameTimer::new();
        ft.update(0.0);
        ft.update(500.0); // 500ms spike
        assert!(ft.delta_seconds <= 0.1);
    }

    #[test]
    fn frame_timer_smoothed_fps() {
        let mut ft = FrameTimer::new();
        for i in 0..120 {
            ft.update(i as f64 * 16.666);
        }
        assert!(ft.smoothed_fps() > 50.0);
        assert!(ft.smoothed_fps() < 70.0);
    }

    #[test]
    fn frame_timer_zero_time() {
        let mut ft = FrameTimer::new();
        ft.update(0.0);
        ft.update(0.0);
        assert!(ft.delta_seconds > 0.0); // clamped to 0.0001
    }

    #[test]
    fn map_key_modifiers() {
        assert_eq!(map_key("ShiftLeft"), Some(Key::LShift));
        assert_eq!(map_key("ShiftRight"), Some(Key::RShift));
        assert_eq!(map_key("ControlLeft"), Some(Key::LCtrl));
    }
}
