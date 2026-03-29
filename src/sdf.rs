//! SDF hybrid rendering: evaluate SDF volumes alongside polygon meshes.
//!
//! This module provides the bridge between ALICE-SDF's node tree evaluation
//! and the game engine's scene graph. SDF volumes can be:
//! - Raymarched directly in a compute/fragment shader
//! - Meshed via marching cubes for physics/rendering
//! - Used as collision volumes (SDF CCD)

use crate::math::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SdfPrimitive — built-in primitives for the engine
// ---------------------------------------------------------------------------

/// Basic SDF primitives that can be evaluated without ALICE-SDF dependency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SdfPrimitive {
    Sphere {
        radius: f32,
    },
    Box {
        half_extents: Vec3,
    },
    Capsule {
        radius: f32,
        height: f32,
    },
    Cylinder {
        radius: f32,
        height: f32,
    },
    Torus {
        major_radius: f32,
        minor_radius: f32,
    },
    Plane {
        normal: Vec3,
        offset: f32,
    },
    Cone {
        radius: f32,
        height: f32,
    },
}

impl SdfPrimitive {
    /// Evaluate the signed distance at point `p` (centered at origin).
    #[inline]
    #[must_use]
    pub fn eval(&self, p: Vec3) -> f32 {
        match self {
            Self::Sphere { radius } => p.length() - radius,

            Self::Box { half_extents } => {
                let qx = p.x().abs() - half_extents.x();
                let qy = p.y().abs() - half_extents.y();
                let qz = p.z().abs() - half_extents.z();
                let outside = Vec3::new(qx.max(0.0), qy.max(0.0), qz.max(0.0)).length();
                let inside = qx.max(qy.max(qz)).min(0.0);
                outside + inside
            }

            Self::Capsule { radius, height } => {
                let half_h = height * 0.5;
                let py = p.y().clamp(-half_h, half_h);
                let nearest = Vec3::new(0.0, py, 0.0);
                (p - nearest).length() - radius
            }

            Self::Cylinder { radius, height } => {
                let half_h = height * 0.5;
                let dx = Vec3::new(p.x(), 0.0, p.z()).length() - radius;
                let dy = p.y().abs() - half_h;
                let outside = Vec3::new(dx.max(0.0), dy.max(0.0), 0.0).length();
                let inside = dx.max(dy).min(0.0);
                outside + inside
            }

            Self::Torus {
                major_radius,
                minor_radius,
            } => {
                let qx = Vec3::new(p.x(), 0.0, p.z()).length() - major_radius;
                let q = Vec3::new(qx, p.y(), 0.0);
                q.length() - minor_radius
            }

            Self::Plane { normal, offset } => p.dot(*normal) - offset,

            Self::Cone { radius, height } => {
                let q_len = Vec3::new(p.x(), 0.0, p.z()).length();
                let tip = Vec3::new(0.0, *height, 0.0);
                let base_edge = Vec3::new(*radius, 0.0, 0.0);
                let cone_dir = (base_edge - tip).normalize();
                let to_p = Vec3::new(q_len, p.y(), 0.0) - tip;
                let proj = to_p.dot(cone_dir).clamp(0.0, (base_edge - tip).length());
                let nearest = tip + cone_dir * proj;
                let dist = (Vec3::new(q_len, p.y(), 0.0) - nearest).length();
                let sign = if q_len * height - p.y() * radius < 0.0 && p.y() < *height {
                    -1.0
                } else {
                    1.0
                };
                dist * sign
            }
        }
    }

    /// Numerical gradient (normal estimation) via central differences.
    #[must_use]
    pub fn normal(&self, p: Vec3, eps: f32) -> Vec3 {
        let dx = self.eval(Vec3::new(p.x() + eps, p.y(), p.z()))
            - self.eval(Vec3::new(p.x() - eps, p.y(), p.z()));
        let dy = self.eval(Vec3::new(p.x(), p.y() + eps, p.z()))
            - self.eval(Vec3::new(p.x(), p.y() - eps, p.z()));
        let dz = self.eval(Vec3::new(p.x(), p.y(), p.z() + eps))
            - self.eval(Vec3::new(p.x(), p.y(), p.z() - eps));
        Vec3::new(dx, dy, dz).normalize()
    }
}

// ---------------------------------------------------------------------------
// SdfOp — boolean operations
// ---------------------------------------------------------------------------

/// Boolean operations on SDF volumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SdfOp {
    Union,
    Intersection,
    Subtraction,
    SmoothUnion,
    SmoothIntersection,
    SmoothSubtraction,
}

/// Applies a boolean operation to two distance values.
#[inline]
#[must_use]
pub fn apply_op(op: SdfOp, a: f32, b: f32, k: f32) -> f32 {
    match op {
        SdfOp::Union => a.min(b),
        SdfOp::Intersection => a.max(b),
        SdfOp::Subtraction => a.max(-b),
        SdfOp::SmoothUnion => smooth_min(a, b, k),
        SdfOp::SmoothIntersection => -smooth_min(-a, -b, k),
        SdfOp::SmoothSubtraction => -smooth_min(-a, b, k),
    }
}

/// Polynomial smooth minimum.
#[inline]
#[must_use]
fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    if k < 1e-6 {
        return a.min(b);
    }
    let h = (0.5 * (b - a)).mul_add(k.recip(), 0.5).clamp(0.0, 1.0);
    (k * h).mul_add(-(1.0 - h), b.mul_add(1.0 - h, a * h))
}

// ---------------------------------------------------------------------------
// SdfNode — tree for the engine (lightweight, not full ALICE-SDF)
// ---------------------------------------------------------------------------

/// Lightweight SDF tree node for in-engine use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SdfNode {
    Primitive(SdfPrimitive),
    Transform {
        translation: Vec3,
        child: Box<Self>,
    },
    /// Full TRS transform.
    FullTransform {
        translation: Vec3,
        rotation: [f32; 4],
        scale: Vec3,
        child: Box<Self>,
    },
    Operation {
        op: SdfOp,
        k: f32,
        children: Vec<Self>,
    },
}

