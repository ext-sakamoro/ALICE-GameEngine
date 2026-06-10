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

/// Closure type used by [`ChoiceCommand`] to pick an option index.
pub type ChooserFn = dyn FnMut(&[String]) -> usize + Send;

/// Multiple-choice dialogue. Asks the supplied chooser closure for a
/// selection, then writes the chosen index to `result_var` (as i64).
///
/// Hosts plug in their own UI via the closure. Tests / starter templates can
/// use [`ChoiceCommand::pick`] to hard-code the index.
pub struct ChoiceCommand {
    pub prompt: String,
    pub options: Vec<String>,
    pub result_var: String,
    chooser: Box<ChooserFn>,
}

impl ChoiceCommand {
    pub fn new(
        prompt: impl Into<String>,
        options: Vec<String>,
        result_var: impl Into<String>,
        chooser: impl FnMut(&[String]) -> usize + Send + 'static,
    ) -> Self {
        Self {
            prompt: prompt.into(),
            options,
            result_var: result_var.into(),
            chooser: Box::new(chooser),
        }
    }

    /// Convenience: always pick option `idx`. Useful in tests and templates.
    #[must_use]
    pub fn pick(
        prompt: impl Into<String>,
        options: Vec<String>,
        result_var: impl Into<String>,
        idx: usize,
    ) -> Self {
        Self::new(prompt, options, result_var, move |_| idx)
    }
}

impl EventCommand for ChoiceCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        ctx.log.push(format!("CHOICE: {}", self.prompt));
        for (i, opt) in self.options.iter().enumerate() {
            ctx.log.push(format!("  ({}) {}", i + 1, opt));
        }
        let idx = (self.chooser)(&self.options);
        if idx >= self.options.len() {
            return CommandStatus::Failed(format!(
                "chooser returned out-of-range index {idx} (options={})",
                self.options.len()
            ));
        }
        // `idx < options.len()` (usize) so `i64::try_from` cannot overflow in
        // practice; the cast is safe on all platforms with usize ≤ 64 bits.
        let idx_i64 = i64::try_from(idx).unwrap_or(i64::MAX);
        ctx.vars.set_int(&self.result_var, idx_i64);
        ctx.log.push(format!("> {}", self.options[idx]));
        CommandStatus::Done
    }
    fn name(&self) -> &'static str {
        "choice"
    }
}

/// Typed value carried by [`SetVarCommand`].
#[derive(Debug, Clone)]
pub enum VarValue {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
}

/// Write a [`VarValue`] into [`ScriptVars`] under `var_name`.
#[derive(Debug, Clone)]
pub struct SetVarCommand {
    pub var_name: String,
    pub value: VarValue,
}

impl SetVarCommand {
    #[must_use]
    pub fn new(var_name: impl Into<String>, value: VarValue) -> Self {
        Self {
            var_name: var_name.into(),
            value,
        }
    }
}

impl EventCommand for SetVarCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        match &self.value {
            VarValue::Int(v) => ctx.vars.set_int(&self.var_name, *v),
            VarValue::Float(v) => ctx.vars.set_float(&self.var_name, *v),
            VarValue::String(v) => ctx.vars.set_string(&self.var_name, v),
            VarValue::Bool(v) => ctx.vars.set_bool(&self.var_name, *v),
        }
        CommandStatus::Done
    }
    fn name(&self) -> &'static str {
        "set_var"
    }
}

/// Comparison operator used by [`IfVarCommand`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

impl Comparison {
    #[must_use]
    pub const fn evaluate(self, lhs: i64, rhs: i64) -> bool {
        match self {
            Self::Eq => lhs == rhs,
            Self::Ne => lhs != rhs,
            Self::Gt => lhs > rhs,
            Self::Lt => lhs < rhs,
            Self::Ge => lhs >= rhs,
            Self::Le => lhs <= rhs,
        }
    }
}

/// Branch on an int [`ScriptVars`] entry compared against a constant.
pub struct IfVarCommand {
    pub var_name: String,
    pub op: Comparison,
    pub rhs: i64,
    pub if_true: Box<dyn EventCommand>,
    pub if_false: Box<dyn EventCommand>,
    branch_taken: Option<bool>,
}

