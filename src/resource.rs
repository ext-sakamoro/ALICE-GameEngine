//! Asynchronous resource management: load, cache, and reference-count
//! meshes, textures, sounds, and SDF definitions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// ResourceId
// ---------------------------------------------------------------------------

/// Opaque handle to a loaded resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceId(pub u32);

impl std::fmt::Display for ResourceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Res({})", self.0)
    }
}

// ---------------------------------------------------------------------------
// ResourceState
// ---------------------------------------------------------------------------

/// Loading state of a resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceState {
    /// Queued for loading.
    Pending,
    /// Successfully loaded.
    Ready,
    /// Failed to load.
    Failed(String),
}

// ---------------------------------------------------------------------------
// ResourceKind
// ---------------------------------------------------------------------------

/// What kind of data the resource holds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceKind {
    Mesh,
    Texture,
    Sound,
    Sdf,
    Material,
    Animation,
    Shader,
}

// ---------------------------------------------------------------------------
// ResourceEntry
// ---------------------------------------------------------------------------

/// Metadata about a single resource.
#[derive(Debug, Clone)]
pub struct ResourceEntry {
    pub id: ResourceId,
    pub path: String,
    pub kind: ResourceKind,
    pub state: ResourceState,
    pub data: Option<Arc<[u8]>>,
    pub ref_count: u32,
}

// ---------------------------------------------------------------------------
// ResourceManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of all engine resources.
pub struct ResourceManager {
    entries: HashMap<ResourceId, ResourceEntry>,
    path_index: HashMap<String, ResourceId>,
    next_id: u32,
}

impl ResourceManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            path_index: HashMap::new(),
            next_id: 0,
        }
    }

    /// Requests a resource. If already loaded, increments ref count.
    pub fn request(&mut self, path: &str, kind: ResourceKind) -> ResourceId {
        if let Some(&id) = self.path_index.get(path) {
            if let Some(entry) = self.entries.get_mut(&id) {
                entry.ref_count += 1;
            }
            return id;
        }

        let id = ResourceId(self.next_id);
        self.next_id += 1;

        let entry = ResourceEntry {
            id,
            path: path.to_string(),
            kind,
            state: ResourceState::Pending,
            data: None,
            ref_count: 1,
        };
        self.entries.insert(id, entry);
        self.path_index.insert(path.to_string(), id);
        id
    }

    /// Marks a resource as loaded with the given data.
    pub fn set_loaded(&mut self, id: ResourceId, data: Vec<u8>) -> bool {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.state = ResourceState::Ready;
            entry.data = Some(Arc::from(data.into_boxed_slice()));
            true
        } else {
            false
        }
    }

    /// Marks a resource as failed.
    pub fn set_failed(&mut self, id: ResourceId, reason: &str) -> bool {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.state = ResourceState::Failed(reason.to_string());
            true
        } else {
            false
        }
    }

    /// Releases a reference. When ref count hits 0, the resource is evicted.
    pub fn release(&mut self, id: ResourceId) -> bool {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.ref_count = entry.ref_count.saturating_sub(1);
            if entry.ref_count == 0 {
                let path = entry.path.clone();
                self.entries.remove(&id);
                self.path_index.remove(&path);
                return true;
            }
        }
        false
    }

    /// Returns the state of a resource.
    #[must_use]
    pub fn state(&self, id: ResourceId) -> Option<&ResourceState> {
        self.entries.get(&id).map(|e| &e.state)
    }

    /// Returns the data of a loaded resource.
    #[must_use]
    pub fn data(&self, id: ResourceId) -> Option<&Arc<[u8]>> {
        self.entries.get(&id).and_then(|e| e.data.as_ref())
    }

    /// Returns the total number of managed resources.
    #[must_use]
    pub fn count(&self) -> usize {
        self.entries.len()
    }

    /// Returns all resource ids of a given kind.
    #[must_use]
    pub fn by_kind(&self, kind: &ResourceKind) -> Vec<ResourceId> {
        self.entries
            .values()
            .filter(|e| e.kind == *kind)
            .map(|e| e.id)
            .collect()
    }

    /// Looks up a resource by path.
    #[must_use]
    pub fn find_by_path(&self, path: &str) -> Option<ResourceId> {
        self.path_index.get(path).copied()
    }

    /// Returns pending resources that need loading.
    #[must_use]
    pub fn pending(&self) -> Vec<ResourceId> {
        self.entries
            .values()
            .filter(|e| e.state == ResourceState::Pending)
            .map(|e| e.id)
            .collect()
    }
}

