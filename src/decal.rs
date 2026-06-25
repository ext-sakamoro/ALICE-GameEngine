//! Deferred decal projection.
//!
//! A decal projects a 2D texture (albedo and optional normal) onto the
//! surface of any geometry already written to the `GBuffer`. The projection
//! volume is an OBB defined by the node's
//! [`LocalTransform`](crate::scene_graph::LocalTransform)
//! (`position` / `rotation` / `scale`). Fragments outside the OBB are
//! discarded; fragments inside are sampled using projector-local XY as UV
//! and blended onto the `GBuffer` according to [`DecalBlendMode`].
//!
//! Decals coexist with mesh and SDF geometry — they are added to the scene
//! graph as a separate node kind ([`NodeKind::Decal`](crate::scene_graph::NodeKind::Decal))
//! and collected once per frame into [`DecalDraw`] records that are then
//! consumed by the decal pass in the renderer.

use serde::{Deserialize, Serialize};

use crate::math::{Color, Mat4, Vec3};

// ---------------------------------------------------------------------------
// DecalBlendMode
// ---------------------------------------------------------------------------

/// How a decal combines its albedo with the underlying `GBuffer` albedo.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DecalBlendMode {
    /// Standard alpha-blend (`dst = lerp(dst, src, src.a * opacity)`).
    ///
    /// Use for bullet holes, blood splatter, stickers, signs.
    #[default]
    AlphaBlend,
    /// Multiply (`dst = dst * lerp(white, src, src.a * opacity)`).
    ///
    /// Use for dirt layers, scorch marks, shadow staining.
    Multiply,
    /// Additive (`dst = dst + src * src.a * opacity`).
    ///
    /// Use for glowing runes, projected emissive logos.
    Additive,
}

impl DecalBlendMode {
    /// Stable numeric identifier for the WGSL shader (`u32` push constant).
    ///
    /// The mapping is fixed and used in `shader::DECAL_FRAGMENT_WGSL`; do
    /// not reorder without updating the shader.
    #[inline]
    #[must_use]
    pub const fn shader_id(self) -> u32 {
        match self {
            Self::AlphaBlend => 0,
            Self::Multiply => 1,
            Self::Additive => 2,
        }
    }
}

// ---------------------------------------------------------------------------
// DecalData
// ---------------------------------------------------------------------------

/// Per-decal payload stored inside a scene graph node.
///
/// The OBB extents come from the owning
/// [`Node`](crate::scene_graph::Node)'s `local_transform.scale` (each axis
/// = half-extent). The OBB orientation comes from
/// `local_transform.rotation`. This keeps decal data lean and reuses the
/// existing transform editing UX.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecalData {
    /// Index into a texture resource table for the albedo image.
    pub albedo_texture: Option<u32>,
    /// Optional normal map. When `None` the decal does not perturb normals.
    pub normal_texture: Option<u32>,
    /// Tint colour multiplied with the sampled albedo before blending.
    pub color: Color,
    /// Overall opacity scaling factor in `[0.0, 1.0]`.
    pub opacity: f32,
    /// Layer mask. The decal only projects onto surfaces whose layer bit is
    /// set in this mask. `u32::MAX` = project onto everything.
    pub layer_mask: u32,
    /// Blending operation against the `GBuffer` albedo.
    pub blend_mode: DecalBlendMode,
}

impl Default for DecalData {
    fn default() -> Self {
        Self {
            albedo_texture: None,
            normal_texture: None,
            color: Color::WHITE,
            opacity: 1.0,
            layer_mask: u32::MAX,
            blend_mode: DecalBlendMode::AlphaBlend,
        }
    }
}

impl DecalData {
    /// Returns `true` when the decal's `layer_mask` overlaps with
    /// `surface_layers`.
    #[inline]
    #[must_use]
    pub const fn projects_onto(&self, surface_layers: u32) -> bool {
        (self.layer_mask & surface_layers) != 0
    }
}

// ---------------------------------------------------------------------------
// DecalDraw
// ---------------------------------------------------------------------------

