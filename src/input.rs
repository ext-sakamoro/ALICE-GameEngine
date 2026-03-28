//! Extended input system: keyboard, mouse, gamepad, and action mapping.

use crate::math::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Key / Button enums
// ---------------------------------------------------------------------------

/// Keyboard key codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Space,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    LShift,
    RShift,
    LCtrl,
    RCtrl,
    LAlt,
    RAlt,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

/// Mouse buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Gamepad buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamepadButton {
    South,
    East,
    North,
    West,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    LeftShoulder,
    RightShoulder,
    LeftStick,
    RightStick,
    Start,
    Select,
}

/// Gamepad axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamepadAxis {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    LeftTrigger,
    RightTrigger,
}

// ---------------------------------------------------------------------------
// InputSource — unified input binding
// ---------------------------------------------------------------------------

/// An input source that can be mapped to an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputSource {
    Key(Key),
    Mouse(MouseButton),
    Gamepad(GamepadButton),
}

/// An axis source for analog input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AxisSource {
    GamepadAxis(GamepadAxis),
    KeyPair { negative: Key, positive: Key },
    MouseX,
    MouseY,
    MouseScroll,
}

// ---------------------------------------------------------------------------
// InputState
// ---------------------------------------------------------------------------

/// Full input state for one frame.
pub struct InputState {
    keys_down: HashSet<Key>,
    keys_just_pressed: HashSet<Key>,
    keys_just_released: HashSet<Key>,
    mouse_buttons_down: HashSet<MouseButton>,
    mouse_just_pressed: HashSet<MouseButton>,
    mouse_position: Vec2,
    mouse_delta: Vec2,
    mouse_scroll: f32,
    gamepad_buttons_down: HashSet<GamepadButton>,
    gamepad_just_pressed: HashSet<GamepadButton>,
    gamepad_axes: HashMap<GamepadAxis, f32>,
}