impl IfVarCommand {
    #[must_use]
    pub fn new(
        var_name: impl Into<String>,
        op: Comparison,
        rhs: i64,
        if_true: Box<dyn EventCommand>,
        if_false: Box<dyn EventCommand>,
    ) -> Self {
        Self {
            var_name: var_name.into(),
            op,
            rhs,
            if_true,
            if_false,
            branch_taken: None,
        }
    }
}

impl EventCommand for IfVarCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        if self.branch_taken.is_none() {
            let lhs = ctx.vars.get_int(&self.var_name).unwrap_or(0);
            self.branch_taken = Some(self.op.evaluate(lhs, self.rhs));
        }
        let cmd: &mut dyn EventCommand = if self.branch_taken == Some(true) {
            self.if_true.as_mut()
        } else {
            self.if_false.as_mut()
        };
        cmd.execute(ctx)
    }
    fn name(&self) -> &'static str {
        "if_var"
    }
}

/// Set a boolean switch (stored as a bool [`ScriptVars`] entry).
/// Conceptually distinct from [`SetVarCommand`] with `VarValue::Bool`:
/// switches are the canonical "global game flags" of an RPG (e.g.
/// `hall_door_unlocked`, `cave_visited`).
#[derive(Debug, Clone)]
pub struct SetSwitchCommand {
    pub switch_name: String,
    pub value: bool,
}

impl SetSwitchCommand {
    #[must_use]
    pub fn new(switch_name: impl Into<String>, value: bool) -> Self {
        Self {
            switch_name: switch_name.into(),
            value,
        }
    }
}

impl EventCommand for SetSwitchCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        ctx.vars.set_bool(&self.switch_name, self.value);
        ctx.log
            .push(format!("[switch {} = {}]", self.switch_name, self.value));
        CommandStatus::Done
    }
    fn name(&self) -> &'static str {
        "set_switch"
    }
}

/// Check whether the inventory holds at least `min_count` of `item_name`.
/// Writes the boolean result to `result_var`.
///
/// Inventory items are stored as ints under the key `"item:<name>"`,
/// matching [`GiveItemCommand`].
#[derive(Debug, Clone)]
pub struct HasItemCommand {
    pub item_name: String,
    pub min_count: i64,
    pub result_var: String,
}

impl HasItemCommand {
    #[must_use]
    pub fn new(
        item_name: impl Into<String>,
        min_count: i64,
        result_var: impl Into<String>,
    ) -> Self {
        Self {
            item_name: item_name.into(),
            min_count,
            result_var: result_var.into(),
        }
    }
}

impl EventCommand for HasItemCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        let key = format!("item:{}", self.item_name);
        let owned = ctx.vars.get_int(&key).unwrap_or(0);
        let has = owned >= self.min_count;
        ctx.vars.set_bool(&self.result_var, has);
        ctx.log.push(format!(
            "[has_item {} >= {} ? {}]",
            self.item_name, self.min_count, has
        ));
        CommandStatus::Done
    }
    fn name(&self) -> &'static str {
        "has_item"
    }
}

/// Remove `count` of `item_name` from the inventory. Fails if not enough.
#[derive(Debug, Clone)]
pub struct TakeItemCommand {
    pub item_name: String,
    pub count: i64,
}

impl TakeItemCommand {
    #[must_use]
    pub fn new(item_name: impl Into<String>, count: i64) -> Self {
        Self {
            item_name: item_name.into(),
            count,
        }
    }
}

impl EventCommand for TakeItemCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        let key = format!("item:{}", self.item_name);
        let owned = ctx.vars.get_int(&key).unwrap_or(0);
        if owned < self.count {
            return CommandStatus::Failed(format!(
                "not enough {} (have {owned}, need {})",
                self.item_name, self.count
            ));
        }
        ctx.vars.set_int(&key, owned - self.count);
        ctx.log
            .push(format!("Took {}x {}", self.count, self.item_name));
        CommandStatus::Done
    }
    fn name(&self) -> &'static str {
        "take_item"
    }
}

