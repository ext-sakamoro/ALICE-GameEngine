//! Verse-compatible gameplay primitives (UE6 Verse language inspired).
//!
//! Provides structured concurrency, failure contexts, transactions,
//! reactive variables, and typed events for gameplay scripting.

// ---------------------------------------------------------------------------
// Failable<T> — Verse <decides> equivalent
// ---------------------------------------------------------------------------

/// Result type for failable operations (Verse `<decides>` equivalent).
/// Expressions either succeed with a value or fail silently.
pub type Failable<T> = Result<T, ()>;

/// Converts a bool condition to `Failable` (Verse comparison-as-failure).
///
/// # Errors
///
/// Returns `Err(())` if the condition is false.
#[inline]
pub const fn decide(condition: bool) -> Failable<()> {
    if condition {
        Ok(())
    } else {
        Err(())
    }
}

/// Chains failable operations with fallback (Verse `or` chaining).
#[inline]
#[must_use]
pub fn or_else<T>(primary: Failable<T>, fallback: impl FnOnce() -> T) -> T {
    primary.unwrap_or_else(|()| fallback())
}

// ---------------------------------------------------------------------------
// Transaction — Verse <transacts> equivalent
// ---------------------------------------------------------------------------

/// Snapshot-based transaction for ECS state rollback.
/// Captures state before speculative operations; rolls back on failure.
#[derive(Debug, Clone)]
pub struct Transaction<S: Clone> {
    snapshot: S,
    committed: bool,
}

impl<S: Clone> Transaction<S> {
    /// Begins a transaction by snapshotting current state.
    #[must_use]
    pub fn begin(state: &S) -> Self {
        Self {
            snapshot: state.clone(),
            committed: false,
        }
    }

    /// Commits the transaction (changes become permanent).
    pub const fn commit(&mut self) {
        self.committed = true;
    }

    /// Rolls back: restores the original state. Returns the snapshot.
    #[must_use]
    pub fn rollback(self) -> S {
        self.snapshot
    }

    /// Returns whether the transaction has been committed.
    #[must_use]
    pub const fn is_committed(&self) -> bool {
        self.committed
    }

    /// Executes a failable operation within the transaction.
    /// On failure, the state is restored from the snapshot.
    ///
    /// # Errors
    ///
    /// Returns `Err(())` if the operation fails; state is rolled back.
    pub fn execute<F, T>(state: &mut S, op: F) -> Failable<T>
    where
        F: FnOnce(&mut S) -> Failable<T>,
    {
        let snapshot = state.clone();
        let result = op(state);
        if result.is_err() {
            *state = snapshot;
        }
        result
    }
}

// ---------------------------------------------------------------------------
// StickyEvent<T> — retains last value
// ---------------------------------------------------------------------------

/// Event that retains its last signaled value (Verse `sticky_event`).
#[derive(Debug, Clone)]
pub struct StickyEvent<T: Clone> {
    value: Option<T>,
    version: u64,
}

impl<T: Clone> StickyEvent<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            value: None,
            version: 0,
        }
    }

    /// Signals a new value.
    pub fn signal(&mut self, value: T) {
        self.value = Some(value);
        self.version += 1;
    }

    /// Returns the current value (if any).
    #[must_use]
    pub const fn get(&self) -> Option<&T> {
        self.value.as_ref()
    }

    /// Returns the version counter.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Clears the stored value.
    pub fn clear(&mut self) {
        self.value = None;
    }
}

impl<T: Clone> Default for StickyEvent<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SubscribableEvent<T> — broadcast to multiple subscribers
// ---------------------------------------------------------------------------

/// Subscriber handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SubId(pub u32);

/// Broadcast event with multiple subscribers (Verse `subscribable_event`).
pub struct SubscribableEvent<T: Clone> {
    next_id: u32,
    pending: Vec<T>,
    subscribers: Vec<SubId>,
}

impl<T: Clone> SubscribableEvent<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_id: 0,
            pending: Vec::new(),
            subscribers: Vec::new(),
        }
    }

    /// Subscribes and returns a handle.
    pub fn subscribe(&mut self) -> SubId {
        let id = SubId(self.next_id);
        self.next_id += 1;
        self.subscribers.push(id);
        id
    }

    /// Unsubscribes.
    pub fn unsubscribe(&mut self, id: SubId) {
        self.subscribers.retain(|&s| s != id);
    }

    /// Signals a value to all subscribers.
    pub fn signal(&mut self, value: T) {
        self.pending.push(value);
    }

    /// Drains pending signals.
    pub fn drain(&mut self) -> Vec<T> {
        std::mem::take(&mut self.pending)
    }

    /// Returns subscriber count.
    #[must_use]
    pub const fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