impl SdfNode {
    /// Evaluate the SDF at point `p`.
    #[must_use]
    pub fn eval(&self, p: Vec3) -> f32 {
        match self {
            Self::Primitive(prim) => prim.eval(p),
            Self::Transform { translation, child } => child.eval(p - *translation),
            Self::FullTransform {
                translation,
                rotation,
                scale,
                child,
            } => {
                let q = glam::Quat::from_array(*rotation).inverse();
                let local = p - *translation;
                let rotated = Vec3::from(q.mul_vec3(local.into()));
                let scaled = Vec3::new(
                    rotated.x() * scale.x().max(1e-10).recip(),
                    rotated.y() * scale.y().max(1e-10).recip(),
                    rotated.z() * scale.z().max(1e-10).recip(),
                );
                child.eval(scaled) * scale.x().min(scale.y().min(scale.z()))
            }
            Self::Operation { op, k, children } => {
                if children.is_empty() {
                    return f32::MAX;
                }
                let mut d = children[0].eval(p);
                for child in &children[1..] {
                    d = apply_op(*op, d, child.eval(p), *k);
                }
                d
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sphere trace (raymarching)
// ---------------------------------------------------------------------------

/// Result of a sphere trace.
#[derive(Debug, Clone, Copy)]
pub struct RayHit {
    pub distance: f32,
    pub position: Vec3,
    pub steps: u32,
}

/// Sphere-traces a ray against an SDF node.
#[must_use]
pub fn sphere_trace(
    node: &SdfNode,
    ray_origin: Vec3,
    ray_dir: Vec3,
    max_steps: u32,
    max_distance: f32,
    epsilon: f32,
) -> Option<RayHit> {
    let mut t = 0.0_f32;
    for step in 0..max_steps {
        let p = ray_origin + ray_dir * t;
        let d = node.eval(p);
        if d < epsilon {
            return Some(RayHit {
                distance: t,
                position: p,
                steps: step + 1,
            });
        }
        t += d;
        if t > max_distance {
            return None;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// SDF Collider — distance-based collision
// ---------------------------------------------------------------------------

/// Contact point from SDF collision detection.
#[derive(Debug, Clone, Copy)]
pub struct SdfContact {
    pub point: Vec3,
    pub normal: Vec3,
    pub penetration: f32,
}

/// Tests a sphere against an SDF and returns the deepest contact.
#[must_use]
pub fn sdf_sphere_test(
    node: &SdfNode,
    sphere_center: Vec3,
    sphere_radius: f32,
) -> Option<SdfContact> {
    let d = node.eval(sphere_center);
    let penetration = sphere_radius - d;
    if penetration <= 0.0 {
        return None;
    }
    let eps = 0.001_f32;
    let nx = node.eval(Vec3::new(
        sphere_center.x() + eps,
        sphere_center.y(),
        sphere_center.z(),
    )) - node.eval(Vec3::new(
        sphere_center.x() - eps,
        sphere_center.y(),
        sphere_center.z(),
    ));
    let ny = node.eval(Vec3::new(
        sphere_center.x(),
        sphere_center.y() + eps,
        sphere_center.z(),
    )) - node.eval(Vec3::new(
        sphere_center.x(),
        sphere_center.y() - eps,
        sphere_center.z(),
    ));
    let nz = node.eval(Vec3::new(
        sphere_center.x(),
        sphere_center.y(),
        sphere_center.z() + eps,
    )) - node.eval(Vec3::new(
        sphere_center.x(),
        sphere_center.y(),
        sphere_center.z() - eps,
    ));
    let normal = Vec3::new(nx, ny, nz).normalize();
    Some(SdfContact {
        point: sphere_center - normal * d,
        normal,
        penetration,
    })
}

// ---------------------------------------------------------------------------
// Marching Cubes — SDF → triangle mesh
// ---------------------------------------------------------------------------

/// A triangle vertex with position and normal.
#[derive(Debug, Clone, Copy)]
pub struct MeshVertex {
    pub position: Vec3,
    pub normal: Vec3,
}

/// Result of marching cubes meshing.
#[derive(Debug, Clone)]
pub struct SdfMesh {
    pub vertices: Vec<MeshVertex>,
    pub indices: Vec<u32>,
}

impl SdfMesh {
    #[must_use]
    pub const fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    #[must_use]
    pub const fn vertex_count(&self) -> usize {
        self.vertices.len()
    }
}

/// Edge table: maps each of the 256 cube configurations to the set of
/// edges (bitmask) that are intersected by the isosurface.
#[rustfmt::skip]
const EDGE_TABLE: [u16; 256] = [
    0x000,0x109,0x203,0x30a,0x406,0x50f,0x605,0x70c,0x80c,0x905,0xa0f,0xb06,0xc0a,0xd03,0xe09,0xf00,
    0x190,0x099,0x393,0x29a,0x596,0x49f,0x795,0x69c,0x99c,0x895,0xb9f,0xa96,0xd9a,0xc93,0xf99,0xe90,
    0x230,0x339,0x033,0x13a,0x636,0x73f,0x435,0x53c,0xa3c,0xb35,0x83f,0x936,0xe3a,0xf33,0xc39,0xd30,
    0x3a0,0x2a9,0x1a3,0x0aa,0x7a6,0x6af,0x5a5,0x4ac,0xbac,0xaa5,0x9af,0x8a6,0xfaa,0xea3,0xda9,0xca0,
    0x460,0x569,0x663,0x76a,0x066,0x16f,0x265,0x36c,0xc6c,0xd65,0xe6f,0xf66,0x86a,0x963,0xa69,0xb60,
    0x5f0,0x4f9,0x7f3,0x6fa,0x1f6,0x0ff,0x3f5,0x2fc,0xdfc,0xcf5,0xfff,0xef6,0x9fa,0x8f3,0xbf9,0xaf0,
    0x650,0x759,0x453,0x55a,0x256,0x35f,0x055,0x15c,0xe5c,0xf55,0xc5f,0xd56,0xa5a,0xb53,0x859,0x950,
    0x7c0,0x6c9,0x5c3,0x4ca,0x3c6,0x2cf,0x1c5,0x0cc,0xfcc,0xec5,0xdcf,0xcc6,0xbca,0xac3,0x9c9,0x8c0,
    0x8c0,0x9c9,0xac3,0xbca,0xcc6,0xdcf,0xec5,0xfcc,0x0cc,0x1c5,0x2cf,0x3c6,0x4ca,0x5c3,0x6c9,0x7c0,
    0x950,0x859,0xb53,0xa5a,0xd56,0xc5f,0xf55,0xe5c,0x15c,0x055,0x35f,0x256,0x55a,0x453,0x759,0x650,
    0xaf0,0xbf9,0x8f3,0x9fa,0xef6,0xfff,0xcf5,0xdfc,0x2fc,0x3f5,0x0ff,0x1f6,0x6fa,0x7f3,0x4f9,0x5f0,
    0xb60,0xa69,0x963,0x86a,0xf66,0xe6f,0xd65,0xc6c,0x36c,0x265,0x16f,0x066,0x76a,0x663,0x569,0x460,
    0xca0,0xda9,0xea3,0xfaa,0x8a6,0x9af,0xaa5,0xbac,0x4ac,0x5a5,0x6af,0x7a6,0x0aa,0x1a3,0x2a9,0x3a0,
    0xd30,0xc39,0xf33,0xe3a,0x936,0x83f,0xb35,0xa3c,0x53c,0x435,0x73f,0x636,0x13a,0x033,0x339,0x230,
    0xe90,0xf99,0xc93,0xd9a,0xa96,0xb9f,0x895,0x99c,0x69c,0x795,0x49f,0x596,0x29a,0x393,0x099,0x190,
    0xf00,0xe09,0xd03,0xc0a,0xb06,0xa0f,0x905,0x80c,0x70c,0x605,0x50f,0x406,0x30a,0x203,0x109,0x000,
];

/// Triangle table: for each of the 256 configurations, lists edge indices
/// forming triangles (terminated by -1).
#[rustfmt::skip]
const TRI_TABLE: [[i8; 16]; 256] = [
    [-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 8, 3,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 1, 9,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 8, 3, 9, 8, 1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 2,10,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 8, 3, 1, 2,10,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 2,10, 0, 2, 9,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 2, 8, 3, 2,10, 8,10, 9, 8,-1,-1,-1,-1,-1,-1,-1],
    [ 3,11, 2,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0,11, 2, 8,11, 0,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 9, 0, 2, 3,11,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 1,11, 2, 1, 9,11, 9, 8,11,-1,-1,-1,-1,-1,-1,-1],
    [ 3,10, 1,11,10, 3,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0,10, 1, 0, 8,10, 8,11,10,-1,-1,-1,-1,-1,-1,-1],
    [ 3, 9, 0, 3,11, 9,11,10, 9,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 8,10,10, 8,11,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 4, 7, 8,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 4, 3, 0, 7, 3, 4,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 1, 9, 8, 4, 7,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 4, 1, 9, 4, 7, 1, 7, 3, 1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 2,10, 8, 4, 7,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 3, 4, 7, 3, 0, 4, 1, 2,10,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 2,10, 9, 0, 2, 8, 4, 7,-1,-1,-1,-1,-1,-1,-1],
    [ 2,10, 9, 2, 9, 7, 2, 7, 3, 7, 9, 4,-1,-1,-1,-1],
    [ 8, 4, 7, 3,11, 2,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [11, 4, 7,11, 2, 4, 2, 0, 4,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 0, 1, 8, 4, 7, 2, 3,11,-1,-1,-1,-1,-1,-1,-1],
    [ 4, 7,11, 9, 4,11, 9,11, 2, 9, 2, 1,-1,-1,-1,-1],
    [ 3,10, 1, 3,11,10, 7, 8, 4,-1,-1,-1,-1,-1,-1,-1],
    [ 1,11,10, 1, 4,11, 1, 0, 4, 7,11, 4,-1,-1,-1,-1],
    [ 4, 7, 8, 9, 0,11, 9,11,10,11, 0, 3,-1,-1,-1,-1],
    [ 4, 7,11, 4,11, 9, 9,11,10,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 5, 4,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 5, 4, 0, 8, 3,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 5, 4, 1, 5, 0,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 8, 5, 4, 8, 3, 5, 3, 1, 5,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 2,10, 9, 5, 4,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 3, 0, 8, 1, 2,10, 4, 9, 5,-1,-1,-1,-1,-1,-1,-1],
    [ 5, 2,10, 5, 4, 2, 4, 0, 2,-1,-1,-1,-1,-1,-1,-1],
    [ 2,10, 5, 3, 2, 5, 3, 5, 4, 3, 4, 8,-1,-1,-1,-1],
    [ 9, 5, 4, 2, 3,11,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0,11, 2, 0, 8,11, 4, 9, 5,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 5, 4, 0, 1, 5, 2, 3,11,-1,-1,-1,-1,-1,-1,-1],
    [ 2, 1, 5, 2, 5, 8, 2, 8,11, 4, 8, 5,-1,-1,-1,-1],
    [10, 3,11,10, 1, 3, 9, 5, 4,-1,-1,-1,-1,-1,-1,-1],
    [ 4, 9, 5, 0, 8, 1, 8,10, 1, 8,11,10,-1,-1,-1,-1],
    [ 5, 4, 0, 5, 0,11, 5,11,10,11, 0, 3,-1,-1,-1,-1],
    [ 5, 4, 8, 5, 8,10,10, 8,11,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 7, 8, 5, 7, 9,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 3, 0, 9, 5, 3, 5, 7, 3,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 7, 8, 0, 1, 7, 1, 5, 7,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 5, 3, 3, 5, 7,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 7, 8, 9, 5, 7,10, 1, 2,-1,-1,-1,-1,-1,-1,-1],
    [10, 1, 2, 9, 5, 0, 5, 3, 0, 5, 7, 3,-1,-1,-1,-1],
    [ 8, 0, 2, 8, 2, 5, 8, 5, 7,10, 5, 2,-1,-1,-1,-1],
    [ 2,10, 5, 2, 5, 3, 3, 5, 7,-1,-1,-1,-1,-1,-1,-1],
    [ 7, 9, 5, 7, 8, 9, 3,11, 2,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 5, 7, 9, 7, 2, 9, 2, 0, 2, 7,11,-1,-1,-1,-1],
    [ 2, 3,11, 0, 1, 8, 1, 7, 8, 1, 5, 7,-1,-1,-1,-1],
    [11, 2, 1,11, 1, 7, 7, 1, 5,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 5, 8, 8, 5, 7,10, 1, 3,10, 3,11,-1,-1,-1,-1],
    [ 5, 7, 0, 5, 0, 9, 7,11, 0, 1, 0,10,11,10, 0,-1],
    [11,10, 0,11, 0, 3,10, 5, 0, 8, 0, 7, 5, 7, 0,-1],
    [11,10, 5, 7,11, 5,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [10, 6, 5,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 8, 3, 5,10, 6,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 0, 1, 5,10, 6,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 8, 3, 1, 9, 8, 5,10, 6,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 6, 5, 2, 6, 1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 6, 5, 1, 2, 6, 3, 0, 8,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 6, 5, 9, 0, 6, 0, 2, 6,-1,-1,-1,-1,-1,-1,-1],
    [ 5, 9, 8, 5, 8, 2, 5, 2, 6, 3, 2, 8,-1,-1,-1,-1],
    [ 2, 3,11,10, 6, 5,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [11, 0, 8,11, 2, 0,10, 6, 5,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 1, 9, 2, 3,11, 5,10, 6,-1,-1,-1,-1,-1,-1,-1],
    [ 5,10, 6, 1, 9, 2, 9,11, 2, 9, 8,11,-1,-1,-1,-1],
    [ 6, 3,11, 6, 5, 3, 5, 1, 3,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 8,11, 0,11, 5, 0, 5, 1, 5,11, 6,-1,-1,-1,-1],
    [ 3,11, 6, 0, 3, 6, 0, 6, 5, 0, 5, 9,-1,-1,-1,-1],
    [ 6, 5, 9, 6, 9,11,11, 9, 8,-1,-1,-1,-1,-1,-1,-1],
    [ 5,10, 6, 4, 7, 8,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 4, 3, 0, 4, 7, 3, 6, 5,10,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 9, 0, 5,10, 6, 8, 4, 7,-1,-1,-1,-1,-1,-1,-1],
    [10, 6, 5, 1, 9, 7, 1, 7, 3, 7, 9, 4,-1,-1,-1,-1],
    [ 6, 1, 2, 6, 5, 1, 4, 7, 8,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 2, 5, 5, 2, 6, 3, 0, 4, 3, 4, 7,-1,-1,-1,-1],
    [ 8, 4, 7, 9, 0, 5, 0, 6, 5, 0, 2, 6,-1,-1,-1,-1],
    [ 7, 3, 9, 7, 9, 4, 3, 2, 9, 5, 9, 6, 2, 6, 9,-1],
    [ 3,11, 2, 7, 8, 4,10, 6, 5,-1,-1,-1,-1,-1,-1,-1],
    [ 5,10, 6, 4, 7, 2, 4, 2, 0, 2, 7,11,-1,-1,-1,-1],
    [ 0, 1, 9, 4, 7, 8, 2, 3,11, 5,10, 6,-1,-1,-1,-1],
    [ 9, 2, 1, 9,11, 2, 9, 4,11, 7,11, 4, 5,10, 6,-1],
    [ 8, 4, 7, 3,11, 5, 3, 5, 1, 5,11, 6,-1,-1,-1,-1],
    [ 5, 1,11, 5,11, 6, 1, 0,11, 7,11, 4, 0, 4,11,-1],
    [ 0, 5, 9, 0, 6, 5, 0, 3, 6,11, 6, 3, 8, 4, 7,-1],
    [ 6, 5, 9, 6, 9,11, 4, 7, 9, 7,11, 9,-1,-1,-1,-1],
    [10, 4, 9, 6, 4,10,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 4,10, 6, 4, 9,10, 0, 8, 3,-1,-1,-1,-1,-1,-1,-1],
    [10, 0, 1,10, 6, 0, 6, 4, 0,-1,-1,-1,-1,-1,-1,-1],
    [ 8, 3, 1, 8, 1, 6, 8, 6, 4, 6, 1,10,-1,-1,-1,-1],
    [ 1, 4, 9, 1, 2, 4, 2, 6, 4,-1,-1,-1,-1,-1,-1,-1],
    [ 3, 0, 8, 1, 2, 9, 2, 4, 9, 2, 6, 4,-1,-1,-1,-1],
    [ 0, 2, 4, 4, 2, 6,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 8, 3, 2, 8, 2, 4, 4, 2, 6,-1,-1,-1,-1,-1,-1,-1],
    [10, 4, 9,10, 6, 4,11, 2, 3,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 8, 2, 2, 8,11, 4, 9,10, 4,10, 6,-1,-1,-1,-1],
    [ 3,11, 2, 0, 1, 6, 0, 6, 4, 6, 1,10,-1,-1,-1,-1],
    [ 6, 4, 1, 6, 1,10, 4, 8, 1, 2, 1,11, 8,11, 1,-1],
    [ 9, 6, 4, 9, 3, 6, 9, 1, 3,11, 6, 3,-1,-1,-1,-1],
    [ 8,11, 1, 8, 1, 0,11, 6, 1, 9, 1, 4, 6, 4, 1,-1],
    [ 3,11, 6, 3, 6, 0, 0, 6, 4,-1,-1,-1,-1,-1,-1,-1],
    [ 6, 4, 8,11, 6, 8,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 7,10, 6, 7, 8,10, 8, 9,10,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 7, 3, 0,10, 7, 0, 9,10, 6, 7,10,-1,-1,-1,-1],
    [10, 6, 7, 1,10, 7, 1, 7, 8, 1, 8, 0,-1,-1,-1,-1],
    [10, 6, 7,10, 7, 1, 1, 7, 3,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 2, 6, 1, 6, 8, 1, 8, 9, 8, 6, 7,-1,-1,-1,-1],
    [ 2, 6, 9, 2, 9, 1, 6, 7, 9, 0, 9, 3, 7, 3, 9,-1],
    [ 7, 8, 0, 7, 0, 6, 6, 0, 2,-1,-1,-1,-1,-1,-1,-1],
    [ 7, 3, 2, 6, 7, 2,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 2, 3,11,10, 6, 8,10, 8, 9, 8, 6, 7,-1,-1,-1,-1],
    [ 2, 0, 7, 2, 7,11, 0, 9, 7, 6, 7,10, 9,10, 7,-1],
    [ 1, 8, 0, 1, 7, 8, 1,10, 7, 6, 7,10, 2, 3,11,-1],
    [11, 2, 1,11, 1, 7,10, 6, 1, 6, 7, 1,-1,-1,-1,-1],
    [ 8, 9, 6, 8, 6, 7, 9, 1, 6,11, 6, 3, 1, 3, 6,-1],
    [ 0, 9, 1,11, 6, 7,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 7, 8, 0, 7, 0, 6, 3,11, 0,11, 6, 0,-1,-1,-1,-1],
    [ 7,11, 6,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 7, 6,11,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 3, 0, 8,11, 7, 6,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 1, 9,11, 7, 6,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 8, 1, 9, 8, 3, 1,11, 7, 6,-1,-1,-1,-1,-1,-1,-1],
    [10, 1, 2, 6,11, 7,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 2,10, 3, 0, 8, 6,11, 7,-1,-1,-1,-1,-1,-1,-1],
    [ 2, 9, 0, 2,10, 9, 6,11, 7,-1,-1,-1,-1,-1,-1,-1],
    [ 6,11, 7, 2,10, 3,10, 8, 3,10, 9, 8,-1,-1,-1,-1],
    [ 7, 2, 3, 6, 2, 7,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 7, 0, 8, 7, 6, 0, 6, 2, 0,-1,-1,-1,-1,-1,-1,-1],
    [ 2, 7, 6, 2, 3, 7, 0, 1, 9,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 6, 2, 1, 8, 6, 1, 9, 8, 8, 7, 6,-1,-1,-1,-1],
    [10, 7, 6,10, 1, 7, 1, 3, 7,-1,-1,-1,-1,-1,-1,-1],
    [10, 7, 6, 1, 7,10, 1, 8, 7, 1, 0, 8,-1,-1,-1,-1],
    [ 0, 3, 7, 0, 7,10, 0,10, 9, 6,10, 7,-1,-1,-1,-1],
    [ 7, 6,10, 7,10, 8, 8,10, 9,-1,-1,-1,-1,-1,-1,-1],
    [ 6, 8, 4,11, 8, 6,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 3, 6,11, 3, 0, 6, 0, 4, 6,-1,-1,-1,-1,-1,-1,-1],
    [ 8, 6,11, 8, 4, 6, 9, 0, 1,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 4, 6, 9, 6, 3, 9, 3, 1,11, 3, 6,-1,-1,-1,-1],
    [ 6, 8, 4, 6,11, 8, 2,10, 1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 2,10, 3, 0,11, 0, 6,11, 0, 4, 6,-1,-1,-1,-1],
    [ 4,11, 8, 4, 6,11, 0, 2, 9, 2,10, 9,-1,-1,-1,-1],
    [10, 9, 3,10, 3, 2, 9, 4, 3,11, 3, 6, 4, 6, 3,-1],
    [ 8, 2, 3, 8, 4, 2, 4, 6, 2,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 4, 2, 4, 6, 2,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 9, 0, 2, 3, 4, 2, 4, 6, 4, 3, 8,-1,-1,-1,-1],
    [ 1, 9, 4, 1, 4, 2, 2, 4, 6,-1,-1,-1,-1,-1,-1,-1],
    [ 8, 1, 3, 8, 6, 1, 8, 4, 6, 6,10, 1,-1,-1,-1,-1],
    [10, 1, 0,10, 0, 6, 6, 0, 4,-1,-1,-1,-1,-1,-1,-1],
    [ 4, 6, 3, 4, 3, 8, 6,10, 3, 0, 3, 9,10, 9, 3,-1],
    [10, 9, 4, 6,10, 4,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 4, 9, 5, 7, 6,11,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 8, 3, 4, 9, 5,11, 7, 6,-1,-1,-1,-1,-1,-1,-1],
    [ 5, 0, 1, 5, 4, 0, 7, 6,11,-1,-1,-1,-1,-1,-1,-1],
    [11, 7, 6, 8, 3, 4, 3, 5, 4, 3, 1, 5,-1,-1,-1,-1],
    [ 9, 5, 4,10, 1, 2, 7, 6,11,-1,-1,-1,-1,-1,-1,-1],
    [ 6,11, 7, 1, 2,10, 0, 8, 3, 4, 9, 5,-1,-1,-1,-1],
    [ 7, 6,11, 5, 4,10, 4, 2,10, 4, 0, 2,-1,-1,-1,-1],
    [ 3, 4, 8, 3, 5, 4, 3, 2, 5,10, 5, 2,11, 7, 6,-1],
    [ 7, 2, 3, 7, 6, 2, 5, 4, 9,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 5, 4, 0, 8, 6, 0, 6, 2, 6, 8, 7,-1,-1,-1,-1],
    [ 3, 6, 2, 3, 7, 6, 1, 5, 0, 5, 4, 0,-1,-1,-1,-1],
    [ 6, 2, 8, 6, 8, 7, 2, 1, 8, 4, 8, 5, 1, 5, 8,-1],
    [ 9, 5, 4,10, 1, 6, 1, 7, 6, 1, 3, 7,-1,-1,-1,-1],
    [ 1, 6,10, 1, 7, 6, 1, 0, 7, 8, 7, 0, 9, 5, 4,-1],
    [ 4, 0,10, 4,10, 5, 0, 3,10, 6,10, 7, 3, 7,10,-1],
    [ 7, 6,10, 7,10, 8, 5, 4,10, 4, 8,10,-1,-1,-1,-1],
    [ 6, 9, 5, 6,11, 9,11, 8, 9,-1,-1,-1,-1,-1,-1,-1],
    [ 3, 6,11, 0, 6, 3, 0, 5, 6, 0, 9, 5,-1,-1,-1,-1],
    [ 0,11, 8, 0, 5,11, 0, 1, 5, 5, 6,11,-1,-1,-1,-1],
    [ 6,11, 3, 6, 3, 5, 5, 3, 1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 2,10, 9, 5,11, 9,11, 8,11, 5, 6,-1,-1,-1,-1],
    [ 0,11, 3, 0, 6,11, 0, 9, 6, 5, 6, 9, 1, 2,10,-1],
    [11, 8, 5,11, 5, 6, 8, 0, 5,10, 5, 2, 0, 2, 5,-1],
    [ 6,11, 3, 6, 3, 5, 2,10, 3,10, 5, 3,-1,-1,-1,-1],
    [ 5, 8, 9, 5, 2, 8, 5, 6, 2, 3, 8, 2,-1,-1,-1,-1],
    [ 9, 5, 6, 9, 6, 0, 0, 6, 2,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 5, 8, 1, 8, 0, 5, 6, 8, 3, 8, 2, 6, 2, 8,-1],
    [ 1, 5, 6, 2, 1, 6,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 3, 6, 1, 6,10, 3, 8, 6, 5, 6, 9, 8, 9, 6,-1],
    [10, 1, 0,10, 0, 6, 9, 5, 0, 5, 6, 0,-1,-1,-1,-1],
    [ 0, 3, 8, 5, 6,10,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [10, 5, 6,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [11, 5,10, 7, 5,11,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [11, 5,10,11, 7, 5, 8, 3, 0,-1,-1,-1,-1,-1,-1,-1],
    [ 5,11, 7, 5,10,11, 1, 9, 0,-1,-1,-1,-1,-1,-1,-1],
    [10, 7, 5,10,11, 7, 9, 8, 1, 8, 3, 1,-1,-1,-1,-1],
    [11, 1, 2,11, 7, 1, 7, 5, 1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 8, 3, 1, 2, 7, 1, 7, 5, 7, 2,11,-1,-1,-1,-1],
    [ 9, 7, 5, 9, 2, 7, 9, 0, 2, 2,11, 7,-1,-1,-1,-1],
    [ 7, 5, 2, 7, 2,11, 5, 9, 2, 3, 2, 8, 9, 8, 2,-1],
    [ 2, 5,10, 2, 3, 5, 3, 7, 5,-1,-1,-1,-1,-1,-1,-1],
    [ 8, 2, 0, 8, 5, 2, 8, 7, 5,10, 2, 5,-1,-1,-1,-1],
    [ 9, 0, 1, 5,10, 3, 5, 3, 7, 3,10, 2,-1,-1,-1,-1],
    [ 9, 8, 2, 9, 2, 1, 8, 7, 2,10, 2, 5, 7, 5, 2,-1],
    [ 1, 3, 5, 3, 7, 5,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 8, 7, 0, 7, 1, 1, 7, 5,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 0, 3, 9, 3, 5, 5, 3, 7,-1,-1,-1,-1,-1,-1,-1],
    [ 9, 8, 7, 5, 9, 7,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 5, 8, 4, 5,10, 8,10,11, 8,-1,-1,-1,-1,-1,-1,-1],
    [ 5, 0, 4, 5,11, 0, 5,10,11,11, 3, 0,-1,-1,-1,-1],
    [ 0, 1, 9, 8, 4,10, 8,10,11,10, 4, 5,-1,-1,-1,-1],
    [10,11, 4,10, 4, 5,11, 3, 4, 9, 4, 1, 3, 1, 4,-1],
    [ 2, 5, 1, 2, 8, 5, 2,11, 8, 4, 5, 8,-1,-1,-1,-1],
    [ 0, 4,11, 0,11, 3, 4, 5,11, 2,11, 1, 5, 1,11,-1],
    [ 0, 2, 5, 0, 5, 9, 2,11, 5, 4, 5, 8,11, 8, 5,-1],
    [ 9, 4, 5, 2,11, 3,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 2, 5,10, 3, 5, 2, 3, 4, 5, 3, 8, 4,-1,-1,-1,-1],
    [ 5,10, 2, 5, 2, 4, 4, 2, 0,-1,-1,-1,-1,-1,-1,-1],
    [ 3,10, 2, 3, 5,10, 3, 8, 5, 4, 5, 8, 0, 1, 9,-1],
    [ 5,10, 2, 5, 2, 4, 1, 9, 2, 9, 4, 2,-1,-1,-1,-1],
    [ 8, 4, 5, 8, 5, 3, 3, 5, 1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 4, 5, 1, 0, 5,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 8, 4, 5, 8, 5, 3, 9, 0, 5, 0, 3, 5,-1,-1,-1,-1],
    [ 9, 4, 5,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 4,11, 7, 4, 9,11, 9,10,11,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 8, 3, 4, 9, 7, 9,11, 7, 9,10,11,-1,-1,-1,-1],
    [ 1,10,11, 1,11, 4, 1, 4, 0, 7, 4,11,-1,-1,-1,-1],
    [ 3, 1, 4, 3, 4, 8, 1,10, 4, 7, 4,11,10,11, 4,-1],
    [ 4,11, 7, 9,11, 4, 9, 2,11, 9, 1, 2,-1,-1,-1,-1],
    [ 9, 7, 4, 9,11, 7, 9, 1,11, 2,11, 1, 0, 8, 3,-1],
    [11, 7, 4,11, 4, 2, 2, 4, 0,-1,-1,-1,-1,-1,-1,-1],
    [11, 7, 4,11, 4, 2, 8, 3, 4, 3, 2, 4,-1,-1,-1,-1],
    [ 2, 9,10, 2, 7, 9, 2, 3, 7, 7, 4, 9,-1,-1,-1,-1],
    [ 9,10, 7, 9, 7, 4,10, 2, 7, 8, 7, 0, 2, 0, 7,-1],
    [ 3, 7,10, 3,10, 2, 7, 4,10, 1,10, 0, 4, 0,10,-1],
    [ 1,10, 2, 8, 7, 4,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 4, 9, 1, 4, 1, 7, 7, 1, 3,-1,-1,-1,-1,-1,-1,-1],
    [ 4, 9, 1, 4, 1, 7, 0, 8, 1, 8, 7, 1,-1,-1,-1,-1],
    [ 4, 0, 3, 7, 4, 3,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 4, 8, 7,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 9,10, 8,10,11, 8,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 3, 0, 9, 3, 9,11,11, 9,10,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 1,10, 0,10, 8, 8,10,11,-1,-1,-1,-1,-1,-1,-1],
    [ 3, 1,10,11, 3,10,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 2,11, 1,11, 9, 9,11, 8,-1,-1,-1,-1,-1,-1,-1],
    [ 3, 0, 9, 3, 9,11, 1, 2, 9, 2,11, 9,-1,-1,-1,-1],
    [ 0, 2,11, 8, 0,11,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 3, 2,11,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 2, 3, 8, 2, 8,10,10, 8, 9,-1,-1,-1,-1,-1,-1,-1],
    [ 9,10, 2, 0, 9, 2,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 2, 3, 8, 2, 8,10, 0, 1, 8, 1,10, 8,-1,-1,-1,-1],
    [ 1,10, 2,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 1, 3, 8, 9, 1, 8,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 9, 1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [ 0, 3, 8,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
    [-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1],
];

/// 12 edges of a cube: pairs of corner indices.
const EDGE_VERTICES: [(usize, usize); 12] = [
    (0, 1),
    (1, 2),
    (2, 3),
    (3, 0),
    (4, 5),
    (5, 6),
    (6, 7),
    (7, 4),
    (0, 4),
    (1, 5),
    (2, 6),
    (3, 7),
];

/// Generates a triangle mesh from an SDF using the standard Marching Cubes
/// algorithm with the full 256-entry edge and triangle tables.
#[must_use]
pub fn marching_cubes(
    node: &SdfNode,
    min_bound: Vec3,
    max_bound: Vec3,
    resolution: u32,
) -> SdfMesh {
    let res = resolution.max(2);
    let size = max_bound - min_bound;
    let step = Vec3::new(
        size.x() / res as f32,
        size.y() / res as f32,
        size.z() / res as f32,
    );
    let eps = step.x().min(step.y().min(step.z())) * 0.1;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    for iz in 0..res {
        for iy in 0..res {
            for ix in 0..res {
                let x0 = (ix as f32).mul_add(step.x(), min_bound.x());
                let y0 = (iy as f32).mul_add(step.y(), min_bound.y());
                let z0 = (iz as f32).mul_add(step.z(), min_bound.z());

                let corners = [
                    Vec3::new(x0, y0, z0),
                    Vec3::new(x0 + step.x(), y0, z0),
                    Vec3::new(x0 + step.x(), y0 + step.y(), z0),
                    Vec3::new(x0, y0 + step.y(), z0),
                    Vec3::new(x0, y0, z0 + step.z()),
                    Vec3::new(x0 + step.x(), y0, z0 + step.z()),
                    Vec3::new(x0 + step.x(), y0 + step.y(), z0 + step.z()),
                    Vec3::new(x0, y0 + step.y(), z0 + step.z()),
                ];

                let values: [f32; 8] = std::array::from_fn(|i| node.eval(corners[i]));

                // Build cube index from corner signs
                let mut cube_index = 0u8;
                for (i, &v) in values.iter().enumerate() {
                    if v < 0.0 {
                        cube_index |= 1 << i;
                    }
                }

                let edge_mask = EDGE_TABLE[cube_index as usize];
                if edge_mask == 0 {
                    continue;
                }

                // Interpolate edge vertices
                let mut edge_verts = [Vec3::ZERO; 12];
                for (e, &(a, b)) in EDGE_VERTICES.iter().enumerate() {
                    if edge_mask & (1 << e) != 0 {
                        let t = values[a] / (values[a] - values[b]);
                        edge_verts[e] = corners[a].lerp(corners[b], t);
                    }
                }

                // Emit triangles from the triangle table
                let tri_row = &TRI_TABLE[cube_index as usize];
                let mut i = 0;
                while i < 16 && tri_row[i] >= 0 {
                    let base = vertices.len() as u32;
                    for k in 0..3 {
                        #[allow(clippy::cast_sign_loss)]
                        let edge_idx = tri_row[i + k] as usize;
                        let pos = edge_verts[edge_idx];
                        let nx = node.eval(Vec3::new(pos.x() + eps, pos.y(), pos.z()))
                            - node.eval(Vec3::new(pos.x() - eps, pos.y(), pos.z()));
                        let ny = node.eval(Vec3::new(pos.x(), pos.y() + eps, pos.z()))
                            - node.eval(Vec3::new(pos.x(), pos.y() - eps, pos.z()));
                        let nz = node.eval(Vec3::new(pos.x(), pos.y(), pos.z() + eps))
                            - node.eval(Vec3::new(pos.x(), pos.y(), pos.z() - eps));
                        vertices.push(MeshVertex {
                            position: pos,
                            normal: Vec3::new(nx, ny, nz).normalize(),
                        });
                    }
                    indices.push(base);
                    indices.push(base + 1);
                    indices.push(base + 2);
                    i += 3;
                }
            }
        }
    }

    SdfMesh { vertices, indices }
}

/// Parallel Marching Cubes using Rayon. Each Z-slice is processed
/// independently and results are merged.
#[must_use]
pub fn marching_cubes_parallel(
    node: &SdfNode,
    min_bound: Vec3,
    max_bound: Vec3,
    resolution: u32,
) -> SdfMesh {
    use rayon::prelude::*;

    let res = resolution.max(2);
    let size = max_bound - min_bound;
    let step = Vec3::new(
        size.x() / res as f32,
        size.y() / res as f32,
        size.z() / res as f32,
    );
    let eps = step.x().min(step.y().min(step.z())) * 0.1;

    let slices: Vec<(Vec<MeshVertex>, Vec<u32>)> = (0..res)
        .into_par_iter()
        .map(|iz| {
            let mut verts = Vec::new();
            let mut idxs = Vec::new();
            for iy in 0..res {
                for ix in 0..res {
                    let x0 = (ix as f32).mul_add(step.x(), min_bound.x());
                    let y0 = (iy as f32).mul_add(step.y(), min_bound.y());
                    let z0 = (iz as f32).mul_add(step.z(), min_bound.z());
                    let corners = [
                        Vec3::new(x0, y0, z0),
                        Vec3::new(x0 + step.x(), y0, z0),
                        Vec3::new(x0 + step.x(), y0 + step.y(), z0),
                        Vec3::new(x0, y0 + step.y(), z0),
                        Vec3::new(x0, y0, z0 + step.z()),
                        Vec3::new(x0 + step.x(), y0, z0 + step.z()),
                        Vec3::new(x0 + step.x(), y0 + step.y(), z0 + step.z()),
                        Vec3::new(x0, y0 + step.y(), z0 + step.z()),
                    ];
                    let values: [f32; 8] = std::array::from_fn(|i| node.eval(corners[i]));
                    let mut cube_index = 0u8;
                    for (i, &v) in values.iter().enumerate() {
                        if v < 0.0 {
                            cube_index |= 1 << i;
                        }
                    }
                    let edge_mask = EDGE_TABLE[cube_index as usize];
                    if edge_mask == 0 {
                        continue;
                    }
                    let mut edge_verts = [Vec3::ZERO; 12];
                    for (e, &(a, b)) in EDGE_VERTICES.iter().enumerate() {
                        if edge_mask & (1 << e) != 0 {
                            let t = values[a] / (values[a] - values[b]);
                            edge_verts[e] = corners[a].lerp(corners[b], t);
                        }
                    }
                    let tri_row = &TRI_TABLE[cube_index as usize];
                    let mut i = 0;
                    while i < 16 && tri_row[i] >= 0 {
                        let base = verts.len() as u32;
                        for k in 0..3 {
                            #[allow(clippy::cast_sign_loss)]
                            let edge_idx = tri_row[i + k] as usize;
                            let pos = edge_verts[edge_idx];
                            let nx = node.eval(Vec3::new(pos.x() + eps, pos.y(), pos.z()))
                                - node.eval(Vec3::new(pos.x() - eps, pos.y(), pos.z()));
                            let ny = node.eval(Vec3::new(pos.x(), pos.y() + eps, pos.z()))
                                - node.eval(Vec3::new(pos.x(), pos.y() - eps, pos.z()));
                            let nz = node.eval(Vec3::new(pos.x(), pos.y(), pos.z() + eps))
                                - node.eval(Vec3::new(pos.x(), pos.y(), pos.z() - eps));
                            verts.push(MeshVertex {
                                position: pos,
                                normal: Vec3::new(nx, ny, nz).normalize(),
                            });
                        }
                        idxs.push(base);
                        idxs.push(base + 1);
                        idxs.push(base + 2);
                        i += 3;
                    }
                }
            }
            (verts, idxs)
        })
        .collect();

    // Merge slices
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for (verts, idxs) in slices {
        let offset = vertices.len() as u32;
        vertices.extend(verts);
        indices.extend(idxs.iter().map(|&i| i + offset));
    }
    SdfMesh { vertices, indices }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-4;

    #[test]
    fn sphere_center() {
        let s = SdfPrimitive::Sphere { radius: 1.0 };
        assert!((s.eval(Vec3::ZERO) - (-1.0)).abs() < EPS);
    }

    #[test]
    fn sphere_surface() {
        let s = SdfPrimitive::Sphere { radius: 1.0 };
        assert!(s.eval(Vec3::new(1.0, 0.0, 0.0)).abs() < EPS);
    }

    #[test]
    fn sphere_outside() {
        let s = SdfPrimitive::Sphere { radius: 1.0 };
        assert!(s.eval(Vec3::new(2.0, 0.0, 0.0)) > 0.0);
    }

    #[test]
    fn box_center() {
        let b = SdfPrimitive::Box {
            half_extents: Vec3::ONE,
        };
        assert!(b.eval(Vec3::ZERO) < 0.0);
    }

    #[test]
    fn box_corner() {
        let b = SdfPrimitive::Box {
            half_extents: Vec3::ONE,
        };
        assert!(b.eval(Vec3::ONE).abs() < EPS);
    }

    #[test]
    fn box_outside() {
        let b = SdfPrimitive::Box {
            half_extents: Vec3::ONE,
        };
        assert!(b.eval(Vec3::new(2.0, 0.0, 0.0)) > 0.0);
    }

    #[test]
    fn capsule_center() {
        let c = SdfPrimitive::Capsule {
            radius: 0.5,
            height: 2.0,
        };
        assert!(c.eval(Vec3::ZERO) < 0.0);
    }

    #[test]
    fn cylinder_center() {
        let c = SdfPrimitive::Cylinder {
            radius: 1.0,
            height: 2.0,
        };
        assert!(c.eval(Vec3::ZERO) < 0.0);
    }

    #[test]
    fn torus_ring() {
        let t = SdfPrimitive::Torus {
            major_radius: 2.0,
            minor_radius: 0.5,
        };
        assert!(t.eval(Vec3::new(2.0, 0.0, 0.0)).abs() < 0.5 + EPS);
    }

    #[test]
    fn plane_above() {
        let p = SdfPrimitive::Plane {
            normal: Vec3::Y,
            offset: 0.0,
        };
        assert!(p.eval(Vec3::new(0.0, 1.0, 0.0)) > 0.0);
    }

    #[test]
    fn plane_below() {
        let p = SdfPrimitive::Plane {
            normal: Vec3::Y,
            offset: 0.0,
        };
        assert!(p.eval(Vec3::new(0.0, -1.0, 0.0)) < 0.0);
    }

    #[test]
    fn union_op() {
        let d = apply_op(SdfOp::Union, 1.0, 2.0, 0.0);
        assert_eq!(d, 1.0);
    }

    #[test]
    fn intersection_op() {
        let d = apply_op(SdfOp::Intersection, 1.0, 2.0, 0.0);
        assert_eq!(d, 2.0);
    }

    #[test]
    fn subtraction_op() {
        let d = apply_op(SdfOp::Subtraction, 1.0, 0.5, 0.0);
        assert_eq!(d, 1.0);
    }

    #[test]
    fn smooth_union_op() {
        let d = apply_op(SdfOp::SmoothUnion, 0.5, 0.5, 1.0);
        assert!(d < 0.5);
    }

    #[test]
    fn smooth_min_zero_k() {
        let d = smooth_min(3.0, 5.0, 0.0);
        assert_eq!(d, 3.0);
    }

    #[test]
    fn sdf_node_primitive() {
        let node = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        assert!(node.eval(Vec3::ZERO) < 0.0);
    }

    #[test]
    fn sdf_node_transform() {
        let node = SdfNode::Transform {
            translation: Vec3::new(5.0, 0.0, 0.0),
            child: Box::new(SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 })),
        };
        assert!(node.eval(Vec3::new(5.0, 0.0, 0.0)) < 0.0);
        assert!(node.eval(Vec3::ZERO) > 0.0);
    }

    #[test]
    fn sdf_node_union() {
        let node = SdfNode::Operation {
            op: SdfOp::Union,
            k: 0.0,
            children: vec![
                SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 }),
                SdfNode::Transform {
                    translation: Vec3::new(3.0, 0.0, 0.0),
                    child: Box::new(SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 })),
                },
            ],
        };
        assert!(node.eval(Vec3::ZERO) < 0.0);
        assert!(node.eval(Vec3::new(3.0, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn sdf_node_empty_op() {
        let node = SdfNode::Operation {
            op: SdfOp::Union,
            k: 0.0,
            children: vec![],
        };
        assert_eq!(node.eval(Vec3::ZERO), f32::MAX);
    }

    #[test]
    fn sphere_trace_hit() {
        let node = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let hit = sphere_trace(
            &node,
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 0.0, -1.0),
            128,
            100.0,
            0.001,
        );
        assert!(hit.is_some());
        let h = hit.unwrap();
        assert!((h.distance - 4.0).abs() < 0.01);
    }

    #[test]
    fn sphere_trace_miss() {
        let node = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let hit = sphere_trace(
            &node,
            Vec3::new(0.0, 0.0, 5.0),
            Vec3::new(0.0, 1.0, 0.0),
            128,
            100.0,
            0.001,
        );
        assert!(hit.is_none());
    }

    #[test]
    fn normal_estimation() {
        let s = SdfPrimitive::Sphere { radius: 1.0 };
        let n = s.normal(Vec3::new(1.0, 0.0, 0.0), 0.001);
        assert!((n.x() - 1.0).abs() < 0.01);
        assert!(n.y().abs() < 0.01);
    }

    #[test]
    fn cone_outside() {
        let c = SdfPrimitive::Cone {
            radius: 1.0,
            height: 2.0,
        };
        assert!(c.eval(Vec3::new(5.0, 0.0, 0.0)) > 0.0);
    }

    #[test]
    fn sdf_serialization() {
        let node = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 2.5 });
        let json = serde_json::to_string(&node).unwrap();
        let back: SdfNode = serde_json::from_str(&json).unwrap();
        assert!((back.eval(Vec3::ZERO) - (-2.5)).abs() < EPS);
    }

    #[test]
    fn smooth_intersection() {
        let d = apply_op(SdfOp::SmoothIntersection, 0.5, 0.5, 1.0);
        assert!(d > 0.0);
    }

    #[test]
    fn smooth_subtraction() {
        let d = apply_op(SdfOp::SmoothSubtraction, 1.0, -0.5, 0.5);
        assert!(d.is_finite());
    }

    #[test]
    fn full_transform_translation() {
        let node = SdfNode::FullTransform {
            translation: Vec3::new(5.0, 0.0, 0.0),
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: Vec3::ONE,
            child: Box::new(SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 })),
        };
        assert!(node.eval(Vec3::new(5.0, 0.0, 0.0)) < 0.0);
        assert!(node.eval(Vec3::ZERO) > 0.0);
    }

    #[test]
    fn full_transform_scale() {
        let node = SdfNode::FullTransform {
            translation: Vec3::ZERO,
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: Vec3::new(2.0, 2.0, 2.0),
            child: Box::new(SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 })),
        };
        // Scaled sphere radius=2 should contain point at (1.5, 0, 0)
        assert!(node.eval(Vec3::new(1.5, 0.0, 0.0)) < 0.0);
    }

    #[test]
    fn sdf_sphere_collision_hit() {
        let node = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 2.0 });
        let contact = sdf_sphere_test(&node, Vec3::new(1.5, 0.0, 0.0), 1.0);
        assert!(contact.is_some());
        let c = contact.unwrap();
        assert!(c.penetration > 0.0);
    }

    #[test]
    fn sdf_sphere_collision_miss() {
        let node = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let contact = sdf_sphere_test(&node, Vec3::new(5.0, 0.0, 0.0), 0.5);
        assert!(contact.is_none());
    }

    #[test]
    fn sdf_sphere_collision_normal() {
        let node = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 2.0 });
        let contact = sdf_sphere_test(&node, Vec3::new(1.0, 0.0, 0.0), 1.5).unwrap();
        // Normal should point roughly in +X direction
        assert!(contact.normal.x() > 0.5);
    }

    #[test]
    fn marching_cubes_sphere() {
        let node = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let mesh = marching_cubes(
            &node,
            Vec3::new(-2.0, -2.0, -2.0),
            Vec3::new(2.0, 2.0, 2.0),
            8,
        );
        assert!(mesh.vertex_count() > 0);
    }

    #[test]
    fn marching_cubes_box() {
        let node = SdfNode::Primitive(SdfPrimitive::Box {
            half_extents: Vec3::ONE,
        });
        let mesh = marching_cubes(
            &node,
            Vec3::new(-2.0, -2.0, -2.0),
            Vec3::new(2.0, 2.0, 2.0),
            10,
        );
        assert!(mesh.vertex_count() > 0);
    }

    #[test]
    fn marching_cubes_empty() {
        let node = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 0.1 });
        let mesh = marching_cubes(
            &node,
            Vec3::new(10.0, 10.0, 10.0),
            Vec3::new(20.0, 20.0, 20.0),
            4,
        );
        // No surface crossings far from the sphere
        assert_eq!(mesh.vertex_count(), 0);
    }

