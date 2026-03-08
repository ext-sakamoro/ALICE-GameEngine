#![warn(clippy::all, clippy::pedantic, clippy::nursery)]
#![allow(clippy::module_name_repetitions)]

//! ALICE-GameEngine: Game loop and ECS integrated in SDF space.

use std::collections::{HashMap, HashSet};
use std::fmt;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors that can occur within the game engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEngineError {
    /// The entity does not exist or has been destroyed.
    EntityNotFound(EntityId),
    /// A component was not found for the given entity.
    ComponentNotFound(EntityId),
    /// The scene was not found by name.
    SceneNotFound(String),
    /// Attempted to add a duplicate entity to a scene.
    DuplicateEntity(EntityId),
    /// A generic engine error with a message.
    Generic(String),
}

impl fmt::Display for GameEngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EntityNotFound(id) => write!(f, "entity not found: {id}"),
            Self::ComponentNotFound(id) => {
                write!(f, "component not found for entity: {id}")
            }
            Self::SceneNotFound(name) => write!(f, "scene not found: {name}"),
            Self::DuplicateEntity(id) => write!(f, "duplicate entity: {id}"),
            Self::Generic(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for GameEngineError {}

// ---------------------------------------------------------------------------
// EntityId
// ---------------------------------------------------------------------------

/// A unique identifier for an entity, combining an index and a generation
/// counter to allow safe recycling of indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EntityId {
    index: u32,
    generation: u32,
}

impl EntityId {
    /// Creates a new `EntityId` with the given index and generation.
    #[must_use]
    pub const fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    /// Returns the index portion of this entity id.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// Returns the generation portion of this entity id.
    #[must_use]
    pub const fn generation(self) -> u32 {
        self.generation
    }

    /// Packs index and generation into a single `u64`.
    #[must_use]
    pub const fn to_u64(self) -> u64 {
        (self.generation as u64) << 32 | self.index as u64
    }

    /// Unpacks a `u64` into an `EntityId`.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn from_u64(val: u64) -> Self {
        Self {
            index: val as u32,
            generation: (val >> 32) as u32,
        }
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Entity({}v{})", self.index, self.generation)
    }
}

// ---------------------------------------------------------------------------
// EntityManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of entities: creation, destruction, and index
/// recycling with generation tracking.
pub struct EntityManager {
    generations: Vec<u32>,
    alive: Vec<bool>,
    free_indices: Vec<u32>,
    living_count: usize,
}

impl EntityManager {
    /// Creates a new, empty `EntityManager`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            generations: Vec::new(),
            alive: Vec::new(),
            free_indices: Vec::new(),
            living_count: 0,
        }
    }

    /// Spawns a new entity and returns its id.
    pub fn create(&mut self) -> EntityId {
        if let Some(index) = self.free_indices.pop() {
            let idx = index as usize;
            self.generations[idx] += 1;
            self.alive[idx] = true;
            self.living_count += 1;
            EntityId::new(index, self.generations[idx])
        } else {
            #[allow(clippy::cast_possible_truncation)]
            let index = self.generations.len() as u32;
            self.generations.push(0);
            self.alive.push(true);
            self.living_count += 1;
            EntityId::new(index, 0)
        }
    }

    /// Destroys an entity. Returns `true` if the entity was alive.
    pub fn destroy(&mut self, id: EntityId) -> bool {
        let idx = id.index as usize;
        if idx < self.alive.len() && self.alive[idx] && self.generations[idx] == id.generation {
            self.alive[idx] = false;
            self.free_indices.push(id.index);
            self.living_count -= 1;
            true
        } else {
            false
        }
    }

    /// Checks whether the given entity is currently alive.
    #[must_use]
    pub fn is_alive(&self, id: EntityId) -> bool {
        let idx = id.index as usize;
        idx < self.alive.len() && self.alive[idx] && self.generations[idx] == id.generation
    }

    /// Returns the number of living entities.
    #[must_use]
    pub const fn living_count(&self) -> usize {
        self.living_count
    }
}

impl Default for EntityManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ComponentStore<T>
// ---------------------------------------------------------------------------

/// A sparse-set based store that maps entity ids to component data.
pub struct ComponentStore<T> {
    data: HashMap<u32, (u32, T)>,
}

impl<T> ComponentStore<T> {
    /// Creates a new, empty component store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }

    /// Inserts a component for the given entity, replacing any previous value.
    pub fn insert(&mut self, id: EntityId, component: T) {
        self.data.insert(id.index, (id.generation, component));
    }

    /// Removes the component for the given entity.
    /// Returns the component if it existed and the generation matched.
    pub fn remove(&mut self, id: EntityId) -> Option<T> {
        if let Some((gen, _)) = self.data.get(&id.index) {
            if *gen == id.generation {
                return self.data.remove(&id.index).map(|(_, c)| c);
            }
        }
        None
    }

    /// Returns a reference to the component for the given entity.
    #[must_use]
    pub fn get(&self, id: EntityId) -> Option<&T> {
        self.data
            .get(&id.index)
            .filter(|(gen, _)| *gen == id.generation)
            .map(|(_, c)| c)
    }

    /// Returns a mutable reference to the component for the given entity.
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut T> {
        self.data
            .get_mut(&id.index)
            .filter(|(gen, _)| *gen == id.generation)
            .map(|(_, c)| c)
    }

    /// Returns the number of stored components.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Returns `true` if the store is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Returns `true` if a component exists for the given entity.
    #[must_use]
    pub fn contains(&self, id: EntityId) -> bool {
        self.data
            .get(&id.index)
            .is_some_and(|(gen, _)| *gen == id.generation)
    }
}

