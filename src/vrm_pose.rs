//! Posing a VRM model on the CPU: expression weights to morph target weights, morph targets,
//! node transforms and linear blend skinning, in the order glTF 2.0 gives (morph, then skin).
//!
//! The node hierarchy is walked from the roots in a fixed order, a node with two parents or a
//! cycle is refused, and every input weight and transform is checked to be finite. The
//! skinning itself is [`crate::skeleton::apply_skin_cpu`] (one implementation for the crate).

use crate::math::{Mat4, Quat, Vec3};
use crate::skeleton::apply_skin_cpu;
use crate::vrm::VrmModel;

/// Why a pose was not made or not applied
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PoseError {
    /// The model has no expression of that name
    UnknownExpression(String),
    /// An expression weight outside 0..=1 or not finite
    BadWeight,
    /// A transform with a component that is not finite
    NotFinite,
    /// A rotation whose squared length is not within 1e-4 of 1 (glTF rotations are unit quaternions)
    NotUnitRotation,
    /// No node of that index
    NoSuchNode(usize),
    /// No mesh (or primitive of that mesh) of that index
    NoSuchMesh(usize),
    /// A node is its own ancestor, or the child of two nodes
    NodeCycle,
    /// A joint index of a vertex is past the skin's joints
    JointOutOfRange(usize),
    /// The primitive of that mesh has no normals
    NoNormals(usize),
    /// The model has no `lookAt`
    NoLookAt,
}

impl std::fmt::Display for PoseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownExpression(n) => write!(f, "no expression named {n}"),
            Self::BadWeight => f.write_str("an expression weight is outside 0..=1 or not finite"),
            Self::NotFinite => f.write_str("a transform component is not finite"),
            Self::NotUnitRotation => f.write_str("a rotation is not a unit quaternion"),
            Self::NoSuchNode(i) => write!(f, "no node {i}"),
            Self::NoSuchMesh(i) => write!(f, "no mesh or primitive for mesh {i}"),
            Self::NodeCycle => {
                f.write_str("the node hierarchy has a cycle or a node with two parents")
            }
            Self::JointOutOfRange(v) => write!(f, "vertex {v} names a joint past the skin"),
            Self::NoNormals(m) => write!(f, "the primitive of mesh {m} has no normals"),
            Self::NoLookAt => f.write_str("the model has no lookAt"),
        }
    }
}

impl std::error::Error for PoseError {}

/// A pose of a [`VrmModel`]: each node's local transform and each expression's weight
#[derive(Debug, Clone)]
pub struct VrmPose<'a> {
    model: &'a VrmModel,
    translation: Vec<[f32; 3]>,
    rotation: Vec<[f32; 4]>,
    scale: Vec<[f32; 3]>,
    parent: Vec<Option<usize>>,
    /// nodes in an order where every parent comes before its children
    order: Vec<usize>,
    expression: Vec<f32>,
}

impl<'a> VrmPose<'a> {
    /// The rest pose: the node transforms of the file, every expression at 0
    ///
    /// # Errors
    ///
    /// [`PoseError::NodeCycle`], or [`PoseError::NoSuchNode`] for a child index past the nodes
    pub fn new(model: &'a VrmModel) -> Result<Self, PoseError> {
        let n = model.nodes.len();
        let mut parent = vec![None; n];
        for (i, node) in model.nodes.iter().enumerate() {
            for &c in &node.children {
                let c = c as usize;
                if c >= n {
                    return Err(PoseError::NoSuchNode(c));
                }
                if c == i || parent[c].is_some() {
                    return Err(PoseError::NodeCycle);
                }
                parent[c] = Some(i);
            }
        }
        // roots in index order, then each node's children in file order (depth first)
        let mut order = Vec::with_capacity(n);
        let mut stack: Vec<usize> = (0..n).rev().filter(|&i| parent[i].is_none()).collect();
        while let Some(i) = stack.pop() {
            order.push(i);
            stack.extend(model.nodes[i].children.iter().rev().map(|&c| c as usize));
        }
        // a cycle has no root, so its nodes are never reached
        if order.len() != n {
            return Err(PoseError::NodeCycle);
        }
        Ok(Self {
            model,
            translation: model.nodes.iter().map(|n| n.translation).collect(),
            rotation: model.nodes.iter().map(|n| n.rotation).collect(),
            scale: model.nodes.iter().map(|n| n.scale).collect(),
            parent,
            order,
            expression: vec![0.0; model.document.expressions.len()],
        })
    }