/// A single decal ready to be submitted to the decal render pass.
///
/// The renderer issues one instanced draw per visible decal; the inverse
/// world matrix is computed once on the CPU so the WGSL shader can do a
/// single `mat4 * vec4` instead of inverting per-fragment.
#[derive(Debug, Clone)]
pub struct DecalDraw {
    /// OBB world transform (TRS). Maps `[-1, 1]^3` projector-local space to
    /// the world OBB.
    pub world_matrix: Mat4,
    /// `world_matrix.inverse()`, precomputed for shader use.
    pub inv_world_matrix: Mat4,
    /// Decal payload.
    pub data: DecalData,
}

impl DecalDraw {
    /// Construct a draw record, precomputing the inverse world matrix.
    #[inline]
    #[must_use]
    pub fn new(world_matrix: Mat4, data: DecalData) -> Self {
        Self {
            world_matrix,
            inv_world_matrix: world_matrix.inverse(),
            data,
        }
    }

    /// Maps a world position into projector-local space (`[-1, 1]^3` inside
    /// the OBB, outside otherwise).
    ///
    /// Surface fragments whose projector-local coordinates have any
    /// component with absolute value greater than `1` are discarded by the
    /// decal pass.
    #[inline]
    #[must_use]
    pub fn world_to_projector_local(&self, world_pos: Vec3) -> Vec3 {
        self.inv_world_matrix.transform_point3(world_pos)
    }

    /// True when `world_pos` lies within the projector OBB.
    #[inline]
    #[must_use]
    pub fn contains(&self, world_pos: Vec3) -> bool {
        let local = self.world_to_projector_local(world_pos);
        local.x().abs() <= 1.0 && local.y().abs() <= 1.0 && local.z().abs() <= 1.0
    }

    /// Computes the world-space AABB that bounds this decal's OBB.
    ///
    /// Used for frustum culling: a decal whose AABB is fully outside the
    /// camera frustum can skip the decal pass entirely.
    #[must_use]
    pub fn world_aabb(&self) -> (Vec3, Vec3) {
        // OBB has 8 corners at (+/-1, +/-1, +/-1) in projector-local space.
        // Transform each into world and accumulate componentwise min/max
        // via glam's SIMD path.
        let mut min = glam::Vec3::splat(f32::INFINITY);
        let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
        let signs = [-1.0_f32, 1.0];
        for &cx in &signs {
            for &cy in &signs {
                for &cz in &signs {
                    let world = self.world_matrix.transform_point3(Vec3::new(cx, cy, cz));
                    min = min.min(world.0);
                    max = max.max(world.0);
                }
            }
        }
        (Vec3(min), Vec3(max))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Quat;

    fn obb_at(pos: Vec3, scale: Vec3) -> Mat4 {
        Mat4::from_trs(pos, Quat::IDENTITY, scale)
    }

    #[test]
    fn decal_data_default_is_alpha_blend_white_full_layers() {
        let d = DecalData::default();
        assert_eq!(d.blend_mode, DecalBlendMode::AlphaBlend);
        assert_eq!(d.color, Color::WHITE);
        assert!((d.opacity - 1.0).abs() < 1e-6);
        assert_eq!(d.layer_mask, u32::MAX);
        assert!(d.albedo_texture.is_none());
        assert!(d.normal_texture.is_none());
    }

    #[test]
    fn blend_mode_shader_ids_are_stable() {
        // The shader hard-codes these — do not reorder.
        assert_eq!(DecalBlendMode::AlphaBlend.shader_id(), 0);
        assert_eq!(DecalBlendMode::Multiply.shader_id(), 1);
        assert_eq!(DecalBlendMode::Additive.shader_id(), 2);
    }

    #[test]
    fn blend_mode_serde_round_trip() {
        let modes = [
            DecalBlendMode::AlphaBlend,
            DecalBlendMode::Multiply,
            DecalBlendMode::Additive,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let back: DecalBlendMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, back);
        }
    }

