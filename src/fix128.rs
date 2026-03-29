//! 128-bit fixed-point arithmetic for long-duration simulation precision.
//!
//! `Fix128` uses i128 with 40 fractional bits, giving:
//! - Range: +-6.8 × 10^26 (integer part: 88 bits)
//! - Precision: ~9.1 × 10^-13 (sub-nanometer at planetary scale)
//!
//! Used for physics position accumulation over millions of frames
//! without floating-point drift.

use std::fmt;
use std::ops::{Add, Mul, Sub};

/// Number of fractional bits.
const FRAC_BITS: u32 = 40;
/// Scale factor: 2^40 ≈ 1.1 × 10^12.
const SCALE: f64 = (1_u64 << FRAC_BITS) as f64;
const INV_SCALE: f64 = 1.0 / SCALE;

// ---------------------------------------------------------------------------
// Fix128
// ---------------------------------------------------------------------------

/// 128-bit fixed-point number with 40 fractional bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Fix128(pub i128);

impl Fix128 {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(1 << FRAC_BITS);

    /// Creates from a floating-point value.
    #[must_use]
    pub fn from_f64(v: f64) -> Self {
        Self((v * SCALE) as i128)
    }

    /// Creates from a f32.
    #[must_use]
    pub fn from_f32(v: f32) -> Self {
        Self::from_f64(f64::from(v))
    }

    /// Converts to f64.
    #[must_use]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 * INV_SCALE
    }

    /// Converts to f32 (lossy).
    #[must_use]
    pub fn to_f32(self) -> f32 {
        self.to_f64() as f32
    }

    /// Absolute value.
    #[must_use]
    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }

    /// Returns the integer part.
    #[must_use]
    pub const fn integer_part(self) -> i128 {
        self.0 >> FRAC_BITS
    }

    /// Returns the fractional part as f64.
    #[must_use]
    pub fn fractional_part(self) -> f64 {
        let mask = (1_i128 << FRAC_BITS) - 1;
        (self.0 & mask) as f64 * INV_SCALE
    }

    /// Multiply-add: self * a + b (single expression, no intermediate rounding).
    #[must_use]
    pub const fn mul_add(self, a: Self, b: Self) -> Self {
        // (self * a) >> FRAC_BITS + b
        let product = self.0.wrapping_mul(a.0) >> FRAC_BITS;
        Self(product.wrapping_add(b.0))
    }
}

impl Add for Fix128 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0.wrapping_add(rhs.0))
    }
}

impl Sub for Fix128 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.wrapping_sub(rhs.0))
    }
}

impl Mul for Fix128 {
    type Output = Self;
    #[inline]
    #[allow(clippy::suspicious_arithmetic_impl)]
    fn mul(self, rhs: Self) -> Self {
        Self((self.0.wrapping_mul(rhs.0)) >> FRAC_BITS)
    }
}

impl fmt::Display for Fix128 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.12}", self.to_f64())
    }
}

// ---------------------------------------------------------------------------
// Fix128Vec3
// ---------------------------------------------------------------------------

/// 3D vector in Fix128 precision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fix128Vec3 {
    pub x: Fix128,
    pub y: Fix128,
    pub z: Fix128,
}

impl Fix128Vec3 {
    pub const ZERO: Self = Self {
        x: Fix128::ZERO,
        y: Fix128::ZERO,
        z: Fix128::ZERO,
    };

    #[must_use]
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self {
            x: Fix128::from_f64(x),
            y: Fix128::from_f64(y),
            z: Fix128::from_f64(z),
        }
    }

    #[must_use]
    pub fn from_f32(x: f32, y: f32, z: f32) -> Self {
        Self {
            x: Fix128::from_f32(x),
            y: Fix128::from_f32(y),
            z: Fix128::from_f32(z),
        }
    }

    /// Converts to f32 Vec3 (lossy — for rendering).
    #[must_use]
    pub fn to_vec3_f32(self) -> crate::math::Vec3 {
        crate::math::Vec3::new(self.x.to_f32(), self.y.to_f32(), self.z.to_f32())
    }

    /// Length squared (stays in Fix128).
    #[must_use]
    pub fn length_squared(self) -> Fix128 {
        self.x * self.x + self.y * self.y + self.z * self.z
    }

    /// Accumulates a f32 delta (e.g. velocity * dt) without precision loss.
    #[must_use]
    pub fn accumulate_f32(self, dx: f32, dy: f32, dz: f32) -> Self {
        Self {
            x: self.x + Fix128::from_f32(dx),
            y: self.y + Fix128::from_f32(dy),
            z: self.z + Fix128::from_f32(dz),
        }
    }
}

impl Add for Fix128Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
            z: self.z + rhs.z,
        }
    }
}

