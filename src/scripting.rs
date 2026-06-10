//! Scripting support: event bus, timers, and script execution context.
//!
//! Provides a decoupled event-driven communication layer (publish/subscribe)
//! plus frame-based and real-time timers for scheduled callbacks.

//!
//! ```rust
//! use alice_game_engine::scripting::*;
//! let mut bus = EventBus::new();
//! bus.publish(Event::new("test"));
//! assert_eq!(bus.drain().len(), 1);
//! ```
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
// EventCommand — no-code event scripting (RPG-Cobo inspired)
// ---------------------------------------------------------------------------

/// Result of executing one step of an [`EventCommand`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandStatus {
    /// Command finished — script advances to next command.
    Done,
    /// Command needs more ticks (e.g. [`WaitCommand`]).
    Pending,
    /// Command failed irrecoverably; script reports the error and stops.
    Failed(String),
}

/// Mutable context passed to each [`EventCommand`] on every tick.
///
/// Owns the script's variable store, an optional reference to a battler's
/// attributes (so commands can heal / damage / modify), and a log buffer.
pub struct EventContext<'a> {
    pub vars: &'a mut ScriptVars,
    pub attrs: Option<&'a mut crate::ability::AttributeSet>,
    pub log: &'a mut Vec<String>,
    pub elapsed_ticks: u32,
}

/// A single executable command in an [`EventScript`].
///
/// Designed after RPG-Cobo `.sk` `EventCommands` — commands are small,
/// composable, and may run across multiple ticks via
/// [`CommandStatus::Pending`].
pub trait EventCommand: Send {
    /// Advance the command. Called once per tick until it returns
    /// [`CommandStatus::Done`] or [`CommandStatus::Failed`].
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus;
    /// Identifier for logging / debugging.
    fn name(&self) -> &str;
}

/// Display a line of dialogue or narration.
#[derive(Debug, Clone)]
pub struct MessageCommand {
    pub speaker: String,
    pub text: String,
}

impl MessageCommand {
    #[must_use]
    pub fn new(speaker: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            speaker: speaker.into(),
            text: text.into(),
        }
    }
}

impl EventCommand for MessageCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        ctx.log.push(format!("{}: {}", self.speaker, self.text));
        CommandStatus::Done
    }
    fn name(&self) -> &'static str {
        "message"
    }
}

/// Modify a named attribute on the current battler (if `ctx.attrs` is set).
#[derive(Debug, Clone)]
pub struct ChangeAttrCommand {
    pub attr_name: String,
    pub delta: f32,
}

impl ChangeAttrCommand {
    #[must_use]
    pub fn new(attr_name: impl Into<String>, delta: f32) -> Self {
        Self {
            attr_name: attr_name.into(),
            delta,
        }
    }
}

impl EventCommand for ChangeAttrCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        let Some(attrs) = ctx.attrs.as_mut() else {
            return CommandStatus::Failed("no attrs in context".into());
        };
        if attrs.modify(&self.attr_name, self.delta) {
            ctx.log.push(format!(
                "{} {} by {:+.0} (now {})",
                self.attr_name,
                if self.delta >= 0.0 {
                    "raised"
                } else {
                    "reduced"
                },
                self.delta,
                attrs.value(&self.attr_name)
            ));
            CommandStatus::Done
        } else {
            CommandStatus::Failed(format!("attr '{}' not found", self.attr_name))
        }
    }
    fn name(&self) -> &'static str {
        "change_attr"
    }
}

/// Block the script for `ticks` ticks.
#[derive(Debug, Clone)]
pub struct WaitCommand {
    pub ticks: u32,
    elapsed: u32,
}

impl WaitCommand {
    #[must_use]
    pub const fn new(ticks: u32) -> Self {
        Self { ticks, elapsed: 0 }
    }
}

impl EventCommand for WaitCommand {
    fn execute(&mut self, _ctx: &mut EventContext) -> CommandStatus {
        self.elapsed += 1;
        if self.elapsed >= self.ticks {
            CommandStatus::Done
        } else {
            CommandStatus::Pending
        }
    }
    fn name(&self) -> &'static str {
        "wait"
    }
}

/// Branch on the value of a script variable (boolean).
///
/// On the first tick, evaluates `vars.get_bool(var_name)`; subsequent ticks
/// delegate to the chosen branch until that branch is `Done`.
pub struct BranchCommand {
    pub var_name: String,
    pub if_true: Box<dyn EventCommand>,
    pub if_false: Box<dyn EventCommand>,
    branch_taken: Option<bool>,
}

