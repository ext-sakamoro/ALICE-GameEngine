//! Math primitives: thin wrappers over glam with serde + engine conveniences.

use glam::{Mat4 as GMat4, Quat as GQuat, Vec2 as GVec2, Vec3 as GVec3, Vec4 as GVec4};
use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul, Sub};

// ---------------------------------------------------------------------------
// Vec2
// ---------------------------------------------------------------------------

/// 2D vector.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec2(pub GVec2);

impl Vec2 {
    pub const ZERO: Self = Self(GVec2::ZERO);
    pub const ONE: Self = Self(GVec2::ONE);

    #[inline]
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self(GVec2::new(x, y))
    }

    #[inline]
    #[must_use]
    pub const fn x(self) -> f32 {
        self.0.x
    }

    #[inline]
    #[must_use]
    pub const fn y(self) -> f32 {
        self.0.y
    }

    #[inline]
    #[must_use]
    pub fn length(self) -> f32 {
        self.0.length()
    }

    #[inline]
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.0.length_squared()
    }

    #[inline]
    #[must_use]
    pub fn normalize(self) -> Self {
        Self(self.0.normalize())
    }

    #[inline]
    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.0.dot(other.0)
    }

    #[inline]
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self(self.0.lerp(other.0, t))
    }
}

impl Add for Vec2 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Vec2 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self(self.0 * rhs)
    }
}

impl Default for Vec2 {
    fn default() -> Self {
        Self::ZERO
    }
}

impl From<GVec2> for Vec2 {
    fn from(v: GVec2) -> Self {
        Self(v)
    }
}

impl From<Vec2> for GVec2 {
    fn from(v: Vec2) -> Self {
        v.0
    }
}

// ---------------------------------------------------------------------------
// Vec3
// ---------------------------------------------------------------------------

/// 3D vector.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec3(pub GVec3);

impl Vec3 {
    pub const ZERO: Self = Self(GVec3::ZERO);
    pub const ONE: Self = Self(GVec3::ONE);
    pub const X: Self = Self(GVec3::X);
    pub const Y: Self = Self(GVec3::Y);
    pub const Z: Self = Self(GVec3::Z);

    #[inline]
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self(GVec3::new(x, y, z))
    }

    #[inline]
    #[must_use]
    pub const fn x(self) -> f32 {
        self.0.x
    }

    #[inline]
    #[must_use]
    pub const fn y(self) -> f32 {
        self.0.y
    }

    #[inline]
    #[must_use]
    pub const fn z(self) -> f32 {
        self.0.z
    }

    #[inline]
    #[must_use]
    pub fn length(self) -> f32 {
        self.0.length()
    }

    #[inline]
    #[must_use]
    pub fn length_squared(self) -> f32 {
        self.0.length_squared()
    }

    #[inline]
    #[must_use]
    pub fn normalize(self) -> Self {
        Self(self.0.normalize())
    }

    #[inline]
    #[must_use]
    pub fn dot(self, other: Self) -> f32 {
        self.0.dot(other.0)
    }

    #[inline]
    #[must_use]
    pub fn cross(self, other: Self) -> Self {
        Self(self.0.cross(other.0))
    }

    #[inline]
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self(self.0.lerp(other.0, t))
    }

    #[inline]
    #[must_use]
    pub fn distance(self, other: Self) -> f32 {
        (self - other).length()
    }
}

impl Add for Vec3 {
    type Output = Self;
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Vec3 {
    type Output = Self;
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: f32) -> Self {
        Self(self.0 * rhs)
    }
}

impl std::ops::Neg for Vec3 {
    type Output = Self;
    #[inline]
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Default for Vec3 {
    fn default() -> Self {
        Self::ZERO
    }
}

impl From<GVec3> for Vec3 {
    fn from(v: GVec3) -> Self {
        Self(v)
    }
}

impl From<Vec3> for GVec3 {
    fn from(v: Vec3) -> Self {
        v.0
    }
}

// ---------------------------------------------------------------------------
// Vec4
// ---------------------------------------------------------------------------

/// 4D vector.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec4(pub GVec4);

impl Vec4 {
    pub const ZERO: Self = Self(GVec4::ZERO);