impl<T: Clone> Default for SubscribableEvent<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// LiveVar<T> — reactive variable (Verse live variables)
// ---------------------------------------------------------------------------

/// Reactive variable that tracks changes.
#[derive(Debug, Clone)]
pub struct LiveVar<T: Clone + PartialEq> {
    value: T,
    dirty: bool,
    version: u64,
}

impl<T: Clone + PartialEq> LiveVar<T> {
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self {
            value,
            dirty: false,
            version: 0,
        }
    }

    /// Gets the current value.
    #[must_use]
    pub const fn get(&self) -> &T {
        &self.value
    }

    /// Sets a new value. Marks dirty if changed.
    pub fn set(&mut self, new_value: T) {
        if self.value != new_value {
            self.value = new_value;
            self.dirty = true;
            self.version += 1;
        }
    }

    /// Returns true if the value has changed since last `clear_dirty()`.
    #[must_use]
    pub const fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clears the dirty flag.
    pub const fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Returns the change version.
    #[must_use]
    pub const fn version(&self) -> u64 {
        self.version
    }
}

// ---------------------------------------------------------------------------
// Coroutine — tick-aligned cooperative task
// ---------------------------------------------------------------------------

/// State of a coroutine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoroutineState {
    Running,
    Sleeping { wake_at: u64 },
    WaitingForNextTick,
    Completed,
    Cancelled,
}

/// A lightweight coroutine that executes within the game tick.
#[derive(Debug, Clone)]
pub struct Coroutine {
    pub name: String,
    pub state: CoroutineState,
    pub priority: i32,
}

impl Coroutine {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            state: CoroutineState::Running,
            priority: 0,
        }
    }

    /// Requests sleep for `ticks` frames.
    pub const fn sleep(&mut self, wake_at_tick: u64) {
        self.state = CoroutineState::Sleeping {
            wake_at: wake_at_tick,
        };
    }

    /// Yields until next tick.
    pub const fn next_tick(&mut self) {
        self.state = CoroutineState::WaitingForNextTick;
    }

    /// Marks completed.
    pub const fn complete(&mut self) {
        self.state = CoroutineState::Completed;
    }

    /// Cancels.
    pub const fn cancel(&mut self) {
        self.state = CoroutineState::Cancelled;
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(
            self.state,
            CoroutineState::Running
                | CoroutineState::Sleeping { .. }
                | CoroutineState::WaitingForNextTick
        )
    }
}

// ---------------------------------------------------------------------------
// TickExecutor — runs coroutines per frame
// ---------------------------------------------------------------------------

/// Single-threaded tick-aligned coroutine executor.
pub struct TickExecutor {
    pub coroutines: Vec<Coroutine>,
    pub current_tick: u64,
}

impl TickExecutor {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            coroutines: Vec::new(),
            current_tick: 0,
        }
    }

    /// Spawns a coroutine.
    pub fn spawn(&mut self, coroutine: Coroutine) -> usize {
        self.coroutines.push(coroutine);
        self.coroutines.len() - 1
    }

    /// Advances one tick: wakes sleeping coroutines, resets next-tick waiters.
    pub fn tick(&mut self) {
        self.current_tick += 1;
        for co in &mut self.coroutines {
            match co.state {
                CoroutineState::Sleeping { wake_at } if self.current_tick >= wake_at => {
                    co.state = CoroutineState::Running;
                }
                CoroutineState::WaitingForNextTick => {
                    co.state = CoroutineState::Running;
                }
                _ => {}
            }
        }
    }

    /// Returns the number of active (non-completed/cancelled) coroutines.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.coroutines.iter().filter(|c| c.is_active()).count()
    }

    /// Cancels all coroutines (Verse scope exit behavior).
    pub fn cancel_all(&mut self) {
        for co in &mut self.coroutines {
            if co.is_active() {
                co.state = CoroutineState::Cancelled;
            }
        }
    }

    /// Removes completed/cancelled coroutines.
    pub fn cleanup(&mut self) {
        self.coroutines.retain(Coroutine::is_active);
    }
}

impl Default for TickExecutor {
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
    fn decide_pass() {
        assert_eq!(decide(true), Ok(()));
    }

    #[test]
    fn decide_fail() {
        assert_eq!(decide(false), Err(()));
    }

    #[test]
    fn or_else_primary() {
        let result = or_else(Ok(42), || 0);
        assert_eq!(result, 42);
    }

