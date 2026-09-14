//! SDF hybrid rendering: evaluate SDF volumes alongside polygon meshes.
//!
//! This module provides the bridge between ALICE-SDF's node tree evaluation
//! and the game engine's scene graph. The distance functions themselves come
//! from `alice_sdf` (single source of the law); only the lightweight node
//! tree, meshing and tracing utilities live here. SDF volumes can be:
//! - Raymarched directly in a compute/fragment shader
//! - Meshed via marching cubes for physics/rendering
//! - Used as collision volumes (SDF CCD)

//!
//! ```rust
//! use alice_game_engine::sdf::*;
//! use alice_game_engine::math::Vec3;
//! let s = SdfPrimitive::Sphere { radius: 1.0 };
//! assert!(s.eval(Vec3::ZERO) < 0.0);
//! ```
use crate::math::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// SdfPrimitive — built-in primitives for the engine
// ---------------------------------------------------------------------------

/// Basic SDF primitives for in-engine use.
///
/// The distance law is **not** re-implemented here: every variant delegates
/// to the matching `alice_sdf::primitives::sdf_*` function, so the engine
/// and ALICE-SDF (and everything ALICE-SDF transpiles to GLSL / WGSL / HLSL)
/// evaluate the same field.  Parameter conventions kept from the engine's
/// original API: `height` is the full height (ALICE-SDF takes half-heights),
/// `Cone` has its base disc at `y = 0` and its tip at `y = height`
/// (ALICE-SDF's cone is centred on the origin, so it is evaluated at
/// `p - (0, height / 2, 0)`).
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
        use alice_sdf::primitives as law;
        let g = p.0;
        match self {
            Self::Sphere { radius } => law::sdf_sphere(g, *radius),
            Self::Box { half_extents } => law::sdf_box3d(g, half_extents.0),
            Self::Capsule { radius, height } => law::sdf_capsule_vertical(g, height * 0.5, *radius),
            Self::Cylinder { radius, height } => law::sdf_cylinder(g, *radius, height * 0.5),
            Self::Torus {
                major_radius,
                minor_radius,
            } => law::sdf_torus(g, *major_radius, *minor_radius),
            Self::Plane { normal, offset } => law::sdf_plane(g, normal.0, *offset),
            Self::Cone { radius, height } => {
                let half_h = height * 0.5;
                law::sdf_cone(g - glam::Vec3::new(0.0, half_h, 0.0), *radius, half_h)
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
///
/// Smooth variants delegate to `alice_sdf::operations` (polynomial smooth
/// min / max) so the blend law matches ALICE-SDF exactly.
#[inline]
#[must_use]
pub fn apply_op(op: SdfOp, a: f32, b: f32, k: f32) -> f32 {
    use alice_sdf::operations as law;
    match op {
        SdfOp::Union => a.min(b),
        SdfOp::Intersection => a.max(b),
        SdfOp::Subtraction => a.max(-b),
        SdfOp::SmoothUnion => law::sdf_smooth_union(a, b, k),
        SdfOp::SmoothIntersection => law::sdf_smooth_intersection(a, b, k),
        SdfOp::SmoothSubtraction => law::sdf_smooth_subtraction(a, b, k),
    }
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
// Conversion to the ALICE-SDF node tree
// ---------------------------------------------------------------------------

impl From<&SdfPrimitive> for alice_sdf::SdfNode {
    /// Same field as [`SdfPrimitive::eval`] (see the parameter notes there).
    fn from(prim: &SdfPrimitive) -> Self {
        match prim {
            SdfPrimitive::Sphere { radius } => Self::Sphere { radius: *radius },
            SdfPrimitive::Box { half_extents } => Self::Box3d {
                half_extents: half_extents.0,
            },
            SdfPrimitive::Capsule { radius, height } => Self::Capsule {
                point_a: glam::Vec3::new(0.0, -height * 0.5, 0.0),
                point_b: glam::Vec3::new(0.0, height * 0.5, 0.0),
                radius: *radius,
            },
            SdfPrimitive::Cylinder { radius, height } => Self::Cylinder {
                radius: *radius,
                half_height: height * 0.5,
            },
            SdfPrimitive::Torus {
                major_radius,
                minor_radius,
            } => Self::Torus {
                major_radius: *major_radius,
                minor_radius: *minor_radius,
            },
            SdfPrimitive::Plane { normal, offset } => Self::Plane {
                normal: normal.0,
                distance: *offset,
            },
            SdfPrimitive::Cone { radius, height } => Self::Cone {
                radius: *radius,
                half_height: height * 0.5,
            }
            .translate(0.0, height * 0.5, 0.0),
        }
    }
}

impl From<&SdfNode> for alice_sdf::SdfNode {
    /// Lossless conversion into the ALICE-SDF tree (`alice_sdf::eval` of the
    /// result equals [`SdfNode::eval`], see the `alice_sdf_parity` tests).
    fn from(node: &SdfNode) -> Self {
        use std::sync::Arc;
        match node {
            SdfNode::Primitive(prim) => prim.into(),
            SdfNode::Transform { translation, child } => Self::from(child.as_ref()).translate(
                translation.x(),
                translation.y(),
                translation.z(),
            ),
            SdfNode::FullTransform {
                translation,
                rotation,
                scale,
                child,
            } => {
                // Engine order: p → -translation → inverse rotation → 1/scale → child × min(scale)
                let scaled = Self::ScaleNonUniform {
                    child: Arc::new(Self::from(child.as_ref())),
                    factors: glam::Vec3::new(
                        scale.x().max(1e-10),
                        scale.y().max(1e-10),
                        scale.z().max(1e-10),
                    ),
                };
                scaled.rotate(glam::Quat::from_array(*rotation)).translate(
                    translation.x(),
                    translation.y(),
                    translation.z(),
                )
            }
            SdfNode::Operation { op, k, children } => {
                let mut iter = children.iter().map(Self::from);
                let Some(first) = iter.next() else {
                    // Engine returns f32::MAX for an empty operation: an empty
                    // union is "nothing", i.e. infinitely far away.
                    return Self::Sphere { radius: -f32::MAX };
                };
                iter.fold(first, |acc, next| {
                    let (a, b) = (Arc::new(acc), Arc::new(next));
                    match op {
                        SdfOp::Union => Self::Union { a, b },
                        SdfOp::Intersection => Self::Intersection { a, b },
                        SdfOp::Subtraction => Self::Subtraction { a, b },
                        SdfOp::SmoothUnion => Self::SmoothUnion { a, b, k: *k },
                        SdfOp::SmoothIntersection => Self::SmoothIntersection { a, b, k: *k },
                        SdfOp::SmoothSubtraction => Self::SmoothSubtraction { a, b, k: *k },
                    }
                })
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

/// Converts an ALICE-SDF mesh into the engine's lightweight [`SdfMesh`]
/// (position + normal only; UVs / tangents / materials are dropped).
fn from_alice_mesh(mesh: alice_sdf::mesh::Mesh) -> SdfMesh {
    SdfMesh {
        vertices: mesh
            .vertices
            .into_iter()
            .map(|v| MeshVertex {
                position: Vec3(v.position),
                normal: Vec3(v.normal),
            })
            .collect(),
        indices: mesh.indices,
    }
}

/// Meshes an SDF node with marching cubes.
///
/// Delegates to [`alice_sdf::mesh::marching_cubes`] (the engine keeps no
/// edge / triangle tables of its own). `resolution` is the number of cells
/// per axis (minimum 2); the ALICE-SDF implementation is already
/// parallelised by Z-slabs.
#[must_use]
pub fn marching_cubes(
    node: &SdfNode,
    min_bound: Vec3,
    max_bound: Vec3,
    resolution: u32,
) -> SdfMesh {
    let config = alice_sdf::mesh::MarchingCubesConfig {
        resolution: resolution.max(2) as usize,
        compute_normals: true,
        ..Default::default()
    };
    from_alice_mesh(alice_sdf::mesh::marching_cubes(
        &node.into(),
        min_bound.0,
        max_bound.0,
        &config,
    ))
}

/// Parallel marching cubes.
///
/// Kept for API compatibility: [`marching_cubes`] already runs the
/// ALICE-SDF Z-slab parallel implementation, so both entry points produce
/// the same mesh.
#[must_use]
pub fn marching_cubes_parallel(
    node: &SdfNode,
    min_bound: Vec3,
    max_bound: Vec3,
    resolution: u32,
) -> SdfMesh {
    marching_cubes(node, min_bound, max_bound, resolution)
}

/// `Self::eval(p) + offset).abs() - thickness * 0.5` — turns a solid
/// SDF into a hollow shell of the configured thickness around its
/// surface, optionally shifted by `offset` along the normal. Mirrors
/// the ALICE-SDF `shell` modifier.
#[must_use]
pub fn shell_offset(node: &SdfNode, p: Vec3, thickness: f32, offset: f32) -> f32 {
    (node.eval(p) + offset).abs() - thickness * 0.5
}

/// Monte-Carlo volume estimate by sampling uniform random points
/// inside `[min, max]^3` and counting interior hits. Returns
/// `(volume, standard_error)` in the same units as `(max - min)^3`.
#[must_use]
pub fn volume_monte_carlo(
    node: &SdfNode,
    min: Vec3,
    max: Vec3,
    samples: u32,
    seed: u32,
) -> (f32, f32) {
    let extent = max - min;
    let box_volume = extent.x() * extent.y() * extent.z();
    if samples == 0 || box_volume <= 0.0 {
        return (0.0, 0.0);
    }
    let mut state = seed.max(1);
    let mut hits = 0_u32;
    let inv_max = 1.0 / (u32::MAX as f32 + 1.0);
    for _ in 0..samples {
        let x = next_rand_01(&mut state, inv_max);
        let y = next_rand_01(&mut state, inv_max);
        let z = next_rand_01(&mut state, inv_max);
        let p = min + Vec3::new(extent.x() * x, extent.y() * y, extent.z() * z);
        if node.eval(p) <= 0.0 {
            hits += 1;
        }
    }
    let p_hit = (hits as f32) / (samples as f32);
    let volume = p_hit * box_volume;
    // Standard error of a Bernoulli estimator times box volume.
    let stderr = box_volume * (p_hit * (1.0 - p_hit) / (samples as f32)).sqrt();
    (volume, stderr)
}

/// Monte-Carlo surface area estimate via a narrow band around the
/// zero level set: counts points whose SDF magnitude is below `band`
/// and divides by `2 * band` to get a length × box-volume estimate.
#[must_use]
pub fn surface_area_monte_carlo(
    node: &SdfNode,
    min: Vec3,
    max: Vec3,
    samples: u32,
    band: f32,
    seed: u32,
) -> (f32, f32) {
    let extent = max - min;
    let box_volume = extent.x() * extent.y() * extent.z();
    if samples == 0 || box_volume <= 0.0 || band <= 0.0 {
        return (0.0, 0.0);
    }
    let mut state = seed.max(1);
    let mut hits = 0_u32;
    let inv_max = 1.0 / (u32::MAX as f32 + 1.0);
    for _ in 0..samples {
        let x = next_rand_01(&mut state, inv_max);
        let y = next_rand_01(&mut state, inv_max);
        let z = next_rand_01(&mut state, inv_max);
        let p = min + Vec3::new(extent.x() * x, extent.y() * y, extent.z() * z);
        if node.eval(p).abs() <= band {
            hits += 1;
        }
    }
    let p_hit = (hits as f32) / (samples as f32);
    // band * 2 wide slab -> area = (volume of slab) / (2 * band).
    let area = p_hit * box_volume / (2.0 * band);
    let stderr = box_volume * (p_hit * (1.0 - p_hit) / (samples as f32)).sqrt() / (2.0 * band);
    (area, stderr)
}

fn next_rand_01(state: &mut u32, inv_max: f32) -> f32 {
    *state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    (*state as f32) * inv_max
}

/// Adaptive Marching Cubes (ALICE-SDF style): recursively subdivides
/// the input box into octants and runs the standard MC on each
/// octant. Cells whose interior is uniformly inside or outside
/// (= `|sdf at center| > diag`) are skipped entirely; cells whose
/// gradient changes a lot (= `error > threshold`) are subdivided up
/// to `max_subdivision` times. Falls back to the standard
/// `marching_cubes` at the leaves.
#[must_use]
pub fn adaptive_marching_cubes(
    node: &SdfNode,
    min: Vec3,
    max: Vec3,
    base_resolution: u32,
    max_subdivision: u32,
    error_threshold: f32,
) -> SdfMesh {
    let mut acc = SdfMesh {
        vertices: Vec::new(),
        indices: Vec::new(),
    };
    refine_octant(
        node,
        min,
        max,
        base_resolution,
        max_subdivision,
        error_threshold,
        &mut acc,
    );
    acc
}

fn refine_octant(
    node: &SdfNode,
    min: Vec3,
    max: Vec3,
    base_resolution: u32,
    levels_left: u32,
    threshold: f32,
    acc: &mut SdfMesh,
) {
    let centre = (min + max) * 0.5;
    let half = (max - min) * 0.5;
    let diag = (half.x() * half.x() + half.y() * half.y() + half.z() * half.z()).sqrt();
    let centre_d = node.eval(centre);
    // Conservative empty / full octant skip — Lipschitz-1 SDF
    // assumption (= classic raymarcher precondition).
    if centre_d.abs() > diag {
        return;
    }
    // Estimate error as max corner deviation from the centre value.
    let mut max_corner_diff = 0.0_f32;
    for sx in [-1.0_f32, 1.0] {
        for sy in [-1.0_f32, 1.0] {
            for sz in [-1.0_f32, 1.0] {
                let corner = centre + Vec3::new(sx * half.x(), sy * half.y(), sz * half.z());
                let d = node.eval(corner);
                let diff = (d - centre_d).abs();
                if diff > max_corner_diff {
                    max_corner_diff = diff;
                }
            }
        }
    }
    if levels_left == 0 || max_corner_diff < threshold {
        let part = marching_cubes(node, min, max, base_resolution);
        let base = acc.vertices.len() as u32;
        acc.vertices.extend(part.vertices);
        for idx in part.indices {
            acc.indices.push(base + idx);
        }
        return;
    }
    for sx in 0..2 {
        for sy in 0..2 {
            for sz in 0..2 {
                let sub_min = Vec3::new(
                    if sx == 0 { min.x() } else { centre.x() },
                    if sy == 0 { min.y() } else { centre.y() },
                    if sz == 0 { min.z() } else { centre.z() },
                );
                let sub_max = Vec3::new(
                    if sx == 0 { centre.x() } else { max.x() },
                    if sy == 0 { centre.y() } else { max.y() },
                    if sz == 0 { centre.z() } else { max.z() },
                );
                refine_octant(
                    node,
                    sub_min,
                    sub_max,
                    base_resolution,
                    levels_left - 1,
                    threshold,
                    acc,
                );
            }
        }
    }
}

/// Dual contouring mesher.
///
/// Delegates to [`alice_sdf::mesh::dual_contouring()`] with `res` cells per
/// axis (minimum 2) and the ALICE-SDF defaults for gradient epsilon /
/// bisection iterations.
#[must_use]
pub fn dual_contouring(node: &SdfNode, min: Vec3, max: Vec3, res: u32) -> SdfMesh {
    let config = alice_sdf::mesh::DualContouringConfig {
        resolution: res.max(2) as usize,
        compute_normals: true,
        ..Default::default()
    };
    from_alice_mesh(alice_sdf::mesh::dual_contouring(
        &node.into(),
        min.0,
        max.0,
        &config,
    ))
}

/// Parity with ALICE-SDF: the engine tree and its converted `alice_sdf`
/// tree must evaluate the same field (the engine has no distance law of its
/// own any more; this pins that fact).
#[cfg(test)]
mod alice_sdf_parity {
    use super::*;

    /// Deterministic LCG points in a ±4 box (covers inside / outside / edges).
    fn points(n: usize) -> Vec<Vec3> {
        let mut state: u64 = 0x9a3e_e11e_5df0_0001;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (((state >> 40) as f32) / ((1u64 << 24) as f32)).mul_add(8.0, -4.0)
        };
        (0..n).map(|_| Vec3::new(next(), next(), next())).collect()
    }

    fn assert_parity(node: &SdfNode, label: &str) {
        let alice: alice_sdf::SdfNode = node.into();
        for p in points(500) {
            let engine = node.eval(p);
            let reference = alice_sdf::eval(&alice, p.0);
            let tol = 1e-4 * engine.abs().max(1.0);
            assert!(
                (engine - reference).abs() <= tol,
                "{label}: engine={engine} alice={reference} at {p:?}"
            );
        }
    }

    fn primitives() -> Vec<(&'static str, SdfPrimitive)> {
        vec![
            ("sphere", SdfPrimitive::Sphere { radius: 1.3 }),
            (
                "box",
                SdfPrimitive::Box {
                    half_extents: Vec3::new(1.0, 0.5, 2.0),
                },
            ),
            (
                "capsule",
                SdfPrimitive::Capsule {
                    radius: 0.4,
                    height: 2.5,
                },
            ),
            (
                "cylinder",
                SdfPrimitive::Cylinder {
                    radius: 0.8,
                    height: 1.8,
                },
            ),
            (
                "torus",
                SdfPrimitive::Torus {
                    major_radius: 1.5,
                    minor_radius: 0.35,
                },
            ),
            (
                "plane",
                SdfPrimitive::Plane {
                    normal: Vec3::new(0.0, 1.0, 0.0),
                    offset: 0.25,
                },
            ),
            (
                "cone",
                SdfPrimitive::Cone {
                    radius: 1.1,
                    height: 2.2,
                },
            ),
        ]
    }

    #[test]
    fn every_primitive_matches_alice_sdf() {
        for (name, prim) in primitives() {
            assert_parity(&SdfNode::Primitive(prim), name);
        }
    }

    #[test]
    fn every_operation_matches_alice_sdf() {
        let ops = [
            SdfOp::Union,
            SdfOp::Intersection,
            SdfOp::Subtraction,
            SdfOp::SmoothUnion,
            SdfOp::SmoothIntersection,
            SdfOp::SmoothSubtraction,
        ];
        let prims = primitives();
        for op in ops {
            for k in [0.0, 0.35] {
                let node = SdfNode::Operation {
                    op,
                    k,
                    children: vec![
                        SdfNode::Primitive(prims[0].1.clone()),
                        SdfNode::Transform {
                            translation: Vec3::new(0.7, 0.2, -0.3),
                            child: Box::new(SdfNode::Primitive(prims[1].1.clone())),
                        },
                        SdfNode::Primitive(prims[4].1.clone()),
                    ],
                };
                assert_parity(&node, &format!("{op:?} k={k}"));
            }
        }
    }

    #[test]
    fn transforms_match_alice_sdf() {
        let rotation =
            glam::Quat::from_axis_angle(glam::Vec3::new(0.3, 1.0, 0.2).normalize(), 0.9).to_array();
        for (name, prim) in primitives() {
            let node = SdfNode::FullTransform {
                translation: Vec3::new(0.5, -0.25, 1.0),
                rotation,
                scale: Vec3::new(1.5, 0.75, 1.25),
                child: Box::new(SdfNode::Transform {
                    translation: Vec3::new(-0.2, 0.1, 0.0),
                    child: Box::new(SdfNode::Primitive(prim)),
                }),
            };
            assert_parity(&node, &format!("full_transform({name})"));
        }
    }

    #[test]
    fn cone_zero_set_is_base_at_origin_tip_at_height() {
        // Engine convention preserved: base disc at y = 0, tip at y = height.
        let cone = SdfPrimitive::Cone {
            radius: 1.0,
            height: 2.0,
        };
        assert!(
            cone.eval(Vec3::new(0.0, 1.0, 0.0)) < 0.0,
            "axis midpoint inside"
        );
        assert!(
            cone.eval(Vec3::new(0.0, 2.0, 0.0)).abs() < 1e-4,
            "tip on surface"
        );
        assert!(
            cone.eval(Vec3::new(1.0, 0.0, 0.0)).abs() < 1e-4,
            "base rim on surface"
        );
        assert!(
            cone.eval(Vec3::new(0.0, -0.5, 0.0)) > 0.0,
            "below base outside"
        );
        assert!(
            cone.eval(Vec3::new(0.0, 2.5, 0.0)) > 0.0,
            "above tip outside"
        );
    }
}

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
        let d = apply_op(SdfOp::SmoothUnion, 3.0, 5.0, 0.0);
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
    fn shell_offset_around_sphere_surface_is_near_zero() {
        let n = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let d = shell_offset(&n, Vec3::new(1.0, 0.0, 0.0), 0.1, 0.0);
        // Surface of sphere → eval = 0 → |0| - 0.05 = -0.05 (inside shell).
        assert!(d.abs() < 0.06, "got {d}");
    }

    #[test]
    fn volume_monte_carlo_estimates_unit_sphere() {
        let n = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let (v, err) = volume_monte_carlo(
            &n,
            Vec3::new(-1.5, -1.5, -1.5),
            Vec3::new(1.5, 1.5, 1.5),
            5000,
            7,
        );
        let expected = (4.0 / 3.0) * std::f32::consts::PI;
        assert!(
            (v - expected).abs() < 0.5,
            "volume = {v}, expected ≈ {expected}, err = {err}"
        );
    }

    #[test]
    fn surface_area_monte_carlo_estimates_unit_sphere() {
        let n = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let (a, _) = surface_area_monte_carlo(
            &n,
            Vec3::new(-1.5, -1.5, -1.5),
            Vec3::new(1.5, 1.5, 1.5),
            20000,
            0.05,
            42,
        );
        let expected = 4.0 * std::f32::consts::PI;
        assert!(
            (a - expected).abs() < 4.0,
            "area = {a}, expected ≈ {expected}"
        );
    }

    #[test]
    fn adaptive_mc_produces_some_geometry_for_sphere() {
        let n = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let mesh = adaptive_marching_cubes(
            &n,
            Vec3::new(-1.5, -1.5, -1.5),
            Vec3::new(1.5, 1.5, 1.5),
            4,
            2,
            0.4,
        );
        assert!(mesh.vertex_count() > 0);
        assert!(mesh.triangle_count() > 0);
    }

    #[test]
    fn dual_contouring_produces_geometry_for_sphere() {
        let n = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let mesh = dual_contouring(&n, Vec3::new(-1.5, -1.5, -1.5), Vec3::new(1.5, 1.5, 1.5), 8);
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