impl BranchCommand {
    #[must_use]
    pub fn new(
        var_name: impl Into<String>,
        if_true: Box<dyn EventCommand>,
        if_false: Box<dyn EventCommand>,
    ) -> Self {
        Self {
            var_name: var_name.into(),
            if_true,
            if_false,
            branch_taken: None,
        }
    }
}

impl EventCommand for BranchCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        if self.branch_taken.is_none() {
            self.branch_taken = Some(ctx.vars.get_bool(&self.var_name).unwrap_or(false));
        }
        let cmd: &mut dyn EventCommand = if self.branch_taken == Some(true) {
            self.if_true.as_mut()
        } else {
            self.if_false.as_mut()
        };
        cmd.execute(ctx)
    }
    fn name(&self) -> &'static str {
        "branch"
    }
}

/// Add `count` of `item_name` to the inventory, stored in `ctx.vars` under
/// the key `"item:<name>"`.
#[derive(Debug, Clone)]
pub struct GiveItemCommand {
    pub item_name: String,
    pub count: i64,
}

impl GiveItemCommand {
    #[must_use]
    pub fn new(item_name: impl Into<String>, count: i64) -> Self {
        Self {
            item_name: item_name.into(),
            count,
        }
    }
}

impl EventCommand for GiveItemCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        let key = format!("item:{}", self.item_name);
        let current = ctx.vars.get_int(&key).unwrap_or(0);
        ctx.vars.set_int(&key, current + self.count);
        ctx.log
            .push(format!("Got {}x {}", self.count, self.item_name));
        CommandStatus::Done
    }
    fn name(&self) -> &'static str {
        "give_item"
    }
}

/// Signal that a battle should begin. The actual [`crate::battle`] runner is
/// driven by the outer game loop; this command merely sets the variable
/// `"pending_battle"` (string) to the `encounter_id`. The host reads that and
/// transitions into battle, clearing the variable once resolved.
#[derive(Debug, Clone)]
pub struct BeginBattleCommand {
    pub encounter_id: String,
}

impl BeginBattleCommand {
    #[must_use]
    pub fn new(encounter_id: impl Into<String>) -> Self {
        Self {
            encounter_id: encounter_id.into(),
        }
    }
}

impl EventCommand for BeginBattleCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        ctx.vars.set_string("pending_battle", &self.encounter_id);
        ctx.log
            .push(format!("[Battle begins: {}]", self.encounter_id));
        CommandStatus::Done
    }
    fn name(&self) -> &'static str {
        "begin_battle"
    }
}

// ---------------------------------------------------------------------------
// EventScript — sequence of commands
// ---------------------------------------------------------------------------

/// An ordered sequence of [`EventCommand`]s executed one at a time.
///
/// A script `step` advances the current command; if that command returns
/// [`CommandStatus::Done`], the index moves forward.
pub struct EventScript {
    commands: Vec<Box<dyn EventCommand>>,
    current_idx: usize,
    finished: bool,
    failure: Option<String>,
}

impl EventScript {
    #[must_use]
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            current_idx: 0,
            finished: false,
            failure: None,
        }
    }

    /// Append a command. Returns self for builder-style chaining.
    pub fn push(&mut self, cmd: Box<dyn EventCommand>) -> &mut Self {
        self.commands.push(cmd);
        self
    }

    #[must_use]
    pub const fn is_done(&self) -> bool {
        self.finished
    }

    #[must_use]
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    #[must_use]
    pub const fn current_idx(&self) -> usize {
        self.current_idx
    }

    #[must_use]
    pub fn command_count(&self) -> usize {
        self.commands.len()
    }

    /// Advance the script by one tick. Returns the current command's
    /// [`CommandStatus`], or `Done` if the entire script has finished.
    pub fn step(&mut self, ctx: &mut EventContext) -> CommandStatus {
        if self.finished {
            return CommandStatus::Done;
        }
        if self.current_idx >= self.commands.len() {
            self.finished = true;
            return CommandStatus::Done;
        }
        let cmd = self.commands[self.current_idx].as_mut();
        match cmd.execute(ctx) {
            CommandStatus::Done => {
                self.current_idx += 1;
                if self.current_idx >= self.commands.len() {
                    self.finished = true;
                }
                CommandStatus::Done
            }
            CommandStatus::Pending => CommandStatus::Pending,
            CommandStatus::Failed(msg) => {
                self.finished = true;
                self.failure = Some(msg.clone());
                CommandStatus::Failed(msg)
            }
        }
    }
}

impl Default for EventScript {
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

    // -----------------------------------------------------------------------
    // EventCommand tests
    // -----------------------------------------------------------------------

    use crate::ability::{Attribute, AttributeSet};

