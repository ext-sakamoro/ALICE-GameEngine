//! 3D Gaussian Splatting — CPU data + tile-based depth sort.
//!
//! Represents a scene as an unordered collection of 3-D Gaussians.
//! Each Gaussian carries a position, anisotropic covariance (encoded
//! as a quaternion + 3 scales), an opacity, and a colour (optionally
//! view-dependent through low-order spherical harmonics). At render
//! time the Gaussians are projected to screen, depth-sorted, and
//! alpha-blended back-to-front — see Kerbl et al. 2023.
//!
//! This module owns the **CPU data structures + the per-frame
//! preparation pass** (frustum cull → screen-space projection → depth
//! sort) so it can be unit-tested without a GPU device. The actual
//! tile-based blending lives in a compute / fragment shader; that
//! integration goes in a follow-up PR.

use serde::{Deserialize, Serialize};

use crate::math::{Color, Mat4, Vec3};

// ---------------------------------------------------------------------------
// Splat data
// ---------------------------------------------------------------------------

/// One 3-D Gaussian.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Splat {
    pub position: Vec3,
    /// Scales along the local principal axes (covariance eigenvalues
    /// before rotation). All values must be positive; the renderer
    /// usually stores `log(scale)` to keep ranges sane, but the data
    /// model exposes raw scales for clarity.
    pub scale: Vec3,
    /// Rotation as a quaternion (w, x, y, z) — encodes the
    /// covariance orientation.
    pub rotation: [f32; 4],
    pub color: Color,
    pub opacity: f32,
    /// Optional first-band spherical-harmonic coefficients (3 × 3 = 9
    /// values, RGB × 3 dirs) for view-dependent colour. Use `[0; 9]`
    /// when constant colour is enough.
    pub sh_band1: [f32; 9],
}

impl Splat {
    /// Construct a constant-colour Gaussian (no view dependence).
    #[must_use]
    pub const fn isotropic(position: Vec3, radius: f32, color: Color, opacity: f32) -> Self {
        Self {
            position,
            scale: Vec3::new(radius, radius, radius),
            rotation: [1.0, 0.0, 0.0, 0.0],
            color,
            opacity,
            sh_band1: [0.0; 9],
        }
    }
}

// ---------------------------------------------------------------------------
// Cloud + projection
// ---------------------------------------------------------------------------