impl Default for ResourceManager {
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
    fn request_creates_pending() {
        let mut rm = ResourceManager::new();
        let id = rm.request("models/cube.obj", ResourceKind::Mesh);
        assert_eq!(rm.state(id), Some(&ResourceState::Pending));
        assert_eq!(rm.count(), 1);
    }

    #[test]
    fn request_same_path_increments_ref() {
        let mut rm = ResourceManager::new();
        let id1 = rm.request("tex.png", ResourceKind::Texture);
        let id2 = rm.request("tex.png", ResourceKind::Texture);
        assert_eq!(id1, id2);
        assert_eq!(rm.count(), 1);
    }

    #[test]
    fn set_loaded() {
        let mut rm = ResourceManager::new();
        let id = rm.request("sound.wav", ResourceKind::Sound);
        rm.set_loaded(id, vec![1, 2, 3]);
        assert_eq!(rm.state(id), Some(&ResourceState::Ready));
        assert_eq!(rm.data(id).map(|d| d.len()), Some(3));
    }

    #[test]
    fn set_failed() {
        let mut rm = ResourceManager::new();
        let id = rm.request("missing.obj", ResourceKind::Mesh);
        rm.set_failed(id, "file not found");
        assert!(matches!(rm.state(id), Some(ResourceState::Failed(_))));
    }

    #[test]
    fn release_decrements_ref() {
        let mut rm = ResourceManager::new();
        let id = rm.request("x.obj", ResourceKind::Mesh);
        let _ = rm.request("x.obj", ResourceKind::Mesh);
        let evicted = rm.release(id);
        assert!(!evicted);
        assert_eq!(rm.count(), 1);
        let evicted = rm.release(id);
        assert!(evicted);
        assert_eq!(rm.count(), 0);
    }

    #[test]
    fn by_kind_filters() {
        let mut rm = ResourceManager::new();
        rm.request("a.obj", ResourceKind::Mesh);
        rm.request("b.obj", ResourceKind::Mesh);
        rm.request("c.png", ResourceKind::Texture);
        assert_eq!(rm.by_kind(&ResourceKind::Mesh).len(), 2);
        assert_eq!(rm.by_kind(&ResourceKind::Texture).len(), 1);
        assert_eq!(rm.by_kind(&ResourceKind::Sound).len(), 0);
    }

    #[test]
    fn find_by_path() {
        let mut rm = ResourceManager::new();
        let id = rm.request("model.glb", ResourceKind::Mesh);
        assert_eq!(rm.find_by_path("model.glb"), Some(id));
        assert_eq!(rm.find_by_path("nope.glb"), None);
    }

    #[test]
    fn pending_list() {
        let mut rm = ResourceManager::new();
        let id1 = rm.request("a.obj", ResourceKind::Mesh);
        let id2 = rm.request("b.obj", ResourceKind::Mesh);
        rm.set_loaded(id1, vec![]);
        let pending = rm.pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0], id2);
    }

    #[test]
    fn resource_id_display() {
        assert_eq!(format!("{}", ResourceId(7)), "Res(7)");
    }

    #[test]
    fn data_returns_none_when_pending() {
        let mut rm = ResourceManager::new();
        let id = rm.request("x.obj", ResourceKind::Mesh);
        assert!(rm.data(id).is_none());
    }

    #[test]
    fn release_nonexistent() {
        let mut rm = ResourceManager::new();
        let evicted = rm.release(ResourceId(999));
        assert!(!evicted);
    }

    #[test]
    fn default_is_empty() {
        let rm = ResourceManager::default();
        assert_eq!(rm.count(), 0);
    }
}
