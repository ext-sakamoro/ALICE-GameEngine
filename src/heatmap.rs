//! Distance-field cross-section heatmap (ALICE-SDF style).
//!
//! Slices an SDF along one axis, samples it on a `resolution²`
//! grid, normalises by the maximum |distance|, and maps the result
//! to one of four scientific-visualisation colormaps. Useful for
//! debug overlays and design-time tooling where you want to see
//! the SDF level sets.

use serde::{Deserialize, Serialize};

use crate::math::Vec3;
use crate::sdf::SdfNode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Colormap {
    /// Diverging blue→white→red (= classic).
    CoolWarm,
    /// Black inside / white outside.
    Binary,
    /// Perceptually uniform purple→green→yellow.
    Viridis,
    /// Perceptually uniform black→magenta→yellow.
    Magma,
}

/// Sample the SDF across a slice plane and return an RGBA buffer
/// (length = `resolution² × 4`) ready to upload to a `Rgba8Unorm`
/// texture for visualisation.
#[must_use]
pub fn heatmap_slice(
    node: &SdfNode,
    axis: Axis,
    depth: f32,
    min: Vec3,
    max: Vec3,
    resolution: u32,
    colormap: Colormap,
) -> Vec<u8> {
    let res = resolution.max(1);
    let extent = max - min;
    // First pass: sample SDF + find max |distance| for normalisation.
    let mut sdf_grid: Vec<f32> = Vec::with_capacity((res * res) as usize);
    let inv = (res as f32).recip();
    for j in 0..res {
        for i in 0..res {
            let u = (i as f32 + 0.5) * inv;
            let v = (j as f32 + 0.5) * inv;
            let p = match axis {
                Axis::X => Vec3::new(depth, min.y() + extent.y() * u, min.z() + extent.z() * v),
                Axis::Y => Vec3::new(min.x() + extent.x() * u, depth, min.z() + extent.z() * v),
                Axis::Z => Vec3::new(min.x() + extent.x() * u, min.y() + extent.y() * v, depth),
            };
            sdf_grid.push(node.eval(p));
        }
    }
    let max_abs = sdf_grid.iter().fold(1e-6_f32, |a, b| a.max(b.abs()));
    // Second pass: normalise + colormap.
    let mut out = Vec::with_capacity(sdf_grid.len() * 4);
    for &d in &sdf_grid {
        let t = (d / max_abs).clamp(-1.0, 1.0);
        let [r, g, b] = match colormap {
            Colormap::CoolWarm => cool_warm(t),
            Colormap::Binary => {
                if d <= 0.0 {
                    [0, 0, 0]
                } else {
                    [255, 255, 255]
                }
            }
            Colormap::Viridis => viridis(t * 0.5 + 0.5),
            Colormap::Magma => magma(t * 0.5 + 0.5),
        };
        out.extend_from_slice(&[r, g, b, 255]);
    }
    out
}

// ---------------------------------------------------------------------------
// Colormap functions (= cheap piecewise polynomial fits)
// ---------------------------------------------------------------------------

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let t = t.clamp(0.0, 1.0);
    (a as f32 * (1.0 - t) + b as f32 * t).round() as u8
}

fn cool_warm(t: f32) -> [u8; 3] {
    // t ∈ [-1, 1]: -1 cold blue, 0 white, 1 warm red.
    if t < 0.0 {
        let s = (t + 1.0).clamp(0.0, 1.0);
        [
            lerp_u8(50, 255, s),
            lerp_u8(80, 255, s),
            lerp_u8(200, 255, s),
        ]
    } else {
        let s = t.clamp(0.0, 1.0);
        [
            lerp_u8(255, 200, s),
            lerp_u8(255, 50, s),
            lerp_u8(255, 50, s),
        ]
    }
}

fn viridis(t: f32) -> [u8; 3] {
    // 5-stop linear approximation of the matplotlib `viridis` map.
    const STOPS: [[u8; 3]; 5] = [
        [68, 1, 84],
        [59, 82, 139],
        [33, 145, 140],
        [94, 201, 98],
        [253, 231, 37],
    ];
    sample_colormap(t, &STOPS)
}

fn magma(t: f32) -> [u8; 3] {
    const STOPS: [[u8; 3]; 5] = [
        [0, 0, 4],
        [80, 18, 123],
        [183, 55, 121],
        [251, 136, 97],
        [252, 253, 191],
    ];
    sample_colormap(t, &STOPS)
}

fn sample_colormap(t: f32, stops: &[[u8; 3]; 5]) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0) * 4.0;
    let i = (t.floor() as usize).min(3);
    let frac = t - i as f32;
    [
        lerp_u8(stops[i][0], stops[i + 1][0], frac),
        lerp_u8(stops[i][1], stops[i + 1][1], frac),
        lerp_u8(stops[i][2], stops[i + 1][2], frac),
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdf::{SdfNode, SdfPrimitive};

    #[test]
    fn heatmap_slice_returns_rgba_buffer() {
        let n = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let buf = heatmap_slice(
            &n,
            Axis::Z,
            0.0,
            Vec3::new(-2.0, -2.0, -2.0),
            Vec3::new(2.0, 2.0, 2.0),
            16,
            Colormap::CoolWarm,
        );
        assert_eq!(buf.len(), 16 * 16 * 4);
        // Centre of slice (sphere interior) should be on the cool side.
        let centre_idx = (8 * 16 + 8) * 4;
        let r = buf[centre_idx];
        let b = buf[centre_idx + 2];
        assert!(b > r, "centre should be cool (blue dominant)");
    }

    #[test]
    fn binary_colormap_clamps_to_black_or_white() {
        let n = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let buf = heatmap_slice(
            &n,
            Axis::Z,
            0.0,
            Vec3::new(-2.0, -2.0, -2.0),
            Vec3::new(2.0, 2.0, 2.0),
            8,
            Colormap::Binary,
        );
        for chunk in buf.chunks_exact(4) {
            let is_black = chunk[0] == 0 && chunk[1] == 0 && chunk[2] == 0;
            let is_white = chunk[0] == 255 && chunk[1] == 255 && chunk[2] == 255;
            assert!(is_black || is_white, "got {chunk:?}");
        }
    }

    #[test]
    fn viridis_and_magma_produce_distinct_palettes() {
        let n = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let v = heatmap_slice(
            &n,
            Axis::Z,
            0.0,
            Vec3::new(-2.0, -2.0, -2.0),
            Vec3::new(2.0, 2.0, 2.0),
            8,
            Colormap::Viridis,
        );
        let m = heatmap_slice(
            &n,
            Axis::Z,
            0.0,
            Vec3::new(-2.0, -2.0, -2.0),
            Vec3::new(2.0, 2.0, 2.0),
            8,
            Colormap::Magma,
        );
        // They should not be byte-identical.
        assert_ne!(v, m);
    }

    #[test]
    fn empty_resolution_yields_empty_buffer() {
        let n = SdfNode::Primitive(SdfPrimitive::Sphere { radius: 1.0 });
        let buf = heatmap_slice(
            &n,
            Axis::Z,
            0.0,
            Vec3::new(-2.0, -2.0, -2.0),
            Vec3::new(2.0, 2.0, 2.0),
            1,
            Colormap::CoolWarm,
        );
        assert_eq!(buf.len(), 4);
    }
}
