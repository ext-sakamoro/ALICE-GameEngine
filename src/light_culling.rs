//! Tiled (Forward+ style) CPU-side light culling.
//!
//! Splits the screen into uniform square tiles and, for each tile, lists
//! the indices of the lights whose bounding spheres overlap the tile in
//! screen space. The downstream deferred-lighting shader then only loops
//! over its tile's light list instead of every light in the scene,
//! which is the standard solution for scenes with dozens to thousands
//! of dynamic lights.
//!
//! This module performs the culling on the **CPU**: it's intended to be
//! the first stage. A future PR can add a compute-shader variant that
//! runs the same algorithm on the GPU directly against the depth buffer.
//!
//! ## Pipeline
//!
//! 1. Build a [`TiledLightCuller`] from screen size + config.
//! 2. Call [`TiledLightCuller::cull`] every frame with the camera's
//!    view / projection and the list of [`LightRenderData`] from
//!    [`crate::renderer::FrameContext`].
//! 3. The returned [`TileLightList`] holds (a) per-tile light index
//!    lists and (b) a separate list of directional-light indices that
//!    affect every tile (uploaded once per frame instead of per tile).
//!
//! ## Quick example
//!
//! ```rust
//! use alice_game_engine::light_culling::{LightCullingConfig, TiledLightCuller};
//! use alice_game_engine::renderer::LightRenderData;
//! use alice_game_engine::scene_graph::LightVariant;
//! use alice_game_engine::math::{Color, Mat4, Vec3};
//!
//! let culler = TiledLightCuller::new(LightCullingConfig::default(), 1920, 1080);
//! assert_eq!(culler.tile_count_x, 120);
//! assert_eq!(culler.tile_count_y, 68);
//!
//! let view = Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
//! let proj = Mat4::perspective(std::f32::consts::FRAC_PI_3, 16.0 / 9.0, 0.1, 100.0);
//! let result = culler.cull(&[], view, proj);
//! assert_eq!(result.tiles.len(), 120 * 68);
//! ```

use crate::math::{Mat4, Vec3};
use crate::renderer::LightRenderData;
use crate::scene_graph::LightVariant;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Tunable parameters for the tiled culler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LightCullingConfig {
    /// Tile edge in pixels. 16 is the industry default (Forward+, Wicked,
    /// Doom 2016 use this size). Smaller = finer cull but more tiles.
    pub tile_size: u32,
    /// Hard cap on lights per tile. When more lights cover a tile the
    /// culler keeps the `max_lights_per_tile` closest ones (by camera
    /// distance) and drops the rest.
    pub max_lights_per_tile: u32,
}

impl Default for LightCullingConfig {
    fn default() -> Self {
        Self {
            tile_size: 16,
            max_lights_per_tile: 64,
        }
    }
}

// ---------------------------------------------------------------------------
// Culler
// ---------------------------------------------------------------------------

/// Screen-space tiled light culler.
#[derive(Debug, Clone)]
pub struct TiledLightCuller {
    pub config: LightCullingConfig,
    pub screen_w: u32,
    pub screen_h: u32,
    pub tile_count_x: u32,
    pub tile_count_y: u32,
}

impl TiledLightCuller {
    /// Construct a culler for the given screen size. `screen_w` and
    /// `screen_h` may be any positive value; the tile counts round up
    /// so the rightmost / bottommost tiles may be partial.
    #[must_use]
    pub fn new(config: LightCullingConfig, screen_w: u32, screen_h: u32) -> Self {
        let tile_size = config.tile_size.max(1);
        let tile_count_x = screen_w.div_ceil(tile_size);
        let tile_count_y = screen_h.div_ceil(tile_size);
        Self {
            config,
            screen_w,
            screen_h,
            tile_count_x,
            tile_count_y,
        }
    }

    /// Returns the linear tile index for a screen pixel. Out-of-range
    /// coordinates are clamped to the last tile.
    #[must_use]
    pub fn tile_index_at(&self, screen_x: u32, screen_y: u32) -> u32 {
        let tile_size = self.config.tile_size.max(1);
        let tx = (screen_x / tile_size).min(self.tile_count_x.saturating_sub(1));
        let ty = (screen_y / tile_size).min(self.tile_count_y.saturating_sub(1));
        ty * self.tile_count_x + tx
    }