/// Signal that the player should transition to another map / zone. Sets
/// `"pending_map_transition"` (string) to `destination_id`. The host (engine
/// driver) reads this and performs the actual `WorldProvider::teleport_to`.
#[derive(Debug, Clone)]
pub struct MapTransitionCommand {
    pub destination_id: String,
}

impl MapTransitionCommand {
    #[must_use]
    pub fn new(destination_id: impl Into<String>) -> Self {
        Self {
            destination_id: destination_id.into(),
        }
    }
}

impl EventCommand for MapTransitionCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        ctx.vars
            .set_string("pending_map_transition", &self.destination_id);
        ctx.log
            .push(format!("[map transition -> {}]", self.destination_id));
        CommandStatus::Done
    }
    fn name(&self) -> &'static str {
        "map_transition"
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

/// One line of a [`CutsceneCommand`].
#[derive(Debug, Clone)]
pub struct CutsceneLine {
    pub speaker: String,
    pub text: String,
    /// Ticks to pause after emitting this line.
    pub wait_ticks: u32,
}

impl CutsceneLine {
    #[must_use]
    pub fn new(speaker: impl Into<String>, text: impl Into<String>, wait_ticks: u32) -> Self {
        Self {
            speaker: speaker.into(),
            text: text.into(),
            wait_ticks,
        }
    }
}

/// Emits a sequence of (`speaker`, `text`, `wait_ticks`) lines. Each line is
/// logged immediately, then the command pends for `wait_ticks` ticks before
/// moving on. Acts like an inlined "cutscene" of dialogue without manually
/// chaining [`MessageCommand`] + [`WaitCommand`].
pub struct CutsceneCommand {
    pub lines: Vec<CutsceneLine>,
    current: usize,
    wait_remaining: u32,
    emitted_current: bool,
}

impl CutsceneCommand {
    #[must_use]
    pub const fn new(lines: Vec<CutsceneLine>) -> Self {
        Self {
            lines,
            current: 0,
            wait_remaining: 0,
            emitted_current: false,
        }
    }
}

impl EventCommand for CutsceneCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        if self.current >= self.lines.len() {
            return CommandStatus::Done;
        }
        let line = &self.lines[self.current];
        if !self.emitted_current {
            ctx.log.push(format!("{}: {}", line.speaker, line.text));
            self.emitted_current = true;
            self.wait_remaining = line.wait_ticks;
        }
        if self.wait_remaining > 0 {
            self.wait_remaining -= 1;
            return CommandStatus::Pending;
        }
        self.current += 1;
        self.emitted_current = false;
        if self.current >= self.lines.len() {
            CommandStatus::Done
        } else {
            CommandStatus::Pending
        }
    }
    fn name(&self) -> &'static str {
        "cutscene"
    }
}

/// Run multiple [`EventCommand`]s "in parallel" — each is ticked once per
/// outer step, and the parallel block is done only when **all** children are
/// done. Useful for e.g. animating two NPCs simultaneously, or running a
/// timed countdown alongside dialogue.
///
/// If any child returns [`CommandStatus::Failed`], the parallel block fails
/// immediately with that error.
pub struct ParallelCommand {
    commands: Vec<Box<dyn EventCommand>>,
    done: Vec<bool>,
}

impl ParallelCommand {
    #[must_use]
    pub fn new(commands: Vec<Box<dyn EventCommand>>) -> Self {
        let n = commands.len();
        Self {
            commands,
            done: vec![false; n],
        }
    }
}

impl EventCommand for ParallelCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        let mut all_done = true;
        for (i, cmd) in self.commands.iter_mut().enumerate() {
            if self.done[i] {
                continue;
            }
            match cmd.execute(ctx) {
                CommandStatus::Done => {
                    self.done[i] = true;
                }
                CommandStatus::Pending => {
                    all_done = false;
                }
                CommandStatus::Failed(msg) => {
                    return CommandStatus::Failed(msg);
                }
            }
        }
        if all_done {
            CommandStatus::Done
        } else {
            CommandStatus::Pending
        }
    }
    fn name(&self) -> &'static str {
        "parallel"
    }
}