    /// Sets the weight of the expression `name` (0..=1)
    ///
    /// # Errors
    ///
    /// [`PoseError::UnknownExpression`] or [`PoseError::BadWeight`]
    pub fn set_expression(&mut self, name: &str, weight: f32) -> Result<(), PoseError> {
        if !(0.0..=1.0).contains(&weight) {
            return Err(PoseError::BadWeight);
        }
        let i = self
            .model
            .document
            .expressions
            .iter()
            .position(|e| e.name == name)
            .ok_or_else(|| PoseError::UnknownExpression(name.to_owned()))?;
        self.expression[i] = weight;
        Ok(())
    }

    /// Sets every channel of `set` on the expression of the same VRM name (the weights are
    /// already in 0..=1) and returns the names the model has no expression for, in the set's order
    pub fn apply_expression_set(&mut self, set: &crate::humanoid::ExpressionSet) -> Vec<String> {
        let mut missing = Vec::new();
        for (channel, &w) in set.iter() {
            let name = channel.vrm_name();
            match self
                .model
                .document
                .expressions
                .iter()
                .position(|e| e.name == name)
            {
                Some(i) => self.expression[i] = w,
                None => missing.extend([name.to_owned()]),
            }
        }
        missing
    }

    /// Points the gaze `yaw_left_deg` degrees to the model's left (negative: right) and
    /// `pitch_up_deg` degrees up (negative: down), through the model's `lookAt`
    ///
    /// Expression type: `lookLeft` / `lookRight` take the horizontal outer map of the yaw,
    /// `lookUp` / `lookDown` the vertical maps of the pitch (clamped to 0..=1); the opposite one
    /// is set to 0 when the model has it. Bone type: each eye bone turns by the mapped degrees
    /// about +Y (yaw; the outer map when the eye turns outward, the inner one inward) and about
    /// -X (pitch), after its rest rotation.
    ///
    /// # Errors
    ///
    /// [`PoseError::NotFinite`], [`PoseError::NoLookAt`], or [`PoseError::UnknownExpression`]
    /// naming a look expression the model lacks for a non-zero weight
    pub fn set_look(&mut self, yaw_left_deg: f32, pitch_up_deg: f32) -> Result<(), PoseError> {
        if !yaw_left_deg.is_finite() || !pitch_up_deg.is_finite() {
            return Err(PoseError::NotFinite);
        }
        let la = self.model.document.look_at.ok_or(PoseError::NoLookAt)?;
        match la.kind {
            crate::vrm::LookAtKind::Expression => {
                let h = la.horizontal_outer.map(yaw_left_deg).clamp(0.0, 1.0);
                let (left, right) = if yaw_left_deg > 0.0 {
                    (h, 0.0)
                } else {
                    (0.0, h)
                };
                let v = if pitch_up_deg > 0.0 {
                    la.vertical_up.map(pitch_up_deg)
                } else {
                    la.vertical_down.map(pitch_up_deg)
                }
                .clamp(0.0, 1.0);
                let (up, down) = if pitch_up_deg > 0.0 {
                    (v, 0.0)
                } else {
                    (0.0, v)
                };
                for (name, w) in [
                    ("lookLeft", left),
                    ("lookRight", right),
                    ("lookUp", up),
                    ("lookDown", down),
                ] {
                    let i = self
                        .model
                        .document
                        .expressions
                        .iter()
                        .position(|e| e.name == name);
                    match (i, w > 0.0) {
                        (Some(i), _) => self.expression[i] = w,
                        (None, true) => return Err(PoseError::UnknownExpression(name.to_owned())),
                        (None, false) => {}
                    }
                }
            }
            crate::vrm::LookAtKind::Bone => {
                for (eye, outward_is_left) in [("leftEye", true), ("rightEye", false)] {
                    let Some(&(_, node)) = self.model.document.bones.iter().find(|(b, _)| b == eye)
                    else {
                        continue;
                    };
                    let node = node as usize;
                    let outward = (yaw_left_deg > 0.0) == outward_is_left;
                    let map = if outward {
                        la.horizontal_outer
                    } else {
                        la.horizontal_inner
                    };
                    let yaw = map.map(yaw_left_deg).copysign(yaw_left_deg);
                    let pitch = if pitch_up_deg > 0.0 {
                        la.vertical_up.map(pitch_up_deg)
                    } else {
                        -la.vertical_down.map(pitch_up_deg)
                    };
                    let rest = self.model.nodes[node].rotation;
                    let q = glam::Quat::from_array(rest)
                        * glam::Quat::from_rotation_y(yaw.to_radians())
                        * glam::Quat::from_rotation_x(-pitch.to_radians());
                    self.rotation[node] = q.to_array();
                }
            }
        }
        Ok(())
    }

