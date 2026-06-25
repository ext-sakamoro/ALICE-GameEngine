//! Volumetric clouds — Horizon Zero Dawn / Wicked Engine style
//! raymarched cloud volume.
//!
//! Each cloud sample combines a low-frequency Perlin-like base shape
//! with a high-frequency Worley-like detail to break up silhouettes.
//! The density function is then marched along a view ray, accumulating
//! Beer's-law transmittance and Henyey-Greenstein scattering. The
//! whole module is CPU-only so it works for offline previews, sky
//! probes, and unit tests; a future PR can port the same density
//! function to a compute shader by re-using [`cloud_density`].
//!
//! Tuned for cheap correctness, not AAA shader quality — `step_count`
//! defaults to 16, plenty for sky thumbnails and probe capture.

use crate::math::{Color, Vec2, Vec3};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumetricCloudConfig {
    /// Macro cloud coverage in `[0, 1]`. 0 = clear sky, 1 = overcast.
    pub coverage: f32,
    /// Multiplier applied to the per-sample density.
    pub density_scale: f32,
    /// Wind XZ velocity (m/s). Translates the noise lookup over time.
    pub wind: Vec2,
    /// Cloud-layer bottom altitude (metres above origin).
    pub base_height: f32,
    /// Cloud-layer top altitude.
    pub max_height: f32,
    /// Number of march steps. Higher = smoother but slower.
    pub step_count: u32,
    /// Sun-direction approximation used for Beer's-law in-scattering.
    pub sun_direction: Vec3,
    /// Sun color × intensity.
    pub sun_color: Color,
}

impl Default for VolumetricCloudConfig {
    fn default() -> Self {
        Self {
            coverage: 0.5,
            density_scale: 1.0,
            wind: Vec2::new(2.0, 0.0),
            base_height: 1500.0,
            max_height: 3500.0,
            step_count: 16,
            sun_direction: Vec3::new(0.3, 0.6, -0.7),
            sun_color: Color::new(1.0, 0.95, 0.85, 1.0),
        }
    }
}

// ---------------------------------------------------------------------------
// Density function
// ---------------------------------------------------------------------------

/// Cheap deterministic noise (`hash`-style). Output in `[0, 1]`.
fn hash3(p: Vec3) -> f32 {
    let xy = p.x().mul_add(12.9898, p.y() * 78.233);
    let mut n = p.z().mul_add(37.719, xy).sin() * 43_758.547;
    n -= n.floor();
    n.abs()
}

/// Trilinear-interpolated value noise.
fn value_noise(p: Vec3) -> f32 {
    let pi = Vec3::new(p.x().floor(), p.y().floor(), p.z().floor());
    let pf = Vec3::new(p.x() - pi.x(), p.y() - pi.y(), p.z() - pi.z());
    let smooth = |t: f32| t * t * 2.0_f32.mul_add(-t, 3.0);
    let u = Vec3::new(smooth(pf.x()), smooth(pf.y()), smooth(pf.z()));

    let mut acc = 0.0_f32;
    for dz in 0..2 {
        for dy in 0..2 {
            for dx in 0..2 {
                let corner = Vec3::new(pi.x() + dx as f32, pi.y() + dy as f32, pi.z() + dz as f32);
                let w = (if dx == 0 { 1.0 - u.x() } else { u.x() })
                    * (if dy == 0 { 1.0 - u.y() } else { u.y() })
                    * (if dz == 0 { 1.0 - u.z() } else { u.z() });
                acc += hash3(corner) * w;
            }
        }
    }
    acc
}

/// 4-octave FBM in `[0, 1]`.
fn fbm(p: Vec3) -> f32 {
    let mut amp = 0.5_f32;
    let mut freq = 1.0_f32;
    let mut acc = 0.0_f32;
    let mut max_sum = 0.0_f32;
    for _ in 0..4 {
        acc += value_noise(p * freq) * amp;
        max_sum += amp;
        freq *= 2.0;
        amp *= 0.5;
    }
    acc / max_sum.max(1e-6)
}

