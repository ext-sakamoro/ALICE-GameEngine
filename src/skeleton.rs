//! Skeletal animation: bones, skinning, glTF skin import.
//!
//! ```rust
//! use alice_game_engine::skeleton::*;
//! use alice_game_engine::math::{Mat4, Vec3, Quat};
//!
//! let mut skel = Skeleton::new();
//! let root = skel.add_bone(Bone::new("root", Mat4::IDENTITY));
//! let arm = skel.add_bone(Bone::with_parent("arm", root, Mat4::IDENTITY));
//! assert_eq!(skel.bone_count(), 2);
//! ```

use crate::math::{Mat4, Quat, Vec3};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Bone
// ---------------------------------------------------------------------------

/// A bone in a skeleton hierarchy.
#[derive(Debug, Clone)]
pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    pub bind_pose: Mat4,
    pub inverse_bind: Mat4,
    pub local_transform: BoneTransform,
}

/// Bone local transform (TRS).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BoneTransform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Default for BoneTransform {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
        }
    }
}

impl BoneTransform {
    #[must_use]
    pub fn to_matrix(self) -> Mat4 {
        Mat4::from_trs(self.translation, self.rotation, self.scale)
    }
}

impl Bone {
    #[must_use]
    pub fn new(name: &str, bind_pose: Mat4) -> Self {
        Self {
            name: name.to_string(),
            parent: None,
            bind_pose,
            inverse_bind: bind_pose.inverse(),
            local_transform: BoneTransform::default(),
        }
    }

    #[must_use]
    pub fn with_parent(name: &str, parent: usize, bind_pose: Mat4) -> Self {
        Self {
            name: name.to_string(),
            parent: Some(parent),
            bind_pose,
            inverse_bind: bind_pose.inverse(),
            local_transform: BoneTransform::default(),
        }
    }
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

/// A skeleton: a hierarchy of bones.
pub struct Skeleton {
    pub bones: Vec<Bone>,
    pub world_matrices: Vec<Mat4>,
}

impl Skeleton {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bones: Vec::new(),
            world_matrices: Vec::new(),
        }
    }

    /// Adds a bone and returns its index.
    pub fn add_bone(&mut self, bone: Bone) -> usize {
        self.bones.push(bone);
        self.world_matrices.push(Mat4::IDENTITY);
        self.bones.len() - 1
    }

    /// Finds a bone by name.
    #[must_use]
    pub fn find_bone(&self, name: &str) -> Option<usize> {
        self.bones.iter().position(|b| b.name == name)
    }

    #[must_use]
    pub const fn bone_count(&self) -> usize {
        self.bones.len()
    }

    /// Computes world matrices for all bones.
    pub fn update(&mut self) {
        for i in 0..self.bones.len() {
            let local = self.bones[i].local_transform.to_matrix();
            self.world_matrices[i] = match self.bones[i].parent {
                Some(p) => self.world_matrices[p] * local,
                None => local,
            };
        }
    }

    /// Returns the skinning matrix for a bone (world × `inverse_bind`).
    #[must_use]
    pub fn skin_matrix(&self, bone_idx: usize) -> Mat4 {
        self.world_matrices[bone_idx] * self.bones[bone_idx].inverse_bind
    }

    /// Returns all skinning matrices (for GPU upload).
    #[must_use]
    pub fn skin_matrices(&self) -> Vec<Mat4> {
        (0..self.bones.len()).map(|i| self.skin_matrix(i)).collect()
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SkinData — vertex→bone weights
// ---------------------------------------------------------------------------

/// Per-vertex skinning data (max 4 bones per vertex).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SkinWeight {
    pub joints: [u16; 4],
    pub weights: [f32; 4],
}

impl SkinWeight {
    pub const NONE: Self = Self {
        joints: [0; 4],
        weights: [1.0, 0.0, 0.0, 0.0],
    };

    /// Normalizes weights to sum to 1.0.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let sum: f32 = self.weights.iter().sum();
        if sum > 0.0 {
            let inv = sum.recip();
            for w in &mut self.weights {
                *w *= inv;
            }
        }
        self
    }
}

/// Skin data for a mesh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkinData {
    pub weights: Vec<SkinWeight>,
    pub skeleton_name: String,
}

impl SkinData {
    #[must_use]
    pub fn new(skeleton_name: &str) -> Self {
        Self {
            weights: Vec::new(),
            skeleton_name: skeleton_name.to_string(),
        }
    }

    #[must_use]
    pub const fn vertex_count(&self) -> usize {
        self.weights.len()
    }
}

// ---------------------------------------------------------------------------
// SkeletalAnimation — bone-specific tracks
// ---------------------------------------------------------------------------

/// A skeletal animation clip: per-bone keyframe tracks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkeletalAnimation {
    pub name: String,
    pub duration: f32,
    pub bone_tracks: Vec<BoneTrack>,
}

/// Keyframed transform for a single bone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoneTrack {
    pub bone_name: String,
    pub translations: Vec<(f32, Vec3)>,
    pub rotations: Vec<(f32, Quat)>,
}

impl BoneTrack {
    #[must_use]
    pub fn new(bone_name: &str) -> Self {
        Self {
            bone_name: bone_name.to_string(),
            translations: Vec::new(),
            rotations: Vec::new(),
        }
    }