impl<T> Default for ComponentStore<T> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Position, rotation, and scale of an entity in 2D space.
#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    pub x: f64,
    pub y: f64,
    pub rotation: f64,
    pub scale_x: f64,
    pub scale_y: f64,
}

impl Transform {
    /// Creates a new transform at the given position with default rotation
    /// and unit scale.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            rotation: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
        }
    }

    /// Creates a transform with all fields specified.
    #[must_use]
    pub const fn with_all(x: f64, y: f64, rotation: f64, scale_x: f64, scale_y: f64) -> Self {
        Self {
            x,
            y,
            rotation,
            scale_x,
            scale_y,
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::new(0.0, 0.0)
    }
}

/// Linear velocity of an entity.
#[derive(Debug, Clone, PartialEq)]
pub struct Velocity {
    pub dx: f64,
    pub dy: f64,
}

impl Velocity {
    /// Creates a new velocity vector.
    #[must_use]
    pub const fn new(dx: f64, dy: f64) -> Self {
        Self { dx, dy }
    }

    /// Returns the squared magnitude.
    #[must_use]
    pub fn magnitude_sq(&self) -> f64 {
        self.dx.mul_add(self.dx, self.dy * self.dy)
    }

    /// Returns the magnitude.
    #[must_use]
    pub fn magnitude(&self) -> f64 {
        self.magnitude_sq().sqrt()
    }
}

impl Default for Velocity {
    fn default() -> Self {
        Self::new(0.0, 0.0)
    }
}

/// Visual representation of an entity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sprite {
    pub width: u32,
    pub height: u32,
    pub color: [u8; 4],
    pub visible: bool,
}

impl Sprite {
    /// Creates a new visible sprite with the given dimensions and color.
    #[must_use]
    pub const fn new(width: u32, height: u32, color: [u8; 4]) -> Self {
        Self {
            width,
            height,
            color,
            visible: true,
        }
    }
}

impl Default for Sprite {
    fn default() -> Self {
        Self::new(1, 1, [255, 255, 255, 255])
    }
}

/// An axis-aligned bounding box used for collision detection.
#[derive(Debug, Clone, PartialEq)]
pub struct AABB {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl AABB {
    /// Creates a new AABB.
    #[must_use]
    pub const fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    /// Tests whether this AABB intersects another.
    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        self.min_x <= other.max_x
            && self.max_x >= other.min_x
            && self.min_y <= other.max_y
            && self.max_y >= other.min_y
    }

    /// Tests whether this AABB contains the given point.
    #[must_use]
    pub fn contains_point(&self, x: f64, y: f64) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }

    /// Returns the center of this AABB.
    #[must_use]
    pub fn center(&self) -> (f64, f64) {
        (
            (self.min_x + self.max_x) * 0.5,
            (self.min_y + self.max_y) * 0.5,
        )
    }

    /// Returns a new AABB expanded by the given amount in all directions.
    #[must_use]
    pub fn expand(&self, amount: f64) -> Self {
        Self {
            min_x: self.min_x - amount,
            min_y: self.min_y - amount,
            max_x: self.max_x + amount,
            max_y: self.max_y + amount,
        }
    }

    /// Returns the width of this AABB.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    /// Returns the height of this AABB.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }

    /// Returns the area of this AABB.
    #[must_use]
    pub fn area(&self) -> f64 {
        self.width() * self.height()
    }
}

impl Default for AABB {
    fn default() -> Self {
        Self::new(0.0, 0.0, 1.0, 1.0)
    }
}

/// A collider component wrapping an AABB and a collision layer.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Collider {
    pub aabb: AABB,
    pub layer: u32,
}

impl Collider {
    /// Creates a new collider with the given AABB and layer.
    #[must_use]
    pub const fn new(aabb: AABB, layer: u32) -> Self {
        Self { aabb, layer }
    }
}

// ---------------------------------------------------------------------------
// World
// ---------------------------------------------------------------------------

/// The central container holding the entity manager and all component stores.
pub struct World {
    pub entity_manager: EntityManager,
    pub transform_store: ComponentStore<Transform>,
    pub velocity_store: ComponentStore<Velocity>,
    pub sprite_store: ComponentStore<Sprite>,
    pub collider_store: ComponentStore<Collider>,
}

impl World {
    /// Creates a new, empty world.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entity_manager: EntityManager::new(),
            transform_store: ComponentStore::new(),
            velocity_store: ComponentStore::new(),
            sprite_store: ComponentStore::new(),
            collider_store: ComponentStore::new(),
        }
    }

    /// Spawns a new entity and returns its id.
    pub fn spawn(&mut self) -> EntityId {
        self.entity_manager.create()
    }

    /// Destroys an entity and removes all its components.
    pub fn despawn(&mut self, id: EntityId) -> bool {
        if self.entity_manager.destroy(id) {
            self.transform_store.remove(id);
            self.velocity_store.remove(id);
            self.sprite_store.remove(id);
            self.collider_store.remove(id);
            true
        } else {
            false
        }
    }

    /// Returns `true` if the entity is alive.
    #[must_use]
    pub fn is_alive(&self, id: EntityId) -> bool {
        self.entity_manager.is_alive(id)
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// PhysicsSystem
// ---------------------------------------------------------------------------

/// A collision pair produced by the physics system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionPair {
    pub entity_a: EntityId,
    pub entity_b: EntityId,
}

/// Handles position updates and AABB collision detection.
pub struct PhysicsSystem;

