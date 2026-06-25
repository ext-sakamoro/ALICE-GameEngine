//! Scene-graph inspector (Bevy `bevy_inspector_egui` style).
//!
//! Walks a [`SceneGraph`] and produces a flat list of
//! [`InspectorRow`]s suitable for rendering in the [`crate::imgui`]
//! `UiContext` or any other UI layer. The inspector is read-only by
//! itself; mutation flows through `Editor::apply` so undo/redo stays
//! consistent.

use crate::scene_graph::{NodeId, NodeKind, SceneGraph};

#[derive(Debug, Clone, PartialEq)]
pub struct InspectorRow {
    pub node_id: NodeId,
    /// Indentation depth (= ancestors above the row).
    pub depth: u32,
    pub name: String,
    pub kind_label: String,
    pub visible: bool,
}

#[derive(Debug, Default, Clone)]
pub struct Inspector;

impl Inspector {
    /// Walk every node in the scene in tree order. Roots first,
    /// then recursive children.
    #[must_use]
    pub fn rows(&self, scene: &SceneGraph) -> Vec<InspectorRow> {
        let mut out = Vec::new();
        let roots = collect_roots(scene);
        for r in roots {
            walk(scene, r, 0, &mut out);
        }
        out
    }

    /// Single-node detail view: returns a `(field, value)` list for
    /// the node's `NodeKind` payload + transform. Useful for the
    /// right-side property panel in an inspector UI.
    #[must_use]
    pub fn detail(&self, scene: &SceneGraph, id: NodeId) -> Vec<(String, String)> {
        let Some(node) = scene.get(id) else {
            return Vec::new();
        };
        let t = node.local_transform;
        let mut out = vec![
            ("name".into(), node.name.clone()),
            (
                "position".into(),
                format!(
                    "({:.3}, {:.3}, {:.3})",
                    t.position.x(),
                    t.position.y(),
                    t.position.z()
                ),
            ),
            (
                "scale".into(),
                format!(
                    "({:.3}, {:.3}, {:.3})",
                    t.scale.x(),
                    t.scale.y(),
                    t.scale.z()
                ),
            ),
            ("visible".into(), node.visible.to_string()),
            ("kind".into(), kind_label(&node.kind).to_string()),
        ];
        // Kind-specific extras.
        match &node.kind {
            NodeKind::Mesh(m) => {
                out.push(("mesh_id".into(), m.mesh_id.to_string()));
                out.push(("material_id".into(), m.material_id.to_string()));
            }
            NodeKind::Light(l) => {
                out.push((
                    "color".into(),
                    format!("({:.2}, {:.2}, {:.2})", l.color.r, l.color.g, l.color.b),
                ));
                out.push(("intensity".into(), format!("{:.3}", l.intensity)));
            }
            NodeKind::Sdf(s) => {
                out.push((
                    "half_extents".into(),
                    format!(
                        "({:.2}, {:.2}, {:.2})",
                        s.half_extents.x(),
                        s.half_extents.y(),
                        s.half_extents.z()
                    ),
                ));
            }
            _ => {}
        }
        out
    }
}

fn collect_roots(scene: &SceneGraph) -> Vec<NodeId> {
    let mut roots = Vec::new();
    for i in 0..scene.node_count() {
        #[allow(clippy::cast_possible_truncation)]
        let id = NodeId(i as u32);
        if let Some(node) = scene.get(id) {
            if node.parent.is_none() {
                roots.push(id);
            }
        }
    }
    roots
}

fn walk(scene: &SceneGraph, id: NodeId, depth: u32, out: &mut Vec<InspectorRow>) {
    let Some(node) = scene.get(id) else { return };
    out.push(InspectorRow {
        node_id: id,
        depth,
        name: node.name.clone(),
        kind_label: kind_label(&node.kind).to_string(),
        visible: node.visible,
    });
    for child in node.children.clone() {
        walk(scene, child, depth + 1, out);
    }
}

fn kind_label(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::Empty => "Empty",
        NodeKind::Mesh(_) => "Mesh",
        NodeKind::Sdf(_) => "Sdf",
        NodeKind::Camera(_) => "Camera",
        NodeKind::Light(_) => "Light",
        NodeKind::AudioEmitter(_) => "AudioEmitter",
        NodeKind::ParticleEmitter(_) => "ParticleEmitter",
        NodeKind::Decal(_) => "Decal",
        NodeKind::EnvProbe(_) => "EnvProbe",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_graph::{CameraData, MeshData, Node};

    fn scene_with_three_nodes() -> SceneGraph {
        let mut s = SceneGraph::new("inspector-test");
        s.add(Node::new("cam", NodeKind::Camera(CameraData::default())));
        let parent = s.add(Node::new("parent", NodeKind::Empty));
        s.add_child(
            parent,
            Node::new("child", NodeKind::Mesh(MeshData::default())),
        );
        s
    }

    #[test]
    fn rows_returns_one_per_node() {
        let s = scene_with_three_nodes();
        let inspector = Inspector;
        let rows = inspector.rows(&s);
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn child_row_has_depth_one() {
        let s = scene_with_three_nodes();
        let inspector = Inspector;
        let rows = inspector.rows(&s);
        let child = rows.iter().find(|r| r.name == "child").unwrap();
        assert_eq!(child.depth, 1);
    }

    #[test]
    fn detail_for_camera_lists_kind() {
        let s = scene_with_three_nodes();
        let inspector = Inspector;
        let cam_id = NodeId(0);
        let detail = inspector.detail(&s, cam_id);
        assert!(detail.iter().any(|(k, v)| k == "kind" && v == "Camera"));
    }

    #[test]
    fn detail_for_missing_node_returns_empty() {
        let s = scene_with_three_nodes();
        let inspector = Inspector;
        assert!(inspector.detail(&s, NodeId(999)).is_empty());
    }

    #[test]
    fn rows_includes_kind_label() {
        let s = scene_with_three_nodes();
        let inspector = Inspector;
        let rows = inspector.rows(&s);
        let kinds: std::collections::HashSet<&str> =
            rows.iter().map(|r| r.kind_label.as_str()).collect();
        assert!(kinds.contains("Camera"));
        assert!(kinds.contains("Empty"));
        assert!(kinds.contains("Mesh"));
    }
}
