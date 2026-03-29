//! Scene serialization: save/load entire scene graphs to JSON.
//!
//! ```rust
//! use alice_game_engine::scene_io::*;
//! use alice_game_engine::scene_graph::*;
//!
//! let mut scene = SceneGraph::new("level");
//! scene.add(Node::new("cam", NodeKind::Camera(CameraData::default())));
//! let json = scene_to_json(&scene);
//! let loaded = scene_from_json(&json).unwrap();
//! assert_eq!(loaded.node_count(), 1);
//! ```

use crate::scene_graph::{LocalTransform, Node, NodeId, NodeKind, SceneGraph};
use serde::{Deserialize, Serialize};

/// Serializable scene snapshot.
#[derive(Serialize, Deserialize)]
struct SceneSnapshot {
    name: String,
    nodes: Vec<NodeSnapshot>,
}

#[derive(Serialize, Deserialize)]
struct NodeSnapshot {
    name: String,
    kind: NodeKind,
    local_transform: LocalTransform,
    parent_idx: Option<usize>,
    visible: bool,
}

/// Serializes a scene graph to JSON.
#[must_use]
pub fn scene_to_json(scene: &SceneGraph) -> String {
    let mut nodes = Vec::new();
    for i in 0..10000_u32 {
        let id = NodeId(i);
        if let Some(node) = scene.get(id) {
            let parent_idx = if node.parent.is_none() {
                None
            } else {
                Some(node.parent.0 as usize)
            };
            nodes.push(NodeSnapshot {
                name: node.name.clone(),
                kind: node.kind.clone(),
                local_transform: node.local_transform,
                parent_idx,
                visible: node.visible,
            });
        }
    }
    let snapshot = SceneSnapshot {
        name: scene.name().to_string(),
        nodes,
    };
    serde_json::to_string_pretty(&snapshot).unwrap_or_default()
}

/// Serializes a scene graph to compact JSON (no whitespace).
#[must_use]
pub fn scene_to_json_compact(scene: &SceneGraph) -> String {
    let mut nodes = Vec::new();
    for i in 0..10000_u32 {
        let id = NodeId(i);
        if let Some(node) = scene.get(id) {
            let parent_idx = if node.parent.is_none() {
                None
            } else {
                Some(node.parent.0 as usize)
            };
            nodes.push(NodeSnapshot {
                name: node.name.clone(),
                kind: node.kind.clone(),
                local_transform: node.local_transform,
                parent_idx,
                visible: node.visible,
            });
        }
    }
    let snapshot = SceneSnapshot {
        name: scene.name().to_string(),
        nodes,
    };
    serde_json::to_string(&snapshot).unwrap_or_default()
}

/// Deserializes a scene graph from JSON.
///
/// # Errors
///
/// Returns an error string if the JSON is malformed.
pub fn scene_from_json(json: &str) -> Result<SceneGraph, String> {
    let snapshot: SceneSnapshot =
        serde_json::from_str(json).map_err(|e| format!("Parse error: {e}"))?;

    let mut scene = SceneGraph::new(&snapshot.name);

    // First pass: add all nodes
    let mut ids = Vec::new();
    for ns in &snapshot.nodes {
        let mut node = Node::new(&ns.name, ns.kind.clone());
        node.local_transform = ns.local_transform;
        node.visible = ns.visible;
        ids.push(scene.add(node));
    }

    // Second pass: set parents
    for (i, ns) in snapshot.nodes.iter().enumerate() {
        if let Some(pi) = ns.parent_idx {
            if pi < ids.len() {
                scene.reparent(ids[i], ids[pi]);
            }
        }
    }

    Ok(scene)
}

/// Saves a scene to a file.
///
/// # Errors
///
/// Returns error on write failure.
pub fn save_scene(scene: &SceneGraph, path: &str) -> Result<(), String> {
    let json = scene_to_json(scene);
    std::fs::write(path, json).map_err(|e| format!("Write error: {e}"))
}

/// Loads a scene from a file.
///
/// # Errors
///
/// Returns error on read/parse failure.
pub fn load_scene(path: &str) -> Result<SceneGraph, String> {
    let json = std::fs::read_to_string(path).map_err(|e| format!("Read error: {e}"))?;
    scene_from_json(&json)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Vec3;
    use crate::scene_graph::*;

    #[test]
    fn roundtrip_empty() {
        let scene = SceneGraph::new("empty");
        let json = scene_to_json(&scene);
        let loaded = scene_from_json(&json).unwrap();
        assert_eq!(loaded.node_count(), 0);
        assert_eq!(loaded.name(), "empty");
    }

    #[test]
    fn roundtrip_with_nodes() {
        let mut scene = SceneGraph::new("level1");
        scene.add(Node::new("cam", NodeKind::Camera(CameraData::default())));
        let mut cube = Node::new("cube", NodeKind::Mesh(MeshData::default()));
        cube.local_transform.position = Vec3::new(1.0, 2.0, 3.0);
        scene.add(cube);
        scene.add(Node::new("light", NodeKind::Light(LightData::default())));

        let json = scene_to_json(&scene);
        let loaded = scene_from_json(&json).unwrap();
        assert_eq!(loaded.node_count(), 3);

        let cube_node = loaded.find_by_name("cube").and_then(|id| loaded.get(id));
        assert!(cube_node.is_some());
        assert!((cube_node.unwrap().local_transform.position.x() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn roundtrip_sdf() {
        let mut scene = SceneGraph::new("sdf_test");
        scene.add(Node::new(
            "sphere",
            NodeKind::Sdf(SdfData {
                sdf_json: r#"{"Primitive":{"Sphere":{"radius":2.0}}}"#.to_string(),
                half_extents: Vec3::ONE,
                generate_collider: true,
            }),
        ));
        let json = scene_to_json(&scene);
        let loaded = scene_from_json(&json).unwrap();
        assert_eq!(loaded.sdf_volumes().len(), 1);
    }

    #[test]
    fn compact_json() {
        let mut scene = SceneGraph::new("compact");
        scene.add(Node::new("a", NodeKind::Empty));
        let compact = scene_to_json_compact(&scene);
        assert!(!compact.contains('\n'));
    }

    #[test]
    fn invalid_json() {
        assert!(scene_from_json("not json").is_err());
    }

    #[test]
    fn save_load_file() {
        let mut scene = SceneGraph::new("file_test");
        scene.add(Node::new("node", NodeKind::Empty));
        let path = "/tmp/alice_scene_test.json";
        save_scene(&scene, path).unwrap();
        let loaded = load_scene(path).unwrap();
        assert_eq!(loaded.node_count(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn parent_hierarchy() {
        let mut scene = SceneGraph::new("hierarchy");
        let root = scene.add(Node::new("root", NodeKind::Empty));
        scene.add_child(root, Node::new("child", NodeKind::Empty));

        let json = scene_to_json(&scene);
        let loaded = scene_from_json(&json).unwrap();
        assert_eq!(loaded.node_count(), 2);
    }
}
