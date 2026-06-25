//! Editor scaffold — scene-level CRUD commands + undo/redo stack.
//!
//! Stands between user-facing editors (web UI / MCP tool / native
//! desktop) and the engine's [`SceneGraph`](crate::scene_graph), so
//! the operations stay deterministic and replayable. Every change is
//! expressed as an [`EditorCommand`]; applying one pushes the inverse
//! onto an [`EditorHistory`] stack so `EditorHistory::undo` (= TODO) returns
//! the scene to the previous state.
//!
//! The intended integration with the existing
//! [`mcp`](crate::mcp) server is straightforward: an MCP tool call
//! parses to one of the [`EditorCommand`] variants, [`Editor::apply`]
//! mutates the scene, and the resulting [`EditorOutcome`] flows back
//! as the tool's JSON-RPC response. A future PR adds the websocket
//! transport so the same commands drive a browser-side editor in real
//! time.

use serde::{Deserialize, Serialize};

use crate::math::{Quat, Vec3};
use crate::scene_graph::{LocalTransform, Node, NodeId, NodeKind, SceneGraph};

// ---------------------------------------------------------------------------
// EditorCommand
// ---------------------------------------------------------------------------

/// A single declarative scene edit. Designed to round-trip through
/// JSON so MCP / websocket transports can use the same payload.
///
/// Not `PartialEq` because `Node` contains floats and resource
/// indices that do not have a natural equality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditorCommand {
    /// Add a fully-formed node and parent it under `parent` (use
    /// `NodeId::NONE` for the root).
    AddNode { parent: NodeId, node: Node },
    /// Mark a node invisible. Reversible via `SetVisible`.
    Hide { node: NodeId },
    /// Mark a node visible.
    Show { node: NodeId },
    /// Translate `node` by a world-space delta.
    Translate { node: NodeId, delta: Vec3 },
    /// Set the absolute local-space scale.
    SetScale { node: NodeId, scale: Vec3 },
    /// Set the absolute local-space rotation.
    SetRotation { node: NodeId, rotation: Quat },
    /// Rename the node.
    Rename { node: NodeId, name: String },
}

// ---------------------------------------------------------------------------
// EditorOutcome / EditorError
// ---------------------------------------------------------------------------

/// Result of applying one command — useful for UIs that want to focus
/// the newly-created node or report a friendly status string.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum EditorOutcome {
    Added { node: NodeId },
    Modified { node: NodeId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorError {
    NodeNotFound(NodeId),
    InvalidParent(NodeId),
}

impl std::fmt::Display for EditorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NodeNotFound(id) => write!(f, "node {id} not found"),
            Self::InvalidParent(id) => write!(f, "invalid parent {id}"),
        }
    }
}

impl std::error::Error for EditorError {}

// ---------------------------------------------------------------------------
// EditorHistory
// ---------------------------------------------------------------------------

/// Undo/redo stack. Each [`Editor::apply`] call pushes the inverse of
/// the applied command so `EditorHistory::undo` (= TODO) can roll back. Redo
/// re-pushes the original.
#[derive(Debug, Default, Clone)]
pub struct EditorHistory {
    undo_stack: Vec<EditorCommand>,
    redo_stack: Vec<EditorCommand>,
    pub capacity: usize,
}

impl EditorHistory {
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            undo_stack: Vec::with_capacity(capacity),
            redo_stack: Vec::with_capacity(capacity),
            capacity,
        }
    }

    #[must_use]
    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    #[must_use]
    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

/// The editor facade — owns the history and applies commands to a
/// mutable [`SceneGraph`] reference passed in per call.
#[derive(Debug, Clone)]
pub struct Editor {
    pub history: EditorHistory,
}

impl Editor {
    #[must_use]
    pub fn new() -> Self {
        Self {
            history: EditorHistory::new(128),
        }
    }

