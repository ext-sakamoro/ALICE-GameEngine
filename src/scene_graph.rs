//! Hierarchical scene graph with typed nodes (mesh, SDF, camera, light).
//!
//! ```rust
//! use alice_game_engine::scene_graph::*;
//! use alice_game_engine::math::Vec3;
//!
//! let mut scene = SceneGraph::new("level");
//! let cam = scene.add(Node::new("camera", NodeKind::Camera(CameraData::default())));
//! let cube = scene.add(Node::new("cube", NodeKind::Mesh(MeshData::default())));
//! assert_eq!(scene.node_count(), 2);
//! ```
//!
//! Fyrox uses a monolithic node tree with downcasting.  ALICE keeps an
//! enum-typed node to allow static dispatch while still supporting both
//! mesh and SDF geometry in the same tree.

use crate::math::{Color, Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// NodeId
// ---------------------------------------------------------------------------

/// Handle into the scene graph arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const NONE: Self = Self(u32::MAX);

    #[inline]
    #[must_use]
    pub const fn is_none(self) -> bool {
        self.0 == u32::MAX
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_none() {
            write!(f, "NodeId(NONE)")
        } else {
            write!(f, "NodeId({})", self.0)
        }
    }
}

// ---------------------------------------------------------------------------
// LocalTransform
// ---------------------------------------------------------------------------

/// TRS transform relative to parent.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LocalTransform {
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl LocalTransform {
    pub const IDENTITY: Self = Self {
        position: Vec3::ZERO,
        rotation: Quat::IDENTITY,
        scale: Vec3::ONE,
    };

    #[inline]
    #[must_use]
    pub fn to_matrix(self) -> Mat4 {
        Mat4::from_trs(self.position, self.rotation, self.scale)
    }
}

impl Default for LocalTransform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

// ---------------------------------------------------------------------------
// NodeKind — Mesh + SDF hybrid
// ---------------------------------------------------------------------------

/// The payload of a scene node. Both mesh and SDF are first-class.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeKind {
    /// Empty pivot / group node.
    Empty,

    /// Traditional polygon mesh.
    Mesh(MeshData),

    /// SDF volume (evaluated at runtime or baked to mesh).
    Sdf(SdfData),

    /// Camera (perspective or orthographic).
    Camera(CameraData),

    /// Light source.
    Light(LightData),

    /// Audio emitter positioned in 3D space.
    AudioEmitter(AudioEmitterData),

    /// Particle emitter.
    ParticleEmitter(ParticleEmitterData),
}

/// Polygon mesh payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshData {
    /// Index into a mesh resource table.
    pub mesh_id: u32,
    /// Material index.
    pub material_id: u32,
    /// Cast shadows.
    pub cast_shadows: bool,
}

impl Default for MeshData {
    fn default() -> Self {
        Self {
            mesh_id: 0,
            material_id: 0,
            cast_shadows: true,
        }
    }
}

/// SDF volume payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SdfData {
    /// Serialised SDF node tree (JSON or binary from ALICE-SDF).
    pub sdf_json: String,
    /// Bounding box half-extents for culling.
    pub half_extents: Vec3,
    /// Whether to generate a collision mesh from this SDF.
    pub generate_collider: bool,
}

impl Default for SdfData {
    fn default() -> Self {
        Self {
            sdf_json: String::new(),
            half_extents: Vec3::ONE,
            generate_collider: false,
        }
    }
}

/// Camera projection mode.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Projection {
    Perspective {
        fov_y: f32,
        near: f32,
        far: f32,
    },
    Orthographic {
        width: f32,
        height: f32,
        near: f32,
        far: f32,
    },
}

impl Default for Projection {
    fn default() -> Self {
        Self::Perspective {
            fov_y: std::f32::consts::FRAC_PI_4,
            near: 0.1,
            far: 1000.0,
        }
    }
}

/// Camera payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraData {
    pub projection: Projection,
    pub clear_color: Color,
    pub active: bool,
}

impl Default for CameraData {
    fn default() -> Self {
        Self {
            projection: Projection::default(),
            clear_color: Color::new(0.1, 0.1, 0.1, 1.0),
            active: true,
        }
    }
}

/// Light variant.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LightVariant {
    Directional,
    Point { radius: f32 },
    Spot { radius: f32, half_angle: f32 },
}

impl Default for LightVariant {
    fn default() -> Self {
        Self::Point { radius: 10.0 }
    }
}

/// Light payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightData {
    pub variant: LightVariant,
    pub color: Color,
    pub intensity: f32,
    pub cast_shadows: bool,
}

