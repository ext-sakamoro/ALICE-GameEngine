//! Scripting support: event bus, timers, and script execution context.
//!
//! Provides a decoupled event-driven communication layer (publish/subscribe)
//! plus frame-based and real-time timers for scheduled callbacks.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Event Bus
// ---------------------------------------------------------------------------

/// An event with a name and optional payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    pub payload: EventPayload,
}

/// Typed event payloads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventPayload {
    None,
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Vec3([f32; 3]),
}

impl Event {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            payload: EventPayload::None,
        }
    }

    #[must_use]
    pub fn with_int(name: &str, value: i64) -> Self {
        Self {
            name: name.to_string(),
            payload: EventPayload::Int(value),
        }
    }

    #[must_use]
    pub fn with_float(name: &str, value: f64) -> Self {
        Self {
            name: name.to_string(),
            payload: EventPayload::Float(value),
        }
    }

    #[must_use]
    pub fn with_string(name: &str, value: &str) -> Self {
        Self {
            name: name.to_string(),
            payload: EventPayload::String(value.to_string()),
        }
    }

    #[must_use]
    pub fn with_bool(name: &str, value: bool) -> Self {
        Self {
            name: name.to_string(),
            payload: EventPayload::Bool(value),
        }
    }
}

/// Subscriber ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubscriberId(pub u32);

/// Publish/Subscribe event bus.
pub struct EventBus {
    queue: Vec<Event>,
    subscribers: HashMap<String, Vec<SubscriberId>>,
    next_id: u32,
}

impl EventBus {
    #[must_use]
    pub fn new() -> Self {
        Self {
            queue: Vec::new(),
            subscribers: HashMap::new(),
            next_id: 0,
        }
    }

    /// Publishes an event to the queue.
    pub fn publish(&mut self, event: Event) {
        self.queue.push(event);
    }

    /// Subscribes to events with the given name. Returns a subscriber ID.
    pub fn subscribe(&mut self, event_name: &str) -> SubscriberId {
        let id = SubscriberId(self.next_id);
        self.next_id += 1;
        self.subscribers
            .entry(event_name.to_string())
            .or_default()
            .push(id);
        id
    }

    /// Unsubscribes a subscriber from an event.
    pub fn unsubscribe(&mut self, event_name: &str, id: SubscriberId) {
        if let Some(subs) = self.subscribers.get_mut(event_name) {
            subs.retain(|&s| s != id);
        }
    }

    /// Returns all subscribers for a given event name.
    #[must_use]
    pub fn subscribers_for(&self, event_name: &str) -> &[SubscriberId] {
        self.subscribers
            .get(event_name)
            .map_or(&[], |v| v.as_slice())
    }

    /// Drains the event queue.
    pub fn drain(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.queue)
    }

    /// Returns the number of queued events.
    #[must_use]
    pub const fn queued_count(&self) -> usize {
        self.queue.len()
    }

    /// Returns the total number of unique event names with subscribers.
    #[must_use]
    pub fn subscription_count(&self) -> usize {
        self.subscribers.len()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Timer
// ---------------------------------------------------------------------------

/// Timer mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimerMode {
    /// Fires once and stops.
    OneShot,
    /// Repeats indefinitely.
    Repeating,
}

/// A timer that fires events.
#[derive(Debug, Clone)]
pub struct Timer {
    pub name: String,
    pub duration: f32,
    pub elapsed: f32,
    pub mode: TimerMode,
    pub active: bool,
    pub fires: u32,
}

impl Timer {
    #[must_use]
    pub fn new(name: &str, duration: f32, mode: TimerMode) -> Self {
        Self {
            name: name.to_string(),
            duration,
            elapsed: 0.0,
            mode,
            active: true,
            fires: 0,
        }
    }

    /// Advances the timer by `dt` seconds. Returns true if the timer fired.
    pub fn update(&mut self, dt: f32) -> bool {
        if !self.active {
            return false;
        }
        self.elapsed += dt;
        if self.elapsed >= self.duration {
            self.fires += 1;
            match self.mode {
                TimerMode::OneShot => {
                    self.active = false;
                }
                TimerMode::Repeating => {
                    self.elapsed -= self.duration;
                }
            }
            true
        } else {
            false
        }
    }

    /// Resets the timer.
    pub const fn reset(&mut self) {
        self.elapsed = 0.0;
        self.fires = 0;
        self.active = true;
    }

