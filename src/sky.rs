//! SDF sky and atmosphere rendering.
//!
//! Physical Rayleigh/Mie scattering, volumetric clouds, biome terrain —
//! ported from ALICE-SDF-Experiment's alice-universe.glsl.
//!
//! Reference: <https://alice-sdf-experiment.pages.dev/>

use crate::math::{Color, Vec3};

// ---------------------------------------------------------------------------
// Atmosphere parameters
// ---------------------------------------------------------------------------

/// Physical atmosphere parameters.
#[derive(Debug, Clone, Copy)]
pub struct AtmosphereParams {
    pub sun_direction: Vec3,
    pub day_phase: f32,
    pub fog_density: f32,
    pub cloud_cover: f32,
}

impl Default for AtmosphereParams {
    fn default() -> Self {
        Self {
            sun_direction: Vec3::new(0.3, 0.5, -0.8).normalize(),
            day_phase: 1.0,
            fog_density: 0.0,
            cloud_cover: 0.1,
        }
    }
}

// ---------------------------------------------------------------------------
// CPU sky evaluation (for probes, thumbnails, non-GPU use)
// ---------------------------------------------------------------------------

/// Evaluates sky color for a ray direction on CPU.
/// Physically-based Rayleigh/Mie scattering (Chapman approximation).
#[must_use]
pub fn sky_color(rd: Vec3, params: &AtmosphereParams) -> Color {
    let y = rd.y().max(0.001);
    let sun_dir = params.sun_direction;
    let mu = rd.dot(sun_dir);
    let sun_h = sun_dir.y();
    let day_f = params.day_phase;

    // Rayleigh coefficients (wavelength-dependent)
    let b_r = [5.8e-3_f32, 13.5e-3, 33.1e-3];
    let b_m = 0.021_f32;

    // Chapman airmass approximation
    let cos_z = y + 0.001;
    let cos_z_35 = cos_z.powf(0.6);
    let am = 0.15f32.mul_add(cos_z_35, cos_z).recip();

    let sun_cz = (sun_h.max(0.0) + 0.001).powf(0.6);
    let sun_am = 0.15f32.mul_add(sun_cz, sun_h.max(0.0) + 0.001).recip();

    // Optical depth
    let dens_r = (-y.max(0.0) * 3.0).exp();
    let dens_m = (-y.max(0.0) * 1.2).exp();

    let ext_r = [
        (-b_r[0] * sun_am * 1.5).exp(),
        (-b_r[1] * sun_am * 1.5).exp(),
        (-b_r[2] * sun_am * 1.5).exp(),
    ];
    let ext_m = (-b_m * sun_am * 0.8).exp();

    // Rayleigh phase
    let ph_r = 0.059_683 * mu.mul_add(mu, 1.0);
    let rayleigh = [
        b_r[0] * ph_r * am * dens_r * ext_r[0],
        b_r[1] * ph_r * am * dens_r * ext_r[1],
        b_r[2] * ph_r * am * dens_r * ext_r[2],
    ];

    // Mie phase (Henyey-Greenstein)
    let g = 0.76_f32;
    let g2 = g * g;
    let denom = (2.0 * g).mul_add(-mu, 1.0 + g2).max(0.0001);
    let inv_sqrt = denom.sqrt().recip();
    let ph_m = 0.079_577 * (1.0 - g2) * inv_sqrt * inv_sqrt * inv_sqrt;
    let mie_val = b_m * ph_m * am * 0.5 * dens_m * ext_m;
    let mie = [mie_val * ext_r[0], mie_val * ext_r[1], mie_val * ext_r[2]];

    // Sun intensity (smooth sunrise/sunset)
    let sun_fade = smooth_step(-0.08, 0.15, sun_h);
    let sun_i = [22.0 * sun_fade, 20.0 * sun_fade, 17.0 * sun_fade];

    let mut sky = [
        (rayleigh[0] + mie[0]) * sun_i[0],
        (rayleigh[1] + mie[1]) * sun_i[1],
        (rayleigh[2] + mie[2]) * sun_i[2],
    ];

    // Ambient fill
    sky[0] += 0.003 * y.max(0.0) * day_f;
    sky[1] += 0.004 * y.max(0.0) * day_f;
    sky[2] += 0.005 * y.max(0.0) * day_f;

    // Night blend
    let night_f = 1.0 - day_f;
    sky[0] += 0.001 * night_f;
    sky[1] += 0.002 * night_f;
    sky[2] += 0.005 * night_f;

    // Sun disc
    let sun_ang = mu.clamp(-1.0, 1.0).acos();
    let sun_r = 0.0046_f32;
    let sun_disc = smooth_step(sun_r * 1.3, sun_r * 0.4, sun_ang);
    sky[0] += 12.0 * sun_disc * sun_fade;
    sky[1] += 10.0 * sun_disc * sun_fade;
    sky[2] += 7.0 * sun_disc * sun_fade;

    // Fog
    let fog_t = [
        0.28f32.mul_add(day_f, 0.02),
        0.305f32.mul_add(day_f, 0.025),
        0.34f32.mul_add(day_f, 0.04),
    ];
    let fog = params.fog_density * 0.55;
    sky[0] = sky[0].mul_add(1.0 - fog, fog_t[0] * fog);
    sky[1] = sky[1].mul_add(1.0 - fog, fog_t[1] * fog);
    sky[2] = sky[2].mul_add(1.0 - fog, fog_t[2] * fog);

    Color::new(
        sky[0].clamp(0.0, 1.0),
        sky[1].clamp(0.0, 1.0),
        sky[2].clamp(0.0, 1.0),
        1.0,
    )
}