impl PhysicsSystem {
    /// Updates entity positions based on their velocities.
    pub fn update(world: &mut World, time: &GameTime) {
        let dt = time.delta_seconds;

        // Collect entity ids that have both transform and velocity.
        let ids: Vec<EntityId> = world
            .velocity_store
            .data
            .iter()
            .map(|(&index, &(gen, _))| EntityId::new(index, gen))
            .collect();

        for id in ids {
            if let (Some(vel), Some(tfm)) = (
                world.velocity_store.get(id).cloned(),
                world.transform_store.get_mut(id),
            ) {
                tfm.x += vel.dx * dt;
                tfm.y += vel.dy * dt;
            }
        }
    }

    /// Detects all AABB collision pairs among entities that have both a
    /// transform and a collider. The collider AABB is offset by the
    /// entity's position.
    #[must_use]
    pub fn detect_collisions(world: &World) -> Vec<CollisionPair> {
        let mut entries: Vec<(EntityId, AABB, u32)> = Vec::new();

        for (&index, (gen, collider)) in &world.collider_store.data {
            let id = EntityId::new(index, *gen);
            if let Some(tfm) = world.transform_store.get(id) {
                let world_aabb = AABB::new(
                    collider.aabb.min_x + tfm.x,
                    collider.aabb.min_y + tfm.y,
                    collider.aabb.max_x + tfm.x,
                    collider.aabb.max_y + tfm.y,
                );
                entries.push((id, world_aabb, collider.layer));
            }
        }

        let mut pairs = Vec::new();
        for i in 0..entries.len() {
            for j in (i + 1)..entries.len() {
                if entries[i].1.intersects(&entries[j].1) {
                    pairs.push(CollisionPair {
                        entity_a: entries[i].0,
                        entity_b: entries[j].0,
                    });
                }
            }
        }
        pairs
    }
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// Tracks the current state of keyboard input.
#[derive(Debug, Clone)]
pub struct Input {
    keys_pressed: HashSet<String>,
}

impl Input {
    /// Creates a new input tracker with no keys pressed.
    #[must_use]
    pub fn new() -> Self {
        Self {
            keys_pressed: HashSet::new(),
        }
    }

    /// Records a key press.
    pub fn key_down(&mut self, key: &str) {
        self.keys_pressed.insert(key.to_string());
    }

    /// Records a key release.
    pub fn key_up(&mut self, key: &str) {
        self.keys_pressed.remove(key);
    }

    /// Returns `true` if the given key is currently pressed.
    #[must_use]
    pub fn is_pressed(&self, key: &str) -> bool {
        self.keys_pressed.contains(key)
    }

    /// Returns the number of keys currently pressed.
    #[must_use]
    pub fn pressed_count(&self) -> usize {
        self.keys_pressed.len()
    }

    /// Clears all pressed keys.
    pub fn clear(&mut self) {
        self.keys_pressed.clear();
    }
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// GameTime
// ---------------------------------------------------------------------------

/// Tracks timing information for the game loop.
#[derive(Debug, Clone, PartialEq)]
pub struct GameTime {
    pub delta_seconds: f64,
    pub total_seconds: f64,
    pub frame_count: u64,
}

impl GameTime {
    /// Creates a new `GameTime` at time zero.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            delta_seconds: 0.0,
            total_seconds: 0.0,
            frame_count: 0,
        }
    }

    /// Advances the clock by the given delta (in seconds).
    pub fn tick(&mut self, delta: f64) {
        self.delta_seconds = delta;
        self.total_seconds += delta;
        self.frame_count += 1;
    }

    /// Returns the current frames-per-second estimate based on delta.
    #[must_use]
    pub fn fps(&self) -> f64 {
        if self.delta_seconds > 0.0 {
            1.0 / self.delta_seconds
        } else {
            0.0
        }
    }
}