    /// Returns progress as 0.0..1.0.
    #[must_use]
    pub fn progress(&self) -> f32 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// TimerManager
// ---------------------------------------------------------------------------

/// Manages multiple named timers.
pub struct TimerManager {
    timers: Vec<Timer>,
}

impl TimerManager {
    #[must_use]
    pub const fn new() -> Self {
        Self { timers: Vec::new() }
    }

    /// Adds a timer.
    pub fn add(&mut self, timer: Timer) -> usize {
        self.timers.push(timer);
        self.timers.len() - 1
    }

    /// Updates all timers. Returns names of timers that fired.
    pub fn update(&mut self, dt: f32) -> Vec<String> {
        let mut fired = Vec::new();
        for timer in &mut self.timers {
            if timer.update(dt) {
                fired.push(timer.name.clone());
            }
        }
        fired
    }

    /// Finds a timer by name.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&Timer> {
        self.timers.iter().find(|t| t.name == name)
    }

    /// Returns the number of active timers.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.timers.iter().filter(|t| t.active).count()
    }

    #[must_use]
    pub const fn count(&self) -> usize {
        self.timers.len()
    }
}

impl Default for TimerManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ScriptVar — generic variable storage for scripts
// ---------------------------------------------------------------------------

/// Script-accessible variable storage.
pub struct ScriptVars {
    ints: HashMap<String, i64>,
    floats: HashMap<String, f64>,
    strings: HashMap<String, String>,
    bools: HashMap<String, bool>,
}

impl ScriptVars {
    #[must_use]
    pub fn new() -> Self {
        Self {
            ints: HashMap::new(),
            floats: HashMap::new(),
            strings: HashMap::new(),
            bools: HashMap::new(),
        }
    }

    pub fn set_int(&mut self, key: &str, value: i64) {
        self.ints.insert(key.to_string(), value);
    }

    #[must_use]
    pub fn get_int(&self, key: &str) -> Option<i64> {
        self.ints.get(key).copied()
    }

    pub fn set_float(&mut self, key: &str, value: f64) {
        self.floats.insert(key.to_string(), value);
    }

    #[must_use]
    pub fn get_float(&self, key: &str) -> Option<f64> {
        self.floats.get(key).copied()
    }

    pub fn set_string(&mut self, key: &str, value: &str) {
        self.strings.insert(key.to_string(), value.to_string());
    }

    #[must_use]
    pub fn get_string(&self, key: &str) -> Option<&str> {
        self.strings.get(key).map(std::string::String::as_str)
    }

    pub fn set_bool(&mut self, key: &str, value: bool) {
        self.bools.insert(key.to_string(), value);
    }

    #[must_use]
    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.bools.get(key).copied()
    }

    #[must_use]
    pub fn total_count(&self) -> usize {
        self.ints.len() + self.floats.len() + self.strings.len() + self.bools.len()
    }
}

impl Default for ScriptVars {
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
    fn event_new() {
        let e = Event::new("player_died");
        assert_eq!(e.name, "player_died");
        assert!(matches!(e.payload, EventPayload::None));
    }

    #[test]
    fn event_with_int() {
        let e = Event::with_int("score", 100);
        assert!(matches!(e.payload, EventPayload::Int(100)));
    }

    #[test]
    fn event_with_float() {
        let e = Event::with_float("speed", 3.14);
        assert!(matches!(e.payload, EventPayload::Float(v) if (v - 3.14).abs() < 1e-10));
    }

    #[test]
    fn event_with_string() {
        let e = Event::with_string("msg", "hello");
        assert!(matches!(e.payload, EventPayload::String(ref s) if s == "hello"));
    }

    #[test]
    fn event_with_bool() {
        let e = Event::with_bool("alive", true);
        assert!(matches!(e.payload, EventPayload::Bool(true)));
    }

    #[test]
    fn event_bus_publish_drain() {
        let mut bus = EventBus::new();
        bus.publish(Event::new("test"));
        bus.publish(Event::new("test2"));
        assert_eq!(bus.queued_count(), 2);
        let events = bus.drain();
        assert_eq!(events.len(), 2);
        assert_eq!(bus.queued_count(), 0);
    }

    #[test]
    fn event_bus_subscribe() {
        let mut bus = EventBus::new();
        let id = bus.subscribe("hit");
        assert_eq!(bus.subscribers_for("hit").len(), 1);
        assert_eq!(bus.subscribers_for("hit")[0], id);
    }