/// Cloud density at a world-space point. Returns `0.0` outside the
/// configured altitude band. Inside the band, combines a base FBM
/// shape with high-frequency detail and clamps by `coverage`.
#[must_use]
pub fn cloud_density(world_pos: Vec3, time: f32, config: &VolumetricCloudConfig) -> f32 {
    let h = world_pos.y();
    if h < config.base_height || h > config.max_height {
        return 0.0;
    }
    // Wind translates the noise lookup.
    let wind_offset = config.wind * time;
    let lookup = Vec3::new(
        (world_pos.x() + wind_offset.x()) * 0.0008,
        h * 0.0012,
        (world_pos.z() + wind_offset.y()) * 0.0008,
    );
    let base = fbm(lookup);
    let detail = fbm(lookup * 4.0);
    // Smoothstep the base by coverage, then erode by the detail.
    let coverage = config.coverage.clamp(0.0, 1.0);
    let base_clipped = ((base - (1.0 - coverage)) / coverage.max(1e-3)).clamp(0.0, 1.0);
    let detail_erode = detail * 0.4;
    let raw = (base_clipped - detail_erode).max(0.0);

    // Height falloff: 0 at edges, 1 in the middle of the layer.
    let h_norm = (h - config.base_height) / (config.max_height - config.base_height).max(1e-3);
    let height_window = (h_norm * 2.0).min(2.0 - h_norm * 2.0).clamp(0.0, 1.0);

    raw * height_window * config.density_scale
}

// ---------------------------------------------------------------------------
// Raymarch
// ---------------------------------------------------------------------------

/// One column-march result: accumulated cloud RGB and transmittance.
#[derive(Debug, Clone, Copy)]
pub struct CloudRayResult {
    pub scattered: Color,
    pub transmittance: f32,
}