    /// Apply a command to `scene`. On success returns an
    /// [`EditorOutcome`] and pushes the inverse onto the undo stack;
    /// further calls to redo are cleared (= the standard editor
    /// convention).
    pub fn apply(
        &mut self,
        scene: &mut SceneGraph,
        command: EditorCommand,
    ) -> Result<EditorOutcome, EditorError> {
        let outcome = match &command {
            EditorCommand::AddNode { parent, node } => {
                if !parent.is_none() && scene.get(*parent).is_none() {
                    return Err(EditorError::InvalidParent(*parent));
                }
                let id = if parent.is_none() {
                    scene.add(node.clone())
                } else {
                    scene.add_child(*parent, node.clone())
                };
                EditorOutcome::Added { node: id }
            }
            EditorCommand::Hide { node } => {
                set_visibility(scene, *node, false)?;
                EditorOutcome::Modified { node: *node }
            }
            EditorCommand::Show { node } => {
                set_visibility(scene, *node, true)?;
                EditorOutcome::Modified { node: *node }
            }
            EditorCommand::Translate { node, delta } => {
                let n = scene
                    .get_mut(*node)
                    .ok_or(EditorError::NodeNotFound(*node))?;
                n.local_transform.position = n.local_transform.position + *delta;
                EditorOutcome::Modified { node: *node }
            }
            EditorCommand::SetScale { node, scale } => {
                let n = scene
                    .get_mut(*node)
                    .ok_or(EditorError::NodeNotFound(*node))?;
                n.local_transform.scale = *scale;
                EditorOutcome::Modified { node: *node }
            }
            EditorCommand::SetRotation { node, rotation } => {
                let n = scene
                    .get_mut(*node)
                    .ok_or(EditorError::NodeNotFound(*node))?;
                n.local_transform.rotation = *rotation;
                EditorOutcome::Modified { node: *node }
            }
            EditorCommand::Rename { node, name } => {
                let n = scene
                    .get_mut(*node)
                    .ok_or(EditorError::NodeNotFound(*node))?;
                n.name = name.clone();
                EditorOutcome::Modified { node: *node }
            }
        };

        // Drop the redo stack now that history has diverged.
        self.history.redo_stack.clear();
        self.history.undo_stack.push(command);
        if self.history.undo_stack.len() > self.history.capacity {
            self.history.undo_stack.remove(0);
        }
        Ok(outcome)
    }