    /// Culls every light against every tile and returns a tile-indexed
    /// light list plus a separate directional-light list.
    #[must_use]
    pub fn cull(&self, lights: &[LightRenderData], view: Mat4, proj: Mat4) -> TileLightList {
        let total_tiles = (self.tile_count_x * self.tile_count_y) as usize;
        let mut tiles: Vec<Vec<u32>> = vec![Vec::new(); total_tiles];
        let mut directional: Vec<u32> = Vec::new();
        let view_proj = proj * view;

        for (light_idx_usize, light) in lights.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let light_idx = light_idx_usize as u32;

            // Directional lights affect every tile; recorded separately.
            let radius = match light.variant {
                LightVariant::Directional => {
                    directional.push(light_idx);
                    continue;
                }
                LightVariant::Point { radius } | LightVariant::Spot { radius, .. } => radius,
            };

            if radius <= 0.0 {
                continue;
            }

            // Project the bounding sphere to a screen-space AABB. The
            // approximation transforms the 8 corners of the world-space
            // AABB that bounds the sphere and takes their screen-space
            // extent — over-conservative but correct.
            let Some((min_tx, max_tx, min_ty, max_ty)) =
                sphere_to_tile_range(light.position, radius, view_proj, self)
            else {
                continue;
            };

            for ty in min_ty..=max_ty {
                for tx in min_tx..=max_tx {
                    let tile = (ty * self.tile_count_x + tx) as usize;
                    tiles[tile].push(light_idx);
                }
            }
        }

        // Enforce the per-tile cap, keeping the lights whose source is
        // closest to the camera (= largest brightness contribution).
        let cap = self.config.max_lights_per_tile as usize;
        if cap > 0 {
            let camera_pos = view.inverse().transform_point3(Vec3::ZERO);
            for tile in &mut tiles {
                if tile.len() > cap {
                    tile.sort_by(|&a, &b| {
                        let da = (lights[a as usize].position - camera_pos).length_squared();
                        let db = (lights[b as usize].position - camera_pos).length_squared();
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    tile.truncate(cap);
                }
            }
        }

        TileLightList {
            tile_count_x: self.tile_count_x,
            tile_count_y: self.tile_count_y,
            tiles,
            directional,
        }
    }
}

/// Project a world-space sphere to a screen-space tile range. Returns
/// `None` if the sphere falls completely off the screen or entirely
/// behind the camera.
fn sphere_to_tile_range(
    center: Vec3,
    radius: f32,
    view_proj: Mat4,
    culler: &TiledLightCuller,
) -> Option<(u32, u32, u32, u32)> {
    // 8 AABB corners around the sphere.
    let mut any_in_front = false;
    let mut screen_min_x = f32::INFINITY;
    let mut screen_max_x = f32::NEG_INFINITY;
    let mut screen_min_y = f32::INFINITY;
    let mut screen_max_y = f32::NEG_INFINITY;

    let signs = [-1.0_f32, 1.0];
    for &sx in &signs {
        for &sy in &signs {
            for &sz in &signs {
                let corner = Vec3::new(
                    sx.mul_add(radius, center.x()),
                    sy.mul_add(radius, center.y()),
                    sz.mul_add(radius, center.z()),
                );
                // Manual mat4 * vec4(corner, 1.0) and perspective divide:
                let v = glam::Vec4::new(corner.x(), corner.y(), corner.z(), 1.0);
                let clip = view_proj.0 * v;
                if clip.w <= 0.0 {
                    // Behind the camera; skip but allow other corners.
                    continue;
                }
                any_in_front = true;
                let inv_w = clip.w.recip();
                let ndc_x = clip.x * inv_w;
                let ndc_y = clip.y * inv_w;
                // NDC to [0, 1] screen, Y flipped to match top-left origin.
                let sx_pix = (ndc_x * 0.5 + 0.5) * (culler.screen_w as f32);
                let sy_pix = (0.5 - ndc_y * 0.5) * (culler.screen_h as f32);
                screen_min_x = screen_min_x.min(sx_pix);
                screen_max_x = screen_max_x.max(sx_pix);
                screen_min_y = screen_min_y.min(sy_pix);
                screen_max_y = screen_max_y.max(sy_pix);
            }
        }
    }

    if !any_in_front {
        return None;
    }

    // If every corner sits to one side of the screen, the sphere is
    // outside the frustum.
    let sw = culler.screen_w as f32;
    let sh = culler.screen_h as f32;
    if screen_max_x < 0.0 || screen_min_x > sw || screen_max_y < 0.0 || screen_min_y > sh {
        return None;
    }

    // Clamp to screen bounds, convert to tile indices.
    let tile_size = culler.config.tile_size.max(1);
    let max_tx = culler.tile_count_x.saturating_sub(1);
    let max_ty = culler.tile_count_y.saturating_sub(1);
    let min_tx_f = (screen_min_x.max(0.0) / (tile_size as f32)).floor();
    let max_tx_f = (screen_max_x.min(sw) / (tile_size as f32)).floor();
    let min_ty_f = (screen_min_y.max(0.0) / (tile_size as f32)).floor();
    let max_ty_f = (screen_max_y.min(sh) / (tile_size as f32)).floor();

    #[allow(clippy::cast_sign_loss)]
    let min_tx = (min_tx_f as u32).min(max_tx);
    #[allow(clippy::cast_sign_loss)]
    let max_tx_idx = (max_tx_f as u32).min(max_tx);
    #[allow(clippy::cast_sign_loss)]
    let min_ty = (min_ty_f as u32).min(max_ty);
    #[allow(clippy::cast_sign_loss)]
    let max_ty_idx = (max_ty_f as u32).min(max_ty);

    Some((min_tx, max_tx_idx, min_ty, max_ty_idx))
}

// ---------------------------------------------------------------------------
// TileLightList
// ---------------------------------------------------------------------------

/// Per-frame culling output: per-tile light index list + directional
/// lights that affect every tile.
#[derive(Debug, Clone)]
pub struct TileLightList {
    pub tile_count_x: u32,
    pub tile_count_y: u32,
    /// Length = `tile_count_x * tile_count_y`. Each entry holds the
    /// indices (into the original `lights` slice) of the lights whose
    /// bounding sphere overlaps that tile.
    pub tiles: Vec<Vec<u32>>,
    /// Indices of directional lights, recorded once instead of replicated
    /// into every tile.
    pub directional: Vec<u32>,
}

impl TileLightList {
    /// Tile lookup by linear index.
    #[must_use]
    pub fn tile(&self, tile_idx: usize) -> Option<&Vec<u32>> {
        self.tiles.get(tile_idx)
    }