    #[inline]
    #[must_use]
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self(GVec4::new(x, y, z, w))
    }

    #[inline]
    #[must_use]
    pub fn x(self) -> f32 {
        self.0.x
    }

    #[inline]
    #[must_use]
    pub fn y(self) -> f32 {
        self.0.y
    }

    #[inline]
    #[must_use]
    pub fn z(self) -> f32 {
        self.0.z
    }

    #[inline]
    #[must_use]
    pub fn w(self) -> f32 {
        self.0.w
    }
}

impl Default for Vec4 {
    fn default() -> Self {
        Self::ZERO
    }
}

// ---------------------------------------------------------------------------
// Mat4
// ---------------------------------------------------------------------------

/// 4x4 matrix.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mat4(pub GMat4);

impl Mat4 {
    pub const IDENTITY: Self = Self(GMat4::IDENTITY);

    #[inline]
    #[must_use]
    pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        Self(GMat4::perspective_rh(fov_y, aspect, near, far))
    }

    #[inline]
    #[must_use]
    pub fn look_at(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        Self(GMat4::look_at_rh(eye.0, target.0, up.0))
    }

    #[inline]
    #[must_use]
    pub fn orthographic(width: f32, height: f32, near: f32, far: f32) -> Self {
        Self(GMat4::orthographic_rh(
            -width * 0.5,
            width * 0.5,
            -height * 0.5,
            height * 0.5,
            near,
            far,
        ))
    }

    #[inline]
    #[must_use]
    pub fn from_translation(t: Vec3) -> Self {
        Self(GMat4::from_translation(t.0))
    }

    #[inline]
    #[must_use]
    pub fn from_rotation(q: Quat) -> Self {
        Self(GMat4::from_quat(q.0))
    }

    #[inline]
    #[must_use]
    pub fn from_scale(s: Vec3) -> Self {
        Self(GMat4::from_scale(s.0))
    }

    #[inline]
    #[must_use]
    pub fn from_trs(translation: Vec3, rotation: Quat, scale: Vec3) -> Self {
        Self(GMat4::from_scale_rotation_translation(
            scale.0,
            rotation.0,
            translation.0,
        ))
    }

    #[inline]
    #[must_use]
    pub fn inverse(self) -> Self {
        Self(self.0.inverse())
    }

    #[inline]
    #[must_use]
    pub fn transform_point3(self, p: Vec3) -> Vec3 {
        Vec3(self.0.transform_point3(p.0))
    }

    #[inline]
    #[must_use]
    pub fn transform_vector3(self, v: Vec3) -> Vec3 {
        Vec3(self.0.transform_vector3(v.0))
    }
}

impl Mul for Mat4 {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self(self.0 * rhs.0)
    }
}

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

// ---------------------------------------------------------------------------
// Quat
// ---------------------------------------------------------------------------

/// Quaternion rotation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Quat(pub GQuat);

impl Quat {
    pub const IDENTITY: Self = Self(GQuat::IDENTITY);

    #[inline]
    #[must_use]
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Self {
        Self(GQuat::from_axis_angle(axis.0, angle))
    }

    #[inline]
    #[must_use]
    pub fn from_euler(yaw: f32, pitch: f32, roll: f32) -> Self {
        Self(GQuat::from_euler(glam::EulerRot::YXZ, yaw, pitch, roll))
    }

    #[inline]
    #[must_use]
    pub fn slerp(self, other: Self, t: f32) -> Self {
        Self(self.0.slerp(other.0, t))
    }

    #[inline]
    #[must_use]
    pub fn inverse(self) -> Self {
        Self(self.0.inverse())
    }

    #[inline]
    #[must_use]
    pub fn normalize(self) -> Self {
        Self(self.0.normalize())
    }
}