    #[test]
    fn event_bus_unsubscribe() {
        let mut bus = EventBus::new();
        let id = bus.subscribe("hit");
        bus.unsubscribe("hit", id);
        assert_eq!(bus.subscribers_for("hit").len(), 0);
    }

    #[test]
    fn event_bus_multiple_subscribers() {
        let mut bus = EventBus::new();
        bus.subscribe("damage");
        bus.subscribe("damage");
        bus.subscribe("heal");
        assert_eq!(bus.subscribers_for("damage").len(), 2);
        assert_eq!(bus.subscribers_for("heal").len(), 1);
    }

    #[test]
    fn event_bus_no_subscribers() {
        let bus = EventBus::new();
        assert_eq!(bus.subscribers_for("nothing").len(), 0);
    }

    #[test]
    fn event_bus_subscription_count() {
        let mut bus = EventBus::new();
        bus.subscribe("a");
        bus.subscribe("b");
        assert_eq!(bus.subscription_count(), 2);
    }

    #[test]
    fn timer_one_shot() {
        let mut t = Timer::new("boom", 1.0, TimerMode::OneShot);
        assert!(!t.update(0.5));
        assert!(t.update(0.6));
        assert!(!t.active);
        assert_eq!(t.fires, 1);
    }

    #[test]
    fn timer_repeating() {
        let mut t = Timer::new("tick", 0.5, TimerMode::Repeating);
        assert!(t.update(0.6));
        assert!(t.active);
        assert!(t.update(0.5));
        assert_eq!(t.fires, 2);
    }

    #[test]
    fn timer_inactive_no_fire() {
        let mut t = Timer::new("off", 0.1, TimerMode::OneShot);
        t.active = false;
        assert!(!t.update(1.0));
    }

    #[test]
    fn timer_reset() {
        let mut t = Timer::new("r", 1.0, TimerMode::OneShot);
        t.update(1.5);
        t.reset();
        assert!(t.active);
        assert_eq!(t.fires, 0);
        assert_eq!(t.elapsed, 0.0);
    }

    #[test]
    fn timer_progress() {
        let mut t = Timer::new("p", 2.0, TimerMode::OneShot);
        t.update(1.0);
        assert!((t.progress() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn timer_manager_update() {
        let mut tm = TimerManager::new();
        tm.add(Timer::new("fast", 0.1, TimerMode::OneShot));
        tm.add(Timer::new("slow", 10.0, TimerMode::OneShot));
        let fired = tm.update(0.2);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0], "fast");
    }

    #[test]
    fn timer_manager_find() {
        let mut tm = TimerManager::new();
        tm.add(Timer::new("x", 1.0, TimerMode::OneShot));
        assert!(tm.find("x").is_some());
        assert!(tm.find("y").is_none());
    }

    #[test]
    fn timer_manager_active_count() {
        let mut tm = TimerManager::new();
        tm.add(Timer::new("a", 0.1, TimerMode::OneShot));
        tm.add(Timer::new("b", 10.0, TimerMode::OneShot));
        tm.update(0.2);
        assert_eq!(tm.active_count(), 1);
    }

    #[test]
    fn script_vars_int() {
        let mut v = ScriptVars::new();
        v.set_int("score", 42);
        assert_eq!(v.get_int("score"), Some(42));
        assert_eq!(v.get_int("nope"), None);
    }

    #[test]
    fn script_vars_float() {
        let mut v = ScriptVars::new();
        v.set_float("speed", 3.14);
        assert!((v.get_float("speed").unwrap() - 3.14).abs() < 1e-10);
    }

    #[test]
    fn script_vars_string() {
        let mut v = ScriptVars::new();
        v.set_string("name", "Alice");
        assert_eq!(v.get_string("name"), Some("Alice"));
    }

    #[test]
    fn script_vars_bool() {
        let mut v = ScriptVars::new();
        v.set_bool("alive", true);
        assert_eq!(v.get_bool("alive"), Some(true));
    }

    #[test]
    fn script_vars_total_count() {
        let mut v = ScriptVars::new();
        v.set_int("a", 1);
        v.set_float("b", 2.0);
        v.set_string("c", "3");
        v.set_bool("d", true);
        assert_eq!(v.total_count(), 4);
    }
}