    #[test]
    fn decal_data_serde_round_trip() {
        let d = DecalData {
            albedo_texture: Some(7),
            normal_texture: Some(11),
            color: Color::new(0.2, 0.5, 0.9, 1.0),
            opacity: 0.75,
            layer_mask: 0b0000_1111,
            blend_mode: DecalBlendMode::Multiply,
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: DecalData = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn decal_draw_precomputes_inverse() {
        let world = obb_at(Vec3::new(10.0, 0.0, 0.0), Vec3::new(2.0, 2.0, 2.0));
        let draw = DecalDraw::new(world, DecalData::default());
        // world * inv == identity
        let product = world * draw.inv_world_matrix;
        for row in 0..4 {
            for col in 0..4 {
                let expected = if row == col { 1.0 } else { 0.0 };
                let actual = product.0.col(col)[row];
                assert!(
                    (actual - expected).abs() < 1e-4,
                    "row {row} col {col}: expected {expected}, got {actual}",
                );
            }
        }
    }

    #[test]
    fn world_to_projector_local_inside_obb_returns_unit_range() {
        let world = obb_at(Vec3::new(5.0, 1.0, 0.0), Vec3::new(2.0, 2.0, 2.0));
        let draw = DecalDraw::new(world, DecalData::default());
        // OBB center maps to origin.
        let local_center = draw.world_to_projector_local(Vec3::new(5.0, 1.0, 0.0));
        assert!(local_center.length() < 1e-5);
        // OBB +X edge (world 7.0, 1.0, 0.0) maps to local x = 1.0.
        let local_edge = draw.world_to_projector_local(Vec3::new(7.0, 1.0, 0.0));
        assert!((local_edge.x() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn contains_inside_and_outside_obb() {
        let world = obb_at(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        let draw = DecalDraw::new(world, DecalData::default());
        assert!(draw.contains(Vec3::new(0.5, 0.5, 0.5)));
        assert!(draw.contains(Vec3::new(-1.0, -1.0, -1.0)));
        assert!(!draw.contains(Vec3::new(1.5, 0.0, 0.0)));
        assert!(!draw.contains(Vec3::new(0.0, 0.0, -2.0)));
    }

    #[test]
    fn world_aabb_bounds_axis_aligned_obb() {
        let world = obb_at(Vec3::new(10.0, 0.0, -5.0), Vec3::new(2.0, 3.0, 1.0));
        let draw = DecalDraw::new(world, DecalData::default());
        let (min, max) = draw.world_aabb();
        assert!((min.x() - 8.0).abs() < 1e-4);
        assert!((max.x() - 12.0).abs() < 1e-4);
        assert!((min.y() + 3.0).abs() < 1e-4);
        assert!((max.y() - 3.0).abs() < 1e-4);
        assert!((min.z() + 6.0).abs() < 1e-4);
        assert!((max.z() + 4.0).abs() < 1e-4);
    }

    #[test]
    fn world_aabb_bounds_rotated_obb() {
        // 45-degree rotation around Y: AABB half-extent = sqrt(2) * scale.
        let world = Mat4::from_trs(
            Vec3::ZERO,
            Quat::from_axis_angle(Vec3::Y, std::f32::consts::FRAC_PI_4),
            Vec3::new(1.0, 1.0, 1.0),
        );
        let draw = DecalDraw::new(world, DecalData::default());
        let (min, max) = draw.world_aabb();
        let expected = 2.0_f32.sqrt();
        assert!((max.x() - expected).abs() < 1e-3);
        assert!((min.x() + expected).abs() < 1e-3);
        // Y axis is the rotation axis, half-extent unchanged.
        assert!((max.y() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn projects_onto_layer_mask_matches_bitwise_and() {
        let d = DecalData {
            layer_mask: 0b0000_1111,
            ..DecalData::default()
        };
        assert!(d.projects_onto(0b0000_0001));
        assert!(d.projects_onto(0b0000_1000));
        assert!(!d.projects_onto(0b0001_0000));
        assert!(!d.projects_onto(0));
    }
}