/// Factory type for commands created lazily inside [`RepeatCommand`] /
/// [`LoopUntilCommand`].
pub type CommandFactory = dyn FnMut() -> Box<dyn EventCommand> + Send;

/// Run a command `count` times by re-building it on each iteration via the
/// factory closure. Done after the `count`-th iteration completes.
pub struct RepeatCommand {
    factory: Box<CommandFactory>,
    inner: Box<dyn EventCommand>,
    remaining: u32,
}

impl RepeatCommand {
    /// `count` must be ≥ 1 (anything less is treated as 1).
    pub fn new<F>(count: u32, mut factory: F) -> Self
    where
        F: FnMut() -> Box<dyn EventCommand> + Send + 'static,
    {
        let first = factory();
        let remaining = count.saturating_sub(1);
        Self {
            factory: Box::new(factory),
            inner: first,
            remaining,
        }
    }
}

impl EventCommand for RepeatCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        match self.inner.execute(ctx) {
            CommandStatus::Done => {
                if self.remaining == 0 {
                    CommandStatus::Done
                } else {
                    self.remaining -= 1;
                    self.inner = (self.factory)();
                    CommandStatus::Pending
                }
            }
            other => other,
        }
    }
    fn name(&self) -> &'static str {
        "repeat"
    }
}

/// Run a command repeatedly until a `ScriptVars` int satisfies the given
/// [`Comparison`]. Each completion of the inner command triggers a re-build
/// via the factory closure. Done when the condition first matches.
pub struct LoopUntilCommand {
    var_name: String,
    op: Comparison,
    rhs: i64,
    factory: Box<CommandFactory>,
    inner: Box<dyn EventCommand>,
}

impl LoopUntilCommand {
    pub fn new<F>(var_name: impl Into<String>, op: Comparison, rhs: i64, mut factory: F) -> Self
    where
        F: FnMut() -> Box<dyn EventCommand> + Send + 'static,
    {
        let first = factory();
        Self {
            var_name: var_name.into(),
            op,
            rhs,
            factory: Box::new(factory),
            inner: first,
        }
    }
}

impl EventCommand for LoopUntilCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        let lhs = ctx.vars.get_int(&self.var_name).unwrap_or(0);
        if self.op.evaluate(lhs, self.rhs) {
            return CommandStatus::Done;
        }
        match self.inner.execute(ctx) {
            CommandStatus::Done => {
                self.inner = (self.factory)();
                CommandStatus::Pending
            }
            other => other,
        }
    }
    fn name(&self) -> &'static str {
        "loop_until"
    }
}

/// Closure type for [`LlmDialogueCommand`].
pub type LlmResponder = dyn FnMut(&str) -> Option<String> + Send;

/// Ask an LLM for a response and log it as a dialogue line. The actual LLM
/// call is delegated to a closure provided at construction, keeping the
/// command transport-agnostic (works with `MockLlm`, real `OpenAI`, on-device
/// Llama, etc.).
pub struct LlmDialogueCommand {
    pub speaker: String,
    pub prompt: String,
    responder: Box<LlmResponder>,
    cached: Option<String>,
}

impl LlmDialogueCommand {
    pub fn new<F>(speaker: impl Into<String>, prompt: impl Into<String>, responder: F) -> Self
    where
        F: FnMut(&str) -> Option<String> + Send + 'static,
    {
        Self {
            speaker: speaker.into(),
            prompt: prompt.into(),
            responder: Box::new(responder),
            cached: None,
        }
    }

    /// Convenience constructor that hard-codes the response — for tests
    /// and starter templates.
    #[must_use]
    pub fn canned(
        speaker: impl Into<String>,
        prompt: impl Into<String>,
        response: impl Into<String>,
    ) -> Self {
        let response: String = response.into();
        Self::new(speaker, prompt, move |_| Some(response.clone()))
    }
}