impl Default for GameTime {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Scene
// ---------------------------------------------------------------------------

/// A named collection of entities that can be activated or deactivated.
#[derive(Debug, Clone)]
pub struct Scene {
    pub entities: Vec<EntityId>,
    pub active: bool,
    pub name: String,
}

impl Scene {
    /// Creates a new, active scene with the given name.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            entities: Vec::new(),
            active: true,
            name: name.to_string(),
        }
    }

    /// Adds an entity to this scene.
    ///
    /// # Errors
    ///
    /// Returns `GameEngineError::DuplicateEntity` if the entity is already in
    /// the scene.
    pub fn add_entity(&mut self, id: EntityId) -> Result<(), GameEngineError> {
        if self.entities.contains(&id) {
            return Err(GameEngineError::DuplicateEntity(id));
        }
        self.entities.push(id);
        Ok(())
    }

    /// Removes an entity from this scene.
    ///
    /// # Errors
    ///
    /// Returns `GameEngineError::EntityNotFound` if the entity is not in the
    /// scene.
    pub fn remove_entity(&mut self, id: EntityId) -> Result<(), GameEngineError> {
        if let Some(pos) = self.entities.iter().position(|&e| e == id) {
            self.entities.swap_remove(pos);
            Ok(())
        } else {
            Err(GameEngineError::EntityNotFound(id))
        }
    }

    /// Returns the number of entities in this scene.
    #[must_use]
    pub const fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Returns `true` if the scene contains the given entity.
    #[must_use]
    pub fn contains(&self, id: EntityId) -> bool {
        self.entities.contains(&id)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // --- EntityId ---

    #[test]
    fn entity_id_new() {
        let id = EntityId::new(1, 0);
        assert_eq!(id.index(), 1);
        assert_eq!(id.generation(), 0);
    }

    #[test]
    fn entity_id_to_u64_roundtrip() {
        let id = EntityId::new(42, 7);
        let packed = id.to_u64();
        let unpacked = EntityId::from_u64(packed);
        assert_eq!(id, unpacked);
    }

    #[test]
    fn entity_id_display() {
        let id = EntityId::new(5, 3);
        assert_eq!(format!("{id}"), "Entity(5v3)");
    }

    #[test]
    fn entity_id_equality() {
        let a = EntityId::new(1, 0);
        let b = EntityId::new(1, 0);
        let c = EntityId::new(1, 1);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn entity_id_hash() {
        let mut set = HashSet::new();
        let a = EntityId::new(1, 0);
        let b = EntityId::new(1, 0);
        set.insert(a);
        assert!(set.contains(&b));
    }

    #[test]
    fn entity_id_clone() {
        let a = EntityId::new(10, 20);
        let b = a;
        assert_eq!(a, b);
    }

    #[test]
    fn entity_id_from_u64_zero() {
        let id = EntityId::from_u64(0);
        assert_eq!(id.index(), 0);
        assert_eq!(id.generation(), 0);
    }

    #[test]
    fn entity_id_from_u64_max_index() {
        let id = EntityId::new(u32::MAX, 0);
        let rt = EntityId::from_u64(id.to_u64());
        assert_eq!(rt.index(), u32::MAX);
    }

    // --- EntityManager ---

    #[test]
    fn entity_manager_create() {
        let mut mgr = EntityManager::new();
        let e = mgr.create();
        assert_eq!(e.index(), 0);
        assert_eq!(e.generation(), 0);
        assert!(mgr.is_alive(e));
    }

    #[test]
    fn entity_manager_create_multiple() {
        let mut mgr = EntityManager::new();
        let a = mgr.create();
        let b = mgr.create();
        assert_ne!(a, b);
        assert_eq!(mgr.living_count(), 2);
    }

    #[test]
    fn entity_manager_destroy() {
        let mut mgr = EntityManager::new();
        let e = mgr.create();
        assert!(mgr.destroy(e));
        assert!(!mgr.is_alive(e));
    }

    #[test]
    fn entity_manager_destroy_already_dead() {
        let mut mgr = EntityManager::new();
        let e = mgr.create();
        mgr.destroy(e);
        assert!(!mgr.destroy(e));
    }

    #[test]
    fn entity_manager_recycle() {
        let mut mgr = EntityManager::new();
        let a = mgr.create();
        mgr.destroy(a);
        let b = mgr.create();
        assert_eq!(b.index(), a.index());
        assert_eq!(b.generation(), 1);
        assert!(!mgr.is_alive(a));
        assert!(mgr.is_alive(b));
    }

    #[test]
    fn entity_manager_living_count() {
        let mut mgr = EntityManager::new();
        assert_eq!(mgr.living_count(), 0);
        let a = mgr.create();
        let _b = mgr.create();
        assert_eq!(mgr.living_count(), 2);
        mgr.destroy(a);
        assert_eq!(mgr.living_count(), 1);
    }

    #[test]
    fn entity_manager_default() {
        let mgr = EntityManager::default();
        assert_eq!(mgr.living_count(), 0);
    }

    #[test]
    fn entity_manager_is_alive_invalid_index() {
        let mgr = EntityManager::new();
        let fake = EntityId::new(999, 0);
        assert!(!mgr.is_alive(fake));
    }

    #[test]
    fn entity_manager_destroy_invalid() {
        let mut mgr = EntityManager::new();
        let fake = EntityId::new(999, 0);
        assert!(!mgr.destroy(fake));
    }

    #[test]
    fn entity_manager_stale_generation() {
        let mut mgr = EntityManager::new();
        let a = mgr.create();
        mgr.destroy(a);
        let _b = mgr.create();
        assert!(!mgr.is_alive(a));
    }

    // --- ComponentStore ---

    #[test]
    fn component_store_insert_get() {
        let mut store = ComponentStore::<i32>::new();
        let id = EntityId::new(0, 0);
        store.insert(id, 42);
        assert_eq!(store.get(id), Some(&42));
    }

    #[test]
    fn component_store_get_mut() {
        let mut store = ComponentStore::<i32>::new();
        let id = EntityId::new(0, 0);
        store.insert(id, 10);
        if let Some(val) = store.get_mut(id) {
            *val = 20;
        }
        assert_eq!(store.get(id), Some(&20));
    }

    #[test]
    fn component_store_remove() {
        let mut store = ComponentStore::<i32>::new();
        let id = EntityId::new(0, 0);
        store.insert(id, 5);
        assert_eq!(store.remove(id), Some(5));
        assert!(store.get(id).is_none());
    }

    #[test]
    fn component_store_remove_wrong_generation() {
        let mut store = ComponentStore::<i32>::new();
        let id = EntityId::new(0, 0);
        store.insert(id, 5);
        let stale = EntityId::new(0, 1);
        assert_eq!(store.remove(stale), None);
    }

    #[test]
    fn component_store_get_wrong_generation() {
        let mut store = ComponentStore::<i32>::new();
        let id = EntityId::new(0, 0);
        store.insert(id, 5);
        let stale = EntityId::new(0, 1);
        assert!(store.get(stale).is_none());
    }

    #[test]
    fn component_store_len_empty() {
        let store = ComponentStore::<i32>::new();
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());
    }

    #[test]
    fn component_store_len_after_insert() {
        let mut store = ComponentStore::<i32>::new();
        store.insert(EntityId::new(0, 0), 1);
        store.insert(EntityId::new(1, 0), 2);
        assert_eq!(store.len(), 2);
        assert!(!store.is_empty());
    }

    #[test]
    fn component_store_contains() {
        let mut store = ComponentStore::<i32>::new();
        let id = EntityId::new(0, 0);
        assert!(!store.contains(id));
        store.insert(id, 1);
        assert!(store.contains(id));
    }

    #[test]
    fn component_store_default() {
        let store = ComponentStore::<f64>::default();
        assert!(store.is_empty());
    }

    #[test]
    fn component_store_overwrite() {
        let mut store = ComponentStore::<i32>::new();
        let id = EntityId::new(0, 0);
        store.insert(id, 1);
        store.insert(id, 2);
        assert_eq!(store.get(id), Some(&2));
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn component_store_get_mut_wrong_gen() {
        let mut store = ComponentStore::<i32>::new();
        let id = EntityId::new(0, 0);
        store.insert(id, 10);
        let stale = EntityId::new(0, 5);
        assert!(store.get_mut(stale).is_none());
    }

    #[test]
    fn component_store_contains_wrong_gen() {
        let mut store = ComponentStore::<i32>::new();
        let id = EntityId::new(0, 0);
        store.insert(id, 10);
        let stale = EntityId::new(0, 99);
        assert!(!store.contains(stale));
    }

    // --- Transform ---

    #[test]
    fn transform_new() {
        let t = Transform::new(1.0, 2.0);
        assert!((t.x - 1.0).abs() < f64::EPSILON);
        assert!((t.y - 2.0).abs() < f64::EPSILON);
        assert!((t.rotation).abs() < f64::EPSILON);
        assert!((t.scale_x - 1.0).abs() < f64::EPSILON);
        assert!((t.scale_y - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn transform_with_all() {
        let t = Transform::with_all(1.0, 2.0, 3.125, 0.5, 0.5);
        assert!((t.rotation - 3.125).abs() < f64::EPSILON);
        assert!((t.scale_x - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn transform_default() {
        let t = Transform::default();
        assert!((t.x).abs() < f64::EPSILON);
        assert!((t.y).abs() < f64::EPSILON);
    }

    #[test]
    fn transform_clone_eq() {
        let a = Transform::new(1.0, 2.0);
        let b = a.clone();
        assert_eq!(a, b);
    }

    // --- Velocity ---

    #[test]
    fn velocity_new() {
        let v = Velocity::new(3.0, 4.0);
        assert!((v.dx - 3.0).abs() < f64::EPSILON);
        assert!((v.dy - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn velocity_magnitude() {
        let v = Velocity::new(3.0, 4.0);
        assert!((v.magnitude() - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn velocity_magnitude_sq() {
        let v = Velocity::new(3.0, 4.0);
        assert!((v.magnitude_sq() - 25.0).abs() < f64::EPSILON);
    }

    #[test]
    fn velocity_zero() {
        let v = Velocity::default();
        assert!((v.magnitude()).abs() < f64::EPSILON);
    }

    #[test]
    fn velocity_clone_eq() {
        let a = Velocity::new(1.0, 2.0);
        let b = a.clone();
        assert_eq!(a, b);
    }

    // --- Sprite ---

    #[test]
    fn sprite_new() {
        let s = Sprite::new(32, 32, [255, 0, 0, 255]);
        assert_eq!(s.width, 32);
        assert_eq!(s.height, 32);
        assert!(s.visible);
        assert_eq!(s.color, [255, 0, 0, 255]);
    }

    #[test]
    fn sprite_default() {
        let s = Sprite::default();
        assert_eq!(s.width, 1);
        assert_eq!(s.height, 1);
        assert!(s.visible);
    }

    #[test]
    fn sprite_clone_eq() {
        let a = Sprite::new(16, 16, [0, 0, 0, 255]);
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn sprite_visibility_toggle() {
        let s = Sprite {
            visible: false,
            ..Sprite::default()
        };
        assert!(!s.visible);
    }

    // --- AABB ---

    #[test]
    fn aabb_new() {
        let a = AABB::new(0.0, 0.0, 10.0, 10.0);
        assert!((a.min_x).abs() < f64::EPSILON);
        assert!((a.max_x - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn aabb_intersects_true() {
        let a = AABB::new(0.0, 0.0, 10.0, 10.0);
        let b = AABB::new(5.0, 5.0, 15.0, 15.0);
        assert!(a.intersects(&b));
        assert!(b.intersects(&a));
    }

    #[test]
    fn aabb_intersects_false() {
        let a = AABB::new(0.0, 0.0, 10.0, 10.0);
        let b = AABB::new(20.0, 20.0, 30.0, 30.0);
        assert!(!a.intersects(&b));
    }

    #[test]
    fn aabb_intersects_edge() {
        let a = AABB::new(0.0, 0.0, 10.0, 10.0);
        let b = AABB::new(10.0, 0.0, 20.0, 10.0);
        assert!(a.intersects(&b));
    }

    #[test]
    fn aabb_contains_point_inside() {
        let a = AABB::new(0.0, 0.0, 10.0, 10.0);
        assert!(a.contains_point(5.0, 5.0));
    }

    #[test]
    fn aabb_contains_point_outside() {
        let a = AABB::new(0.0, 0.0, 10.0, 10.0);
        assert!(!a.contains_point(15.0, 5.0));
    }

    #[test]
    fn aabb_contains_point_edge() {
        let a = AABB::new(0.0, 0.0, 10.0, 10.0);
        assert!(a.contains_point(0.0, 0.0));
        assert!(a.contains_point(10.0, 10.0));
    }

    #[test]
    fn aabb_center() {
        let a = AABB::new(0.0, 0.0, 10.0, 10.0);
        let (cx, cy) = a.center();
        assert!((cx - 5.0).abs() < f64::EPSILON);
        assert!((cy - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn aabb_expand() {
        let a = AABB::new(0.0, 0.0, 10.0, 10.0);
        let b = a.expand(5.0);
        assert!((b.min_x - (-5.0)).abs() < f64::EPSILON);
        assert!((b.max_x - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn aabb_width_height() {
        let a = AABB::new(2.0, 3.0, 7.0, 8.0);
        assert!((a.width() - 5.0).abs() < f64::EPSILON);
        assert!((a.height() - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn aabb_area() {
        let a = AABB::new(0.0, 0.0, 4.0, 5.0);
        assert!((a.area() - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn aabb_default() {
        let a = AABB::default();
        assert!((a.min_x).abs() < f64::EPSILON);
        assert!((a.max_y - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn aabb_clone_eq() {
        let a = AABB::new(1.0, 2.0, 3.0, 4.0);
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn aabb_contains_point_negative() {
        let a = AABB::new(-10.0, -10.0, 10.0, 10.0);
        assert!(a.contains_point(0.0, 0.0));
        assert!(a.contains_point(-10.0, -10.0));
        assert!(!a.contains_point(-11.0, 0.0));
    }

    // --- Collider ---

    #[test]
    fn collider_new() {
        let c = Collider::new(AABB::new(0.0, 0.0, 1.0, 1.0), 1);
        assert_eq!(c.layer, 1);
    }

    #[test]
    fn collider_default() {
        let c = Collider::default();
        assert_eq!(c.layer, 0);
    }

    #[test]
    fn collider_clone_eq() {
        let a = Collider::new(AABB::new(0.0, 0.0, 5.0, 5.0), 2);
        let b = a.clone();
        assert_eq!(a, b);
    }

    // --- World ---

    #[test]
    fn world_spawn() {
        let mut world = World::new();
        let e = world.spawn();
        assert!(world.is_alive(e));
    }

    #[test]
    fn world_despawn() {
        let mut world = World::new();
        let e = world.spawn();
        world.transform_store.insert(e, Transform::new(1.0, 2.0));
        world.velocity_store.insert(e, Velocity::new(0.0, 0.0));
        assert!(world.despawn(e));
        assert!(!world.is_alive(e));
        assert!(world.transform_store.get(e).is_none());
        assert!(world.velocity_store.get(e).is_none());
    }

    #[test]
    fn world_despawn_nonexistent() {
        let mut world = World::new();
        let fake = EntityId::new(999, 0);
        assert!(!world.despawn(fake));
    }

    #[test]
    fn world_default() {
        let world = World::default();
        assert_eq!(world.entity_manager.living_count(), 0);
    }

    #[test]
    fn world_spawn_multiple() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        let c = world.spawn();
        assert!(world.is_alive(a));
        assert!(world.is_alive(b));
        assert!(world.is_alive(c));
        assert_eq!(world.entity_manager.living_count(), 3);
    }

    #[test]
    fn world_despawn_cleans_all_components() {
        let mut world = World::new();
        let e = world.spawn();
        world.transform_store.insert(e, Transform::default());
        world.velocity_store.insert(e, Velocity::default());
        world.sprite_store.insert(e, Sprite::default());
        world.collider_store.insert(e, Collider::default());
        world.despawn(e);
        assert!(!world.transform_store.contains(e));
        assert!(!world.velocity_store.contains(e));
        assert!(!world.sprite_store.contains(e));
        assert!(!world.collider_store.contains(e));
    }

    // --- PhysicsSystem ---

    #[test]
    fn physics_update_positions() {
        let mut world = World::new();
        let e = world.spawn();
        world.transform_store.insert(e, Transform::new(0.0, 0.0));
        world.velocity_store.insert(e, Velocity::new(10.0, 5.0));
        let mut time = GameTime::new();
        time.tick(1.0);
        PhysicsSystem::update(&mut world, &time);
        let t = world.transform_store.get(e).unwrap();
        assert!((t.x - 10.0).abs() < f64::EPSILON);
        assert!((t.y - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn physics_update_fractional_dt() {
        let mut world = World::new();
        let e = world.spawn();
        world.transform_store.insert(e, Transform::new(0.0, 0.0));
        world.velocity_store.insert(e, Velocity::new(60.0, 0.0));
        let mut time = GameTime::new();
        time.tick(1.0 / 60.0);
        PhysicsSystem::update(&mut world, &time);
        let t = world.transform_store.get(e).unwrap();
        assert!((t.x - 1.0).abs() < 1e-10);
    }

    #[test]
    fn physics_no_velocity_no_move() {
        let mut world = World::new();
        let e = world.spawn();
        world.transform_store.insert(e, Transform::new(5.0, 5.0));
        let mut time = GameTime::new();
        time.tick(1.0);
        PhysicsSystem::update(&mut world, &time);
        let t = world.transform_store.get(e).unwrap();
        assert!((t.x - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn physics_detect_collisions_overlap() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        world.transform_store.insert(a, Transform::new(0.0, 0.0));
        world.transform_store.insert(b, Transform::new(0.5, 0.5));
        world
            .collider_store
            .insert(a, Collider::new(AABB::new(0.0, 0.0, 1.0, 1.0), 0));
        world
            .collider_store
            .insert(b, Collider::new(AABB::new(0.0, 0.0, 1.0, 1.0), 0));
        let pairs = PhysicsSystem::detect_collisions(&world);
        assert_eq!(pairs.len(), 1);
    }

    #[test]
    fn physics_detect_collisions_no_overlap() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        world.transform_store.insert(a, Transform::new(0.0, 0.0));
        world
            .transform_store
            .insert(b, Transform::new(100.0, 100.0));
        world
            .collider_store
            .insert(a, Collider::new(AABB::new(0.0, 0.0, 1.0, 1.0), 0));
        world
            .collider_store
            .insert(b, Collider::new(AABB::new(0.0, 0.0, 1.0, 1.0), 0));
        let pairs = PhysicsSystem::detect_collisions(&world);
        assert!(pairs.is_empty());
    }

    #[test]
    fn physics_detect_collisions_no_transform() {
        let mut world = World::new();
        let a = world.spawn();
        world
            .collider_store
            .insert(a, Collider::new(AABB::new(0.0, 0.0, 1.0, 1.0), 0));
        let pairs = PhysicsSystem::detect_collisions(&world);
        assert!(pairs.is_empty());
    }

    #[test]
    fn physics_multiple_entities_update() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();
        world.transform_store.insert(a, Transform::new(0.0, 0.0));
        world.transform_store.insert(b, Transform::new(10.0, 10.0));
        world.velocity_store.insert(a, Velocity::new(1.0, 0.0));
        world.velocity_store.insert(b, Velocity::new(0.0, -1.0));
        let mut time = GameTime::new();
        time.tick(2.0);
        PhysicsSystem::update(&mut world, &time);
        let ta = world.transform_store.get(a).unwrap();
        let tb = world.transform_store.get(b).unwrap();
        assert!((ta.x - 2.0).abs() < f64::EPSILON);
        assert!((tb.y - 8.0).abs() < f64::EPSILON);
    }

    // --- Input ---

    #[test]
    fn input_key_down_up() {
        let mut input = Input::new();
        input.key_down("Space");
        assert!(input.is_pressed("Space"));
        input.key_up("Space");
        assert!(!input.is_pressed("Space"));
    }

    #[test]
    fn input_multiple_keys() {
        let mut input = Input::new();
        input.key_down("W");
        input.key_down("A");
        assert_eq!(input.pressed_count(), 2);
    }

    #[test]
    fn input_clear() {
        let mut input = Input::new();
        input.key_down("W");
        input.key_down("S");
        input.clear();
        assert_eq!(input.pressed_count(), 0);
    }

    #[test]
    fn input_default() {
        let input = Input::default();
        assert_eq!(input.pressed_count(), 0);
    }

    #[test]
    fn input_not_pressed() {
        let input = Input::new();
        assert!(!input.is_pressed("Escape"));
    }

    #[test]
    fn input_duplicate_key_down() {
        let mut input = Input::new();
        input.key_down("W");
        input.key_down("W");
        assert_eq!(input.pressed_count(), 1);
    }

    #[test]
    fn input_key_up_not_pressed() {
        let mut input = Input::new();
        input.key_up("W");
        assert_eq!(input.pressed_count(), 0);
    }

    // --- GameTime ---

    #[test]
    fn game_time_new() {
        let t = GameTime::new();
        assert!((t.delta_seconds).abs() < f64::EPSILON);
        assert!((t.total_seconds).abs() < f64::EPSILON);
        assert_eq!(t.frame_count, 0);
    }

    #[test]
    fn game_time_tick() {
        let mut t = GameTime::new();
        t.tick(1.0 / 60.0);
        assert!((t.delta_seconds - 1.0 / 60.0).abs() < f64::EPSILON);
        assert_eq!(t.frame_count, 1);
    }

    #[test]
    fn game_time_multiple_ticks() {
        let mut t = GameTime::new();
        t.tick(0.5);
        t.tick(0.5);
        assert!((t.total_seconds - 1.0).abs() < f64::EPSILON);
        assert_eq!(t.frame_count, 2);
    }

    #[test]
    fn game_time_fps() {
        let mut t = GameTime::new();
        t.tick(1.0 / 60.0);
        assert!((t.fps() - 60.0).abs() < 1e-10);
    }

    #[test]
    fn game_time_fps_zero_delta() {
        let t = GameTime::new();
        assert!((t.fps()).abs() < f64::EPSILON);
    }

    #[test]
    fn game_time_default() {
        let t = GameTime::default();
        assert_eq!(t.frame_count, 0);
    }

    #[test]
    fn game_time_clone_eq() {
        let mut a = GameTime::new();
        a.tick(0.016);
        let b = a.clone();
        assert_eq!(a, b);
    }

    // --- Scene ---

    #[test]
    fn scene_new() {
        let s = Scene::new("level_1");
        assert_eq!(s.name, "level_1");
        assert!(s.active);
        assert_eq!(s.entity_count(), 0);
    }

    #[test]
    fn scene_add_entity() {
        let mut s = Scene::new("test");
        let id = EntityId::new(0, 0);
        assert!(s.add_entity(id).is_ok());
        assert_eq!(s.entity_count(), 1);
        assert!(s.contains(id));
    }

    #[test]
    fn scene_add_duplicate_entity() {
        let mut s = Scene::new("test");
        let id = EntityId::new(0, 0);
        s.add_entity(id).unwrap();
        let result = s.add_entity(id);
        assert_eq!(result, Err(GameEngineError::DuplicateEntity(id)));
    }

    #[test]
    fn scene_remove_entity() {
        let mut s = Scene::new("test");
        let id = EntityId::new(0, 0);
        s.add_entity(id).unwrap();
        assert!(s.remove_entity(id).is_ok());
        assert_eq!(s.entity_count(), 0);
    }

    #[test]
    fn scene_remove_nonexistent_entity() {
        let mut s = Scene::new("test");
        let id = EntityId::new(0, 0);
        let result = s.remove_entity(id);
        assert_eq!(result, Err(GameEngineError::EntityNotFound(id)));
    }

    #[test]
    fn scene_active_toggle() {
        let mut s = Scene::new("test");
        assert!(s.active);
        s.active = false;
        assert!(!s.active);
    }

    #[test]
    fn scene_multiple_entities() {
        let mut s = Scene::new("test");
        for i in 0..10 {
            s.add_entity(EntityId::new(i, 0)).unwrap();
        }
        assert_eq!(s.entity_count(), 10);
    }

    #[test]
    fn scene_contains_false() {
        let s = Scene::new("test");
        assert!(!s.contains(EntityId::new(0, 0)));
    }

    // --- GameEngineError ---

    #[test]
    fn error_display_entity_not_found() {
        let e = GameEngineError::EntityNotFound(EntityId::new(1, 0));
        assert_eq!(format!("{e}"), "entity not found: Entity(1v0)");
    }

    #[test]
    fn error_display_component_not_found() {
        let e = GameEngineError::ComponentNotFound(EntityId::new(2, 1));
        assert_eq!(
            format!("{e}"),
            "component not found for entity: Entity(2v1)"
        );
    }

    #[test]
    fn error_display_scene_not_found() {
        let e = GameEngineError::SceneNotFound("missing".to_string());
        assert_eq!(format!("{e}"), "scene not found: missing");
    }

    #[test]
    fn error_display_duplicate_entity() {
        let e = GameEngineError::DuplicateEntity(EntityId::new(0, 0));
        assert_eq!(format!("{e}"), "duplicate entity: Entity(0v0)");
    }

    #[test]
    fn error_display_generic() {
        let e = GameEngineError::Generic("oops".to_string());
        assert_eq!(format!("{e}"), "oops");
    }

    #[test]
    fn error_clone_eq() {
        let a = GameEngineError::Generic("test".to_string());
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn error_debug() {
        let e = GameEngineError::Generic("debug".to_string());
        let dbg = format!("{e:?}");
        assert!(dbg.contains("Generic"));
    }

    // --- Integration ---

    #[test]
    fn integration_spawn_move_collide() {
        let mut world = World::new();
        let a = world.spawn();
        let b = world.spawn();

        world.transform_store.insert(a, Transform::new(0.0, 0.0));
        world.velocity_store.insert(a, Velocity::new(5.0, 0.0));
        world
            .collider_store
            .insert(a, Collider::new(AABB::new(-0.5, -0.5, 0.5, 0.5), 0));

        world.transform_store.insert(b, Transform::new(6.0, 0.0));
        world
            .collider_store
            .insert(b, Collider::new(AABB::new(-0.5, -0.5, 0.5, 0.5), 0));

        let mut time = GameTime::new();
        time.tick(1.0);
        PhysicsSystem::update(&mut world, &time);

        let pairs = PhysicsSystem::detect_collisions(&world);
        assert!(!pairs.is_empty());
    }

    #[test]
    fn integration_full_game_loop() {
        let mut world = World::new();
        let mut time = GameTime::new();
        let mut input = Input::new();
        let mut scene = Scene::new("main");

        let player = world.spawn();
        world
            .transform_store
            .insert(player, Transform::new(0.0, 0.0));
        world.velocity_store.insert(player, Velocity::new(0.0, 0.0));
        world
            .sprite_store
            .insert(player, Sprite::new(32, 32, [0, 255, 0, 255]));
        world
            .collider_store
            .insert(player, Collider::new(AABB::new(0.0, 0.0, 32.0, 32.0), 1));
        scene.add_entity(player).unwrap();

        input.key_down("Right");

        // Simulate 60 frames
        for _ in 0..60 {
            time.tick(1.0 / 60.0);

            if input.is_pressed("Right") {
                if let Some(vel) = world.velocity_store.get_mut(player) {
                    vel.dx = 100.0;
                }
            }

            PhysicsSystem::update(&mut world, &time);
        }

        let tfm = world.transform_store.get(player).unwrap();
        assert!((tfm.x - 100.0).abs() < 1e-6);
        assert_eq!(time.frame_count, 60);
        assert!(scene.contains(player));
    }

    #[test]
    fn integration_entity_recycling_with_components() {
        let mut world = World::new();
        let a = world.spawn();
        world.transform_store.insert(a, Transform::new(1.0, 1.0));
        world.despawn(a);

        let b = world.spawn();
        assert_eq!(b.index(), a.index());
        assert_ne!(b.generation(), a.generation());
        assert!(world.transform_store.get(a).is_none());
        assert!(world.transform_store.get(b).is_none());
    }

    #[test]
    fn integration_scene_with_despawn() {
        let mut world = World::new();
        let mut scene = Scene::new("test");

        let e1 = world.spawn();
        let e2 = world.spawn();
        scene.add_entity(e1).unwrap();
        scene.add_entity(e2).unwrap();

        world.despawn(e1);
        scene.remove_entity(e1).unwrap();

        assert!(!world.is_alive(e1));
        assert!(world.is_alive(e2));
        assert_eq!(scene.entity_count(), 1);
    }

    #[test]
    fn collision_pair_eq() {
        let a = CollisionPair {
            entity_a: EntityId::new(0, 0),
            entity_b: EntityId::new(1, 0),
        };
        let b = a.clone();
        assert_eq!(a, b);
    }
}