    /// Evaluates translation at time t (linear interpolation).
    #[must_use]
    pub fn eval_translation(&self, t: f32) -> Vec3 {
        if self.translations.is_empty() {
            return Vec3::ZERO;
        }
        if t <= self.translations[0].0 {
            return self.translations[0].1;
        }
        for pair in self.translations.windows(2) {
            if t >= pair[0].0 && t <= pair[1].0 {
                let frac = (t - pair[0].0) * (pair[1].0 - pair[0].0).recip();
                return pair[0].1.lerp(pair[1].1, frac);
            }
        }
        self.translations.last().map_or(Vec3::ZERO, |&(_, v)| v)
    }

    /// Evaluates rotation at time t (slerp).
    #[must_use]
    pub fn eval_rotation(&self, t: f32) -> Quat {
        if self.rotations.is_empty() {
            return Quat::IDENTITY;
        }
        if t <= self.rotations[0].0 {
            return self.rotations[0].1;
        }
        for pair in self.rotations.windows(2) {
            if t >= pair[0].0 && t <= pair[1].0 {
                let frac = (t - pair[0].0) * (pair[1].0 - pair[0].0).recip();
                return pair[0].1.slerp(pair[1].1, frac);
            }
        }
        self.rotations.last().map_or(Quat::IDENTITY, |&(_, q)| q)
    }
}

impl SkeletalAnimation {
    /// Applies the animation to a skeleton at time t.
    pub fn apply(&self, skeleton: &mut Skeleton, t: f32) {
        for track in &self.bone_tracks {
            if let Some(idx) = skeleton.find_bone(&track.bone_name) {
                skeleton.bones[idx].local_transform.translation = track.eval_translation(t);
                skeleton.bones[idx].local_transform.rotation = track.eval_rotation(t);
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

    #[test]
    fn bone_new() {
        let b = Bone::new("root", Mat4::IDENTITY);
        assert_eq!(b.name, "root");
        assert!(b.parent.is_none());
    }

    #[test]
    fn bone_with_parent() {
        let b = Bone::with_parent("arm", 0, Mat4::IDENTITY);
        assert_eq!(b.parent, Some(0));
    }

    #[test]
    fn skeleton_add_find() {
        let mut s = Skeleton::new();
        let r = s.add_bone(Bone::new("root", Mat4::IDENTITY));
        s.add_bone(Bone::with_parent("spine", r, Mat4::IDENTITY));
        assert_eq!(s.bone_count(), 2);
        assert_eq!(s.find_bone("spine"), Some(1));
        assert_eq!(s.find_bone("nope"), None);
    }

    #[test]
    fn skeleton_update() {
        let mut s = Skeleton::new();
        let root = s.add_bone(Bone::new("root", Mat4::IDENTITY));
        s.bones[root].local_transform.translation = Vec3::new(10.0, 0.0, 0.0);
        let child = s.add_bone(Bone::with_parent("child", root, Mat4::IDENTITY));
        s.bones[child].local_transform.translation = Vec3::new(0.0, 5.0, 0.0);
        s.update();
        let p = s.world_matrices[child].transform_point3(Vec3::ZERO);
        assert!((p.x() - 10.0).abs() < 1e-4);
        assert!((p.y() - 5.0).abs() < 1e-4);
    }

    #[test]
    fn skin_weight_normalize() {
        let sw = SkinWeight {
            joints: [0, 1, 0, 0],
            weights: [2.0, 2.0, 0.0, 0.0],
        }
        .normalized();
        assert!((sw.weights[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn skin_data_new() {
        let sd = SkinData::new("humanoid");
        assert_eq!(sd.skeleton_name, "humanoid");
        assert_eq!(sd.vertex_count(), 0);
    }

    #[test]
    fn bone_track_eval() {
        let mut track = BoneTrack::new("arm");
        track.translations.push((0.0, Vec3::ZERO));
        track.translations.push((1.0, Vec3::new(10.0, 0.0, 0.0)));
        let mid = track.eval_translation(0.5);
        assert!((mid.x() - 5.0).abs() < 0.1);
    }

    #[test]
    fn bone_track_rotation() {
        let mut track = BoneTrack::new("arm");
        track.rotations.push((0.0, Quat::IDENTITY));
        track
            .rotations
            .push((1.0, Quat::from_axis_angle(Vec3::Y, std::f32::consts::PI)));
        let mid = track.eval_rotation(0.5);
        assert_ne!(mid, Quat::IDENTITY);
    }

    #[test]
    fn skeletal_animation_apply() {
        let mut skel = Skeleton::new();
        skel.add_bone(Bone::new("root", Mat4::IDENTITY));
        let mut anim = SkeletalAnimation {
            name: "walk".to_string(),
            duration: 1.0,
            bone_tracks: vec![],
        };
        let mut track = BoneTrack::new("root");
        track.translations.push((0.0, Vec3::ZERO));
        track.translations.push((1.0, Vec3::new(5.0, 0.0, 0.0)));
        anim.bone_tracks.push(track);
        anim.apply(&mut skel, 0.5);
        assert!((skel.bones[0].local_transform.translation.x() - 2.5).abs() < 0.1);
    }

    #[test]
    fn skin_matrices() {
        let mut skel = Skeleton::new();
        skel.add_bone(Bone::new("root", Mat4::IDENTITY));
        skel.update();
        let mats = skel.skin_matrices();
        assert_eq!(mats.len(), 1);
    }

    #[test]
    fn bone_transform_default() {
        let bt = BoneTransform::default();
        assert_eq!(bt.translation, Vec3::ZERO);
        assert_eq!(bt.rotation, Quat::IDENTITY);
    }
}