    #[test]
    fn or_else_fallback() {
        let result = or_else(Err(()), || 99);
        assert_eq!(result, 99);
    }

    #[test]
    fn transaction_commit() {
        let mut state = 100_i32;
        let mut tx = Transaction::begin(&state);
        state -= 30;
        tx.commit();
        assert!(tx.is_committed());
        assert_eq!(state, 70);
    }

    #[test]
    fn transaction_rollback() {
        let mut state = 100_i32;
        let tx = Transaction::begin(&state);
        state -= 30;
        state = tx.rollback();
        assert_eq!(state, 100);
    }

    #[test]
    fn transaction_execute_success() {
        let mut gold = 100_i32;
        let result = Transaction::execute(&mut gold, |g| {
            *g -= 30;
            decide(*g >= 0)
        });
        assert!(result.is_ok());
        assert_eq!(gold, 70);
    }

    #[test]
    fn transaction_execute_rollback() {
        let mut gold = 20_i32;
        let result = Transaction::execute(&mut gold, |g| {
            *g -= 50;
            decide(*g >= 0)
        });
        assert!(result.is_err());
        assert_eq!(gold, 20); // rolled back
    }

    #[test]
    fn sticky_event() {
        let mut ev = StickyEvent::<i32>::new();
        assert!(ev.get().is_none());
        ev.signal(42);
        assert_eq!(*ev.get().unwrap(), 42);
        ev.signal(100);
        assert_eq!(*ev.get().unwrap(), 100);
        assert_eq!(ev.version(), 2);
    }

    #[test]
    fn subscribable_event() {
        let mut ev = SubscribableEvent::<String>::new();
        let _s1 = ev.subscribe();
        let s2 = ev.subscribe();
        assert_eq!(ev.subscriber_count(), 2);
        ev.signal("hello".to_string());
        let pending = ev.drain();
        assert_eq!(pending.len(), 1);
        ev.unsubscribe(s2);
        assert_eq!(ev.subscriber_count(), 1);
    }

    #[test]
    fn live_var_dirty() {
        let mut v = LiveVar::new(10);
        assert!(!v.is_dirty());
        v.set(20);
        assert!(v.is_dirty());
        assert_eq!(*v.get(), 20);
        v.clear_dirty();
        assert!(!v.is_dirty());
    }

    #[test]
    fn live_var_no_change() {
        let mut v = LiveVar::new(10);
        v.set(10); // same value
        assert!(!v.is_dirty());
    }

    #[test]
    fn coroutine_lifecycle() {
        let mut co = Coroutine::new("test");
        assert!(co.is_active());
        co.complete();
        assert!(!co.is_active());
    }

    #[test]
    fn coroutine_sleep_wake() {
        let mut exec = TickExecutor::new();
        let idx = exec.spawn(Coroutine::new("sleeper"));
        exec.coroutines[idx].sleep(3);
        exec.tick(); // tick 1
        assert!(matches!(
            exec.coroutines[idx].state,
            CoroutineState::Sleeping { .. }
        ));
        exec.tick(); // tick 2
        exec.tick(); // tick 3 — wake
        assert_eq!(exec.coroutines[idx].state, CoroutineState::Running);
    }

    #[test]
    fn coroutine_next_tick() {
        let mut exec = TickExecutor::new();
        let idx = exec.spawn(Coroutine::new("yielder"));
        exec.coroutines[idx].next_tick();
        assert_eq!(
            exec.coroutines[idx].state,
            CoroutineState::WaitingForNextTick
        );
        exec.tick();
        assert_eq!(exec.coroutines[idx].state, CoroutineState::Running);
    }

    #[test]
    fn executor_cancel_all() {
        let mut exec = TickExecutor::new();
        exec.spawn(Coroutine::new("a"));
        exec.spawn(Coroutine::new("b"));
        exec.cancel_all();
        assert_eq!(exec.active_count(), 0);
    }

    #[test]
    fn executor_cleanup() {
        let mut exec = TickExecutor::new();
        exec.spawn(Coroutine::new("active"));
        let idx = exec.spawn(Coroutine::new("done"));
        exec.coroutines[idx].complete();
        exec.cleanup();
        assert_eq!(exec.coroutines.len(), 1);
    }

    #[test]
    fn live_var_version() {
        let mut v = LiveVar::new(0);
        v.set(1);
        v.set(2);
        assert_eq!(v.version(), 2);
    }

    #[test]
    fn sticky_event_clear() {
        let mut ev = StickyEvent::<i32>::new();
        ev.signal(42);
        ev.clear();
        assert!(ev.get().is_none());
    }
}
