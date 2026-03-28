//! Level of Detail (LOD) system (UE5 Nanite inspired).
//!
//! Automatic mesh/SDF quality switching based on screen-space size.
//! SDF nodes can reduce evaluation complexity at distance.

use crate::math::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// LodLevel
// ---------------------------------------------------------------------------

/// A single LOD level definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LodLevel {
    /// Minimum screen-space coverage (0.0..1.0) to use this LOD.
    pub min_screen_coverage: f32,
    /// Mesh asset ID for this LOD (lower LODs use simpler meshes).
    pub mesh_id: u32,
    /// SDF evaluation resolution for this LOD.
    pub sdf_resolution: u32,
}

// ---------------------------------------------------------------------------
// LodGroup
// ---------------------------------------------------------------------------

/// A group of LOD levels for a single object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LodGroup {
    pub levels: Vec<LodLevel>,
}

impl LodGroup {
    #[must_use]
    pub const fn new(levels: Vec<LodLevel>) -> Self {
        Self { levels }
    }

    /// Selects the appropriate LOD index based on screen coverage.
    /// Returns the index of the best matching LOD level.
    #[must_use]
    pub fn select(&self, screen_coverage: f32) -> usize {
        for (i, level) in self.levels.iter().enumerate() {
            if screen_coverage >= level.min_screen_coverage {
                return i;
            }
        }
        self.levels.len().saturating_sub(1)
    }

    #[must_use]
    pub const fn level_count(&self) -> usize {
        self.levels.len()
    }
}

// ---------------------------------------------------------------------------
// Screen coverage calculation
// ---------------------------------------------------------------------------

/// Computes screen-space coverage of a sphere (bounding sphere) from camera.
/// Returns 0.0..1.0 where 1.0 fills the entire screen.
#[must_use]
pub fn screen_coverage(
    object_pos: Vec3,
    bounding_radius: f32,
    camera_pos: Vec3,
    fov_y: f32,
    screen_height: f32,
) -> f32 {
    let dist = object_pos.distance(camera_pos);
    if dist < 1e-6 {
        return 1.0;
    }
    let projected_radius = bounding_radius / (dist * (fov_y * 0.5).tan());
    let pixel_size = projected_radius * screen_height;
    (pixel_size / screen_height).clamp(0.0, 1.0)
}

/// Decides whether an object should be culled entirely (too small on screen).
#[must_use]
pub fn should_cull(screen_coverage: f32, min_pixel_size: f32, screen_height: f32) -> bool {
    screen_coverage * screen_height < min_pixel_size
}

// ---------------------------------------------------------------------------
// LodSelector — batch LOD selection
// ---------------------------------------------------------------------------

/// Batch LOD selection result.
#[derive(Debug, Clone, Copy)]
pub struct LodSelection {
    pub object_index: usize,
    pub lod_index: usize,
    pub screen_coverage: f32,
    pub culled: bool,
}

/// Selects LODs for multiple objects at once.
#[must_use]
pub fn select_lods(
    positions: &[Vec3],
    radii: &[f32],
    groups: &[LodGroup],
    camera_pos: Vec3,
    fov_y: f32,
    screen_height: f32,
    cull_threshold_pixels: f32,
) -> Vec<LodSelection> {
    let count = positions.len().min(radii.len()).min(groups.len());
    let mut results = Vec::with_capacity(count);
    for i in 0..count {
        let coverage = screen_coverage(positions[i], radii[i], camera_pos, fov_y, screen_height);
        let culled = should_cull(coverage, cull_threshold_pixels, screen_height);
        let lod_index = if culled {
            groups[i].level_count().saturating_sub(1)
        } else {
            groups[i].select(coverage)
        };
        results.push(LodSelection {
            object_index: i,
            lod_index,
            screen_coverage: coverage,
            culled,
        });
    }
    results
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn test_group() -> LodGroup {
        LodGroup::new(vec![
            LodLevel {
                min_screen_coverage: 0.3,
                mesh_id: 0,
                sdf_resolution: 64,
            },
            LodLevel {
                min_screen_coverage: 0.1,
                mesh_id: 1,
                sdf_resolution: 32,
            },
            LodLevel {
                min_screen_coverage: 0.01,
                mesh_id: 2,
                sdf_resolution: 16,
            },
        ])
    }

    #[test]
    fn lod_select_close() {
        let group = test_group();
        assert_eq!(group.select(0.5), 0);
    }

    #[test]
    fn lod_select_medium() {
        let group = test_group();
        assert_eq!(group.select(0.15), 1);
    }

    #[test]
    fn lod_select_far() {
        let group = test_group();
        assert_eq!(group.select(0.05), 2);
    }

    #[test]
    fn lod_select_very_far() {
        let group = test_group();
        assert_eq!(group.select(0.001), 2);
    }

    #[test]
    fn lod_level_count() {
        let group = test_group();
        assert_eq!(group.level_count(), 3);
    }

    #[test]
    fn screen_coverage_close() {
        let cov = screen_coverage(
            Vec3::new(0.0, 0.0, -5.0),
            1.0,
            Vec3::ZERO,
            std::f32::consts::FRAC_PI_4,
            1080.0,
        );
        assert!(cov > 0.0);
    }

    #[test]
    fn screen_coverage_far() {
        let cov = screen_coverage(
            Vec3::new(0.0, 0.0, -1000.0),
            1.0,
            Vec3::ZERO,
            std::f32::consts::FRAC_PI_4,
            1080.0,
        );
        assert!(cov < 0.01);
    }

    #[test]
    fn screen_coverage_zero_distance() {
        let cov = screen_coverage(Vec3::ZERO, 1.0, Vec3::ZERO, 1.0, 1080.0);
        assert_eq!(cov, 1.0);
    }

    #[test]
    fn should_cull_tiny() {
        assert!(should_cull(0.0001, 2.0, 1080.0));
    }

    #[test]
    fn should_not_cull_visible() {
        assert!(!should_cull(0.1, 2.0, 1080.0));
    }

    #[test]
    fn batch_lod_selection() {
        let positions = vec![Vec3::new(0.0, 0.0, -5.0), Vec3::new(0.0, 0.0, -500.0)];
        let radii = vec![1.0, 1.0];
        let groups = vec![test_group(), test_group()];
        let results = select_lods(
            &positions,
            &radii,
            &groups,
            Vec3::ZERO,
            std::f32::consts::FRAC_PI_4,
            1080.0,
            2.0,
        );
        assert_eq!(results.len(), 2);
        assert!(results[0].lod_index <= results[1].lod_index);
    }

    #[test]
    fn batch_empty() {
        let results = select_lods(&[], &[], &[], Vec3::ZERO, 1.0, 1080.0, 2.0);
        assert!(results.is_empty());
    }

    #[test]
    fn lod_selection_struct() {
        let sel = LodSelection {
            object_index: 0,
            lod_index: 1,
            screen_coverage: 0.15,
            culled: false,
        };
        assert_eq!(sel.lod_index, 1);
        assert!(!sel.culled);
    }
}
