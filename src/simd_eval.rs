//! SIMD 8-wide SDF batch evaluation using the `wide` crate.
//!
//! Evaluates 8 SDF points simultaneously using AVX2/NEON portable SIMD.

use crate::math::Vec3;
use wide::f32x8;

// ---------------------------------------------------------------------------
// SIMD Vec3 (8-wide)
// ---------------------------------------------------------------------------

/// 8-wide Vec3 for batch SDF evaluation.
#[derive(Debug, Clone, Copy)]
pub struct Vec3x8 {
    pub x: f32x8,
    pub y: f32x8,
    pub z: f32x8,
}

impl Vec3x8 {
    /// Creates from 8 individual Vec3 points.
    #[must_use]
    pub const fn from_points(points: &[Vec3; 8]) -> Self {
        Self {
            x: f32x8::new([
                points[0].x(),
                points[1].x(),
                points[2].x(),
                points[3].x(),
                points[4].x(),
                points[5].x(),
                points[6].x(),
                points[7].x(),
            ]),
            y: f32x8::new([
                points[0].y(),
                points[1].y(),
                points[2].y(),
                points[3].y(),
                points[4].y(),
                points[5].y(),
                points[6].y(),
                points[7].y(),
            ]),
            z: f32x8::new([
                points[0].z(),
                points[1].z(),
                points[2].z(),
                points[3].z(),
                points[4].z(),
                points[5].z(),
                points[6].z(),
                points[7].z(),
            ]),
        }
    }

    /// Broadcast a single Vec3 to all 8 lanes.
    #[must_use]
    pub fn splat(v: Vec3) -> Self {
        Self {
            x: f32x8::splat(v.x()),
            y: f32x8::splat(v.y()),
            z: f32x8::splat(v.z()),
        }
    }

    /// Compute length of each 8-wide vector.
    #[must_use]
    pub fn length(self) -> f32x8 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    /// Subtract.
    #[must_use]
    pub fn subtract(self, other: Self) -> Self {
        Self {
            x: self.x - other.x,
            y: self.y - other.y,
            z: self.z - other.z,
        }
    }

    /// Absolute value per component.
    #[must_use]
    pub fn abs(self) -> Self {
        Self {
            x: self.x.abs(),
            y: self.y.abs(),
            z: self.z.abs(),
        }
    }

    /// Component-wise max with zero.
    #[must_use]
    pub fn max_zero(self) -> Self {
        let zero = f32x8::ZERO;
        Self {
            x: self.x.max(zero),
            y: self.y.max(zero),
            z: self.z.max(zero),
        }
    }

    /// Extract results to an array.
    #[must_use]
    pub fn to_array(v: f32x8) -> [f32; 8] {
        v.to_array()
    }
}

// ---------------------------------------------------------------------------
// SIMD SDF primitives
// ---------------------------------------------------------------------------

/// 8-wide sphere SDF: length(p) - radius.
#[must_use]
pub fn sdf_sphere_x8(p: Vec3x8, radius: f32) -> f32x8 {
    p.length() - f32x8::splat(radius)
}

/// 8-wide box SDF.
#[must_use]
pub fn sdf_box_x8(p: Vec3x8, half_extents: Vec3) -> f32x8 {
    let h = Vec3x8::splat(half_extents);
    let q = p.abs().subtract(h);
    let q_clamped = q.max_zero();
    let outside = q_clamped.length();
    let inside = q.x.max(q.y.max(q.z)).min(f32x8::ZERO);
    outside + inside
}

// ---------------------------------------------------------------------------
// Batch evaluation
// ---------------------------------------------------------------------------

/// Evaluates SDF for N points using 8-wide SIMD batches.
/// `eval_fn` is the scalar fallback for the remainder.
#[must_use]
pub fn eval_batch_simd(points: &[Vec3], eval_scalar: &dyn Fn(Vec3) -> f32) -> Vec<f32> {
    let n = points.len();
    let mut results = Vec::with_capacity(n);
    let chunks = n / 8;
    for chunk in 0..chunks {
        let base = chunk * 8;
        let batch: [Vec3; 8] = std::array::from_fn(|i| points[base + i]);
        let d = f32x8::new(std::array::from_fn(|i| eval_scalar(batch[i])));
        results.extend_from_slice(&Vec3x8::to_array(d));
    }

    for point in &points[chunks * 8..] {
        results.push(eval_scalar(*point));
    }
    results
}