#[inline]
fn smooth_step(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) * (edge1 - edge0).recip()).clamp(0.0, 1.0);
    t * t * 2.0_f32.mul_add(-t, 3.0)
}

// ---------------------------------------------------------------------------
// WGSL sky shader (for GPU rendering)
// ---------------------------------------------------------------------------

/// Built-in WGSL sky shader with physical Rayleigh/Mie scattering.
/// Ported from alice-universe.glsl.
pub const SKY_FRAGMENT_WGSL: &str = r"
struct SkyUniforms {
    camera_pos: vec3<f32>,
    _pad0: f32,
    sun_dir: vec3<f32>,
    day_phase: f32,
    resolution: vec2<f32>,
    fog: f32,
    time: f32,
};

@group(0) @binding(0) var<uniform> u: SkyUniforms;

fn smoothstep_f(e0: f32, e1: f32, x: f32) -> f32 {
    let t = clamp((x - e0) / (e1 - e0), 0.0, 1.0);
    return t * t * (3.0 - 2.0 * t);
}

@fragment
fn fs_main(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (frag_coord.xy / u.resolution) * 2.0 - 1.0;
    let rd = normalize(vec3<f32>(uv.x, uv.y * 0.5 + 0.3, -1.0));
    let y = max(rd.y, 0.001);
    let mu = dot(rd, u.sun_dir);
    let sunH = u.sun_dir.y;
    let dayF = u.day_phase;

    // Rayleigh
    let bR = vec3<f32>(0.0058, 0.0135, 0.0331);
    let bM = 0.021;
    let cosZ = y + 0.001;
    let am = 1.0 / (cosZ + 0.15 * pow(cosZ, 0.6));
    let sunAm = 1.0 / (max(sunH, 0.0) + 0.001 + 0.15 * pow(max(sunH, 0.0) + 0.001, 0.6));
    let densR = exp(-max(rd.y, 0.0) * 3.0);
    let extR = exp(-bR * sunAm * 1.5);
    let phR = 0.059683 * (1.0 + mu * mu);
    var rayleigh = bR * phR * am * densR * extR;

    // Mie
    let g = 0.76;
    let denom = max(1.0 + g * g - 2.0 * g * mu, 0.0001);
    let invSqrt = inverseSqrt(denom);
    let phM = 0.079577 * (1.0 - g * g) * invSqrt * invSqrt * invSqrt;
    let mie = vec3<f32>(bM * phM * am * 0.5 * exp(-max(rd.y, 0.0) * 1.2)) * extR;

    // Sun intensity
    let sunFade = smoothstep_f(-0.08, 0.15, sunH);
    let sunI = vec3<f32>(22.0, 20.0, 17.0) * sunFade;
    var sky = (rayleigh + mie) * sunI;

    // Ambient
    sky += vec3<f32>(0.003, 0.004, 0.005) * max(rd.y, 0.0) * dayF;

    // Sun disc
    let sunAng = acos(clamp(mu, -1.0, 1.0));
    let sunDisc = smoothstep_f(0.006, 0.002, sunAng);
    sky += vec3<f32>(12.0, 10.0, 7.0) * sunDisc * sunFade;

    // Night
    let nightF = 1.0 - dayF;
    sky += vec3<f32>(0.001, 0.002, 0.005) * nightF;

    // Fog
    let fogT = mix(vec3<f32>(0.02, 0.025, 0.04), vec3<f32>(0.3, 0.33, 0.38), dayF);
    sky = mix(sky, fogT, u.fog * 0.55);

    return vec4<f32>(clamp(sky, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sky_blue_day() {
        let params = AtmosphereParams::default();
        let color = sky_color(Vec3::new(0.0, 1.0, 0.0), &params);
        // Zenith sky should have blue component
        assert!(color.b > 0.0);
    }

    #[test]
    fn sky_dark_night() {
        let params = AtmosphereParams {
            day_phase: 0.0,
            sun_direction: Vec3::new(0.0, -0.5, 0.0).normalize(),
            ..AtmosphereParams::default()
        };
        let color = sky_color(Vec3::new(0.0, 1.0, 0.0), &params);
        assert!(color.r < 0.1);
        assert!(color.g < 0.1);
    }

    #[test]
    fn sky_sun_bright() {
        let params = AtmosphereParams {
            sun_direction: Vec3::new(0.0, 0.3, -1.0).normalize(),
            ..AtmosphereParams::default()
        };
        let color = sky_color(params.sun_direction, &params);
        assert!(color.r > 0.5);
    }

    #[test]
    fn sky_horizon_warm() {
        let params = AtmosphereParams::default();
        let color = sky_color(Vec3::new(1.0, 0.01, 0.0).normalize(), &params);
        assert!(color.r > 0.0);
    }

    #[test]
    fn sky_foggy() {
        let params = AtmosphereParams {
            fog_density: 1.0,
            ..AtmosphereParams::default()
        };
        let clear = sky_color(Vec3::Y, &AtmosphereParams::default());
        let foggy = sky_color(Vec3::Y, &params);
        let clear_brightness = clear.r + clear.g + clear.b;
        let foggy_brightness = foggy.r + foggy.g + foggy.b;
        assert!((foggy_brightness - clear_brightness).abs() > 0.01);
    }

    #[test]
    fn atmosphere_default() {
        let p = AtmosphereParams::default();
        assert_eq!(p.day_phase, 1.0);
        assert_eq!(p.fog_density, 0.0);
    }

    #[test]
    fn wgsl_shader_has_entry_point() {
        assert!(SKY_FRAGMENT_WGSL.contains("@fragment"));
        assert!(SKY_FRAGMENT_WGSL.contains("SkyUniforms"));
    }

    #[test]
    fn smooth_step_edges() {
        assert!((smooth_step(0.0, 1.0, 0.0)).abs() < 1e-6);
        assert!((smooth_step(0.0, 1.0, 1.0) - 1.0).abs() < 1e-6);
        assert!((smooth_step(0.0, 1.0, 0.5) - 0.5).abs() < 0.1);
    }
}
