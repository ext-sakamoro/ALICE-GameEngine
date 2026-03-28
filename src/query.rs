//! Typed ECS queries and system scheduler.
//!
//! Provides ergonomic multi-component iteration and automatic
//! parallel system execution.

use crate::ecs::{ComponentStore, EntityId};

// ---------------------------------------------------------------------------
// Query helpers
// ---------------------------------------------------------------------------

/// Iterates entities that have both components A and B.
#[must_use]
pub fn query2<'a, A, B>(
    store_a: &'a ComponentStore<A>,
    store_b: &'a ComponentStore<B>,
    entities: &[EntityId],
) -> Vec<(EntityId, &'a A, &'a B)> {
    entities
        .iter()
        .filter_map(|&id| {
            let a = store_a.get(id)?;
            let b = store_b.get(id)?;
            Some((id, a, b))
        })
        .collect()
}

/// Iterates entities that have components A, B, and C.
#[must_use]
pub fn query3<'a, A, B, C>(
    store_a: &'a ComponentStore<A>,
    store_b: &'a ComponentStore<B>,
    store_c: &'a ComponentStore<C>,
    entities: &[EntityId],
) -> Vec<(EntityId, &'a A, &'a B, &'a C)> {
    entities
        .iter()
        .filter_map(|&id| {
            let a = store_a.get(id)?;
            let b = store_b.get(id)?;
            let c = store_c.get(id)?;
            Some((id, a, b, c))
        })
        .collect()
}

/// Counts entities matching component A.
#[must_use]
pub fn count_with<A>(store: &ComponentStore<A>, entities: &[EntityId]) -> usize {
    entities
        .iter()
        .filter(|&&id| store.get(id).is_some())
        .count()
}

/// Returns entity IDs that have component A.
#[must_use]
pub fn filter_with<A>(store: &ComponentStore<A>, entities: &[EntityId]) -> Vec<EntityId> {
    entities
        .iter()
        .filter(|&&id| store.get(id).is_some())
        .copied()
        .collect()
}

/// Returns entity IDs that do NOT have component A.
#[must_use]
pub fn filter_without<A>(store: &ComponentStore<A>, entities: &[EntityId]) -> Vec<EntityId> {
    entities
        .iter()
        .filter(|&&id| store.get(id).is_none())
        .copied()
        .collect()
}

// ---------------------------------------------------------------------------
// SystemScheduler
// ---------------------------------------------------------------------------

/// A named system function.
pub struct ScheduledSystem {
    pub name: String,
    pub enabled: bool,
    pub priority: i32,
}

impl ScheduledSystem {
    #[must_use]
    pub fn new(name: &str, priority: i32) -> Self {
        Self {
            name: name.to_string(),
            enabled: true,
            priority,
        }
    }
}

/// Schedules systems by priority (lower = runs first).
pub struct SystemScheduler {
    systems: Vec<ScheduledSystem>,
}

impl SystemScheduler {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            systems: Vec::new(),
        }
    }

    /// Registers a system.
    pub fn add(&mut self, system: ScheduledSystem) {
        self.systems.push(system);
        self.systems.sort_by_key(|s| s.priority);
    }

    /// Returns execution order (enabled systems, sorted by priority).
    #[must_use]
    pub fn execution_order(&self) -> Vec<&str> {
        self.systems
            .iter()
            .filter(|s| s.enabled)
            .map(|s| s.name.as_str())
            .collect()
    }

    /// Disables a system by name.
    pub fn disable(&mut self, name: &str) {
        for sys in &mut self.systems {
            if sys.name == name {
                sys.enabled = false;
            }
        }
    }

    /// Enables a system by name.
    pub fn enable(&mut self, name: &str) {
        for sys in &mut self.systems {
            if sys.name == name {
                sys.enabled = true;
            }
        }
    }

    #[must_use]
    pub const fn count(&self) -> usize {
        self.systems.len()
    }

    #[must_use]
    pub fn enabled_count(&self) -> usize {
        self.systems.iter().filter(|s| s.enabled).count()
    }
}

impl Default for SystemScheduler {
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
    use crate::ecs::{ComponentStore, EntityId};