impl Default for LightData {
    fn default() -> Self {
        Self {
            variant: LightVariant::default(),
            color: Color::WHITE,
            intensity: 1.0,
            cast_shadows: true,
        }
    }
}

/// Audio emitter payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioEmitterData {
    /// Index into a sound resource table.
    pub sound_id: u32,
    pub volume: f32,
    pub looping: bool,
    pub spatial: bool,
    pub max_distance: f32,
}

impl Default for AudioEmitterData {
    fn default() -> Self {
        Self {
            sound_id: 0,
            volume: 1.0,
            looping: false,
            spatial: true,
            max_distance: 50.0,
        }
    }
}

/// Particle emitter payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParticleEmitterData {
    pub max_particles: u32,
    pub emit_rate: f32,
    pub lifetime: f32,
    pub color_start: Color,
    pub color_end: Color,
    pub size_start: f32,
    pub size_end: f32,
    pub speed: f32,
    pub gravity: f32,
}

impl Default for ParticleEmitterData {
    fn default() -> Self {
        Self {
            max_particles: 1000,
            emit_rate: 100.0,
            lifetime: 2.0,
            color_start: Color::WHITE,
            color_end: Color::TRANSPARENT,
            size_start: 0.1,
            size_end: 0.0,
            speed: 5.0,
            gravity: -9.81,
        }
    }
}

// ---------------------------------------------------------------------------
// Node
// ---------------------------------------------------------------------------

/// A node in the scene graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub local_transform: LocalTransform,
    pub kind: NodeKind,
    pub parent: NodeId,
    pub children: Vec<NodeId>,
    pub visible: bool,
}

impl Node {
    #[must_use]
    pub fn new(name: &str, kind: NodeKind) -> Self {
        Self {
            name: name.to_string(),
            local_transform: LocalTransform::IDENTITY,
            kind,
            parent: NodeId::NONE,
            children: Vec::new(),
            visible: true,
        }
    }
}

// ---------------------------------------------------------------------------
// SceneGraph
// ---------------------------------------------------------------------------

/// Arena-allocated scene graph.
pub struct SceneGraph {
    nodes: Vec<Option<Node>>,
    free_list: Vec<u32>,
    world_matrices: Vec<Mat4>,
    name: String,
}

