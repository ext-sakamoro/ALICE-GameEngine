//! Hair / grass strand simulation (CPU-side data + wind sway + LOD).
//!
//! Each strand is a chain of `segments + 1` control points. The root
//! follows a world-space anchor (typically a scalp/grass-base vertex);
//! the remaining points are integrated under wind, gravity, and a soft
//! length constraint. The result is a per-strand polyline that the
//! renderer can extrude into a triangle ribbon or a TressFX-style
//! quad strip on the GPU.
//!
//! This module mirrors Wicked Engine's `wiHairParticle` data layout
//! (strand chunks + per-strand seed) but keeps the simulation on the
//! CPU so it works without a GPU device. A future PR can move the
//! integrator to a compute shader by re-using [`HairStrand`] as the
//! buffer layout.
//!
//! ## Quick example
//!
//! ```rust
//! use alice_game_engine::hair::{HairConfig, HairSystem};
//! use alice_game_engine::math::Vec3;
//!
//! let mut hair = HairSystem::new(HairConfig::default());
//! hair.add_strand(Vec3::ZERO, Vec3::Y);
//! hair.simulate(0.016, Vec3::new(1.0, 0.0, 0.0));
//! assert_eq!(hair.strand_count(), 1);
//! ```

use crate::math::Vec3;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Hair-system-wide parameters. Per-strand variation is layered on top
/// via the strand's seed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HairConfig {
    /// Number of segments per strand (= control points - 1). 4 is fine
    /// for grass, 8-12 for character hair.
    pub segments: u32,
    /// Total strand length in metres.
    pub length: f32,
    /// Wind influence factor in `[0, 1]`. 0 = stiff, 1 = floppy.
    pub wind_strength: f32,
    /// Gravity acceleration along -Y (m/s²).
    pub gravity: f32,
    /// Length-constraint stiffness in `[0, 1]`. 1 = rigid rod.
    pub stiffness: f32,
    /// LOD cutoff distance (metres). Past this distance the simulator
    /// skips integration and snaps points to a straight chain.
    pub lod_distance: f32,
}

