//! 2D SDF primitives + CSG (ALICE-SDF compatible).
//!
//! A lightweight 2D counterpart to [`crate::sdf`] for use in UI
//! widgets, icons, font glyph rendering, particle masks, and shader
//! noise. The API mirrors the 3D module: every primitive supports
//! [`Sdf2dPrimitive::eval`] returning a signed distance, and the
//! [`Sdf2dNode`] enum composes them with union / intersect /
//! subtract / smooth-union operators.

use serde::{Deserialize, Serialize};

use crate::math::Vec2;

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Sdf2dPrimitive {
    /// Filled disc of `radius`, centred at origin.
    Circle { radius: f32 },
    /// Axis-aligned box of half-extents.
    Box { half_extents: Vec2 },
    /// Rounded box (`corner_radius` shrinks the half-extents).
    RoundedBox {
        half_extents: Vec2,
        corner_radius: f32,
    },
    /// Line segment from `a` to `b` with `thickness`.
    Segment { a: Vec2, b: Vec2, thickness: f32 },
    /// Triangle with three corner vertices.
    Triangle { a: Vec2, b: Vec2, c: Vec2 },
}

impl Sdf2dPrimitive {
    /// Evaluate the 2D signed distance at `p`.
    ///
    /// The law comes from `alice_sdf::primitives` (single source): the
    /// extruded XY shapes are evaluated at `z = 0` with an unbounded half
    /// height, for which `extrude_2d` is the identity, and the triangle is the
    /// exact 3-vertex polygon distance. Engine conventions are kept
    /// (`Segment::thickness` is the full width, i.e. radius `thickness / 2`).
    #[must_use]
    pub fn eval(&self, p: Vec2) -> f32 {
        use alice_sdf::primitives as law;
        let p3 = glam::Vec3::new(p.x(), p.y(), 0.0);
        match *self {
            Self::Circle { radius } => law::sdf_circle_2d(p3, radius, f32::MAX),
            Self::Box { half_extents } => law::sdf_rect_2d(p3, half_extents.0, f32::MAX),
            Self::RoundedBox {
                half_extents,
                corner_radius,
            } => {
                // Engine clamps the inner box at zero so `corner_radius` larger than
                // a half extent degrades to a capsule-like shape instead of inverting.
                let r = corner_radius.min(half_extents.x()).min(half_extents.y());
                law::sdf_rounded_rect_2d(p3, half_extents.0, r, f32::MAX)
            }
            Self::Segment { a, b, thickness } => {
                if (b - a).0.length_squared() < 1e-12 {
                    return (p - a).length() - thickness * 0.5;
                }
                law::sdf_segment_2d(p3, a.0, b.0, thickness * 0.5, f32::MAX)
            }
            Self::Triangle { a, b, c } => law::sdf_polygon_2d_xy(p.0, &[a.0, b.0, c.0]),
        }
    }
}

