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
// Skinning WGSL — vertex stage that transforms a vertex by up to 4 bone
// matrices weighted by their normalised influences. Pair with any
// fragment stage; the renderer uploads `Skeleton::skin_matrices()` to the
// `bones` storage buffer each frame.
// ---------------------------------------------------------------------------

/// Returns the standard skinning vertex shader. Skips the fragment stage —
/// callers append their own. Layout:
///
/// ```text
/// @group(0) binding(0): MvpUniforms { view_proj: mat4x4 }
/// @group(1) binding(0): array<mat4x4<f32>> bones
/// ```
///
/// Vertex inputs:
///   `@location(0) position` (vec3)
///   `@location(1) bone_indices` (`vec4<u32>`)
///   `@location(2) bone_weights` (`vec4<f32>`)
#[must_use]
pub const fn skinning_vs_wgsl() -> &'static str {
    r"
struct Mvp { view_proj: mat4x4<f32> };

@group(0) @binding(0) var<uniform> mvp: Mvp;
@group(1) @binding(0) var<storage, read> bones: array<mat4x4<f32>>;

struct VsIn {
    @location(0) position:    vec3<f32>,
    @location(1) bone_idx:    vec4<u32>,
    @location(2) bone_weight: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var skin_mat: mat4x4<f32> =
          bones[in.bone_idx.x] * in.bone_weight.x
        + bones[in.bone_idx.y] * in.bone_weight.y
        + bones[in.bone_idx.z] * in.bone_weight.z
        + bones[in.bone_idx.w] * in.bone_weight.w;
    let world = skin_mat * vec4<f32>(in.position, 1.0);
    var out: VsOut;
    out.clip_pos = mvp.view_proj * world;
    return out;
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.8, 0.7, 0.6, 1.0);
}
"
}

/// Apply a skinning palette to a CPU-side vertex. Useful for unit tests
/// and for the rare authoring scenarios that need a deterministic
/// reference (e.g. snapshot comparisons against a GPU path).
#[must_use]
pub fn apply_skin_cpu(
    position: crate::math::Vec3,
    bone_indices: [u32; 4],
    bone_weights: [f32; 4],
    bone_matrices: &[crate::math::Mat4],
) -> crate::math::Vec3 {
    use crate::math::Vec3;
    // Sum each bone's transformed point weighted by the bone's influence.
    // Equivalent to `(Σ w_i * M_i) · p` because matrix-vector product is
    // linear, and avoids needing Add/Mul on Mat4.
    let mut acc = Vec3::ZERO;
    for i in 0..4 {
        let idx = bone_indices[i] as usize;
        let weight = bone_weights[i];
        if weight == 0.0 || idx >= bone_matrices.len() {
            continue;
        }
        let p = bone_matrices[idx].transform_point3(position);
        acc = acc + Vec3::new(p.x() * weight, p.y() * weight, p.z() * weight);
    }
    acc
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

    // -----------------------------------------------------------------------
    // Skinning WGSL + CPU reference tests
    // -----------------------------------------------------------------------

    #[test]
    fn skinning_vs_wgsl_parses_with_naga() {
        let m = naga::front::wgsl::parse_str(skinning_vs_wgsl()).expect("parse");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&m)
        .expect("validate");
    }

    #[test]
    fn apply_skin_cpu_single_bone_identity() {
        let mats = vec![Mat4::IDENTITY];
        let p = apply_skin_cpu(
            Vec3::new(1.0, 2.0, 3.0),
            [0, 0, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            &mats,
        );
        assert!((p.x() - 1.0).abs() < 1e-5);
        assert!((p.y() - 2.0).abs() < 1e-5);
        assert!((p.z() - 3.0).abs() < 1e-5);
    }

    #[test]
    fn apply_skin_cpu_blends_two_bones() {
        // Bone 0 is identity; bone 1 translates +X by 10. Blend 50/50 →
        // expect translation of +5 on X.
        let mats = vec![
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0)),
        ];
        let p = apply_skin_cpu(Vec3::ZERO, [0, 1, 0, 0], [0.5, 0.5, 0.0, 0.0], &mats);
        assert!((p.x() - 5.0).abs() < 1e-5);
    }

    #[test]
    fn apply_skin_cpu_skips_zero_weights() {
        let mats = vec![
            Mat4::IDENTITY,
            Mat4::from_translation(Vec3::new(99.0, 0.0, 0.0)),
        ];
        // bone 0 has 100 % weight, bone 1 has 0 → bone 1 shouldn't move
        // the point even though its index is in the list.
        let p = apply_skin_cpu(
            Vec3::new(1.0, 1.0, 1.0),
            [0, 1, 0, 0],
            [1.0, 0.0, 0.0, 0.0],
            &mats,
        );
        assert!((p.x() - 1.0).abs() < 1e-5);
    }
}