/// Owns a collection of Gaussians and the projection scratch buffer.
pub struct GaussianCloud {
    pub splats: Vec<Splat>,
    projected: Vec<ProjectedSplat>,
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectedSplat {
    /// Index into [`GaussianCloud::splats`].
    pub source: u32,
    /// View-space depth (positive = in front of the camera).
    pub depth: f32,
    /// Screen-space NDC `[-1, 1]` × `[-1, 1]`.
    pub ndc: (f32, f32),
    /// Approximate screen-space radius (= largest `scale` projected).
    pub radius: f32,
}

impl GaussianCloud {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            splats: Vec::new(),
            projected: Vec::new(),
        }
    }

    #[must_use]
    pub const fn with_splats(splats: Vec<Splat>) -> Self {
        Self {
            splats,
            projected: Vec::new(),
        }
    }

    pub fn add(&mut self, splat: Splat) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        let idx = self.splats.len() as u32;
        self.splats.push(splat);
        idx
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.splats.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.splats.is_empty()
    }

    /// Per-frame projection + frustum cull + back-to-front depth sort.
    /// Splats whose centre falls behind the camera are dropped; the
    /// returned slice references [`Self::projected`] in render order.
    pub fn prepare_frame(&mut self, view: Mat4, projection: Mat4) -> &[ProjectedSplat] {
        self.projected.clear();
        let view_proj = projection * view;
        for (i, splat) in self.splats.iter().enumerate() {
            let view_pos = view.transform_point3(splat.position);
            let depth = -view_pos.z(); // RH view space → positive in front.
            if depth <= 0.001 {
                continue;
            }
            let clip = view_proj.0
                * glam::Vec4::new(
                    splat.position.x(),
                    splat.position.y(),
                    splat.position.z(),
                    1.0,
                );
            if clip.w <= 0.0 {
                continue;
            }
            let inv_w = clip.w.recip();
            let ndc_x = clip.x * inv_w;
            let ndc_y = clip.y * inv_w;
            // Cheap conservative cull: skip splats whose centre + max
            // scale falls outside ±1 NDC.
            let max_scale = splat.scale.x().max(splat.scale.y()).max(splat.scale.z());
            let screen_radius = max_scale * inv_w;
            if ndc_x + screen_radius < -1.0
                || ndc_x - screen_radius > 1.0
                || ndc_y + screen_radius < -1.0
                || ndc_y - screen_radius > 1.0
            {
                continue;
            }
            #[allow(clippy::cast_possible_truncation)]
            self.projected.push(ProjectedSplat {
                source: i as u32,
                depth,
                ndc: (ndc_x, ndc_y),
                radius: screen_radius,
            });
        }
        // Back-to-front for alpha blending.
        self.projected.sort_by(|a, b| {
            b.depth
                .partial_cmp(&a.depth)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        &self.projected
    }

    #[must_use]
    pub fn projected(&self) -> &[ProjectedSplat] {
        &self.projected
    }
}

impl Default for GaussianCloud {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SH band 0/1 evaluator (used by the future tile shader)
// ---------------------------------------------------------------------------

/// Evaluate the first SH band for a view direction. The 9 coefficients
/// are packed as `[r0, g0, b0, r1, g1, b1, r-1, g-1, b-1]` (matching
/// the convention used by gsplat reference implementations).
#[must_use]
pub fn evaluate_sh_band1(coeffs: &[f32; 9], view_dir: Vec3) -> Color {
    let dir = view_dir.normalize();
    let c1 = 0.488_602_5_f32; // sqrt(3 / 4π)
    let (x, y, z) = (dir.x(), dir.y(), dir.z());
    let basis = [c1 * z, -c1 * y, c1 * x];
    let r = coeffs[6].mul_add(basis[2], coeffs[0].mul_add(basis[0], coeffs[3] * basis[1]));
    let g = coeffs[7].mul_add(basis[2], coeffs[1].mul_add(basis[0], coeffs[4] * basis[1]));
    let b = coeffs[8].mul_add(basis[2], coeffs[2].mul_add(basis[0], coeffs[5] * basis[1]));
    Color::new(r, g, b, 1.0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> (Mat4, Mat4) {
        let view = Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective(std::f32::consts::FRAC_PI_3, 16.0 / 9.0, 0.1, 100.0);
        (view, proj)
    }

    #[test]
    fn isotropic_constructor_sets_uniform_scale() {
        let s = Splat::isotropic(Vec3::ZERO, 0.5, Color::RED, 0.8);
        assert!((s.scale.x() - 0.5).abs() < 1e-6);
        assert!((s.scale.y() - 0.5).abs() < 1e-6);
        assert!((s.scale.z() - 0.5).abs() < 1e-6);
        assert!((s.opacity - 0.8).abs() < 1e-6);
    }

    #[test]
    fn add_returns_sequential_indices() {
        let mut c = GaussianCloud::new();
        let i0 = c.add(Splat::isotropic(Vec3::ZERO, 0.1, Color::WHITE, 1.0));
        let i1 = c.add(Splat::isotropic(Vec3::X, 0.1, Color::WHITE, 1.0));
        assert_eq!(i0, 0);
        assert_eq!(i1, 1);
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn cloud_is_empty_when_default() {
        let c = GaussianCloud::new();
        assert!(c.is_empty());
    }

    #[test]
    fn prepare_frame_keeps_visible_splats() {
        let (view, proj) = camera();
        let mut c = GaussianCloud::new();
        c.add(Splat::isotropic(Vec3::ZERO, 0.1, Color::WHITE, 1.0));
        let projected = c.prepare_frame(view, proj);
        assert_eq!(projected.len(), 1);
        assert!(projected[0].depth > 0.0);
    }

    #[test]
    fn prepare_frame_culls_splats_behind_camera() {
        let (view, proj) = camera();
        let mut c = GaussianCloud::new();
        c.add(Splat::isotropic(
            Vec3::new(0.0, 0.0, 100.0),
            0.1,
            Color::WHITE,
            1.0,
        ));
        let projected = c.prepare_frame(view, proj);
        assert_eq!(projected.len(), 0);
    }

    #[test]
    fn prepare_frame_culls_splats_off_screen() {
        let (view, proj) = camera();
        let mut c = GaussianCloud::new();
        c.add(Splat::isotropic(
            Vec3::new(50.0, 0.0, 0.0),
            0.1,
            Color::WHITE,
            1.0,
        ));
        let projected = c.prepare_frame(view, proj);
        assert_eq!(projected.len(), 0);
    }

    #[test]
    fn prepare_frame_sorts_back_to_front() {
        let (view, proj) = camera();
        let mut c = GaussianCloud::new();
        c.add(Splat::isotropic(Vec3::ZERO, 0.1, Color::WHITE, 1.0)); // depth 5
        c.add(Splat::isotropic(
            Vec3::new(0.0, 0.0, -3.0),
            0.1,
            Color::WHITE,
            1.0,
        )); // depth 8
        c.add(Splat::isotropic(
            Vec3::new(0.0, 0.0, 2.0),
            0.1,
            Color::WHITE,
            1.0,
        )); // depth 3
        let projected = c.prepare_frame(view, proj);
        assert_eq!(projected.len(), 3);
        // Largest depth first.
        for w in projected.windows(2) {
            assert!(w[0].depth >= w[1].depth);
        }
    }

    #[test]
    fn evaluate_sh_band1_returns_zero_for_zero_coeffs() {
        let c = evaluate_sh_band1(&[0.0; 9], Vec3::new(0.0, 1.0, 0.0));
        assert!(c.r.abs() < 1e-6);
        assert!(c.g.abs() < 1e-6);
        assert!(c.b.abs() < 1e-6);
    }

    #[test]
    fn evaluate_sh_band1_is_view_dependent() {
        let coeffs = [
            1.0, 0.0, 0.0, // R band
            0.0, 1.0, 0.0, // G band
            0.0, 0.0, 1.0, // B band
        ];
        let a = evaluate_sh_band1(&coeffs, Vec3::new(0.0, 1.0, 0.0));
        let b = evaluate_sh_band1(&coeffs, Vec3::new(0.0, -1.0, 0.0));
        assert!((a.r - b.r).abs() > 1e-3 || (a.g - b.g).abs() > 1e-3 || (a.b - b.b).abs() > 1e-3);
    }

    #[test]
    fn splat_serde_round_trip() {
        let s = Splat::isotropic(Vec3::new(1.0, 2.0, 3.0), 0.4, Color::GREEN, 0.7);
        let j = serde_json::to_string(&s).unwrap();
        let back: Splat = serde_json::from_str(&j).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn with_splats_seeds_the_cloud() {
        let splats = vec![
            Splat::isotropic(Vec3::ZERO, 0.1, Color::WHITE, 1.0),
            Splat::isotropic(Vec3::X, 0.1, Color::RED, 0.8),
        ];
        let c = GaussianCloud::with_splats(splats);
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn prepare_frame_clears_previous_projection() {
        let (view, proj) = camera();
        let mut c = GaussianCloud::new();
        c.add(Splat::isotropic(Vec3::ZERO, 0.1, Color::WHITE, 1.0));
        c.prepare_frame(view, proj);
        c.splats.clear();
        let projected = c.prepare_frame(view, proj);
        assert_eq!(projected.len(), 0);
    }
}
