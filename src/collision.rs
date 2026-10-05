//! Convex collision queries delegated to `alice_physics` (GJK / EPA), and
//! an SDF + mesh hybrid narrow phase (requires the `physics` feature).
//!
//! The shapes stay engine types (`f32`); the intersection law is
//! `alice_physics::collider::{gjk, contact}`.

use crate::math::Vec3;
use crate::physics3d::{from_fix, to_fix};

// ---------------------------------------------------------------------------
// Shapes
// ---------------------------------------------------------------------------

/// A convex shape that can compute a support point.
pub trait ConvexShape {
    /// Returns the farthest point in the given direction.
    fn support(&self, direction: Vec3) -> Vec3;
}

/// A convex hull defined by a set of points (at least one).
#[derive(Debug, Clone)]
pub struct ConvexHull {
    pub points: Vec<Vec3>,
}

impl ConvexHull {
    #[must_use]
    pub const fn new(points: Vec<Vec3>) -> Self {
        Self { points }
    }
}

impl ConvexShape for ConvexHull {
    fn support(&self, direction: Vec3) -> Vec3 {
        let mut best = self.points[0];
        let mut best_dot = best.dot(direction);
        for &p in &self.points[1..] {
            let d = p.dot(direction);
            if d > best_dot {
                best_dot = d;
                best = p;
            }
        }
        best
    }
}

/// A sphere as a convex shape.
#[derive(Debug, Clone, Copy)]
pub struct ConvexSphere {
    pub center: Vec3,
    pub radius: f32,
}

impl ConvexShape for ConvexSphere {
    fn support(&self, direction: Vec3) -> Vec3 {
        let sphere =
            alice_physics::Sphere::new(to_fix(self.center.0), crate::physics3d::fx(self.radius));
        Vec3(from_fix(alice_physics::Support::support(
            &sphere,
            to_fix(direction.0),
        )))
    }
}

/// An engine shape seen through `alice_physics`' support interface.
struct AsSupport<'a>(&'a dyn ConvexShape);

impl alice_physics::Support for AsSupport<'_> {
    fn support(&self, direction: alice_physics::Vec3Fix) -> alice_physics::Vec3Fix {
        to_fix(self.0.support(Vec3(from_fix(direction))).0)
    }
}

// ---------------------------------------------------------------------------
// GJK / EPA
// ---------------------------------------------------------------------------

/// GJK result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GjkResult {
    Intersecting,
    Separated,
}

/// Whether two convex shapes overlap (`alice_physics` GJK, fixed iteration
/// count).
#[must_use]
pub fn gjk(a: &dyn ConvexShape, b: &dyn ConvexShape) -> GjkResult {
    if alice_physics::collider::gjk(&AsSupport(a), &AsSupport(b)).colliding {
        GjkResult::Intersecting
    } else {
        GjkResult::Separated
    }
}

/// Penetration of two overlapping convex shapes (EPA).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConvexContact {
    /// Unit normal from `a` to `b`: moving `a` by `-depth · normal`
    /// separates the shapes.
    pub normal: Vec3,
    /// Penetration depth (≥ 0).
    pub depth: f32,
    /// Deepest point of `a` inside `b`.
    pub point_a: Vec3,
    /// Deepest point of `b` inside `a`.
    pub point_b: Vec3,
}

/// Contact of two convex shapes, or `None` when they do not overlap
/// (`alice_physics` GJK + EPA; exact for flat faces, a few parts per
/// thousand for curved ones).
#[must_use]
pub fn contact(a: &dyn ConvexShape, b: &dyn ConvexShape) -> Option<ConvexContact> {
    let c = alice_physics::collider::contact(&AsSupport(a), &AsSupport(b))?;
    Some(ConvexContact {
        // alice_physics reports the normal from `b` to `a`.
        normal: -Vec3(from_fix(c.normal)),
        depth: c.depth.to_f32().max(0.0),
        point_a: Vec3(from_fix(c.point_a)),
        point_b: Vec3(from_fix(c.point_b)),
    })
}