impl Mul for Quat {
    type Output = Self;
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        Self(self.0 * rhs.0)
    }
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

/// Linear RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const RED: Self = Self {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };
    pub const GREEN: Self = Self {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };
    pub const BLUE: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    #[inline]
    #[must_use]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// sRGB byte [0..255] to linear float.
    #[inline]
    #[must_use]
    pub fn from_srgb_u8(r: u8, g: u8, b: u8, a: u8) -> Self {
        const INV: f32 = 1.0 / 255.0;
        Self {
            r: srgb_to_linear(f32::from(r) * INV),
            g: srgb_to_linear(f32::from(g) * INV),
            b: srgb_to_linear(f32::from(b) * INV),
            a: f32::from(a) * INV,
        }
    }

    #[inline]
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        let inv = 1.0 - t;
        Self {
            r: self.r.mul_add(inv, other.r * t),
            g: self.g.mul_add(inv, other.g * t),
            b: self.b.mul_add(inv, other.b * t),
            a: self.a.mul_add(inv, other.a * t),
        }
    }

    #[inline]
    #[must_use]
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::WHITE
    }
}

/// sRGB gamma to linear.
#[inline]
#[must_use]
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c * (1.0 / 12.92)
    } else {
        ((c + 0.055) * (1.0 / 1.055)).powf(2.4)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vec2_basic() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, 4.0);
        let c = a + b;
        assert_eq!(c.x(), 4.0);
        assert_eq!(c.y(), 6.0);
    }

    #[test]
    fn vec2_length() {
        let v = Vec2::new(3.0, 4.0);
        assert!((v.length() - 5.0).abs() < 1e-6);
    }

    #[test]
    fn vec2_normalize() {
        let v = Vec2::new(0.0, 5.0).normalize();
        assert!((v.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn vec2_dot() {
        let a = Vec2::new(1.0, 0.0);
        let b = Vec2::new(0.0, 1.0);
        assert!((a.dot(b)).abs() < 1e-6);
    }

    #[test]
    fn vec2_lerp() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 10.0);
        let mid = a.lerp(b, 0.5);
        assert!((mid.x() - 5.0).abs() < 1e-6);
    }

    #[test]
    fn vec2_sub_mul() {
        let v = Vec2::new(2.0, 3.0) - Vec2::new(1.0, 1.0);
        let scaled = v * 2.0;
        assert_eq!(scaled.x(), 2.0);
        assert_eq!(scaled.y(), 4.0);
    }

    #[test]
    fn vec2_default() {
        assert_eq!(Vec2::default(), Vec2::ZERO);
    }

    #[test]
    fn vec2_from_glam() {
        let g = GVec2::new(1.0, 2.0);
        let v: Vec2 = g.into();
        let back: GVec2 = v.into();
        assert_eq!(g, back);
    }

    #[test]
    fn vec3_basic() {
        let a = Vec3::new(1.0, 2.0, 3.0);
        let b = Vec3::new(4.0, 5.0, 6.0);
        let c = a + b;
        assert_eq!(c.x(), 5.0);
        assert_eq!(c.y(), 7.0);
        assert_eq!(c.z(), 9.0);
    }

    #[test]
    fn vec3_cross() {
        let x = Vec3::X;
        let y = Vec3::Y;
        let z = x.cross(y);
        assert!((z.x()).abs() < 1e-6);
        assert!((z.y()).abs() < 1e-6);
        assert!((z.z() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn vec3_distance() {
        let a = Vec3::new(0.0, 0.0, 0.0);
        let b = Vec3::new(3.0, 4.0, 0.0);
        assert!((a.distance(b) - 5.0).abs() < 1e-6);
    }

    #[test]
    fn vec3_normalize() {
        let v = Vec3::new(1.0, 1.0, 1.0).normalize();
        assert!((v.length() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn vec3_default() {
        assert_eq!(Vec3::default(), Vec3::ZERO);
    }

    #[test]
    fn vec4_basic() {
        let v = Vec4::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(v.x(), 1.0);
        assert_eq!(v.w(), 4.0);
    }

    #[test]
    fn mat4_identity() {
        let m = Mat4::IDENTITY;
        let p = Vec3::new(1.0, 2.0, 3.0);
        let q = m.transform_point3(p);
        assert!((q.x() - 1.0).abs() < 1e-6);
        assert!((q.y() - 2.0).abs() < 1e-6);
        assert!((q.z() - 3.0).abs() < 1e-6);
    }

    #[test]
    fn mat4_translation() {
        let m = Mat4::from_translation(Vec3::new(10.0, 0.0, 0.0));
        let p = m.transform_point3(Vec3::ZERO);
        assert!((p.x() - 10.0).abs() < 1e-6);
    }

    #[test]
    fn mat4_inverse() {
        let m = Mat4::from_translation(Vec3::new(5.0, 3.0, 1.0));
        let inv = m.inverse();
        let result = m * inv;
        let p = result.transform_point3(Vec3::new(1.0, 2.0, 3.0));
        assert!((p.x() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn mat4_trs() {
        let m = Mat4::from_trs(Vec3::new(1.0, 0.0, 0.0), Quat::IDENTITY, Vec3::ONE);
        let p = m.transform_point3(Vec3::ZERO);
        assert!((p.x() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn mat4_perspective() {
        let m = Mat4::perspective(std::f32::consts::FRAC_PI_4, 1.0, 0.1, 100.0);
        assert_ne!(m, Mat4::IDENTITY);
    }

    #[test]
    fn mat4_look_at() {
        let m = Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        assert_ne!(m, Mat4::IDENTITY);
    }

    #[test]
    fn mat4_orthographic() {
        let m = Mat4::orthographic(20.0, 15.0, 0.1, 100.0);
        assert_ne!(m, Mat4::IDENTITY);
        let p = m.transform_point3(Vec3::ZERO);
        assert!(p.z().is_finite());
    }

    #[test]
    fn quat_identity() {
        let q = Quat::IDENTITY;
        assert_eq!(q, Quat::default());
    }

    #[test]
    fn quat_axis_angle() {
        let q = Quat::from_axis_angle(Vec3::Y, std::f32::consts::PI);
        let q2 = q.inverse();
        let product = q * q2;
        let diff = (product.0.x.powi(2) + product.0.y.powi(2) + product.0.z.powi(2)).sqrt();
        assert!(diff < 1e-5);
    }

    #[test]
    fn quat_slerp() {
        let a = Quat::IDENTITY;
        let b = Quat::from_axis_angle(Vec3::Y, std::f32::consts::PI);
        let mid = a.slerp(b, 0.5);
        let n = mid.normalize();
        let len = (n.0.x.powi(2) + n.0.y.powi(2) + n.0.z.powi(2) + n.0.w.powi(2)).sqrt();
        assert!((len - 1.0).abs() < 1e-6);
    }

    #[test]
    fn color_white() {
        let c = Color::WHITE;
        assert_eq!(c.to_array(), [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn color_from_srgb() {
        let c = Color::from_srgb_u8(255, 0, 0, 255);
        assert!((c.r - 1.0).abs() < 1e-3);
        assert!(c.g < 1e-6);
    }

    #[test]
    fn color_lerp() {
        let a = Color::BLACK;
        let b = Color::WHITE;
        let mid = a.lerp(b, 0.5);
        assert!((mid.r - 0.5).abs() < 1e-6);
        assert!((mid.g - 0.5).abs() < 1e-6);
    }

    #[test]
    fn color_transparent() {
        let c = Color::TRANSPARENT;
        assert_eq!(c.a, 0.0);
    }

    #[test]
    fn srgb_to_linear_low() {
        let v = srgb_to_linear(0.01);
        assert!(v < 0.01);
    }

    #[test]
    fn srgb_to_linear_high() {
        let v = srgb_to_linear(1.0);
        assert!((v - 1.0).abs() < 1e-6);
    }
}
