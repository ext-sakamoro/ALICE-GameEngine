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
    /// Evaluate the signed distance at `p` (negative inside, positive
    /// outside, zero on the silhouette).
    #[must_use]
    pub fn eval(&self, p: Vec2) -> f32 {
        match *self {
            Self::Circle { radius } => p.length() - radius,
            Self::Box { half_extents } => {
                let q = Vec2::new(
                    p.x().abs() - half_extents.x(),
                    p.y().abs() - half_extents.y(),
                );
                let outside = Vec2::new(q.x().max(0.0), q.y().max(0.0)).length();
                let inside = q.x().max(q.y()).min(0.0);
                outside + inside
            }
            Self::RoundedBox {
                half_extents,
                corner_radius,
            } => {
                let h = Vec2::new(
                    (half_extents.x() - corner_radius).max(0.0),
                    (half_extents.y() - corner_radius).max(0.0),
                );
                let q = Vec2::new(p.x().abs() - h.x(), p.y().abs() - h.y());
                let outside = Vec2::new(q.x().max(0.0), q.y().max(0.0)).length();
                let inside = q.x().max(q.y()).min(0.0);
                outside + inside - corner_radius
            }
            Self::Segment { a, b, thickness } => {
                let pa = p - a;
                let ba = b - a;
                let ba_len2 = (ba.x() * ba.x() + ba.y() * ba.y()).max(1e-12);
                let h = ((pa.x() * ba.x() + pa.y() * ba.y()) / ba_len2).clamp(0.0, 1.0);
                let dx = pa.x() - ba.x() * h;
                let dy = pa.y() - ba.y() * h;
                (dx * dx + dy * dy).sqrt() - thickness * 0.5
            }
            Self::Triangle { a, b, c } => triangle_sdf(p, a, b, c),
        }
    }
}

fn triangle_sdf(p: Vec2, a: Vec2, b: Vec2, c: Vec2) -> f32 {
    let edge = |x: Vec2, y: Vec2| -> f32 {
        let pa = p - x;
        let ba = y - x;
        let ba_len2 = (ba.x() * ba.x() + ba.y() * ba.y()).max(1e-12);
        let h = ((pa.x() * ba.x() + pa.y() * ba.y()) / ba_len2).clamp(0.0, 1.0);
        let dx = pa.x() - ba.x() * h;
        let dy = pa.y() - ba.y() * h;
        (dx * dx + dy * dy).sqrt()
    };
    let d = edge(a, b).min(edge(b, c)).min(edge(c, a));
    // Sign from edge cross products (= inside vs outside).
    let s = ((b.x() - a.x()) * (p.y() - a.y()) - (b.y() - a.y()) * (p.x() - a.x())).signum();
    let s2 = ((c.x() - b.x()) * (p.y() - b.y()) - (c.y() - b.y()) * (p.x() - b.x())).signum();
    let s3 = ((a.x() - c.x()) * (p.y() - c.y()) - (a.y() - c.y()) * (p.x() - c.x())).signum();
    let inside = (s > 0.0) == (s2 > 0.0) && (s2 > 0.0) == (s3 > 0.0);
    if inside {
        -d
    } else {
        d
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
            let h = (0.5 + 0.5 * (b - a) / k.max(1e-6)).clamp(0.0, 1.0);
            (b * (1.0 - h) + a * h) - k * h * (1.0 - h)
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