impl Default for HairConfig {
    fn default() -> Self {
        Self {
            segments: 8,
            length: 0.5,
            wind_strength: 0.4,
            gravity: 9.81,
            stiffness: 0.9,
            lod_distance: 30.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Strand
// ---------------------------------------------------------------------------

/// One simulated hair / grass strand. Layout mirrors what the GPU
/// extruder needs: a contiguous `points` array of length `segments + 1`.
#[derive(Debug, Clone)]
pub struct HairStrand {
    pub anchor: Vec3,
    /// World-space rest direction for the strand (unit vector).
    pub up: Vec3,
    /// Control points (positions). `points[0]` is always the anchor.
    pub points: Vec<Vec3>,
    /// Per-point velocity for Verlet-style integration.
    pub velocities: Vec<Vec3>,
    /// Deterministic seed (= per-strand wind phase / colour jitter).
    pub seed: u32,
}

impl HairStrand {
    fn new(anchor: Vec3, up: Vec3, segments: u32, length: f32, seed: u32) -> Self {
        let up = if up.length_squared() > 1e-8 {
            up.normalize()
        } else {
            Vec3::Y
        };
        let segment_len = length / (segments as f32);
        let mut points = Vec::with_capacity((segments + 1) as usize);
        let mut velocities = Vec::with_capacity((segments + 1) as usize);
        for i in 0..=segments {
            points.push(anchor + up * (segment_len * i as f32));
            velocities.push(Vec3::ZERO);
        }
        Self {
            anchor,
            up,
            points,
            velocities,
            seed,
        }
    }
}

// ---------------------------------------------------------------------------
// HairSystem
// ---------------------------------------------------------------------------

/// Owns the strands + configuration. Call [`simulate`] once per frame.
///
/// [`simulate`]: HairSystem::simulate
pub struct HairSystem {
    pub config: HairConfig,
    strands: Vec<HairStrand>,
    next_seed: u32,
}

impl HairSystem {
    #[must_use]
    pub const fn new(config: HairConfig) -> Self {
        Self {
            config,
            strands: Vec::new(),
            next_seed: 0,
        }
    }

    /// Append a strand. Returns its index.
    pub fn add_strand(&mut self, anchor: Vec3, up: Vec3) -> usize {
        let strand = HairStrand::new(
            anchor,
            up,
            self.config.segments,
            self.config.length,
            self.next_seed,
        );
        self.next_seed = self.next_seed.wrapping_add(1);
        self.strands.push(strand);
        self.strands.len() - 1
    }

    /// Adds many strands from a flat list of (anchor, up) pairs.
    pub fn add_strands<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (Vec3, Vec3)>,
    {
        for (anchor, up) in iter {
            self.add_strand(anchor, up);
        }
    }

    #[must_use]
    pub const fn strand_count(&self) -> usize {
        self.strands.len()
    }

    #[must_use]
    pub fn strands(&self) -> &[HairStrand] {
        &self.strands
    }

    /// Advance the simulation by `dt` seconds under a constant world-
    /// space `wind_velocity`.
    pub fn simulate(&mut self, dt: f32, wind_velocity: Vec3) {
        let gravity = Vec3::new(0.0, -self.config.gravity, 0.0);
        let stiffness = self.config.stiffness.clamp(0.0, 1.0);
        let segment_len = self.config.length / (self.config.segments as f32);
        let wind_scale = self.config.wind_strength;

        for strand in &mut self.strands {
            // Per-strand wind phase from the seed for variation.
            let phase = (strand.seed as f32) * 0.137;
            let wind = wind_velocity + Vec3::new(phase.sin(), 0.0, phase.cos()) * 0.1;

            // Verlet integrate every point except the anchor.
            for i in 1..strand.points.len() {
                let force = gravity + wind * wind_scale;
                strand.velocities[i] = strand.velocities[i] + force * dt;
                strand.points[i] = strand.points[i] + strand.velocities[i] * dt;
            }

            // Length constraint: walk segment-by-segment from the anchor.
            strand.points[0] = strand.anchor;
            for i in 1..strand.points.len() {
                let diff = strand.points[i] - strand.points[i - 1];
                let dist = diff.length();
                if dist < 1e-6 {
                    continue;
                }
                let correction = (dist - segment_len) * stiffness;
                let dir = diff * dist.recip();
                strand.points[i] = strand.points[i] - dir * correction;
            }
        }
    }

    /// Snap every strand to a straight rest pose along `up`. Useful for
    /// distant LOD where the per-frame integrator can be skipped.
    pub fn snap_to_rest(&mut self) {
        let segment_len = self.config.length / (self.config.segments as f32);
        for strand in &mut self.strands {
            for i in 0..strand.points.len() {
                strand.points[i] = strand.anchor + strand.up * (segment_len * i as f32);
                strand.velocities[i] = Vec3::ZERO;
            }
        }
    }

    /// Returns true when `camera_pos` is past the LOD cutoff for the
    /// strand patch centred at `centre`.
    #[must_use]
    pub fn past_lod(&self, camera_pos: Vec3, centre: Vec3) -> bool {
        (camera_pos - centre).length() > self.config.lod_distance
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_grass_friendly() {
        let c = HairConfig::default();
        assert_eq!(c.segments, 8);
        assert!((c.length - 0.5).abs() < 1e-6);
    }

    #[test]
    fn add_strand_creates_segment_plus_one_points() {
        let mut h = HairSystem::new(HairConfig::default());
        h.add_strand(Vec3::ZERO, Vec3::Y);
        let strand = &h.strands()[0];
        assert_eq!(strand.points.len(), 9); // segments=8 + 1
        assert_eq!(strand.points[0], Vec3::ZERO);
        // Tip should be at length distance up.
        let tip = strand.points[8];
        assert!((tip.y() - 0.5).abs() < 1e-4, "tip y = {}", tip.y());
    }

    #[test]
    fn simulate_applies_wind_to_non_anchor_points() {
        let mut h = HairSystem::new(HairConfig {
            wind_strength: 1.0,
            gravity: 0.0,
            stiffness: 0.0, // no length constraint → free motion
            ..HairConfig::default()
        });
        h.add_strand(Vec3::ZERO, Vec3::Y);
        let before = h.strands()[0].points[4];
        for _ in 0..30 {
            h.simulate(1.0 / 60.0, Vec3::new(10.0, 0.0, 0.0));
        }
        let after = h.strands()[0].points[4];
        assert!(
            (after.x() - before.x()).abs() > 0.01,
            "wind should have moved point 4: before {before:?}, after {after:?}",
        );
    }

    #[test]
    fn simulate_anchor_unchanged() {
        let mut h = HairSystem::new(HairConfig::default());
        h.add_strand(Vec3::new(5.0, 0.0, -2.0), Vec3::Y);
        for _ in 0..60 {
            h.simulate(1.0 / 60.0, Vec3::new(20.0, 0.0, 0.0));
        }
        let p0 = h.strands()[0].points[0];
        assert!((p0 - Vec3::new(5.0, 0.0, -2.0)).length() < 1e-4);
    }

    #[test]
    fn stiffness_preserves_length_under_wind() {
        let mut h = HairSystem::new(HairConfig {
            stiffness: 1.0,
            wind_strength: 1.0,
            gravity: 9.81,
            ..HairConfig::default()
        });
        h.add_strand(Vec3::ZERO, Vec3::Y);
        for _ in 0..120 {
            h.simulate(1.0 / 60.0, Vec3::new(50.0, 0.0, 0.0));
        }
        let strand = &h.strands()[0];
        let segment_len = 0.5 / 8.0;
        for i in 1..strand.points.len() {
            let d = (strand.points[i] - strand.points[i - 1]).length();
            assert!(
                (d - segment_len).abs() < segment_len * 0.6,
                "segment {i} length {d} drifted from rest {segment_len}",
            );
        }
    }

    #[test]
    fn snap_to_rest_returns_straight_chain() {
        let mut h = HairSystem::new(HairConfig::default());
        h.add_strand(Vec3::ZERO, Vec3::Y);
        // Disturb the chain.
        h.simulate(0.5, Vec3::new(100.0, 0.0, 0.0));
        h.snap_to_rest();
        let strand = &h.strands()[0];
        for i in 1..strand.points.len() {
            let p = strand.points[i];
            assert!(p.x().abs() < 1e-4);
            assert!(p.z().abs() < 1e-4);
        }
    }

    #[test]
    fn add_strands_bulk_iterator() {
        let mut h = HairSystem::new(HairConfig::default());
        let anchors = (0..50).map(|i| (Vec3::new(i as f32, 0.0, 0.0), Vec3::Y));
        h.add_strands(anchors);
        assert_eq!(h.strand_count(), 50);
    }

    #[test]
    fn past_lod_distance_check() {
        let h = HairSystem::new(HairConfig {
            lod_distance: 10.0,
            ..HairConfig::default()
        });
        assert!(!h.past_lod(Vec3::new(5.0, 0.0, 0.0), Vec3::ZERO));
        assert!(h.past_lod(Vec3::new(20.0, 0.0, 0.0), Vec3::ZERO));
    }

    #[test]
    fn each_strand_gets_unique_seed() {
        let mut h = HairSystem::new(HairConfig::default());
        for _ in 0..16 {
            h.add_strand(Vec3::ZERO, Vec3::Y);
        }
        let seeds: std::collections::HashSet<u32> = h.strands().iter().map(|s| s.seed).collect();
        assert_eq!(seeds.len(), 16);
    }

    #[test]
    fn zero_up_falls_back_to_y_axis() {
        let mut h = HairSystem::new(HairConfig::default());
        h.add_strand(Vec3::ZERO, Vec3::ZERO);
        assert!((h.strands()[0].up - Vec3::Y).length() < 1e-6);
    }

    #[test]
    fn gravity_pulls_unstiff_chain_down() {
        let mut h = HairSystem::new(HairConfig {
            stiffness: 0.0,
            wind_strength: 0.0,
            gravity: 9.81,
            ..HairConfig::default()
        });
        h.add_strand(Vec3::ZERO, Vec3::Y);
        let before = h.strands()[0].points[4].y();
        for _ in 0..30 {
            h.simulate(1.0 / 60.0, Vec3::ZERO);
        }
        let after = h.strands()[0].points[4].y();
        assert!(
            after < before,
            "gravity should drop the chain ({before} → {after})"
        );
    }
}