    #[test]
    fn marching_cubes_normals_finite() {
        let node = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let mesh = marching_cubes(
            &node,
            Vec3::new(-2.0, -2.0, -2.0),
            Vec3::new(2.0, 2.0, 2.0),
            6,
        );
        for v in &mesh.vertices {
            assert!(v.normal.x().is_finite());
            assert!(v.normal.y().is_finite());
        }
    }

    #[test]
    fn sdf_mesh_triangle_count() {
        let mesh = SdfMesh {
            vertices: vec![
                MeshVertex {
                    position: Vec3::ZERO,
                    normal: Vec3::Y,
                },
                MeshVertex {
                    position: Vec3::X,
                    normal: Vec3::Y,
                },
                MeshVertex {
                    position: Vec3::Z,
                    normal: Vec3::Y,
                },
            ],
            indices: vec![0, 1, 2],
        };
        assert_eq!(mesh.triangle_count(), 1);
    }

    #[test]
    fn parallel_mc_sphere() {
        let node = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let mesh = marching_cubes_parallel(
            &node,
            Vec3::new(-2.0, -2.0, -2.0),
            Vec3::new(2.0, 2.0, 2.0),
            8,
        );
        assert!(mesh.vertex_count() > 0);
        assert!(mesh.triangle_count() > 0);
    }

    #[test]
    fn parallel_mc_matches_serial() {
        let node = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let serial = marching_cubes(
            &node,
            Vec3::new(-2.0, -2.0, -2.0),
            Vec3::new(2.0, 2.0, 2.0),
            6,
        );
        let parallel = marching_cubes_parallel(
            &node,
            Vec3::new(-2.0, -2.0, -2.0),
            Vec3::new(2.0, 2.0, 2.0),
            6,
        );
        assert_eq!(serial.vertex_count(), parallel.vertex_count());
        assert_eq!(serial.triangle_count(), parallel.triangle_count());
    }
}