    fn ctx<'a>(
        vars: &'a mut ScriptVars,
        attrs: Option<&'a mut AttributeSet>,
        log: &'a mut Vec<String>,
    ) -> EventContext<'a> {
        EventContext {
            vars,
            attrs,
            log,
            elapsed_ticks: 0,
        }
    }

    #[test]
    fn message_command_logs_text() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = MessageCommand::new("Elder", "Welcome, traveler.");
        let status = cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(status, CommandStatus::Done);
        assert_eq!(log.len(), 1);
        assert!(log[0].contains("Elder") && log[0].contains("Welcome"));
    }

    #[test]
    fn change_attr_modifies_set() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut attrs = AttributeSet::new();
        attrs.add(Attribute::new("hp", 50.0, 0.0, 100.0));
        let mut cmd = ChangeAttrCommand::new("hp", 20.0);
        let status = cmd.execute(&mut ctx(&mut vars, Some(&mut attrs), &mut log));
        assert_eq!(status, CommandStatus::Done);
        assert!((attrs.value("hp") - 70.0).abs() < f32::EPSILON);
    }

    #[test]
    fn change_attr_fails_without_context_attrs() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = ChangeAttrCommand::new("hp", 10.0);
        let status = cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert!(matches!(status, CommandStatus::Failed(_)));
    }

    #[test]
    fn wait_command_pends_then_done() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = WaitCommand::new(3);
        assert_eq!(
            cmd.execute(&mut ctx(&mut vars, None, &mut log)),
            CommandStatus::Pending
        );
        assert_eq!(
            cmd.execute(&mut ctx(&mut vars, None, &mut log)),
            CommandStatus::Pending
        );
        assert_eq!(
            cmd.execute(&mut ctx(&mut vars, None, &mut log)),
            CommandStatus::Done
        );
    }

    #[test]
    fn branch_command_takes_true_path() {
        let mut vars = ScriptVars::new();
        vars.set_bool("flag", true);
        let mut log = Vec::new();
        let mut cmd = BranchCommand::new(
            "flag",
            Box::new(MessageCommand::new("T", "true path")),
            Box::new(MessageCommand::new("F", "false path")),
        );
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert!(log[0].contains("true path"));
    }

    #[test]
    fn branch_command_falls_back_when_var_missing() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = BranchCommand::new(
            "missing",
            Box::new(MessageCommand::new("T", "yes")),
            Box::new(MessageCommand::new("F", "no")),
        );
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert!(log[0].contains("no"));
    }

    #[test]
    fn give_item_accumulates_in_vars() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = GiveItemCommand::new("potion", 2);
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(vars.get_int("item:potion"), Some(2));
        let mut again = GiveItemCommand::new("potion", 3);
        again.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(vars.get_int("item:potion"), Some(5));
    }

    #[test]
    fn begin_battle_sets_pending_var() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = BeginBattleCommand::new("slime_cave_01");
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(vars.get_string("pending_battle"), Some("slime_cave_01"));
    }

    #[test]
    fn event_script_runs_in_order() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut script = EventScript::new();
        script.push(Box::new(MessageCommand::new("A", "1")));
        script.push(Box::new(MessageCommand::new("B", "2")));
        script.push(Box::new(MessageCommand::new("C", "3")));
        while !script.is_done() {
            script.step(&mut ctx(&mut vars, None, &mut log));
        }
        assert_eq!(log.len(), 3);
        assert!(log[0].contains("A: 1") && log[2].contains("C: 3"));
    }

    #[test]
    fn event_script_pauses_on_wait() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut script = EventScript::new();
        script.push(Box::new(MessageCommand::new("A", "first")));
        script.push(Box::new(WaitCommand::new(2)));
        script.push(Box::new(MessageCommand::new("B", "after wait")));
        // Tick 1: emit "first"
        script.step(&mut ctx(&mut vars, None, &mut log));
        // Tick 2: wait pending
        let s2 = script.step(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(s2, CommandStatus::Pending);
        // Tick 3: wait done
        script.step(&mut ctx(&mut vars, None, &mut log));
        // Tick 4: emit "after wait"
        script.step(&mut ctx(&mut vars, None, &mut log));
        assert!(script.is_done());
        assert!(log[0].contains("first") && log[1].contains("after wait"));
    }

    #[test]
    fn event_script_stops_on_failure() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut script = EventScript::new();
        script.push(Box::new(ChangeAttrCommand::new("hp", 10.0))); // fails (no attrs)
        script.push(Box::new(MessageCommand::new("X", "never runs")));
        let s = script.step(&mut ctx(&mut vars, None, &mut log));
        assert!(matches!(s, CommandStatus::Failed(_)));
        assert!(script.is_done());
        assert!(script.failure().is_some());
        assert!(log.is_empty());
    }
}
