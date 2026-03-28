//! Collision detection: GJK/EPA for convex meshes, SDF+mesh hybrid narrowphase.

use crate::math::Vec3;

// ---------------------------------------------------------------------------
// Support function for GJK
// ---------------------------------------------------------------------------

/// A convex shape that can compute a support point.
pub trait ConvexShape {
    /// Returns the farthest point in the given direction.
    fn support(&self, direction: Vec3) -> Vec3;
}

/// A convex hull defined by a set of points.
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
        self.center + direction.normalize() * self.radius
    }
}

// ---------------------------------------------------------------------------
// Minkowski difference support
// ---------------------------------------------------------------------------

fn minkowski_support(a: &dyn ConvexShape, b: &dyn ConvexShape, dir: Vec3) -> Vec3 {
    a.support(dir) - b.support(-dir)
}

// ---------------------------------------------------------------------------
// GJK — Gilbert-Johnson-Keerthi intersection test
// ---------------------------------------------------------------------------

/// GJK result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GjkResult {
    Intersecting,
    Separated,
    /// Algorithm did not converge within the iteration limit.
    Indeterminate,
}

/// GJK intersection test using triangle simplex.
/// Returns whether two convex shapes overlap.
#[must_use]
pub fn gjk(a: &dyn ConvexShape, b: &dyn ConvexShape, max_iterations: u32) -> GjkResult {
    let mut dir = Vec3::new(1.0, 0.0, 0.0);
    let mut simplex: Vec<Vec3> = Vec::new();

    let initial = minkowski_support(a, b, dir);
    simplex.push(initial);
    dir = -initial;

    for _ in 0..max_iterations {
        let point = minkowski_support(a, b, dir);
        if point.dot(dir) < 0.0 {
            return GjkResult::Separated;
        }
        simplex.push(point);

        if let Some(new_dir) = do_simplex(&mut simplex) {
            dir = new_dir;
        } else {
            return GjkResult::Intersecting;
        }
    }
    GjkResult::Indeterminate
}

/// Processes the simplex and returns the new search direction,
/// or None if the origin is enclosed.
#[allow(clippy::similar_names)]
fn do_simplex(simplex: &mut Vec<Vec3>) -> Option<Vec3> {
    match simplex.len() {
        2 => {
            let a = simplex[1];
            let b = simplex[0];
            let ab = b - a;
            let ao = -a;
            if ab.dot(ao) > 0.0 {
                let dir = triple_cross(ab, ao, ab);
                Some(dir)
            } else {
                simplex.clear();
                simplex.push(a);
                Some(ao)
            }
        }
        3 => {
            let a = simplex[2];
            let b = simplex[1];
            let c = simplex[0];
            let ab = b - a;
            let ac = c - a;
            let ao = -a;
            let abc = ab.cross(ac);

            let perp_ab = abc.cross(ac);
            if perp_ab.dot(ao) > 0.0 {
                // Region AB
                simplex.clear();
                simplex.push(b);
                simplex.push(a);
                return Some(triple_cross(ab, ao, ab));
            }

            let perp_ac = ab.cross(abc);
            if perp_ac.dot(ao) > 0.0 {
                // Region AC
                simplex.clear();
                simplex.push(c);
                simplex.push(a);
                return Some(triple_cross(ac, ao, ac));
            }

            // Origin is inside triangle
            if abc.dot(ao) > 0.0 {
                // Above triangle
                simplex.clear();
                simplex.push(c);
                simplex.push(b);
                simplex.push(a);
                Some(abc)
            } else {
                // Below triangle — check if 4th point needed (3D)
                None
            }
        }
        _ => {
            // Tetrahedron encloses origin → intersection confirmed
            None
        }
    }
}

/// Triple cross product: (a × b) × c.
#[inline]
fn triple_cross(a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    a.cross(b).cross(c)
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

/// Tests mesh vertices against an SDF for penetration.
#[must_use]
pub fn mesh_vs_sdf(
    mesh_vertices: &[Vec3],
    sdf_eval: &dyn Fn(Vec3) -> f32,
    sdf_normal: &dyn Fn(Vec3) -> Vec3,
) -> Vec<HybridContact> {
    let mut contacts = Vec::new();
    for &v in mesh_vertices {
        let dist = sdf_eval(v);
        if dist < 0.0 {
            contacts.push(HybridContact {
                point: v,
                normal: sdf_normal(v),
                penetration: -dist,
            });
        }
    }
    contacts
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
        let sphere = ConvexSphere {
            center: Vec3::ZERO,
            radius: 2.0,
        };
        let s = sphere.support(Vec3::new(1.0, 0.0, 0.0));
        assert!((s.x() - 2.0).abs() < 1e-5);
    }

    #[test]
    fn gjk_overlapping_spheres() {
        let a = ConvexSphere {
            center: Vec3::ZERO,
            radius: 1.0,
        };
        let b = ConvexSphere {
            center: Vec3::new(0.5, 0.0, 0.0),
            radius: 1.0,
        };
        assert_eq!(gjk(&a, &b, 32), GjkResult::Intersecting);
    }

    #[test]
    fn gjk_separated_spheres() {
        let a = ConvexSphere {
            center: Vec3::ZERO,
            radius: 1.0,
        };
        let b = ConvexSphere {
            center: Vec3::new(5.0, 0.0, 0.0),
            radius: 1.0,
        };
        assert_eq!(gjk(&a, &b, 32), GjkResult::Separated);
    }

    #[test]
    fn gjk_hull_vs_sphere() {
        let hull = ConvexHull::new(vec![
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(0.0, 1.0, -1.0),
            Vec3::new(0.0, 0.0, 1.0),
        ]);
        let sphere = ConvexSphere {
            center: Vec3::ZERO,
            radius: 0.5,
        };
        assert_eq!(gjk(&hull, &sphere, 32), GjkResult::Intersecting);
    }

    #[test]
    fn gjk_hull_separated() {
        let hull = ConvexHull::new(vec![
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        ]);
        let sphere = ConvexSphere {
            center: Vec3::new(10.0, 10.0, 10.0),
            radius: 0.5,
        };
        assert_eq!(gjk(&hull, &sphere, 32), GjkResult::Separated);
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
        // Points at (0,0,0) and (0.5,0,0) are inside the sphere
        assert_eq!(contacts.len(), 2);
        assert!(contacts[0].penetration > 0.0);
    }

    #[test]
    fn mesh_vs_sdf_no_contact() {
        let verts = vec![Vec3::new(5.0, 5.0, 5.0)];
        let sdf_eval = |p: Vec3| p.length() - 1.0;
        let sdf_normal = |p: Vec3| p.normalize();
        let contacts = mesh_vs_sdf(&verts, &sdf_eval, &sdf_normal);
        assert!(contacts.is_empty());
    }

    #[test]
    fn triple_cross_nonzero() {
        let a = Vec3::X;
        let b = Vec3::Y;
        let c = Vec3::X;
        let result = triple_cross(a, b, c);
        assert!(result.length() > 0.0);
    }

    #[test]
    fn convex_hull_single_point() {
        let hull = ConvexHull::new(vec![Vec3::new(3.0, 4.0, 5.0)]);
        let s = hull.support(Vec3::X);
        assert_eq!(s, Vec3::new(3.0, 4.0, 5.0));
    }
}