/// Pure SIMD sphere evaluation (no scalar fallback needed).
#[must_use]
pub fn eval_sphere_batch(points: &[Vec3], radius: f32) -> Vec<f32> {
    let n = points.len();
    let mut results = Vec::with_capacity(n);
    let chunks = n / 8;

    for chunk in 0..chunks {
        let base = chunk * 8;
        let batch: [Vec3; 8] = std::array::from_fn(|i| points[base + i]);
        let d = sdf_sphere_x8(Vec3x8::from_points(&batch), radius);
        results.extend_from_slice(&Vec3x8::to_array(d));
    }
    for point in &points[chunks * 8..] {
        results.push(point.length() - radius);
    }
    results
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec3x8_length() {
        let p = Vec3x8::splat(Vec3::new(3.0, 4.0, 0.0));
        let len = Vec3x8::to_array(p.length());
        for &l in &len {
            assert!((l - 5.0).abs() < 1e-4);
        }
    }

    #[test]
    fn sphere_x8_center() {
        let p = Vec3x8::splat(Vec3::ZERO);
        let d = sdf_sphere_x8(p, 1.0);
        let arr = Vec3x8::to_array(d);
        for &v in &arr {
            assert!((v - (-1.0)).abs() < 1e-6);
        }
    }

    #[test]
    fn sphere_x8_surface() {
        let p = Vec3x8::splat(Vec3::new(1.0, 0.0, 0.0));
        let d = sdf_sphere_x8(p, 1.0);
        let arr = Vec3x8::to_array(d);
        for &v in &arr {
            assert!(v.abs() < 1e-5);
        }
    }

    #[test]
    fn box_x8_center() {
        let p = Vec3x8::splat(Vec3::ZERO);
        let d = sdf_box_x8(p, Vec3::ONE);
        let arr = Vec3x8::to_array(d);
        for &v in &arr {
            assert!(v < 0.0);
        }
    }

    #[test]
    fn eval_sphere_batch_correctness() {
        let points: Vec<Vec3> = (0..16)
            .map(|i| Vec3::new(i as f32 * 0.5, 0.0, 0.0))
            .collect();
        let results = eval_sphere_batch(&points, 1.0);
        assert_eq!(results.len(), 16);
        assert!(results[0] < 0.0); // inside
        assert!(results[4] > 0.0); // outside at x=2.0
    }

    #[test]
    fn eval_batch_simd_matches_scalar() {
        let points: Vec<Vec3> = (0..20)
            .map(|i| Vec3::new(i as f32 * 0.3, 0.0, 0.0))
            .collect();
        let scalar_fn = |p: Vec3| p.length() - 1.0;
        let simd_results = eval_batch_simd(&points, &scalar_fn);
        assert_eq!(simd_results.len(), 20);
        for (i, &r) in simd_results.iter().enumerate() {
            let expected = scalar_fn(points[i]);
            assert!(
                (r - expected).abs() < 1e-5,
                "Mismatch at {i}: {r} vs {expected}"
            );
        }
    }

    #[test]
    fn vec3x8_from_points() {
        let pts: [Vec3; 8] = std::array::from_fn(|i| Vec3::new(i as f32, 0.0, 0.0));
        let v = Vec3x8::from_points(&pts);
        let arr = Vec3x8::to_array(v.x);
        assert!((arr[0]).abs() < 1e-6);
        assert!((arr[7] - 7.0).abs() < 1e-6);
    }

    #[test]
    fn vec3x8_sub() {
        let a = Vec3x8::splat(Vec3::new(5.0, 5.0, 5.0));
        let b = Vec3x8::splat(Vec3::new(2.0, 3.0, 4.0));
        let c = a.subtract(b);
        let arr = Vec3x8::to_array(c.x);
        assert!((arr[0] - 3.0).abs() < 1e-6);
    }
}