    const fn node(&self, node: usize) -> Result<(), PoseError> {
        if node < self.model.nodes.len() {
            Ok(())
        } else {
            Err(PoseError::NoSuchNode(node))
        }
    }

    /// Sets a node's local translation
    ///
    /// # Errors
    ///
    /// [`PoseError::NoSuchNode`] or [`PoseError::NotFinite`]
    pub fn set_translation(&mut self, node: usize, t: [f32; 3]) -> Result<(), PoseError> {
        self.node(node)?;
        if !t.iter().all(|x| x.is_finite()) {
            return Err(PoseError::NotFinite);
        }
        self.translation[node] = t;
        Ok(())
    }

    /// Sets a node's local rotation (unit quaternion x, y, z, w)
    ///
    /// # Errors
    ///
    /// [`PoseError::NoSuchNode`], [`PoseError::NotFinite`] or [`PoseError::NotUnitRotation`]
    pub fn set_rotation(&mut self, node: usize, q: [f32; 4]) -> Result<(), PoseError> {
        self.node(node)?;
        if !q.iter().all(|x| x.is_finite()) {
            return Err(PoseError::NotFinite);
        }
        let norm2: f32 = q.iter().map(|x| x * x).sum();
        if (norm2 - 1.0).abs() > 1e-4 {
            return Err(PoseError::NotUnitRotation);
        }
        self.rotation[node] = q;
        Ok(())
    }

    /// The morph target weights of mesh `mesh`: for each target, the sum over expressions of the
    /// expression weight times the bind weight, clamped to 0..=1
    #[must_use]
    pub fn morph_weights(&self, mesh: usize) -> Vec<f32> {
        let targets = self
            .model
            .meshes
            .get(mesh)
            .map_or(0, |p| p.iter().map(|p| p.targets.len()).max().unwrap_or(0));
        let mut w = vec![0.0f32; targets];
        for (e, &ew) in self.model.document.expressions.iter().zip(&self.expression) {
            if ew == 0.0 {
                continue;
            }
            for b in &e.binds {
                let on_mesh = self
                    .model
                    .nodes
                    .get(b.node as usize)
                    .and_then(|n| n.mesh)
                    .is_some_and(|m| m as usize == mesh);
                if on_mesh {
                    if let Some(t) = w.get_mut(b.index as usize) {
                        *t += ew * b.weight;
                    }
                }
            }
        }
        for t in &mut w {
            *t = t.clamp(0.0, 1.0);
        }
        w
    }

    /// The world matrix of every node, in node order
    #[must_use]
    pub fn world_matrices(&self) -> Vec<Mat4> {
        let mut world = vec![Mat4::IDENTITY; self.model.nodes.len()];
        for &i in &self.order {
            let [tx, ty, tz] = self.translation[i];
            let [qx, qy, qz, qw] = self.rotation[i];
            let [sx, sy, sz] = self.scale[i];
            let local = Mat4::from_trs(
                Vec3::new(tx, ty, tz),
                Quat(glam::Quat::from_xyzw(qx, qy, qz, qw)),
                Vec3::new(sx, sy, sz),
            );
            world[i] = match self.parent[i] {
                Some(p) => world[p] * local,
                None => local,
            };
        }
        world
    }