impl EventCommand for LlmDialogueCommand {
    fn execute(&mut self, ctx: &mut EventContext) -> CommandStatus {
        if self.cached.is_none() {
            self.cached = (self.responder)(&self.prompt);
        }
        match self.cached.as_ref() {
            Some(text) => {
                ctx.log.push(format!("{}: {}", self.speaker, text));
                CommandStatus::Done
            }
            None => CommandStatus::Failed(format!(
                "LLM responder returned None for prompt '{}'",
                self.prompt
            )),
        }
    }
    fn name(&self) -> &'static str {
        "llm_dialogue"
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

    // -----------------------------------------------------------------------
    // Phase 2 commands (Choice / SetVar / IfVar / SetSwitch / HasItem /
    //                  TakeItem / MapTransition)
    // -----------------------------------------------------------------------

    #[test]
    fn choice_command_writes_index() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = ChoiceCommand::pick(
            "Accept?",
            vec!["Accept".into(), "Decline".into()],
            "accept",
            1,
        );
        let status = cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(status, CommandStatus::Done);
        assert_eq!(vars.get_int("accept"), Some(1));
        assert!(log.iter().any(|s| s.contains("Decline")));
    }

    #[test]
    fn choice_command_fails_on_oor_index() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = ChoiceCommand::pick(
            "Pick",
            vec!["A".into(), "B".into()],
            "r",
            5, // out of range
        );
        let status = cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert!(matches!(status, CommandStatus::Failed(_)));
    }

    #[test]
    fn choice_command_with_dynamic_chooser() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut counter = 0;
        let mut cmd = ChoiceCommand::new(
            "Step",
            vec!["A".into(), "B".into(), "C".into()],
            "step",
            move |_| {
                counter += 1;
                counter % 3
            },
        );
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(vars.get_int("step"), Some(1));
    }

    #[test]
    fn set_var_int() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = SetVarCommand::new("score", VarValue::Int(42));
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(vars.get_int("score"), Some(42));
    }

    #[test]
    fn set_var_string() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = SetVarCommand::new("name", VarValue::String("Lyra".into()));
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(vars.get_string("name"), Some("Lyra"));
    }

    #[test]
    fn set_var_bool_and_float() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        SetVarCommand::new("ready", VarValue::Bool(true))
            .execute(&mut ctx(&mut vars, None, &mut log));
        SetVarCommand::new("ratio", VarValue::Float(0.75))
            .execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(vars.get_bool("ready"), Some(true));
        assert_eq!(vars.get_float("ratio"), Some(0.75));
    }

    #[test]
    fn comparison_evaluates_all_ops() {
        assert!(Comparison::Eq.evaluate(5, 5));
        assert!(!Comparison::Eq.evaluate(5, 6));
        assert!(Comparison::Ne.evaluate(5, 6));
        assert!(Comparison::Gt.evaluate(6, 5));
        assert!(Comparison::Lt.evaluate(5, 6));
        assert!(Comparison::Ge.evaluate(5, 5) && Comparison::Ge.evaluate(6, 5));
        assert!(Comparison::Le.evaluate(5, 5) && Comparison::Le.evaluate(4, 5));
    }

    #[test]
    fn if_var_takes_true_branch() {
        let mut vars = ScriptVars::new();
        vars.set_int("level", 10);
        let mut log = Vec::new();
        let mut cmd = IfVarCommand::new(
            "level",
            Comparison::Ge,
            5,
            Box::new(MessageCommand::new("T", "high level")),
            Box::new(MessageCommand::new("F", "low level")),
        );
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert!(log[0].contains("high level"));
    }

    #[test]
    fn if_var_takes_false_branch() {
        let mut vars = ScriptVars::new();
        vars.set_int("gold", 5);
        let mut log = Vec::new();
        let mut cmd = IfVarCommand::new(
            "gold",
            Comparison::Ge,
            100,
            Box::new(MessageCommand::new("T", "afford")),
            Box::new(MessageCommand::new("F", "broke")),
        );
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert!(log[0].contains("broke"));
    }

    #[test]
    fn if_var_defaults_to_zero_on_missing() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = IfVarCommand::new(
            "never_set",
            Comparison::Eq,
            0,
            Box::new(MessageCommand::new("T", "zero")),
            Box::new(MessageCommand::new("F", "nonzero")),
        );
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert!(log[0].contains("zero"));
    }

    #[test]
    fn set_switch_writes_bool() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = SetSwitchCommand::new("hall_door_unlocked", true);
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(vars.get_bool("hall_door_unlocked"), Some(true));
    }

    #[test]
    fn set_switch_and_branch_chain() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        SetSwitchCommand::new("flag", true).execute(&mut ctx(&mut vars, None, &mut log));
        let mut branch = BranchCommand::new(
            "flag",
            Box::new(MessageCommand::new("T", "on")),
            Box::new(MessageCommand::new("F", "off")),
        );
        branch.execute(&mut ctx(&mut vars, None, &mut log));
        assert!(log.iter().any(|s| s.contains("on")));
    }

    #[test]
    fn has_item_true_when_enough_owned() {
        let mut vars = ScriptVars::new();
        vars.set_int("item:potion", 3);
        let mut log = Vec::new();
        let mut cmd = HasItemCommand::new("potion", 2, "has_potion");
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(vars.get_bool("has_potion"), Some(true));
    }

    #[test]
    fn has_item_false_when_short() {
        let mut vars = ScriptVars::new();
        vars.set_int("item:potion", 1);
        let mut log = Vec::new();
        let mut cmd = HasItemCommand::new("potion", 5, "has_potion");
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(vars.get_bool("has_potion"), Some(false));
    }

    #[test]
    fn take_item_succeeds_when_enough() {
        let mut vars = ScriptVars::new();
        vars.set_int("item:potion", 3);
        let mut log = Vec::new();
        let mut cmd = TakeItemCommand::new("potion", 2);
        let status = cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(status, CommandStatus::Done);
        assert_eq!(vars.get_int("item:potion"), Some(1));
    }

    #[test]
    fn take_item_fails_when_short() {
        let mut vars = ScriptVars::new();
        vars.set_int("item:key", 0);
        let mut log = Vec::new();
        let mut cmd = TakeItemCommand::new("key", 1);
        let status = cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert!(matches!(status, CommandStatus::Failed(_)));
    }

    #[test]
    fn map_transition_sets_pending_var() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = MapTransitionCommand::new("cave_entrance");
        cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(
            vars.get_string("pending_map_transition"),
            Some("cave_entrance")
        );
    }

    #[test]
    fn full_phase2_script_branches_on_choice() {
        // Realistic flow: ask, set switch from choice, branch on switch.
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut script = EventScript::new();
        script.push(Box::new(ChoiceCommand::pick(
            "Take quest?",
            vec!["Yes".into(), "No".into()],
            "accept_idx",
            0,
        )));
        script.push(Box::new(IfVarCommand::new(
            "accept_idx",
            Comparison::Eq,
            0,
            Box::new(SetSwitchCommand::new("quest_active", true)),
            Box::new(SetSwitchCommand::new("quest_active", false)),
        )));
        while !script.is_done() {
            script.step(&mut ctx(&mut vars, None, &mut log));
        }
        assert_eq!(vars.get_bool("quest_active"), Some(true));
    }

    // -----------------------------------------------------------------------
    // Phase B: Cutscene / Parallel / Repeat / LoopUntil / LlmDialogue
    // -----------------------------------------------------------------------

    #[test]
    fn cutscene_emits_all_lines_in_order() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = CutsceneCommand::new(vec![
            CutsceneLine::new("A", "first", 0),
            CutsceneLine::new("B", "second", 0),
            CutsceneLine::new("C", "third", 0),
        ]);
        for _ in 0..10 {
            if matches!(
                cmd.execute(&mut ctx(&mut vars, None, &mut log)),
                CommandStatus::Done
            ) {
                break;
            }
        }
        assert_eq!(log.len(), 3);
        assert!(log[0].contains("A: first"));
        assert!(log[2].contains("C: third"));
    }

    #[test]
    fn cutscene_waits_between_lines() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = CutsceneCommand::new(vec![
            CutsceneLine::new("A", "1", 2),
            CutsceneLine::new("B", "2", 0),
        ]);
        let mut steps = 0;
        loop {
            let s = cmd.execute(&mut ctx(&mut vars, None, &mut log));
            steps += 1;
            if s == CommandStatus::Done || steps > 10 {
                break;
            }
        }
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn parallel_runs_children_concurrently() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = ParallelCommand::new(vec![
            Box::new(MessageCommand::new("A", "alpha")),
            Box::new(MessageCommand::new("B", "beta")),
        ]);
        let s = cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(s, CommandStatus::Done);
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn parallel_pending_when_any_pending() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = ParallelCommand::new(vec![
            Box::new(MessageCommand::new("A", "fast")),
            Box::new(WaitCommand::new(3)),
        ]);
        // Tick 1: A done; Wait(3): 1<3 → Pending → overall Pending
        assert_eq!(
            cmd.execute(&mut ctx(&mut vars, None, &mut log)),
            CommandStatus::Pending
        );
        // Tick 2: Wait(3): 2<3 → Pending
        assert_eq!(
            cmd.execute(&mut ctx(&mut vars, None, &mut log)),
            CommandStatus::Pending
        );
        // Tick 3: Wait(3): 3≥3 → Done → overall Done
        assert_eq!(
            cmd.execute(&mut ctx(&mut vars, None, &mut log)),
            CommandStatus::Done
        );
    }

    #[test]
    fn parallel_fails_when_any_fails() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = ParallelCommand::new(vec![
            Box::new(MessageCommand::new("A", "ok")),
            Box::new(ChangeAttrCommand::new("hp", 1.0)),
        ]);
        assert!(matches!(
            cmd.execute(&mut ctx(&mut vars, None, &mut log)),
            CommandStatus::Failed(_)
        ));
    }

    #[test]
    fn repeat_runs_count_iterations() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = RepeatCommand::new(3, || Box::new(MessageCommand::new("X", "tick")));
        for _ in 0..10 {
            if matches!(
                cmd.execute(&mut ctx(&mut vars, None, &mut log)),
                CommandStatus::Done
            ) {
                break;
            }
        }
        assert_eq!(log.len(), 3);
    }

    #[test]
    fn repeat_minimum_one_iteration() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = RepeatCommand::new(0, || Box::new(MessageCommand::new("X", "once")));
        for _ in 0..5 {
            if matches!(
                cmd.execute(&mut ctx(&mut vars, None, &mut log)),
                CommandStatus::Done
            ) {
                break;
            }
        }
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn loop_until_exits_on_condition() {
        let mut vars = ScriptVars::new();
        vars.set_int("counter", 0);
        let mut log = Vec::new();
        let mut cmd = LoopUntilCommand::new("counter", Comparison::Ge, 3, || {
            Box::new(MessageCommand::new("tick", "."))
        });
        for _ in 0..10 {
            let s = cmd.execute(&mut ctx(&mut vars, None, &mut log));
            if s == CommandStatus::Done {
                break;
            }
            let c = vars.get_int("counter").unwrap_or(0);
            vars.set_int("counter", c + 1);
        }
        assert!(vars.get_int("counter").unwrap_or(0) >= 3);
    }

    #[test]
    fn llm_dialogue_canned_response() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd =
            LlmDialogueCommand::canned("Sage", "What is the meaning of life?", "It is to play.");
        let s = cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert_eq!(s, CommandStatus::Done);
        assert!(log[0].contains("Sage: It is to play."));
    }

    #[test]
    fn llm_dialogue_fails_on_none() {
        let mut vars = ScriptVars::new();
        let mut log = Vec::new();
        let mut cmd = LlmDialogueCommand::new("X", "prompt", |_| None);
        let s = cmd.execute(&mut ctx(&mut vars, None, &mut log));
        assert!(matches!(s, CommandStatus::Failed(_)));
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