    #[test]
    fn query2_basic() {
        let mut store_a = ComponentStore::<f32>::new();
        let mut store_b = ComponentStore::<i32>::new();
        let e1 = EntityId::new(0, 0);
        let e2 = EntityId::new(1, 0);
        let e3 = EntityId::new(2, 0);
        store_a.insert(e1, 1.0);
        store_a.insert(e2, 2.0);
        store_a.insert(e3, 3.0);
        store_b.insert(e1, 10);
        store_b.insert(e3, 30);

        let result = query2(&store_a, &store_b, &[e1, e2, e3]);
        assert_eq!(result.len(), 2);
        assert_eq!(*result[0].1, 1.0);
        assert_eq!(*result[0].2, 10);
    }

    #[test]
    fn query2_empty() {
        let store_a = ComponentStore::<f32>::new();
        let store_b = ComponentStore::<i32>::new();
        let result = query2(&store_a, &store_b, &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn query3_basic() {
        let mut sa = ComponentStore::<f32>::new();
        let mut sb = ComponentStore::<i32>::new();
        let mut sc = ComponentStore::<bool>::new();
        let e = EntityId::new(0, 0);
        sa.insert(e, 1.0);
        sb.insert(e, 2);
        sc.insert(e, true);
        let result = query3(&sa, &sb, &sc, &[e]);
        assert_eq!(result.len(), 1);
        assert_eq!(*result[0].3, true);
    }

    #[test]
    fn query3_partial_miss() {
        let mut sa = ComponentStore::<f32>::new();
        let sb = ComponentStore::<i32>::new();
        let mut sc = ComponentStore::<bool>::new();
        let e = EntityId::new(0, 0);
        sa.insert(e, 1.0);
        sc.insert(e, true);
        let result = query3(&sa, &sb, &sc, &[e]);
        assert!(result.is_empty());
    }

    #[test]
    fn count_with_entities() {
        let mut store = ComponentStore::<f32>::new();
        let e1 = EntityId::new(0, 0);
        let e2 = EntityId::new(1, 0);
        let e3 = EntityId::new(2, 0);
        store.insert(e1, 1.0);
        store.insert(e3, 3.0);
        assert_eq!(count_with(&store, &[e1, e2, e3]), 2);
    }

    #[test]
    fn filter_with_entities() {
        let mut store = ComponentStore::<f32>::new();
        let e1 = EntityId::new(0, 0);
        let e2 = EntityId::new(1, 0);
        store.insert(e1, 1.0);
        let result = filter_with(&store, &[e1, e2]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], e1);
    }

    #[test]
    fn filter_without_entities() {
        let mut store = ComponentStore::<f32>::new();
        let e1 = EntityId::new(0, 0);
        let e2 = EntityId::new(1, 0);
        store.insert(e1, 1.0);
        let result = filter_without(&store, &[e1, e2]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], e2);
    }

    #[test]
    fn scheduler_priority_order() {
        let mut sched = SystemScheduler::new();
        sched.add(ScheduledSystem::new("render", 100));
        sched.add(ScheduledSystem::new("physics", 10));
        sched.add(ScheduledSystem::new("input", 0));
        let order = sched.execution_order();
        assert_eq!(order, vec!["input", "physics", "render"]);
    }

    #[test]
    fn scheduler_disable() {
        let mut sched = SystemScheduler::new();
        sched.add(ScheduledSystem::new("a", 0));
        sched.add(ScheduledSystem::new("b", 1));
        sched.disable("a");
        assert_eq!(sched.execution_order(), vec!["b"]);
        assert_eq!(sched.enabled_count(), 1);
    }

    #[test]
    fn scheduler_enable() {
        let mut sched = SystemScheduler::new();
        sched.add(ScheduledSystem::new("a", 0));
        sched.disable("a");
        sched.enable("a");
        assert_eq!(sched.enabled_count(), 1);
    }

    #[test]
    fn scheduler_empty() {
        let sched = SystemScheduler::new();
        assert!(sched.execution_order().is_empty());
        assert_eq!(sched.count(), 0);
    }
}