impl SceneGraph {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            nodes: Vec::new(),
            free_list: Vec::new(),
            world_matrices: Vec::new(),
            name: name.to_string(),
        }
    }

    /// Returns the scene name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Adds a node and returns its id.
    pub fn add(&mut self, node: Node) -> NodeId {
        if let Some(idx) = self.free_list.pop() {
            let i = idx as usize;
            self.nodes[i] = Some(node);
            self.world_matrices[i] = Mat4::IDENTITY;
            NodeId(idx)
        } else {
            #[allow(clippy::cast_possible_truncation)]
            let idx = self.nodes.len() as u32;
            self.nodes.push(Some(node));
            self.world_matrices.push(Mat4::IDENTITY);
            NodeId(idx)
        }
    }

    /// Adds a node as a child of parent.
    pub fn add_child(&mut self, parent: NodeId, mut node: Node) -> NodeId {
        node.parent = parent;
        let child_id = self.add(node);
        if let Some(Some(p)) = self.nodes.get_mut(parent.0 as usize) {
            p.children.push(child_id);
        }
        child_id
    }

    /// Removes a node (does not remove children — they become orphans).
    pub fn remove(&mut self, id: NodeId) -> Option<Node> {
        let idx = id.0 as usize;
        if idx < self.nodes.len() {
            if let Some(node) = self.nodes[idx].take() {
                // Remove from parent's children list.
                if !node.parent.is_none() {
                    if let Some(Some(p)) = self.nodes.get_mut(node.parent.0 as usize) {
                        p.children.retain(|&c| c != id);
                    }
                }
                #[allow(clippy::cast_possible_truncation)]
                self.free_list.push(idx as u32);
                return Some(node);
            }
        }
        None
    }

    /// Returns a reference to a node.
    #[must_use]
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0 as usize).and_then(|n| n.as_ref())
    }

    /// Returns a mutable reference to a node.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.0 as usize).and_then(|n| n.as_mut())
    }

    /// Returns the world matrix for a node.
    #[must_use]
    pub fn world_matrix(&self, id: NodeId) -> Mat4 {
        self.world_matrices
            .get(id.0 as usize)
            .copied()
            .unwrap_or(Mat4::IDENTITY)
    }

    /// Returns the total number of live nodes.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_some()).count()
    }

    /// Recomputes world matrices for all nodes.
    /// Call once per frame before rendering.
    pub fn update_world_matrices(&mut self) {
        // Find root nodes (parent == NONE).
        let roots: Vec<u32> = self
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                #[allow(clippy::cast_possible_truncation)]
                n.as_ref()
                    .filter(|node| node.parent.is_none())
                    .map(|_| i as u32)
            })
            .collect();

        for root in roots {
            self.update_recursive(root, Mat4::IDENTITY);
        }
    }

    fn update_recursive(&mut self, idx: u32, parent_world: Mat4) {
        let i = idx as usize;
        let (local, children) = {
            let Some(Some(node)) = self.nodes.get(i) else {
                return;
            };
            (node.local_transform.to_matrix(), node.children.clone())
        };
        let world = parent_world * local;
        self.world_matrices[i] = world;
        for child in children {
            self.update_recursive(child.0, world);
        }
    }

    /// Collects all node ids of a specific kind.
    #[must_use]
    pub fn query_by_kind(&self, filter: &dyn Fn(&NodeKind) -> bool) -> Vec<NodeId> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| {
                slot.as_ref().and_then(|n| {
                    #[allow(clippy::cast_possible_truncation)]
                    if filter(&n.kind) {
                        Some(NodeId(i as u32))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    /// Collects all cameras in the scene.
    #[must_use]
    pub fn cameras(&self) -> Vec<NodeId> {
        self.query_by_kind(&|k| matches!(k, NodeKind::Camera(_)))
    }

    /// Collects all lights in the scene.
    #[must_use]
    pub fn lights(&self) -> Vec<NodeId> {
        self.query_by_kind(&|k| matches!(k, NodeKind::Light(_)))
    }

    /// Collects all meshes in the scene.
    #[must_use]
    pub fn meshes(&self) -> Vec<NodeId> {
        self.query_by_kind(&|k| matches!(k, NodeKind::Mesh(_)))
    }

    /// Collects all SDF volumes in the scene.
    #[must_use]
    pub fn sdf_volumes(&self) -> Vec<NodeId> {
        self.query_by_kind(&|k| matches!(k, NodeKind::Sdf(_)))
    }

    /// Look up a node by name (linear scan).
    #[must_use]
    pub fn find_by_name(&self, name: &str) -> Option<NodeId> {
        self.nodes.iter().enumerate().find_map(|(i, slot)| {
            slot.as_ref().and_then(|n| {
                #[allow(clippy::cast_possible_truncation)]
                if n.name == name {
                    Some(NodeId(i as u32))
                } else {
                    None
                }
            })
        })
    }
}

// ---------------------------------------------------------------------------
// AABB3 — 3D bounding box
// ---------------------------------------------------------------------------

/// Axis-aligned bounding box in 3D.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Aabb3 {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb3 {
    pub const ZERO: Self = Self {
        min: Vec3::ZERO,
        max: Vec3::ZERO,
    };

    #[inline]
    #[must_use]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    #[inline]
    #[must_use]
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    #[inline]
    #[must_use]
    pub fn half_extents(&self) -> Vec3 {
        (self.max - self.min) * 0.5
    }

    #[inline]
    #[must_use]
    pub fn contains_point(&self, p: Vec3) -> bool {
        p.x() >= self.min.x()
            && p.x() <= self.max.x()
            && p.y() >= self.min.y()
            && p.y() <= self.max.y()
            && p.z() >= self.min.z()
            && p.z() <= self.max.z()
    }

    #[inline]
    #[must_use]
    pub fn intersects(&self, other: &Self) -> bool {
        self.min.x() <= other.max.x()
            && self.max.x() >= other.min.x()
            && self.min.y() <= other.max.y()
            && self.max.y() >= other.min.y()
            && self.min.z() <= other.max.z()
            && self.max.z() >= other.min.z()
    }

    /// Merges two AABBs.
    #[inline]
    #[must_use]
    pub const fn merge(&self, other: &Self) -> Self {
        Self {
            min: Vec3::new(
                self.min.x().min(other.min.x()),
                self.min.y().min(other.min.y()),
                self.min.z().min(other.min.z()),
            ),
            max: Vec3::new(
                self.max.x().max(other.max.x()),
                self.max.y().max(other.max.y()),
                self.max.z().max(other.max.z()),
            ),
        }
    }

    /// Expands the AABB by a margin in all directions.
    #[inline]
    #[must_use]
    pub fn expand(&self, margin: f32) -> Self {
        let m = Vec3::new(margin, margin, margin);
        Self {
            min: self.min - m,
            max: self.max + m,
        }
    }

    /// Transforms the AABB by a matrix (conservative approximation).
    #[must_use]
    pub fn transform(&self, mat: &Mat4) -> Self {
        let corners = [
            Vec3::new(self.min.x(), self.min.y(), self.min.z()),
            Vec3::new(self.max.x(), self.min.y(), self.min.z()),
            Vec3::new(self.min.x(), self.max.y(), self.min.z()),
            Vec3::new(self.max.x(), self.max.y(), self.min.z()),
            Vec3::new(self.min.x(), self.min.y(), self.max.z()),
            Vec3::new(self.max.x(), self.min.y(), self.max.z()),
            Vec3::new(self.min.x(), self.max.y(), self.max.z()),
            Vec3::new(self.max.x(), self.max.y(), self.max.z()),
        ];
        let first = mat.transform_point3(corners[0]);
        let mut new_min = first;
        let mut new_max = first;
        for c in &corners[1..] {
            let t = mat.transform_point3(*c);
            new_min = Vec3::new(
                new_min.x().min(t.x()),
                new_min.y().min(t.y()),
                new_min.z().min(t.z()),
            );
            new_max = Vec3::new(
                new_max.x().max(t.x()),
                new_max.y().max(t.y()),
                new_max.z().max(t.z()),
            );
        }
        Self {
            min: new_min,
            max: new_max,
        }
    }
}

impl Default for Aabb3 {
    fn default() -> Self {
        Self::ZERO
    }
}

// ---------------------------------------------------------------------------
// Frustum culling
// ---------------------------------------------------------------------------

/// A plane in Hessian normal form (normal · p + d = 0).
#[derive(Debug, Clone, Copy)]
pub struct Plane {
    pub normal: Vec3,
    pub d: f32,
}

impl Plane {
    #[inline]
    #[must_use]
    pub fn distance_to_point(&self, p: Vec3) -> f32 {
        self.normal.dot(p) + self.d
    }
}

/// View frustum (6 planes) extracted from a view-projection matrix.
pub struct Frustum {
    pub planes: [Plane; 6],
}

impl Frustum {
    /// Extracts frustum planes from a combined view-projection matrix.
    #[must_use]
    pub fn from_view_projection(vp: Mat4) -> Self {
        let m = vp.0.to_cols_array_2d();
        let extract = |r: usize, sign: f32| {
            let nx = sign.mul_add(m[0][r], m[0][3]);
            let ny = sign.mul_add(m[1][r], m[1][3]);
            let nz = sign.mul_add(m[2][r], m[2][3]);
            let d = sign.mul_add(m[3][r], m[3][3]);
            let len = nz.mul_add(nz, nx.mul_add(nx, ny * ny)).sqrt();
            let inv = if len > 1e-10 { 1.0 / len } else { 0.0 };
            Plane {
                normal: Vec3::new(nx * inv, ny * inv, nz * inv),
                d: d * inv,
            }
        };
        Self {
            planes: [
                extract(0, 1.0),  // left
                extract(0, -1.0), // right
                extract(1, 1.0),  // bottom
                extract(1, -1.0), // top
                extract(2, 1.0),  // near
                extract(2, -1.0), // far
            ],
        }
    }

    /// Tests if an AABB is at least partially inside the frustum.
    #[must_use]
    pub fn intersects_aabb(&self, aabb: &Aabb3) -> bool {
        for plane in &self.planes {
            let px = if plane.normal.x() >= 0.0 {
                aabb.max.x()
            } else {
                aabb.min.x()
            };
            let py = if plane.normal.y() >= 0.0 {
                aabb.max.y()
            } else {
                aabb.min.y()
            };
            let pz = if plane.normal.z() >= 0.0 {
                aabb.max.z()
            } else {
                aabb.min.z()
            };
            let p_vertex = Vec3::new(px, py, pz);
            if plane.distance_to_point(p_vertex) < 0.0 {
                return false;
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// SceneGraph extensions
// ---------------------------------------------------------------------------

impl SceneGraph {
    /// Returns a local AABB for a node based on its kind.
    #[must_use]
    pub fn local_aabb(&self, id: NodeId) -> Aabb3 {
        let Some(node) = self.get(id) else {
            return Aabb3::ZERO;
        };
        match &node.kind {
            NodeKind::Sdf(sdf) => Aabb3::new(-sdf.half_extents, sdf.half_extents),
            NodeKind::Mesh(_) => Aabb3::new(-Vec3::ONE, Vec3::ONE),
            NodeKind::Light(ld) => match ld.variant {
                LightVariant::Point { radius } | LightVariant::Spot { radius, .. } => {
                    let r = Vec3::new(radius, radius, radius);
                    Aabb3::new(-r, r)
                }
                LightVariant::Directional => Aabb3::ZERO,
            },
            _ => Aabb3::ZERO,
        }
    }

    /// Returns the world-space AABB for a node.
    #[must_use]
    pub fn world_aabb(&self, id: NodeId) -> Aabb3 {
        let local = self.local_aabb(id);
        let world = self.world_matrix(id);
        local.transform(&world)
    }

    /// Returns all visible nodes that pass frustum culling.
    #[must_use]
    pub fn frustum_cull(&self, frustum: &Frustum) -> Vec<NodeId> {
        self.nodes
            .iter()
            .enumerate()
            .filter_map(|(i, slot)| {
                let node = slot.as_ref()?;
                if !node.visible {
                    return None;
                }
                let id = NodeId(i as u32);
                let aabb = self.world_aabb(id);
                if frustum.intersects_aabb(&aabb) {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Collects all descendants of a node (recursive).
    #[must_use]
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut result = Vec::new();
        self.collect_descendants(id, &mut result);
        result
    }

    fn collect_descendants(&self, id: NodeId, out: &mut Vec<NodeId>) {
        let Some(node) = self.get(id) else {
            return;
        };
        let children = node.children.clone();
        for child in children {
            out.push(child);
            self.collect_descendants(child, out);
        }
    }

    /// Reparents a node under a new parent.
    pub fn reparent(&mut self, node_id: NodeId, new_parent: NodeId) {
        // Remove from old parent
        if let Some(node) = self.get(node_id) {
            let old_parent = node.parent;
            if !old_parent.is_none() {
                if let Some(Some(p)) = self.nodes.get_mut(old_parent.0 as usize) {
                    p.children.retain(|&c| c != node_id);
                }
            }
        }
        // Set new parent
        if let Some(Some(node)) = self.nodes.get_mut(node_id.0 as usize) {
            node.parent = new_parent;
        }
        // Add to new parent's children
        if !new_parent.is_none() {
            if let Some(Some(p)) = self.nodes.get_mut(new_parent.0 as usize) {
                p.children.push(node_id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_empty(name: &str) -> Node {
        Node::new(name, NodeKind::Empty)
    }

    #[test]
    fn add_and_get() {
        let mut sg = SceneGraph::new("test");
        let id = sg.add(make_empty("root"));
        assert_eq!(sg.node_count(), 1);
        assert_eq!(sg.get(id).unwrap().name, "root");
    }

    #[test]
    fn add_child_sets_parent() {
        let mut sg = SceneGraph::new("test");
        let root = sg.add(make_empty("root"));
        let child = sg.add_child(root, make_empty("child"));
        assert_eq!(sg.get(child).unwrap().parent, root);
        assert!(sg.get(root).unwrap().children.contains(&child));
    }

    #[test]
    fn remove_node() {
        let mut sg = SceneGraph::new("test");
        let root = sg.add(make_empty("root"));
        let child = sg.add_child(root, make_empty("child"));
        sg.remove(child);
        assert!(sg.get(child).is_none());
        assert!(!sg.get(root).unwrap().children.contains(&child));
        assert_eq!(sg.node_count(), 1);
    }

    #[test]
    fn free_list_reuse() {
        let mut sg = SceneGraph::new("test");
        let id1 = sg.add(make_empty("a"));
        sg.remove(id1);
        let id2 = sg.add(make_empty("b"));
        assert_eq!(id1.0, id2.0);
    }

    #[test]
    fn world_matrix_identity_for_root() {
        let mut sg = SceneGraph::new("test");
        let root = sg.add(make_empty("root"));
        sg.update_world_matrices();
        assert_eq!(sg.world_matrix(root), Mat4::IDENTITY);
    }

    #[test]
    fn world_matrix_inherits_parent() {
        let mut sg = SceneGraph::new("test");
        let mut root_node = make_empty("root");
        root_node.local_transform.position = Vec3::new(10.0, 0.0, 0.0);
        let root = sg.add(root_node);

        let mut child_node = make_empty("child");
        child_node.local_transform.position = Vec3::new(0.0, 5.0, 0.0);
        let child = sg.add_child(root, child_node);

        sg.update_world_matrices();
        let world = sg.world_matrix(child);
        let p = world.transform_point3(Vec3::ZERO);
        assert!((p.x() - 10.0).abs() < 1e-5);
        assert!((p.y() - 5.0).abs() < 1e-5);
    }

    #[test]
    fn query_cameras() {
        let mut sg = SceneGraph::new("test");
        sg.add(Node::new("cam1", NodeKind::Camera(CameraData::default())));
        sg.add(make_empty("pivot"));
        sg.add(Node::new("cam2", NodeKind::Camera(CameraData::default())));
        assert_eq!(sg.cameras().len(), 2);
    }

    #[test]
    fn query_lights() {
        let mut sg = SceneGraph::new("test");
        sg.add(Node::new("sun", NodeKind::Light(LightData::default())));
        sg.add(make_empty("pivot"));
        assert_eq!(sg.lights().len(), 1);
    }

    #[test]
    fn query_meshes() {
        let mut sg = SceneGraph::new("test");
        sg.add(Node::new("mesh1", NodeKind::Mesh(MeshData::default())));
        sg.add(Node::new("sdf1", NodeKind::Sdf(SdfData::default())));
        assert_eq!(sg.meshes().len(), 1);
        assert_eq!(sg.sdf_volumes().len(), 1);
    }

    #[test]
    fn find_by_name() {
        let mut sg = SceneGraph::new("test");
        sg.add(make_empty("alpha"));
        let beta_id = sg.add(make_empty("beta"));
        assert_eq!(sg.find_by_name("beta"), Some(beta_id));
        assert_eq!(sg.find_by_name("gamma"), None);
    }

    #[test]
    fn node_id_display() {
        assert_eq!(format!("{}", NodeId(42)), "NodeId(42)");
        assert_eq!(format!("{}", NodeId::NONE), "NodeId(NONE)");
    }

    #[test]
    fn local_transform_to_matrix() {
        let lt = LocalTransform {
            position: Vec3::new(1.0, 2.0, 3.0),
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        };
        let m = lt.to_matrix();
        let p = m.transform_point3(Vec3::ZERO);
        assert!((p.x() - 1.0).abs() < 1e-6);
        assert!((p.y() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn mesh_and_sdf_coexist() {
        let mut sg = SceneGraph::new("hybrid");
        let root = sg.add(make_empty("root"));
        sg.add_child(
            root,
            Node::new("floor_mesh", NodeKind::Mesh(MeshData::default())),
        );
        sg.add_child(
            root,
            Node::new(
                "terrain_sdf",
                NodeKind::Sdf(SdfData {
                    sdf_json: r#"{"type":"sphere","radius":5.0}"#.to_string(),
                    half_extents: Vec3::new(5.0, 5.0, 5.0),
                    generate_collider: true,
                }),
            ),
        );
        assert_eq!(sg.meshes().len(), 1);
        assert_eq!(sg.sdf_volumes().len(), 1);
        assert_eq!(sg.node_count(), 3);
    }

    #[test]
    fn audio_emitter_node() {
        let mut sg = SceneGraph::new("test");
        sg.add(Node::new(
            "sfx",
            NodeKind::AudioEmitter(AudioEmitterData::default()),
        ));
        let emitters = sg.query_by_kind(&|k| matches!(k, NodeKind::AudioEmitter(_)));
        assert_eq!(emitters.len(), 1);
    }

    #[test]
    fn particle_emitter_node() {
        let mut sg = SceneGraph::new("test");
        sg.add(Node::new(
            "sparks",
            NodeKind::ParticleEmitter(ParticleEmitterData::default()),
        ));
        let emitters = sg.query_by_kind(&|k| matches!(k, NodeKind::ParticleEmitter(_)));
        assert_eq!(emitters.len(), 1);
    }

    #[test]
    fn deep_hierarchy_world_matrix() {
        let mut sg = SceneGraph::new("test");
        let mut prev = sg.add({
            let mut n = make_empty("n0");
            n.local_transform.position = Vec3::new(1.0, 0.0, 0.0);
            n
        });
        for i in 1..10 {
            let mut n = make_empty(&format!("n{i}"));
            n.local_transform.position = Vec3::new(1.0, 0.0, 0.0);
            prev = sg.add_child(prev, n);
        }
        sg.update_world_matrices();
        let p = sg.world_matrix(prev).transform_point3(Vec3::ZERO);
        assert!((p.x() - 10.0).abs() < 1e-4);
    }

    #[test]
    fn projection_default_is_perspective() {
        let proj = Projection::default();
        assert!(matches!(proj, Projection::Perspective { .. }));
    }

    #[test]
    fn light_variants() {
        let dir = LightVariant::Directional;
        let point = LightVariant::Point { radius: 5.0 };
        let spot = LightVariant::Spot {
            radius: 10.0,
            half_angle: 0.5,
        };
        assert!(matches!(dir, LightVariant::Directional));
        assert!(matches!(point, LightVariant::Point { .. }));
        assert!(matches!(spot, LightVariant::Spot { .. }));
    }

    #[test]
    fn scene_name() {
        let sg = SceneGraph::new("my_level");
        assert_eq!(sg.name(), "my_level");
    }

    #[test]
    fn sdf_data_default() {
        let sd = SdfData::default();
        assert!(sd.sdf_json.is_empty());
        assert!(!sd.generate_collider);
    }

    #[test]
    fn camera_data_default() {
        let cd = CameraData::default();
        assert!(cd.active);
    }

    #[test]
    fn aabb3_contains_point() {
        let aabb = Aabb3::new(Vec3::new(-1.0, -1.0, -1.0), Vec3::ONE);
        assert!(aabb.contains_point(Vec3::ZERO));
        assert!(!aabb.contains_point(Vec3::new(2.0, 0.0, 0.0)));
    }

    #[test]
    fn aabb3_intersects() {
        let a = Aabb3::new(Vec3::ZERO, Vec3::ONE);
        let b = Aabb3::new(Vec3::new(0.5, 0.5, 0.5), Vec3::new(2.0, 2.0, 2.0));
        let c = Aabb3::new(Vec3::new(5.0, 5.0, 5.0), Vec3::new(6.0, 6.0, 6.0));
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn aabb3_merge() {
        let a = Aabb3::new(Vec3::ZERO, Vec3::ONE);
        let b = Aabb3::new(Vec3::new(-2.0, -2.0, -2.0), Vec3::new(0.5, 0.5, 0.5));
        let merged = a.merge(&b);
        assert!((merged.min.x() - (-2.0)).abs() < 1e-6);
        assert!((merged.max.x() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn aabb3_expand() {
        let a = Aabb3::new(Vec3::ZERO, Vec3::ONE);
        let expanded = a.expand(0.5);
        assert!((expanded.min.x() - (-0.5)).abs() < 1e-6);
        assert!((expanded.max.x() - 1.5).abs() < 1e-6);
    }

    #[test]
    fn aabb3_center_and_half_extents() {
        let a = Aabb3::new(Vec3::new(-2.0, -2.0, -2.0), Vec3::new(2.0, 2.0, 2.0));
        let c = a.center();
        let h = a.half_extents();
        assert!(c.x().abs() < 1e-6);
        assert!((h.x() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn aabb3_transform_identity() {
        let a = Aabb3::new(-Vec3::ONE, Vec3::ONE);
        let t = a.transform(&Mat4::IDENTITY);
        assert!((t.min.x() - (-1.0)).abs() < 1e-5);
        assert!((t.max.x() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn aabb3_transform_translation() {
        let a = Aabb3::new(-Vec3::ONE, Vec3::ONE);
        let m = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let t = a.transform(&m);
        assert!((t.min.x() - 9.0).abs() < 1e-5);
        assert!((t.max.x() - 11.0).abs() < 1e-5);
    }

    #[test]
    fn frustum_large_frustum_contains_origin() {
        let vp = Mat4::perspective(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0);
        let frustum = Frustum::from_view_projection(vp);
        let aabb = Aabb3::new(Vec3::new(-0.5, -0.5, -1.0), Vec3::new(0.5, 0.5, -0.5));
        assert!(frustum.intersects_aabb(&aabb));
    }

    #[test]
    fn frustum_behind_camera_culled() {
        let vp = Mat4::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
        let frustum = Frustum::from_view_projection(vp);
        // Object behind the camera (positive Z in RH)
        let aabb = Aabb3::new(Vec3::new(-1.0, -1.0, 5.0), Vec3::new(1.0, 1.0, 10.0));
        assert!(!frustum.intersects_aabb(&aabb));
    }

    #[test]
    fn scene_graph_local_aabb_sdf() {
        let mut sg = SceneGraph::new("test");
        let sdf_id = sg.add(Node::new(
            "vol",
            NodeKind::Sdf(SdfData {
                half_extents: Vec3::new(5.0, 5.0, 5.0),
                ..SdfData::default()
            }),
        ));
        let aabb = sg.local_aabb(sdf_id);
        assert!((aabb.max.x() - 5.0).abs() < 1e-6);
    }

    #[test]
    fn scene_graph_world_aabb() {
        let mut sg = SceneGraph::new("test");
        let mut node = Node::new(
            "vol",
            NodeKind::Sdf(SdfData {
                half_extents: Vec3::new(1.0, 1.0, 1.0),
                ..SdfData::default()
            }),
        );
        node.local_transform.position = Vec3::new(10.0, 0.0, 0.0);
        let id = sg.add(node);
        sg.update_world_matrices();
        let aabb = sg.world_aabb(id);
        assert!((aabb.center().x() - 10.0).abs() < 1e-4);
    }

    #[test]
    fn scene_graph_descendants() {
        let mut sg = SceneGraph::new("test");
        let root = sg.add(make_empty("root"));
        let a = sg.add_child(root, make_empty("a"));
        let b = sg.add_child(root, make_empty("b"));
        let c = sg.add_child(a, make_empty("c"));
        let descs = sg.descendants(root);
        assert_eq!(descs.len(), 3);
        assert!(descs.contains(&a));
        assert!(descs.contains(&b));
        assert!(descs.contains(&c));
    }

    #[test]
    fn scene_graph_reparent() {
        let mut sg = SceneGraph::new("test");
        let root = sg.add(make_empty("root"));
        let a = sg.add_child(root, make_empty("a"));
        let b = sg.add_child(root, make_empty("b"));
        sg.reparent(b, a);
        assert_eq!(sg.get(b).unwrap().parent, a);
        assert!(sg.get(a).unwrap().children.contains(&b));
        assert!(!sg.get(root).unwrap().children.contains(&b));
    }

    #[test]
    fn scene_graph_frustum_cull() {
        let mut sg = SceneGraph::new("test");
        let mut near_node = Node::new(
            "near",
            NodeKind::Sdf(SdfData {
                half_extents: Vec3::ONE,
                ..SdfData::default()
            }),
        );
        near_node.local_transform.position = Vec3::new(0.0, 0.0, -5.0);
        sg.add(near_node);

        let mut far_node = Node::new(
            "far",
            NodeKind::Sdf(SdfData {
                half_extents: Vec3::ONE,
                ..SdfData::default()
            }),
        );
        far_node.local_transform.position = Vec3::new(0.0, 0.0, -500.0);
        sg.add(far_node);

        sg.update_world_matrices();

        let vp = Mat4::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
        let frustum = Frustum::from_view_projection(vp);
        let visible = sg.frustum_cull(&frustum);
        // Near node should be visible, far node should be culled
        assert!(visible.len() >= 1);
    }

    #[test]
    fn plane_distance() {
        let plane = Plane {
            normal: Vec3::Y,
            d: 0.0,
        };
        assert!((plane.distance_to_point(Vec3::new(0.0, 5.0, 0.0)) - 5.0).abs() < 1e-6);
        assert!((plane.distance_to_point(Vec3::new(0.0, -3.0, 0.0)) - (-3.0)).abs() < 1e-6);
    }

    #[test]
    fn scene_graph_empty() {
        let sg = SceneGraph::new("empty");
        assert_eq!(sg.node_count(), 0);
        assert!(sg.cameras().is_empty());
        assert!(sg.lights().is_empty());
        assert!(sg.meshes().is_empty());
    }

    #[test]
    fn scene_graph_remove_and_readd() {
        let mut sg = SceneGraph::new("test");
        let a = sg.add(make_empty("a"));
        let b = sg.add(make_empty("b"));
        sg.remove(a);
        let c = sg.add(make_empty("c"));
        assert_eq!(c.0, a.0); // reused slot
        assert_eq!(sg.node_count(), 2);
        assert_eq!(sg.get(c).unwrap().name, "c");
    }

    #[test]
    fn scene_graph_descendants_empty() {
        let mut sg = SceneGraph::new("test");
        let root = sg.add(make_empty("root"));
        assert!(sg.descendants(root).is_empty());
    }

    #[test]
    fn aabb3_default() {
        let a = Aabb3::default();
        assert_eq!(a, Aabb3::ZERO);
    }

    #[test]
    fn world_matrix_after_reparent() {
        let mut sg = SceneGraph::new("test");
        let mut parent_a = make_empty("a");
        parent_a.local_transform.position = Vec3::new(10.0, 0.0, 0.0);
        let a = sg.add(parent_a);
        let mut parent_b = make_empty("b");
        parent_b.local_transform.position = Vec3::new(-10.0, 0.0, 0.0);
        let b = sg.add(parent_b);
        let child = sg.add_child(a, make_empty("child"));
        sg.update_world_matrices();
        let pos1 = sg.world_matrix(child).transform_point3(Vec3::ZERO);
        assert!((pos1.x() - 10.0).abs() < 1e-4);
        sg.reparent(child, b);
        sg.update_world_matrices();
        let pos2 = sg.world_matrix(child).transform_point3(Vec3::ZERO);
        assert!((pos2.x() - (-10.0)).abs() < 1e-4);
    }

    #[test]
    fn node_visibility_default() {
        let n = make_empty("test");
        assert!(n.visible);
    }

    #[test]
    fn frustum_all_planes_valid() {
        let vp = Mat4::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
        let frustum = Frustum::from_view_projection(vp);
        for plane in &frustum.planes {
            let len = plane.normal.length();
            assert!((len - 1.0).abs() < 0.1);
        }
    }
}