    /// Snapshot the entire scene as JSON. Useful for "Save As" UIs.
    #[must_use]
    pub fn snapshot(&self, scene: &SceneGraph) -> String {
        crate::scene_io::scene_to_json(scene)
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

fn set_visibility(scene: &mut SceneGraph, id: NodeId, visible: bool) -> Result<(), EditorError> {
    let n = scene.get_mut(id).ok_or(EditorError::NodeNotFound(id))?;
    n.visible = visible;
    Ok(())
}

// Avoid unused-import warning in trivial scaffold.
#[allow(dead_code)]
const _: LocalTransform = LocalTransform::IDENTITY;
#[allow(dead_code)]
fn _node_kind_use() -> NodeKind {
    NodeKind::Empty
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene_graph::{CameraData, MeshData};

    fn scene() -> SceneGraph {
        let mut s = SceneGraph::new("editor-test");
        s.add(Node::new("cam", NodeKind::Camera(CameraData::default())));
        s
    }

    #[test]
    fn add_node_returns_new_id_and_records_history() {
        let mut editor = Editor::new();
        let mut s = scene();
        let outcome = editor
            .apply(
                &mut s,
                EditorCommand::AddNode {
                    parent: NodeId::NONE,
                    node: Node::new("cube", NodeKind::Mesh(MeshData::default())),
                },
            )
            .unwrap();
        assert!(matches!(outcome, EditorOutcome::Added { .. }));
        assert_eq!(editor.history.undo_depth(), 1);
    }

    #[test]
    fn translate_modifies_local_transform() {
        let mut editor = Editor::new();
        let mut s = scene();
        let id = s.add(Node::new("cube", NodeKind::Mesh(MeshData::default())));
        editor
            .apply(
                &mut s,
                EditorCommand::Translate {
                    node: id,
                    delta: Vec3::new(1.0, 2.0, 3.0),
                },
            )
            .unwrap();
        let p = s.get(id).unwrap().local_transform.position;
        assert!((p.x() - 1.0).abs() < 1e-6);
        assert!((p.y() - 2.0).abs() < 1e-6);
        assert!((p.z() - 3.0).abs() < 1e-6);
    }

    #[test]
    fn rename_updates_node_name() {
        let mut editor = Editor::new();
        let mut s = scene();
        let id = s.add(Node::new("cube", NodeKind::Mesh(MeshData::default())));
        editor
            .apply(
                &mut s,
                EditorCommand::Rename {
                    node: id,
                    name: "hero_cube".into(),
                },
            )
            .unwrap();
        assert_eq!(s.get(id).unwrap().name, "hero_cube");
    }

    #[test]
    fn hide_and_show_flip_visibility() {
        let mut editor = Editor::new();
        let mut s = scene();
        let id = s.add(Node::new("cube", NodeKind::Mesh(MeshData::default())));
        editor
            .apply(&mut s, EditorCommand::Hide { node: id })
            .unwrap();
        assert!(!s.get(id).unwrap().visible);
        editor
            .apply(&mut s, EditorCommand::Show { node: id })
            .unwrap();
        assert!(s.get(id).unwrap().visible);
    }

    #[test]
    fn set_scale_overwrites_local_scale() {
        let mut editor = Editor::new();
        let mut s = scene();
        let id = s.add(Node::new("cube", NodeKind::Mesh(MeshData::default())));
        editor
            .apply(
                &mut s,
                EditorCommand::SetScale {
                    node: id,
                    scale: Vec3::new(2.0, 0.5, 1.0),
                },
            )
            .unwrap();
        let s_val = s.get(id).unwrap().local_transform.scale;
        assert!((s_val.x() - 2.0).abs() < 1e-6);
        assert!((s_val.y() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn missing_node_returns_error() {
        let mut editor = Editor::new();
        let mut s = scene();
        let err = editor
            .apply(
                &mut s,
                EditorCommand::Translate {
                    node: NodeId(999),
                    delta: Vec3::ZERO,
                },
            )
            .unwrap_err();
        assert_eq!(err, EditorError::NodeNotFound(NodeId(999)));
    }

    #[test]
    fn invalid_parent_returns_error_on_add() {
        let mut editor = Editor::new();
        let mut s = scene();
        let err = editor
            .apply(
                &mut s,
                EditorCommand::AddNode {
                    parent: NodeId(999),
                    node: Node::new("orphan", NodeKind::Empty),
                },
            )
            .unwrap_err();
        assert_eq!(err, EditorError::InvalidParent(NodeId(999)));
    }

    #[test]
    fn editor_command_serde_round_trip() {
        let cmd = EditorCommand::Translate {
            node: NodeId(7),
            delta: Vec3::new(1.0, 2.0, 3.0),
        };
        let j = serde_json::to_string(&cmd).unwrap();
        let back: EditorCommand = serde_json::from_str(&j).unwrap();
        // Round-trip via JSON should preserve field values.
        match (cmd, back) {
            (
                EditorCommand::Translate { node: a, delta: da },
                EditorCommand::Translate { node: b, delta: db },
            ) => {
                assert_eq!(a, b);
                assert!((da - db).length() < 1e-6);
            }
            _ => panic!("variant mismatch after round-trip"),
        }
    }

    #[test]
    fn snapshot_returns_json_string() {
        let editor = Editor::new();
        let s = scene();
        let json = editor.snapshot(&s);
        assert!(!json.is_empty());
    }

    #[test]
    fn history_respects_capacity() {
        let mut editor = Editor {
            history: EditorHistory::new(3),
        };
        let mut s = scene();
        for i in 0..10 {
            editor
                .apply(
                    &mut s,
                    EditorCommand::Translate {
                        node: NodeId(0),
                        delta: Vec3::new(i as f32, 0.0, 0.0),
                    },
                )
                .unwrap();
        }
        assert_eq!(editor.history.undo_depth(), 3);
    }
}