// ---------------------------------------------------------------------------
// SDF+Mesh hybrid contact
// ---------------------------------------------------------------------------

/// Contact from SDF-mesh hybrid narrowphase.
#[derive(Debug, Clone, Copy)]
pub struct HybridContact {
    pub point: Vec3,
    pub normal: Vec3,
    pub penetration: f32,
}

/// Mesh vertices inside the SDF (`alice_physics::sdf_collider::collide_point_sdf_field`
/// per vertex): `normal` is the unit outward normal from `sdf_normal`, and
/// `penetration` the depth below the surface (> 0). Vertices on or outside
/// the surface give no contact.
#[must_use]
pub fn mesh_vs_sdf(
    mesh_vertices: &[Vec3],
    sdf_eval: &dyn Fn(Vec3) -> f32,
    sdf_normal: &dyn Fn(Vec3) -> Vec3,
) -> Vec<HybridContact> {
    let field = alice_physics::sdf_collider::ClosureSdfQuery::new(
        |x: f32, y: f32, z: f32| sdf_eval(Vec3::new(x, y, z)),
        |x: f32, y: f32, z: f32| {
            let n = sdf_normal(Vec3::new(x, y, z));
            (n.x(), n.y(), n.z())
        },
    );
    let frame = alice_physics::sdf_collider::SdfFrame::IDENTITY;
    mesh_vertices
        .iter()
        .filter_map(|&v| {
            let c =
                alice_physics::sdf_collider::collide_point_sdf_field(to_fix(v.0), &field, &frame)?;
            Some(HybridContact {
                point: v,
                normal: Vec3(from_fix(c.normal)),
                penetration: c.depth.to_f32(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sphere(x: f32, r: f32) -> ConvexSphere {
        ConvexSphere {
            center: Vec3::new(x, 0.0, 0.0),
            radius: r,
        }
    }

    /// Axis-aligned box `[min, max]` as a hull.
    fn cube(min: Vec3, max: Vec3) -> ConvexHull {
        let mut points = Vec::new();
        for &x in &[min.x(), max.x()] {
            for &y in &[min.y(), max.y()] {
                for &z in &[min.z(), max.z()] {
                    points.push(Vec3::new(x, y, z));
                }
            }
        }
        ConvexHull::new(points)
    }

    #[test]
    fn convex_hull_support() {
        let hull = ConvexHull::new(vec![
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ]);
        let s = hull.support(Vec3::new(1.0, 0.0, 0.0));
        assert!((s.x() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn convex_sphere_support() {
        let s = sphere(0.0, 2.0).support(Vec3::new(1.0, 0.0, 0.0));
        assert!((s.x() - 2.0).abs() < 1e-5);
    }

    #[test]
    fn convex_hull_single_point() {
        let hull = ConvexHull::new(vec![Vec3::new(3.0, 4.0, 5.0)]);
        assert_eq!(hull.support(Vec3::X), Vec3::new(3.0, 4.0, 5.0));
    }

    #[test]
    fn gjk_overlapping_spheres() {
        assert_eq!(
            gjk(&sphere(0.0, 1.0), &sphere(0.5, 1.0)),
            GjkResult::Intersecting
        );
    }

    #[test]
    fn gjk_separated_spheres() {
        assert_eq!(
            gjk(&sphere(0.0, 1.0), &sphere(5.0, 1.0)),
            GjkResult::Separated
        );
    }

    #[test]
    fn gjk_spheres_just_apart_and_just_overlapping() {
        // |c_a - c_b| vs r_a + r_b = 2
        assert_eq!(
            gjk(&sphere(0.0, 1.0), &sphere(2.05, 1.0)),
            GjkResult::Separated
        );
        assert_eq!(
            gjk(&sphere(0.0, 1.0), &sphere(1.95, 1.0)),
            GjkResult::Intersecting
        );
    }

    #[test]
    fn gjk_hull_vs_sphere() {
        let hull = ConvexHull::new(vec![
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(0.0, 1.0, -1.0),
            Vec3::new(0.0, 0.0, 1.0),
        ]);
        let s = ConvexSphere {
            center: Vec3::ZERO,
            radius: 0.5,
        };
        assert_eq!(gjk(&hull, &s), GjkResult::Intersecting);
    }

    #[test]
    fn gjk_hull_separated() {
        let hull = ConvexHull::new(vec![
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ]);
        let s = ConvexSphere {
            center: Vec3::new(10.0, 10.0, 10.0),
            radius: 0.5,
        };
        assert_eq!(gjk(&hull, &s), GjkResult::Separated);
    }

    #[test]
    fn contact_of_overlapping_boxes_is_exact() {
        // Unit cubes overlapping by 0.25 along +X: depth 0.25, normal a→b = +X.
        let a = cube(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        let b = cube(Vec3::new(0.75, 0.0, 0.0), Vec3::new(1.75, 1.0, 1.0));
        let c = contact(&a, &b).expect("boxes overlap");
        assert!((c.depth - 0.25).abs() < 1e-5, "depth {}", c.depth);
        assert!(
            (c.normal - Vec3::X).length() < 1e-5,
            "normal {:?}",
            c.normal
        );
    }

    #[test]
    fn contact_of_spheres_matches_closed_form() {
        // depth = r_a + r_b - d = 2 - 1.5 = 0.5, normal a→b = +X
        let c = contact(&sphere(0.0, 1.0), &sphere(1.5, 1.0)).expect("spheres overlap");
        assert!((c.depth - 0.5).abs() < 5e-3, "depth {}", c.depth);
        assert!(
            (c.normal - Vec3::X).length() < 1e-2,
            "normal {:?}",
            c.normal
        );
    }

    #[test]
    fn contact_of_separated_shapes_is_none() {
        assert!(contact(&sphere(0.0, 1.0), &sphere(3.0, 1.0)).is_none());
    }

    #[test]
    fn mesh_vs_sdf_contact() {
        let verts = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.5, 0.0, 0.0),
            Vec3::new(2.0, 0.0, 0.0),
        ];
        // Sphere SDF at origin, radius 1.0
        let sdf_eval = |p: Vec3| p.length() - 1.0;
        let sdf_normal = |p: Vec3| p.normalize();
        let contacts = mesh_vs_sdf(&verts, &sdf_eval, &sdf_normal);
        // (0,0,0) is 1.0 deep, (0.5,0,0) 0.5 deep, (2,0,0) outside
        assert_eq!(contacts.len(), 2);
        assert!((contacts[0].penetration - 1.0).abs() < 1e-6);
        assert!((contacts[1].penetration - 0.5).abs() < 1e-6);
        // unit outward normal of the sphere at (0.5, 0, 0) is +X
        assert!((contacts[1].normal.0 - glam::Vec3::X).length() < 1e-6);
        assert_eq!(contacts[1].point, Vec3::new(0.5, 0.0, 0.0));
    }

    #[test]
    fn mesh_vs_sdf_reports_the_unit_normal_and_skips_surface_points() {
        // Half-space y < 0, gradient given unnormalised (0, 2, 0)
        let verts = vec![
            Vec3::new(1.0, -0.25, 3.0),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.5, 0.0),
        ];
        let sdf_eval = |p: Vec3| p.y();
        let sdf_normal = |_: Vec3| Vec3::new(0.0, 2.0, 0.0);
        let contacts = mesh_vs_sdf(&verts, &sdf_eval, &sdf_normal);
        assert_eq!(
            contacts.len(),
            1,
            "on-surface and outside vertices give none"
        );
        assert!((contacts[0].penetration - 0.25).abs() < 1e-6);
        assert!((contacts[0].normal.0 - glam::Vec3::Y).length() < 1e-6);
    }

    #[test]
    fn mesh_vs_sdf_no_contact() {
        let verts = vec![Vec3::new(5.0, 5.0, 5.0)];
        let sdf_eval = |p: Vec3| p.length() - 1.0;
        let sdf_normal = |p: Vec3| p.normalize();
        assert!(mesh_vs_sdf(&verts, &sdf_eval, &sdf_normal).is_empty());
    }
}