// ---------------------------------------------------------------------------
// Boolean tree
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sdf2dOp {
    Union,
    Intersect,
    Subtract,
    SmoothUnion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Sdf2dNode {
    Primitive(Sdf2dPrimitive),
    Translate {
        offset: Vec2,
        child: Box<Self>,
    },
    Op {
        op: Sdf2dOp,
        k: f32,
        children: Vec<Self>,
    },
}

impl Sdf2dNode {
    #[must_use]
    pub fn eval(&self, p: Vec2) -> f32 {
        match self {
            Self::Primitive(prim) => prim.eval(p),
            Self::Translate { offset, child } => child.eval(p - *offset),
            Self::Op { op, k, children } => {
                if children.is_empty() {
                    return f32::MAX;
                }
                let mut d = children[0].eval(p);
                for c in &children[1..] {
                    d = combine(*op, d, c.eval(p), *k);
                }
                d
            }
        }
    }
}

fn combine(op: Sdf2dOp, a: f32, b: f32, k: f32) -> f32 {
    match op {
        Sdf2dOp::Union => a.min(b),
        Sdf2dOp::Intersect => a.max(b),
        Sdf2dOp::Subtract => a.max(-b),
        Sdf2dOp::SmoothUnion => {
            // Polynomial smooth minimum, same law as the 3D tree (`alice_sdf`).
            alice_sdf::operations::smooth_min(a, b, k)
        }
    }
}

/// Bilinear-sample the node at the centres of a `width × height`
/// grid covering `[min, max]`. Useful for rasterising the SDF into a
/// texture used by a UI shader (= per-pixel font / icon mask).
#[must_use]
pub fn sample_grid(node: &Sdf2dNode, width: u32, height: u32, min: Vec2, max: Vec2) -> Vec<f32> {
    let mut out = Vec::with_capacity((width * height) as usize);
    if width == 0 || height == 0 {
        return out;
    }
    let extent = max - min;
    let inv_w = (width as f32).recip();
    let inv_h = (height as f32).recip();
    for y in 0..height {
        for x in 0..width {
            let u = (x as f32 + 0.5) * inv_w;
            let v = (y as f32 + 0.5) * inv_h;
            let p = Vec2::new(min.x() + extent.x() * u, min.y() + extent.y() * v);
            out.push(node.eval(p));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// The 2D law equals ALICE-SDF's extruded shapes on the `z = 0` plane (for
/// points closer than the half height, where the extrusion is the identity).
#[cfg(test)]
mod alice_sdf_parity {
    use super::*;

    fn points(n: usize) -> Vec<Vec2> {
        let mut state: u64 = 0x2d5d_f000_0000_0001;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (((state >> 40) as f32) / ((1u64 << 24) as f32)).mul_add(6.0, -3.0)
        };
        (0..n).map(|_| Vec2::new(next(), next())).collect()
    }

    fn assert_matches(name: &str, prim: &Sdf2dPrimitive, node: &alice_sdf::SdfNode) {
        for p in points(400) {
            let d2 = prim.eval(p);
            let d3 = alice_sdf::eval(node, glam::Vec3::new(p.x(), p.y(), 0.0));
            assert!(
                (d2 - d3).abs() <= 1e-5 * d2.abs().max(1.0),
                "{name}: 2d={d2} extruded={d3} at {p:?}"
            );
        }
    }

    #[test]
    fn primitives_match_extruded_alice_sdf_nodes() {
        use alice_sdf::SdfNode as N;
        let h = 100.0; // far larger than any |distance| in the ±3 box
        assert_matches(
            "circle",
            &Sdf2dPrimitive::Circle { radius: 1.2 },
            &N::Circle2D {
                radius: 1.2,
                half_height: h,
            },
        );
        assert_matches(
            "box",
            &Sdf2dPrimitive::Box {
                half_extents: Vec2::new(1.0, 0.5),
            },
            &N::Rect2D {
                half_extents: glam::Vec2::new(1.0, 0.5),
                half_height: h,
            },
        );
        assert_matches(
            "rounded_box",
            &Sdf2dPrimitive::RoundedBox {
                half_extents: Vec2::new(1.0, 0.6),
                corner_radius: 0.2,
            },
            &N::RoundedRect2D {
                half_extents: glam::Vec2::new(1.0, 0.6),
                round_radius: 0.2,
                half_height: h,
            },
        );
        assert_matches(
            "segment",
            &Sdf2dPrimitive::Segment {
                a: Vec2::new(-1.0, -0.3),
                b: Vec2::new(0.8, 0.9),
                thickness: 0.4,
            },
            &N::Segment2D {
                a: glam::Vec2::new(-1.0, -0.3),
                b: glam::Vec2::new(0.8, 0.9),
                thickness: 0.2,
                half_height: h,
            },
        );
        assert_matches(
            "triangle",
            &Sdf2dPrimitive::Triangle {
                a: Vec2::new(-1.0, -0.8),
                b: Vec2::new(1.2, -0.5),
                c: Vec2::new(0.1, 1.1),
            },
            &N::Polygon2D {
                vertices: vec![
                    glam::Vec2::new(-1.0, -0.8),
                    glam::Vec2::new(1.2, -0.5),
                    glam::Vec2::new(0.1, 1.1),
                ],
                half_height: h,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_zero_at_radius() {
        let c = Sdf2dPrimitive::Circle { radius: 1.0 };
        assert!(c.eval(Vec2::new(1.0, 0.0)).abs() < 1e-5);
        assert!(c.eval(Vec2::new(0.0, 0.0)) < 0.0);
        assert!(c.eval(Vec2::new(2.0, 0.0)) > 0.0);
    }

    #[test]
    fn box_evals_axis_distance() {
        let b = Sdf2dPrimitive::Box {
            half_extents: Vec2::new(2.0, 1.0),
        };
        assert!(b.eval(Vec2::new(3.0, 0.0)) > 0.9);
        assert!(b.eval(Vec2::new(0.0, 0.0)) < 0.0);
    }

    #[test]
    fn rounded_box_smooths_corners() {
        let b = Sdf2dPrimitive::RoundedBox {
            half_extents: Vec2::new(1.0, 1.0),
            corner_radius: 0.3,
        };
        // Just outside the corner — should be smaller than the box
        // corner distance because of the rounded inset.
        let d = b.eval(Vec2::new(1.0, 1.0));
        assert!(d > 0.0);
    }

    #[test]
    fn segment_thickness_creates_pill_band() {
        let s = Sdf2dPrimitive::Segment {
            a: Vec2::new(-1.0, 0.0),
            b: Vec2::new(1.0, 0.0),
            thickness: 0.2,
        };
        assert!(s.eval(Vec2::new(0.0, 0.0)) < 0.0);
        assert!(s.eval(Vec2::new(0.0, 0.5)) > 0.0);
    }

    #[test]
    fn triangle_inside_is_negative() {
        let t = Sdf2dPrimitive::Triangle {
            a: Vec2::new(0.0, 0.0),
            b: Vec2::new(1.0, 0.0),
            c: Vec2::new(0.0, 1.0),
        };
        assert!(t.eval(Vec2::new(0.25, 0.25)) < 0.0);
        assert!(t.eval(Vec2::new(1.0, 1.0)) > 0.0);
    }

    #[test]
    fn union_of_two_discs_creates_lens() {
        let n = Sdf2dNode::Op {
            op: Sdf2dOp::Union,
            k: 0.0,
            children: vec![
                Sdf2dNode::Primitive(Sdf2dPrimitive::Circle { radius: 1.0 }),
                Sdf2dNode::Translate {
                    offset: Vec2::new(1.5, 0.0),
                    child: Box::new(Sdf2dNode::Primitive(Sdf2dPrimitive::Circle { radius: 1.0 })),
                },
            ],
        };
        assert!(n.eval(Vec2::new(0.0, 0.0)) < 0.0);
        assert!(n.eval(Vec2::new(0.75, 0.0)) < 0.0);
        assert!(n.eval(Vec2::new(3.0, 0.0)) > 0.0);
    }

    #[test]
    fn sample_grid_returns_width_times_height_floats() {
        let n = Sdf2dNode::Primitive(Sdf2dPrimitive::Circle { radius: 1.0 });
        let buf = sample_grid(&n, 16, 8, Vec2::new(-2.0, -2.0), Vec2::new(2.0, 2.0));
        assert_eq!(buf.len(), 16 * 8);
        assert!(buf.iter().any(|d| *d < 0.0));
        assert!(buf.iter().any(|d| *d > 0.0));
    }
}