impl InputState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            keys_down: HashSet::new(),
            keys_just_pressed: HashSet::new(),
            keys_just_released: HashSet::new(),
            mouse_buttons_down: HashSet::new(),
            mouse_just_pressed: HashSet::new(),
            mouse_position: Vec2::ZERO,
            mouse_delta: Vec2::ZERO,
            mouse_scroll: 0.0,
            gamepad_buttons_down: HashSet::new(),
            gamepad_just_pressed: HashSet::new(),
            gamepad_axes: HashMap::new(),
        }
    }

    /// Call at the start of each frame to clear per-frame state.
    pub fn begin_frame(&mut self) {
        self.keys_just_pressed.clear();
        self.keys_just_released.clear();
        self.mouse_just_pressed.clear();
        self.mouse_delta = Vec2::ZERO;
        self.mouse_scroll = 0.0;
        self.gamepad_just_pressed.clear();
    }

    // --- Keyboard ---

    pub fn key_press(&mut self, key: Key) {
        if self.keys_down.insert(key) {
            self.keys_just_pressed.insert(key);
        }
    }

    pub fn key_release(&mut self, key: Key) {
        if self.keys_down.remove(&key) {
            self.keys_just_released.insert(key);
        }
    }

    #[must_use]
    pub fn is_key_down(&self, key: Key) -> bool {
        self.keys_down.contains(&key)
    }

    #[must_use]
    pub fn is_key_just_pressed(&self, key: Key) -> bool {
        self.keys_just_pressed.contains(&key)
    }

    #[must_use]
    pub fn is_key_just_released(&self, key: Key) -> bool {
        self.keys_just_released.contains(&key)
    }

    // --- Mouse ---

    pub fn mouse_button_press(&mut self, btn: MouseButton) {
        self.mouse_buttons_down.insert(btn);
        self.mouse_just_pressed.insert(btn);
    }

    pub fn mouse_button_release(&mut self, btn: MouseButton) {
        self.mouse_buttons_down.remove(&btn);
    }

    pub const fn mouse_move(&mut self, position: Vec2, delta: Vec2) {
        self.mouse_position = position;
        self.mouse_delta = delta;
    }

    pub fn mouse_scroll_event(&mut self, delta: f32) {
        self.mouse_scroll += delta;
    }

    #[must_use]
    pub fn is_mouse_down(&self, btn: MouseButton) -> bool {
        self.mouse_buttons_down.contains(&btn)
    }

    #[must_use]
    pub fn is_mouse_just_pressed(&self, btn: MouseButton) -> bool {
        self.mouse_just_pressed.contains(&btn)
    }

    #[must_use]
    pub const fn mouse_position(&self) -> Vec2 {
        self.mouse_position
    }

    #[must_use]
    pub const fn mouse_delta(&self) -> Vec2 {
        self.mouse_delta
    }

    #[must_use]
    pub const fn mouse_scroll(&self) -> f32 {
        self.mouse_scroll
    }

    // --- Gamepad ---

    pub fn gamepad_press(&mut self, btn: GamepadButton) {
        self.gamepad_buttons_down.insert(btn);
        self.gamepad_just_pressed.insert(btn);
    }

    pub fn gamepad_release(&mut self, btn: GamepadButton) {
        self.gamepad_buttons_down.remove(&btn);
    }

    pub fn gamepad_axis_update(&mut self, axis: GamepadAxis, value: f32) {
        self.gamepad_axes.insert(axis, value);
    }

    #[must_use]
    pub fn is_gamepad_down(&self, btn: GamepadButton) -> bool {
        self.gamepad_buttons_down.contains(&btn)
    }

    #[must_use]
    pub fn is_gamepad_just_pressed(&self, btn: GamepadButton) -> bool {
        self.gamepad_just_pressed.contains(&btn)
    }

    #[must_use]
    pub fn gamepad_axis(&self, axis: GamepadAxis) -> f32 {
        self.gamepad_axes.get(&axis).copied().unwrap_or(0.0)
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ActionMap — name-based input mapping
// ---------------------------------------------------------------------------

/// Maps action names to input sources.
pub struct ActionMap {
    actions: HashMap<String, Vec<InputSource>>,
    axes: HashMap<String, Vec<AxisSource>>,
}

impl ActionMap {
    #[must_use]
    pub fn new() -> Self {
        Self {
            actions: HashMap::new(),
            axes: HashMap::new(),
        }
    }

    /// Binds an input source to a named action.
    pub fn bind_action(&mut self, name: &str, source: InputSource) {
        self.actions
            .entry(name.to_string())
            .or_default()
            .push(source);
    }

    /// Binds an axis source to a named axis.
    pub fn bind_axis(&mut self, name: &str, source: AxisSource) {
        self.axes.entry(name.to_string()).or_default().push(source);
    }

    /// Returns true if any bound source for the action is pressed.
    #[must_use]
    pub fn is_action_pressed(&self, name: &str, input: &InputState) -> bool {
        self.actions.get(name).is_some_and(|sources| {
            sources.iter().any(|s| match s {
                InputSource::Key(k) => input.is_key_down(*k),
                InputSource::Mouse(b) => input.is_mouse_down(*b),
                InputSource::Gamepad(b) => input.is_gamepad_down(*b),
            })
        })
    }

    /// Returns true if any bound source was just pressed this frame.
    #[must_use]
    pub fn is_action_just_pressed(&self, name: &str, input: &InputState) -> bool {
        self.actions.get(name).is_some_and(|sources| {
            sources.iter().any(|s| match s {
                InputSource::Key(k) => input.is_key_just_pressed(*k),
                InputSource::Mouse(b) => input.is_mouse_just_pressed(*b),
                InputSource::Gamepad(b) => input.is_gamepad_just_pressed(*b),
            })
        })
    }

    /// Returns the axis value (-1.0 to 1.0) for a named axis.
    #[must_use]
    pub fn axis_value(&self, name: &str, input: &InputState) -> f32 {
        let Some(sources) = self.axes.get(name) else {
            return 0.0;
        };
        let mut value = 0.0_f32;
        for source in sources {
            let v = match source {
                AxisSource::GamepadAxis(a) => input.gamepad_axis(*a),
                AxisSource::KeyPair { negative, positive } => {
                    let neg = if input.is_key_down(*negative) {
                        -1.0
                    } else {
                        0.0
                    };
                    let pos = if input.is_key_down(*positive) {
                        1.0
                    } else {
                        0.0
                    };
                    neg + pos
                }
                AxisSource::MouseX => input.mouse_delta().x(),
                AxisSource::MouseY => input.mouse_delta().y(),
                AxisSource::MouseScroll => input.mouse_scroll(),
            };
            if v.abs() > value.abs() {
                value = v;
            }
        }
        value.clamp(-1.0, 1.0)
    }

    /// Returns the number of registered actions.
    #[must_use]
    pub fn action_count(&self) -> usize {
        self.actions.len()
    }

    /// Returns the number of registered axes.
    #[must_use]
    pub fn axis_count(&self) -> usize {
        self.axes.len()
    }
}

impl Default for ActionMap {
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
    fn key_press_release() {
        let mut input = InputState::new();
        input.key_press(Key::W);
        assert!(input.is_key_down(Key::W));
        assert!(input.is_key_just_pressed(Key::W));
        input.begin_frame();
        assert!(input.is_key_down(Key::W));
        assert!(!input.is_key_just_pressed(Key::W));
        input.key_release(Key::W);
        assert!(!input.is_key_down(Key::W));
        assert!(input.is_key_just_released(Key::W));
    }

    #[test]
    fn mouse_state() {
        let mut input = InputState::new();
        input.mouse_button_press(MouseButton::Left);
        assert!(input.is_mouse_down(MouseButton::Left));
        assert!(input.is_mouse_just_pressed(MouseButton::Left));
        input.mouse_move(Vec2::new(100.0, 200.0), Vec2::new(5.0, -3.0));
        assert_eq!(input.mouse_position().x(), 100.0);
        assert_eq!(input.mouse_delta().x(), 5.0);
    }

    #[test]
    fn mouse_scroll() {
        let mut input = InputState::new();
        input.mouse_scroll_event(3.0);
        assert_eq!(input.mouse_scroll(), 3.0);
        input.begin_frame();
        assert_eq!(input.mouse_scroll(), 0.0);
    }

    #[test]
    fn gamepad_buttons() {
        let mut input = InputState::new();
        input.gamepad_press(GamepadButton::South);
        assert!(input.is_gamepad_down(GamepadButton::South));
        assert!(input.is_gamepad_just_pressed(GamepadButton::South));
        input.gamepad_release(GamepadButton::South);
        assert!(!input.is_gamepad_down(GamepadButton::South));
    }

    #[test]
    fn gamepad_axes() {
        let mut input = InputState::new();
        input.gamepad_axis_update(GamepadAxis::LeftStickX, 0.75);
        assert!((input.gamepad_axis(GamepadAxis::LeftStickX) - 0.75).abs() < 1e-6);
        assert_eq!(input.gamepad_axis(GamepadAxis::RightStickY), 0.0);
    }

    #[test]
    fn action_map_key_binding() {
        let mut map = ActionMap::new();
        map.bind_action("jump", InputSource::Key(Key::Space));
        map.bind_action("jump", InputSource::Gamepad(GamepadButton::South));

        let mut input = InputState::new();
        assert!(!map.is_action_pressed("jump", &input));
        input.key_press(Key::Space);
        assert!(map.is_action_pressed("jump", &input));
    }

    #[test]
    fn action_map_just_pressed() {
        let mut map = ActionMap::new();
        map.bind_action("fire", InputSource::Mouse(MouseButton::Left));

        let mut input = InputState::new();
        input.mouse_button_press(MouseButton::Left);
        assert!(map.is_action_just_pressed("fire", &input));
        input.begin_frame();
        assert!(!map.is_action_just_pressed("fire", &input));
    }

    #[test]
    fn action_map_unknown() {
        let map = ActionMap::new();
        let input = InputState::new();
        assert!(!map.is_action_pressed("nonexistent", &input));
    }

    #[test]
    fn axis_key_pair() {
        let mut map = ActionMap::new();
        map.bind_axis(
            "horizontal",
            AxisSource::KeyPair {
                negative: Key::A,
                positive: Key::D,
            },
        );

        let mut input = InputState::new();
        assert_eq!(map.axis_value("horizontal", &input), 0.0);
        input.key_press(Key::D);
        assert!((map.axis_value("horizontal", &input) - 1.0).abs() < 1e-6);
        input.key_press(Key::A);
        assert_eq!(map.axis_value("horizontal", &input), 0.0);
    }

    #[test]
    fn axis_gamepad() {
        let mut map = ActionMap::new();
        map.bind_axis("look_x", AxisSource::GamepadAxis(GamepadAxis::RightStickX));

        let mut input = InputState::new();
        input.gamepad_axis_update(GamepadAxis::RightStickX, -0.5);
        assert!((map.axis_value("look_x", &input) - (-0.5)).abs() < 1e-6);
    }

    #[test]
    fn axis_mouse() {
        let mut map = ActionMap::new();
        map.bind_axis("look_x", AxisSource::MouseX);

        let mut input = InputState::new();
        input.mouse_move(Vec2::ZERO, Vec2::new(0.8, 0.0));
        assert!((map.axis_value("look_x", &input) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn axis_unknown() {
        let map = ActionMap::new();
        let input = InputState::new();
        assert_eq!(map.axis_value("nope", &input), 0.0);
    }

    #[test]
    fn action_map_counts() {
        let mut map = ActionMap::new();
        map.bind_action("jump", InputSource::Key(Key::Space));
        map.bind_axis("move_x", AxisSource::MouseX);
        assert_eq!(map.action_count(), 1);
        assert_eq!(map.axis_count(), 1);
    }

    #[test]
    fn begin_frame_clears_transient() {
        let mut input = InputState::new();
        input.key_press(Key::A);
        input.gamepad_press(GamepadButton::Start);
        input.mouse_button_press(MouseButton::Right);
        input.mouse_scroll_event(1.0);
        input.begin_frame();
        assert!(!input.is_key_just_pressed(Key::A));
        assert!(!input.is_gamepad_just_pressed(GamepadButton::Start));
        assert!(!input.is_mouse_just_pressed(MouseButton::Right));
        assert_eq!(input.mouse_scroll(), 0.0);
        // Keys still held
        assert!(input.is_key_down(Key::A));
    }

    #[test]
    fn axis_scroll() {
        let mut map = ActionMap::new();
        map.bind_axis("zoom", AxisSource::MouseScroll);
        let mut input = InputState::new();
        input.mouse_scroll_event(-0.5);
        assert!((map.axis_value("zoom", &input) - (-0.5)).abs() < 1e-6);
    }

    #[test]
    fn multiple_bindings_same_action() {
        let mut map = ActionMap::new();
        map.bind_action("jump", InputSource::Key(Key::Space));
        map.bind_action("jump", InputSource::Key(Key::W));
        let mut input = InputState::new();
        input.key_press(Key::W);
        assert!(map.is_action_pressed("jump", &input));
    }
}