    /// Sum of every per-tile entry (= total light×tile pairs). Useful for
    /// debug overlays.
    #[must_use]
    pub fn total_light_tile_pairs(&self) -> usize {
        self.tiles.iter().map(Vec::len).sum()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Color;

    fn point_light(pos: Vec3, radius: f32) -> LightRenderData {
        LightRenderData {
            position: pos,
            direction: Vec3::new(0.0, 0.0, -1.0),
            color: Color::WHITE,
            intensity: 1.0,
            variant: LightVariant::Point { radius },
            cast_shadows: false,
        }
    }

    fn directional_light() -> LightRenderData {
        LightRenderData {
            position: Vec3::ZERO,
            direction: Vec3::new(0.0, -1.0, 0.0),
            color: Color::WHITE,
            intensity: 1.0,
            variant: LightVariant::Directional,
            cast_shadows: true,
        }
    }

    fn camera() -> (Mat4, Mat4) {
        let view = Mat4::look_at(Vec3::new(0.0, 0.0, 5.0), Vec3::ZERO, Vec3::Y);
        let proj = Mat4::perspective(std::f32::consts::FRAC_PI_3, 16.0 / 9.0, 0.1, 100.0);
        (view, proj)
    }

    #[test]
    fn config_default_is_16_64() {
        let c = LightCullingConfig::default();
        assert_eq!(c.tile_size, 16);
        assert_eq!(c.max_lights_per_tile, 64);
    }

    #[test]
    fn tile_count_correct_for_1920x1080() {
        let culler = TiledLightCuller::new(LightCullingConfig::default(), 1920, 1080);
        assert_eq!(culler.tile_count_x, 120);
        assert_eq!(culler.tile_count_y, 68); // ceil(1080 / 16) = 68
    }

    #[test]
    fn tile_count_handles_non_multiple_screen() {
        // 1921 → ceil(1921/16) = 121, last tile partial.
        let culler = TiledLightCuller::new(LightCullingConfig::default(), 1921, 1080);
        assert_eq!(culler.tile_count_x, 121);
    }

    #[test]
    fn tile_index_at_clamps_out_of_range() {
        let culler = TiledLightCuller::new(LightCullingConfig::default(), 1920, 1080);
        let last = (culler.tile_count_y - 1) * culler.tile_count_x + (culler.tile_count_x - 1);
        assert_eq!(culler.tile_index_at(10_000, 10_000), last);
    }

    #[test]
    fn single_point_light_at_origin_covers_some_tiles() {
        let culler = TiledLightCuller::new(LightCullingConfig::default(), 1920, 1080);
        let (view, proj) = camera();
        let result = culler.cull(&[point_light(Vec3::ZERO, 1.0)], view, proj);
        let covered = result.total_light_tile_pairs();
        assert!(covered > 0, "expected the central light to cover ≥ 1 tile");
        assert_eq!(result.directional.len(), 0);
    }

    #[test]
    fn off_screen_point_light_is_culled() {
        let culler = TiledLightCuller::new(LightCullingConfig::default(), 1920, 1080);
        let (view, proj) = camera();
        // 50 units behind the camera with small radius → cull.
        let result = culler.cull(&[point_light(Vec3::new(0.0, 0.0, 100.0), 0.5)], view, proj);
        assert_eq!(result.total_light_tile_pairs(), 0);
    }

    #[test]
    fn huge_radius_light_covers_all_tiles() {
        let culler = TiledLightCuller::new(LightCullingConfig::default(), 320, 240);
        let (view, proj) = camera();
        // Massive radius surrounds the camera, projecting to a screen-
        // covering AABB.
        let result = culler.cull(&[point_light(Vec3::ZERO, 200.0)], view, proj);
        let total_tiles = (culler.tile_count_x * culler.tile_count_y) as usize;
        assert_eq!(result.total_light_tile_pairs(), total_tiles);
    }

    #[test]
    fn directional_light_listed_separately_not_in_tiles() {
        let culler = TiledLightCuller::new(LightCullingConfig::default(), 800, 600);
        let (view, proj) = camera();
        let result = culler.cull(&[directional_light()], view, proj);
        assert_eq!(result.directional, vec![0]);
        assert_eq!(result.total_light_tile_pairs(), 0);
    }

    #[test]
    fn mixed_directional_and_point_lights() {
        let culler = TiledLightCuller::new(LightCullingConfig::default(), 1920, 1080);
        let (view, proj) = camera();
        let lights = vec![
            directional_light(),                            // 0
            point_light(Vec3::ZERO, 1.0),                   // 1
            point_light(Vec3::new(100.0, 0.0, 100.0), 0.5), // 2 — far behind / off-screen
        ];
        let result = culler.cull(&lights, view, proj);
        assert_eq!(result.directional, vec![0]);
        let referenced: std::collections::HashSet<u32> =
            result.tiles.iter().flatten().copied().collect();
        assert!(referenced.contains(&1));
        // light 2 must not appear (it's behind the camera + radius < distance)
        assert!(!referenced.contains(&2));
        // Directional must never appear in per-tile lists.
        assert!(!referenced.contains(&0));
    }

    #[test]
    fn max_lights_per_tile_caps_with_distance_priority() {
        let config = LightCullingConfig {
            tile_size: 16,
            max_lights_per_tile: 2,
        };
        let culler = TiledLightCuller::new(config, 320, 240);
        let (view, proj) = camera();
        // Five enormous overlapping lights at varying camera distances.
        let lights = vec![
            point_light(Vec3::new(0.0, 0.0, 0.0), 100.0), // dist 5 from camera
            point_light(Vec3::new(0.0, 0.0, -5.0), 100.0), // dist 10
            point_light(Vec3::new(0.0, 0.0, 2.0), 100.0), // dist 3
            point_light(Vec3::new(0.0, 0.0, -10.0), 100.0), // dist 15
            point_light(Vec3::new(0.0, 0.0, 4.0), 100.0), // dist 1
        ];
        let result = culler.cull(&lights, view, proj);
        for tile in &result.tiles {
            assert!(
                tile.len() <= 2,
                "tile has {} lights, expected ≤ 2",
                tile.len()
            );
            if tile.len() == 2 {
                // Closest two are lights 4 (dist 1) and 2 (dist 3).
                let mut sorted = tile.clone();
                sorted.sort_unstable();
                assert_eq!(sorted, vec![2, 4]);
            }
        }
    }

    #[test]
    fn tile_lookup_helper() {
        let culler = TiledLightCuller::new(LightCullingConfig::default(), 800, 600);
        let (view, proj) = camera();
        let result = culler.cull(&[point_light(Vec3::ZERO, 1.0)], view, proj);
        assert!(result.tile(0).is_some());
        let total_tiles = (culler.tile_count_x * culler.tile_count_y) as usize;
        assert!(result.tile(total_tiles).is_none());
    }
}