/// March a ray through the cloud volume.
///
/// `origin` is the ray's start (typically the camera). `dir` is the
/// ray's unit direction. Returns the in-scattered colour plus the
/// remaining transmittance (= multiply the sky behind the cloud by
/// this to composite).
#[must_use]
pub fn march_cloud_ray(
    origin: Vec3,
    dir: Vec3,
    time: f32,
    config: &VolumetricCloudConfig,
) -> CloudRayResult {
    // Trivial rejection for rays pointing away from the cloud layer.
    let dir_y = dir.y();
    if dir_y.abs() < 1e-4 {
        return CloudRayResult {
            scattered: Color::BLACK,
            transmittance: 1.0,
        };
    }
    let inv_dy = dir_y.recip();
    let t_low = (config.base_height - origin.y()) * inv_dy;
    let t_high = (config.max_height - origin.y()) * inv_dy;
    let (t_enter, t_exit) = if t_low < t_high {
        (t_low, t_high)
    } else {
        (t_high, t_low)
    };
    let t_enter = t_enter.max(0.0);
    if t_exit <= t_enter {
        return CloudRayResult {
            scattered: Color::BLACK,
            transmittance: 1.0,
        };
    }

    let span = t_exit - t_enter;
    let steps = config.step_count.max(1);
    let step_size = span / steps as f32;

    let mut accumulated = Color::BLACK;
    let mut transmittance = 1.0_f32;
    let sun_dot = dir.dot(config.sun_direction.normalize()).clamp(-1.0, 1.0);
    // Henyey-Greenstein approximation with g=0.2.
    let hg = (1.0 - 0.04) / 0.4_f32.mul_add(-sun_dot, 1.0 + 0.04).powf(1.5);

    for step in 0..steps {
        let t = t_enter + step_size * (step as f32 + 0.5);
        let world = origin + dir * t;
        let density = cloud_density(world, time, config);
        if density <= 1e-4 {
            continue;
        }
        let extinction = density * step_size * 0.01;
        let new_transmittance = transmittance * (-extinction).exp();
        let absorbed = transmittance - new_transmittance;
        accumulated.r += config.sun_color.r * absorbed * hg;
        accumulated.g += config.sun_color.g * absorbed * hg;
        accumulated.b += config.sun_color.b * absorbed * hg;
        transmittance = new_transmittance;
        if transmittance < 0.01 {
            break;
        }
    }

    CloudRayResult {
        scattered: accumulated,
        transmittance,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_partly_cloudy() {
        let c = VolumetricCloudConfig::default();
        assert!((c.coverage - 0.5).abs() < 1e-6);
        assert_eq!(c.step_count, 16);
        assert!(c.base_height < c.max_height);
    }

    #[test]
    fn cloud_density_zero_below_layer() {
        let c = VolumetricCloudConfig::default();
        let d = cloud_density(Vec3::new(0.0, 100.0, 0.0), 0.0, &c);
        assert!(d <= 1e-6);
    }

    #[test]
    fn cloud_density_zero_above_layer() {
        let c = VolumetricCloudConfig::default();
        let d = cloud_density(Vec3::new(0.0, 10_000.0, 0.0), 0.0, &c);
        assert!(d <= 1e-6);
    }

    #[test]
    fn cloud_density_nonzero_inside_layer_with_coverage() {
        let c = VolumetricCloudConfig {
            coverage: 1.0,
            ..VolumetricCloudConfig::default()
        };
        let d = cloud_density(Vec3::new(0.0, 2500.0, 0.0), 0.0, &c);
        assert!(d > 0.0, "density should be positive inside the layer");
    }

    #[test]
    fn zero_coverage_kills_clouds() {
        let c = VolumetricCloudConfig {
            coverage: 0.0,
            ..VolumetricCloudConfig::default()
        };
        let d = cloud_density(Vec3::new(0.0, 2500.0, 0.0), 0.0, &c);
        assert!(d <= 1e-3, "expected near-zero density, got {d}");
    }

    #[test]
    fn march_below_layer_upward_passes_through() {
        let c = VolumetricCloudConfig {
            coverage: 1.0,
            ..VolumetricCloudConfig::default()
        };
        let r = march_cloud_ray(Vec3::ZERO, Vec3::Y, 0.0, &c);
        // High coverage → some absorption → transmittance < 1.0.
        assert!(r.transmittance < 1.0);
        assert!(r.transmittance >= 0.0);
    }

    #[test]
    fn march_horizontal_ray_does_not_intersect() {
        let c = VolumetricCloudConfig::default();
        let r = march_cloud_ray(Vec3::ZERO, Vec3::X, 0.0, &c);
        assert!((r.transmittance - 1.0).abs() < 1e-4);
        assert_eq!(r.scattered.r, 0.0);
    }

    #[test]
    fn march_downward_above_cloud_intersects() {
        let c = VolumetricCloudConfig {
            coverage: 1.0,
            ..VolumetricCloudConfig::default()
        };
        let above = Vec3::new(0.0, 5000.0, 0.0);
        let r = march_cloud_ray(above, Vec3::new(0.0, -1.0, 0.0).normalize(), 0.0, &c);
        assert!(r.transmittance < 1.0);
    }

    #[test]
    fn higher_density_scale_absorbs_more() {
        let cfg_low = VolumetricCloudConfig {
            coverage: 1.0,
            density_scale: 0.1,
            ..VolumetricCloudConfig::default()
        };
        let cfg_hi = VolumetricCloudConfig {
            coverage: 1.0,
            density_scale: 5.0,
            ..VolumetricCloudConfig::default()
        };
        let lo = march_cloud_ray(Vec3::ZERO, Vec3::Y, 0.0, &cfg_low);
        let hi = march_cloud_ray(Vec3::ZERO, Vec3::Y, 0.0, &cfg_hi);
        assert!(hi.transmittance < lo.transmittance);
    }

    #[test]
    fn wind_shifts_density_over_time() {
        let c = VolumetricCloudConfig {
            coverage: 0.7,
            wind: Vec2::new(100.0, 0.0),
            ..VolumetricCloudConfig::default()
        };
        let p = Vec3::new(0.0, 2500.0, 0.0);
        let d0 = cloud_density(p, 0.0, &c);
        let d1 = cloud_density(p, 5.0, &c);
        assert!(
            (d0 - d1).abs() > 1e-3,
            "wind should advect noise sample (d0={d0}, d1={d1})",
        );
    }
}