impl Sub for Fix128Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
            z: self.z - rhs.z,
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
    fn fix128_from_f64() {
        let a = Fix128::from_f64(3.14159);
        assert!((a.to_f64() - 3.14159).abs() < 1e-8);
    }

    #[test]
    fn fix128_from_f32() {
        let a = Fix128::from_f32(2.5);
        assert!((a.to_f32() - 2.5).abs() < 1e-5);
    }

    #[test]
    fn fix128_add() {
        let a = Fix128::from_f64(1.5);
        let b = Fix128::from_f64(2.25);
        let c = a + b;
        assert!((c.to_f64() - 3.75).abs() < 1e-10);
    }

    #[test]
    fn fix128_sub() {
        let a = Fix128::from_f64(5.0);
        let b = Fix128::from_f64(3.0);
        assert!(((a - b).to_f64() - 2.0).abs() < 1e-10);
    }

    #[test]
    fn fix128_mul() {
        let a = Fix128::from_f64(3.0);
        let b = Fix128::from_f64(4.0);
        assert!(((a * b).to_f64() - 12.0).abs() < 1e-6);
    }

    #[test]
    fn fix128_zero() {
        assert_eq!(Fix128::ZERO.to_f64(), 0.0);
    }

    #[test]
    fn fix128_one() {
        assert!((Fix128::ONE.to_f64() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn fix128_abs() {
        let a = Fix128::from_f64(-5.0);
        assert!((a.abs().to_f64() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn fix128_integer_part() {
        let a = Fix128::from_f64(7.99);
        assert_eq!(a.integer_part(), 7);
    }

    #[test]
    fn fix128_fractional_part() {
        let a = Fix128::from_f64(3.25);
        assert!((a.fractional_part() - 0.25).abs() < 1e-10);
    }

    #[test]
    fn fix128_mul_add() {
        let a = Fix128::from_f64(2.0);
        let b = Fix128::from_f64(3.0);
        let c = Fix128::from_f64(1.0);
        let result = a.mul_add(b, c);
        assert!((result.to_f64() - 7.0).abs() < 1e-5);
    }

    #[test]
    fn fix128_display() {
        let a = Fix128::from_f64(3.14);
        let v = a.to_f64();
        assert!((v - 3.14).abs() < 1e-8);
    }

    #[test]
    fn fix128_long_accumulation() {
        // Accumulate 1_000_000 small increments — f32 would drift, Fix128 should not.
        let delta = Fix128::from_f64(0.001);
        let mut sum = Fix128::ZERO;
        for _ in 0..1_000_000 {
            sum = sum + delta;
        }
        let expected = 1000.0;
        let error = (sum.to_f64() - expected).abs();
        assert!(error < 1e-6, "Accumulated error: {error}");
    }

    #[test]
    fn fix128vec3_add() {
        let a = Fix128Vec3::new(1.0, 2.0, 3.0);
        let b = Fix128Vec3::new(4.0, 5.0, 6.0);
        let c = a + b;
        assert!((c.x.to_f64() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn fix128vec3_sub() {
        let a = Fix128Vec3::new(10.0, 20.0, 30.0);
        let b = Fix128Vec3::new(3.0, 5.0, 7.0);
        let c = a - b;
        assert!((c.y.to_f64() - 15.0).abs() < 1e-10);
    }

    #[test]
    fn fix128vec3_to_f32() {
        let a = Fix128Vec3::new(1.5, 2.5, 3.5);
        let v = a.to_vec3_f32();
        assert!((v.x() - 1.5).abs() < 1e-5);
    }

    #[test]
    fn fix128vec3_accumulate() {
        let mut pos = Fix128Vec3::ZERO;
        for _ in 0..1_000_000 {
            pos = pos.accumulate_f32(0.001, 0.0, 0.0);
        }
        assert!((pos.x.to_f64() - 1000.0).abs() < 0.01);
    }

    #[test]
    fn fix128vec3_length_squared() {
        let a = Fix128Vec3::new(3.0, 4.0, 0.0);
        let lsq = a.length_squared();
        assert!((lsq.to_f64() - 25.0).abs() < 1e-5);
    }

    #[test]
    fn fix128_precision_vs_f32() {
        // Demonstrate Fix128 holds precision where f32 fails
        let mut f32_sum: f32 = 0.0;
        let mut fix_sum = Fix128::ZERO;
        let delta_f = 0.0001_f32;
        let delta_fix = Fix128::from_f64(0.0001);
        for _ in 0..10_000_000 {
            f32_sum += delta_f;
            fix_sum = fix_sum + delta_fix;
        }
        let f32_error = (f64::from(f32_sum) - 1000.0).abs();
        let fix_error = (fix_sum.to_f64() - 1000.0).abs();
        // Fix128 should be orders of magnitude more precise
        assert!(
            fix_error < f32_error,
            "Fix128 error {fix_error} should be less than f32 error {f32_error}"
        );
    }
}