    /// The posed positions of primitive `prim` of mesh `mesh`: its morph targets at
    /// [`Self::morph_weights`], then skinned by the skin of the node that draws it (or moved
    /// by that node's world matrix when it has no skin)
    ///
    /// # Errors
    ///
    /// [`PoseError::NoSuchMesh`] or [`PoseError::JointOutOfRange`]
    pub fn posed_positions(&self, mesh: usize, prim: usize) -> Result<Vec<[f32; 3]>, PoseError> {
        let p = self
            .model
            .meshes
            .get(mesh)
            .and_then(|m| m.get(prim))
            .ok_or(PoseError::NoSuchMesh(mesh))?;
        let weights = self.morph_weights(mesh);
        let mut pos: Vec<[f32; 3]> = p.positions.clone();
        for (t, delta) in p.targets.iter().enumerate() {
            let w = weights.get(t).copied().unwrap_or(0.0);
            if w == 0.0 {
                continue;
            }
            for (v, d) in pos.iter_mut().zip(delta) {
                for c in 0..3 {
                    v[c] += w * d[c];
                }
            }
        }
        let world = self.world_matrices();
        let owner = self
            .model
            .nodes
            .iter()
            .position(|n| n.mesh.is_some_and(|m| m as usize == mesh));
        let skin = owner
            .and_then(|o| self.model.nodes[o].skin)
            .map(|s| &self.model.skins[s as usize]);
        if let (Some(skin), Some(joints), Some(jw)) = (skin, &p.joints, &p.weights) {
            let palette: Vec<Mat4> = skin
                .joints
                .iter()
                .zip(&skin.inverse_bind_matrices)
                .map(|(&j, ibm)| world[j as usize] * Mat4(glam::Mat4::from_cols_array(ibm)))
                .collect();
            pos.iter()
                .zip(joints.iter().zip(jw))
                .enumerate()
                .map(|(v, (x, (j, w)))| {
                    if j.iter()
                        .zip(w)
                        .any(|(&j, &w)| w != 0.0 && j as usize >= palette.len())
                    {
                        return Err(PoseError::JointOutOfRange(v));
                    }
                    let s =
                        apply_skin_cpu(Vec3::new(x[0], x[1], x[2]), j.map(u32::from), *w, &palette);
                    Ok([s.x(), s.y(), s.z()])
                })
                .collect()
        } else {
            let m = owner.map_or(Mat4::IDENTITY, |o| world[o]);
            Ok(pos
                .iter()
                .map(|x| {
                    let s = m.transform_point3(Vec3::new(x[0], x[1], x[2]));
                    [s.x(), s.y(), s.z()]
                })
                .collect())
        }
    }

    /// The posed normals of primitive `prim` of mesh `mesh`: its morph target normal deltas at
    /// [`Self::morph_weights`], then turned by the inverse transpose of the blended skinning
    /// matrix (or of the drawing node's world matrix without a skin), each scaled back to unit
    /// length (a normal that collapses to zero stays zero)
    ///
    /// # Errors
    ///
    /// [`PoseError::NoSuchMesh`], [`PoseError::NoNormals`] or [`PoseError::JointOutOfRange`]
    pub fn posed_normals(&self, mesh: usize, prim: usize) -> Result<Vec<[f32; 3]>, PoseError> {
        let p = self
            .model
            .meshes
            .get(mesh)
            .and_then(|m| m.get(prim))
            .ok_or(PoseError::NoSuchMesh(mesh))?;
        let mut nor = p.normals.clone().ok_or(PoseError::NoNormals(mesh))?;
        let weights = self.morph_weights(mesh);
        for (t, delta) in p.target_normals.iter().enumerate() {
            let w = weights.get(t).copied().unwrap_or(0.0);
            if let (Some(delta), true) = (delta, w != 0.0) {
                for (v, d) in nor.iter_mut().zip(delta) {
                    for c in 0..3 {
                        v[c] += w * d[c];
                    }
                }
            }
        }
        let world = self.world_matrices();
        let owner = self
            .model
            .nodes
            .iter()
            .position(|n| n.mesh.is_some_and(|m| m as usize == mesh));
        let skin = owner
            .and_then(|o| self.model.nodes[o].skin)
            .map(|s| &self.model.skins[s as usize]);
        let turn = |m: glam::Mat4, n: [f32; 3]| -> [f32; 3] {
            let r = glam::Mat3::from_mat4(m).inverse().transpose() * glam::Vec3::from(n);
            let len = r.length();
            if len > 0.0 && len.is_finite() {
                (r / len).into()
            } else {
                [0.0; 3]
            }
        };
        if let (Some(skin), Some(joints), Some(jw)) = (skin, &p.joints, &p.weights) {
            let palette: Vec<glam::Mat4> = skin
                .joints
                .iter()
                .zip(&skin.inverse_bind_matrices)
                .map(|(&j, ibm)| world[j as usize].0 * glam::Mat4::from_cols_array(ibm))
                .collect();
            nor.iter()
                .zip(joints.iter().zip(jw))
                .enumerate()
                .map(|(v, (n, (j, w)))| {
                    let mut m = glam::Mat4::ZERO;
                    for (&ji, &wi) in j.iter().zip(w) {
                        if wi == 0.0 {
                            continue;
                        }
                        let pm = palette
                            .get(ji as usize)
                            .ok_or(PoseError::JointOutOfRange(v))?;
                        m += *pm * wi;
                    }
                    Ok(turn(m, *n))
                })
                .collect()
        } else {
            let m = owner.map_or(glam::Mat4::IDENTITY, |o| world[o].0);
            Ok(nor.iter().map(|n| turn(m, *n)).collect())
        }
    }
}
