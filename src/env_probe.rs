//! Environment probes — cubemap-based image-based lighting (IBL).
//!
//! An environment probe captures the indirect-lighting environment at a
//! point in the scene. The captured cubemap is then pre-convolved into
//! two derived cubemaps:
//!
//! - **Irradiance** (low-resolution, cosine-weighted) for diffuse IBL.
//! - **Radiance mip chain** (one cubemap per roughness level) for
//!   specular IBL via the split-sum approximation.
//!
//! Capturing the source cubemap from the actual scene requires the GPU
//! renderer (6 face passes through a render target); that integration
//! lives in the renderer wiring. This module owns the **data
//! structures + pre-filter math + IBL lookup helpers** so probes can be
//! authored, transported, and queried entirely on the CPU.

use serde::{Deserialize, Serialize};

use crate::math::{Color, Vec3};

// ---------------------------------------------------------------------------
// EnvProbeData (scene-graph payload)
// ---------------------------------------------------------------------------

/// Per-probe configuration stored inside a scene graph node.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnvProbeData {
    /// Cubemap edge resolution. 32 is plenty for diffuse-only IBL;
    /// bump to 64 / 128 for higher-quality specular reflections.
    pub resolution: u32,
    /// World-space sphere of influence — fragments outside the sphere
    /// fall back to another probe (or to ambient).
    pub influence_radius: f32,
    /// Recapture every frame? Off by default — probes are one-shot.
    pub capture_dynamic: bool,
    /// Number of radiance mip levels for the specular IBL chain.
    pub radiance_mip_count: u32,
}

impl Default for EnvProbeData {
    fn default() -> Self {
        Self {
            resolution: 32,
            influence_radius: 20.0,
            capture_dynamic: false,
            radiance_mip_count: 5,
        }
    }
}

// ---------------------------------------------------------------------------
// Cubemap
// ---------------------------------------------------------------------------

/// Six-face cube map stored as RGBA32F. Face order follows the standard
/// (and Vulkan / DirectX / wgpu) layer order:
///
/// 0 = `+X`, 1 = `-X`, 2 = `+Y`, 3 = `-Y`, 4 = `+Z`, 5 = `-Z`.
#[derive(Debug, Clone)]
pub struct Cubemap {
    pub resolution: u32,
    pub faces: [Vec<f32>; 6],
}

impl Cubemap {
    /// Allocate a cubemap and fill every texel with `color`.
    #[must_use]
    pub fn new_with_color(resolution: u32, color: Color) -> Self {
        let n = (resolution as usize) * (resolution as usize);
        let face = || {
            let mut buf = Vec::with_capacity(n * 4);
            for _ in 0..n {
                buf.extend_from_slice(&color.to_array());
            }
            buf
        };
        Self {
            resolution,
            faces: [face(), face(), face(), face(), face(), face()],
        }
    }

    /// Allocate a cubemap with a distinct constant color per face.
    /// Useful for debugging and the demo example.
    #[must_use]
    pub fn new_per_face_color(resolution: u32, colors: [Color; 6]) -> Self {
        let n = (resolution as usize) * (resolution as usize);
        let mut faces: [Vec<f32>; 6] = std::array::from_fn(|_| Vec::with_capacity(n * 4));
        for (i, c) in colors.iter().enumerate() {
            for _ in 0..n {
                faces[i].extend_from_slice(&c.to_array());
            }
        }
        Self { resolution, faces }
    }

    /// Sample the cubemap with a unit (or non-unit) direction vector.
    /// Bilinear within a face; no cross-face filtering.
    #[must_use]
    pub fn sample(&self, dir: Vec3) -> Color {
        let (face, uv) = direction_to_face_uv(dir);
        let (texel_r, texel_g, texel_b, texel_a) =
            sample_face_bilinear(&self.faces[face], self.resolution, uv.0, uv.1);
        Color::new(texel_r, texel_g, texel_b, texel_a)
    }
}

// ---------------------------------------------------------------------------
// Cubemap capture (sky → cubemap)
// ---------------------------------------------------------------------------

/// World-space view matrices for the six cube faces, centred at
/// `position`. Order matches [`Cubemap`]'s face indexing
/// (+X / -X / +Y / -Y / +Z / -Z).
#[must_use]
pub fn cubemap_face_views(position: crate::math::Vec3) -> [crate::math::Mat4; 6] {
    use crate::math::{Mat4, Vec3};
    let p = position;
    [
        // +X: look down +X, up is -Y so the texture is right-handed.
        Mat4::look_at(p, p + Vec3::new(1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
        // -X
        Mat4::look_at(p, p + Vec3::new(-1.0, 0.0, 0.0), Vec3::new(0.0, -1.0, 0.0)),
        // +Y
        Mat4::look_at(p, p + Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.0, 0.0, 1.0)),
        // -Y
        Mat4::look_at(p, p + Vec3::new(0.0, -1.0, 0.0), Vec3::new(0.0, 0.0, -1.0)),
        // +Z
        Mat4::look_at(p, p + Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.0, -1.0, 0.0)),
        // -Z
        Mat4::look_at(p, p + Vec3::new(0.0, 0.0, -1.0), Vec3::new(0.0, -1.0, 0.0)),
    ]
}

/// Perspective + view matrix pair for one cube face, useful when
/// driving the GPU 6-face render pipeline. The projection is a 90-deg
/// FOV square frustum (= the standard cubemap capture rig).
#[derive(Debug, Clone, Copy)]
pub struct CubemapFaceCamera {
    pub view: crate::math::Mat4,
    pub projection: crate::math::Mat4,
}

/// Build the six per-face cameras for a probe at `position`. Pair with
/// the engine renderer to render the scene six times into a
/// `wgpu::TextureView` array (= the GPU-side companion to
/// [`capture_sky_to_cubemap`]).
#[must_use]
pub fn cubemap_face_cameras(
    position: crate::math::Vec3,
    near: f32,
    far: f32,
) -> [CubemapFaceCamera; 6] {
    let views = cubemap_face_views(position);
    let projection = crate::math::Mat4::perspective(std::f32::consts::FRAC_PI_2, 1.0, near, far);
    std::array::from_fn(|i| CubemapFaceCamera {
        view: views[i],
        projection,
    })
}

/// Description of the GPU side of a 6-face cubemap capture. Wraps a
/// `wgpu::Texture` (= 6-layer 2D texture in `Cube` array layout), one
/// `TextureView` per face, plus the per-face cameras.
///
/// Application code drives the capture by:
/// 1. Calling [`CubemapCaptureTargets::new`] once when the probe is
///    spawned (= allocates the GPU resources).
/// 2. Each frame the renderer should run the existing deferred pass
///    once per face, attaching the matching `face_views[i]` as the
///    color target and using `cameras[i].view` /
///    `cameras[i].projection` as the camera state.
/// 3. The completed cubemap is sampled via `texture_view` as a
///    `texture_cube<f32>` binding.
#[cfg(feature = "gpu")]
pub struct CubemapCaptureTargets {
    pub texture: wgpu::Texture,
    pub texture_view: wgpu::TextureView,
    pub face_views: [wgpu::TextureView; 6],
    pub cameras: [CubemapFaceCamera; 6],
    pub resolution: u32,
}

#[cfg(feature = "gpu")]
impl CubemapCaptureTargets {
    /// Allocate the GPU resources for a probe at `position`.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        position: crate::math::Vec3,
        resolution: u32,
        near: f32,
        far: f32,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("alice-env-probe-cubemap"),
            size: wgpu::Extent3d {
                width: resolution,
                height: resolution,
                depth_or_array_layers: 6,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("alice-env-probe-cubemap-view"),
            dimension: Some(wgpu::TextureViewDimension::Cube),
            ..Default::default()
        });
        let face_views: [wgpu::TextureView; 6] = std::array::from_fn(|i| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("alice-env-probe-face-view"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: i as u32,
                array_layer_count: Some(1),
                ..Default::default()
            })
        });
        Self {
            texture,
            texture_view,
            face_views,
            cameras: cubemap_face_cameras(position, near, far),
            resolution,
        }
    }
}

/// Captures the [`sky_color`](crate::sky::sky_color) atmosphere into a
/// six-face cubemap. Used to seed an environment probe before the full
/// GPU 6-face render pass lands.
#[must_use]
pub fn capture_sky_to_cubemap(
    resolution: u32,
    atmosphere: &crate::sky::AtmosphereParams,
) -> Cubemap {
    let mut cube = Cubemap::new_with_color(resolution, Color::BLACK);
    let inv_n = (resolution as f32).recip();
    for face in 0..6_usize {
        for y in 0..resolution {
            for x in 0..resolution {
                let u = (x as f32 + 0.5) * inv_n;
                let v = (y as f32 + 0.5) * inv_n;
                let dir = face_uv_to_direction(face, u, v).normalize();
                let color = crate::sky::sky_color(dir, atmosphere);
                let idx = ((y * resolution + x) * 4) as usize;
                cube.faces[face][idx] = color.r;
                cube.faces[face][idx + 1] = color.g;
                cube.faces[face][idx + 2] = color.b;
                cube.faces[face][idx + 3] = 1.0;
            }
        }
    }
    cube
}

// ---------------------------------------------------------------------------
// Prefiltered probe
// ---------------------------------------------------------------------------

/// A captured environment probe after prefiltering. `irradiance` is the
/// low-resolution diffuse IBL map; `radiance_mips[0]` is the sharpest
/// specular reflection (roughness 0) and the last entry is the most
/// blurred (roughness 1).
#[derive(Debug, Clone)]
pub struct PrefilteredEnvProbe {
    pub position: Vec3,
    pub influence_radius: f32,
    pub irradiance: Cubemap,
    pub radiance_mips: Vec<Cubemap>,
}

/// Convolve a source cubemap with a cosine kernel to produce a
/// diffuse-IBL irradiance map. Brute-force CPU implementation:
/// `O(output_res² × 6 × source_res² × 6)` — fine for 8/16/32 px
/// outputs, prohibitive for large grids (use a compute shader then).
#[must_use]
pub fn prefilter_irradiance(env: &Cubemap, output_resolution: u32) -> Cubemap {
    let mut out = Cubemap::new_with_color(output_resolution, Color::BLACK);
    let inv_n = (output_resolution as f32).recip();
    for face in 0..6 {
        for y in 0..output_resolution {
            for x in 0..output_resolution {
                // Map output texel center to a direction.
                let u = ((x as f32) + 0.5) * inv_n;
                let v = ((y as f32) + 0.5) * inv_n;
                let normal = face_uv_to_direction(face, u, v).normalize();
                let acc = integrate_cosine(env, normal, 8);
                let idx = ((y * output_resolution + x) * 4) as usize;
                out.faces[face][idx] = acc.r;
                out.faces[face][idx + 1] = acc.g;
                out.faces[face][idx + 2] = acc.b;
                out.faces[face][idx + 3] = 1.0;
            }
        }
    }
    out
}

/// Produce a radiance mip chain from a source cubemap. Each successive
/// mip is convolved with a wider GGX lobe corresponding to a higher
/// roughness; `mip_count` levels span roughness `[0, 1]`.
#[must_use]
pub fn prefilter_radiance(env: &Cubemap, mip_count: u32) -> Vec<Cubemap> {
    let mut mips = Vec::with_capacity(mip_count as usize);
    for level in 0..mip_count {
        let roughness = if mip_count == 1 {
            0.0
        } else {
            (level as f32) / (mip_count as f32 - 1.0)
        };
        // Smaller texture per mip is the standard split-sum convention.
        let res = (env.resolution >> level).max(1);
        let mut out = Cubemap::new_with_color(res, Color::BLACK);
        let inv_res = (res as f32).recip();
        for face in 0..6 {
            for y in 0..res {
                for x in 0..res {
                    let u = ((x as f32) + 0.5) * inv_res;
                    let v = ((y as f32) + 0.5) * inv_res;
                    let normal = face_uv_to_direction(face, u, v).normalize();
                    // Roughness 0 → pass through, otherwise widening
                    // Phong-like lobe (cheap GGX approximation).
                    let exponent = ((1.0 - roughness).max(0.05) * 128.0).max(1.0);
                    let acc = integrate_phong(env, normal, exponent, 8);
                    let idx = ((y * res + x) * 4) as usize;
                    out.faces[face][idx] = acc.r;
                    out.faces[face][idx + 1] = acc.g;
                    out.faces[face][idx + 2] = acc.b;
                    out.faces[face][idx + 3] = 1.0;
                }
            }
        }
        mips.push(out);
    }
    mips
}

// ---------------------------------------------------------------------------
// Direction ↔ face/UV conversion
// ---------------------------------------------------------------------------

/// Convert a world-space direction to the cubemap face index and
/// per-face UV in `[0, 1]`.
fn direction_to_face_uv(dir: Vec3) -> (usize, (f32, f32)) {
    let ax = dir.x().abs();
    let ay = dir.y().abs();
    let az = dir.z().abs();
    let (face, sc, tc, ma) = if ax >= ay && ax >= az {
        if dir.x() > 0.0 {
            (0_usize, -dir.z(), -dir.y(), ax)
        } else {
            (1, dir.z(), -dir.y(), ax)
        }
    } else if ay >= ax && ay >= az {
        if dir.y() > 0.0 {
            (2, dir.x(), dir.z(), ay)
        } else {
            (3, dir.x(), -dir.z(), ay)
        }
    } else if dir.z() > 0.0 {
        (4, dir.x(), -dir.y(), az)
    } else {
        (5, -dir.x(), -dir.y(), az)
    };
    let inv_ma = ma.recip();
    let u = sc.mul_add(inv_ma, 1.0) * 0.5;
    let v = tc.mul_add(inv_ma, 1.0) * 0.5;
    (face, (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)))
}

/// Inverse of [`direction_to_face_uv`].
fn face_uv_to_direction(face: usize, u: f32, v: f32) -> Vec3 {
    let sc = u.mul_add(2.0, -1.0);
    let tc = v.mul_add(2.0, -1.0);
    match face {
        0 => Vec3::new(1.0, -tc, -sc),  // +X
        1 => Vec3::new(-1.0, -tc, sc),  // -X
        2 => Vec3::new(sc, 1.0, tc),    // +Y
        3 => Vec3::new(sc, -1.0, -tc),  // -Y
        4 => Vec3::new(sc, -tc, 1.0),   // +Z
        _ => Vec3::new(-sc, -tc, -1.0), // -Z
    }
}

fn sample_face_bilinear(face: &[f32], resolution: u32, u: f32, v: f32) -> (f32, f32, f32, f32) {
    let n = resolution as f32;
    let x = u.mul_add(n, -0.5).clamp(0.0, n - 1.0);
    let y = v.mul_add(n, -0.5).clamp(0.0, n - 1.0);
    let x0 = x.floor();
    let y0 = y.floor();
    let dx = x - x0;
    let dy = y - y0;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let xi = x0 as u32;
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let yi = y0 as u32;
    let x1 = (xi + 1).min(resolution - 1);
    let y1 = (yi + 1).min(resolution - 1);
    let fetch = |fx: u32, fy: u32| -> (f32, f32, f32, f32) {
        let i = ((fy * resolution + fx) * 4) as usize;
        (face[i], face[i + 1], face[i + 2], face[i + 3])
    };
    let (r00, g00, b00, a00) = fetch(xi, yi);
    let (r10, g10, b10, a10) = fetch(x1, yi);
    let (r01, g01, b01, a01) = fetch(xi, y1);
    let (r11, g11, b11, a11) = fetch(x1, y1);
    let mix = |a: f32, b: f32, t: f32| -> f32 { (b - a).mul_add(t, a) };
    let r = mix(mix(r00, r10, dx), mix(r01, r11, dx), dy);
    let g = mix(mix(g00, g10, dx), mix(g01, g11, dx), dy);
    let b = mix(mix(b00, b10, dx), mix(b01, b11, dx), dy);
    let a = mix(mix(a00, a10, dx), mix(a01, a11, dx), dy);
    (r, g, b, a)
}

// ---------------------------------------------------------------------------
// Cosine / Phong integration (cheap CPU prefilter)
// ---------------------------------------------------------------------------

fn integrate_cosine(env: &Cubemap, normal: Vec3, samples_per_axis: u32) -> Color {
    let mut acc_r = 0.0_f32;
    let mut acc_g = 0.0_f32;
    let mut acc_b = 0.0_f32;
    let mut weight = 0.0_f32;
    let n = samples_per_axis as f32;
    for j in 0..samples_per_axis {
        for i in 0..samples_per_axis {
            let phi = (i as f32 + 0.5) / n * std::f32::consts::TAU;
            let theta = (j as f32 + 0.5) / n * std::f32::consts::FRAC_PI_2;
            let (sin_t, cos_t) = theta.sin_cos();
            let (sin_p, cos_p) = phi.sin_cos();
            // Tangent-space sample, transformed to world via Frisvad.
            let local = Vec3::new(sin_t * cos_p, cos_t, sin_t * sin_p);
            let world = orient_along(normal, local);
            let s = env.sample(world);
            acc_r += s.r * cos_t;
            acc_g += s.g * cos_t;
            acc_b += s.b * cos_t;
            weight += cos_t;
        }
    }
    let inv_w = weight.recip();
    Color::new(acc_r * inv_w, acc_g * inv_w, acc_b * inv_w, 1.0)
}

fn integrate_phong(env: &Cubemap, normal: Vec3, exponent: f32, samples_per_axis: u32) -> Color {
    let mut acc_r = 0.0_f32;
    let mut acc_g = 0.0_f32;
    let mut acc_b = 0.0_f32;
    let mut weight = 0.0_f32;
    let n = samples_per_axis as f32;
    for j in 0..samples_per_axis {
        for i in 0..samples_per_axis {
            let phi = (i as f32 + 0.5) / n * std::f32::consts::TAU;
            let theta = (j as f32 + 0.5) / n * std::f32::consts::FRAC_PI_2;
            let (sin_t, cos_t) = theta.sin_cos();
            let (sin_p, cos_p) = phi.sin_cos();
            let local = Vec3::new(sin_t * cos_p, cos_t, sin_t * sin_p);
            let world = orient_along(normal, local);
            let w = cos_t.powf(exponent);
            let s = env.sample(world);
            acc_r += s.r * w;
            acc_g += s.g * w;
            acc_b += s.b * w;
            weight += w;
        }
    }
    let inv_w = weight.max(1e-6).recip();
    Color::new(acc_r * inv_w, acc_g * inv_w, acc_b * inv_w, 1.0)
}

/// Frisvad-style orthonormal basis around `normal`, then transform
/// `local` (in that basis) to world space.
fn orient_along(normal: Vec3, local: Vec3) -> Vec3 {
    let n = normal.normalize();
    let (tangent, bitangent) = if n.z().abs() < 0.999 {
        let t = Vec3::new(-n.y(), n.x(), 0.0).normalize();
        let b = n.cross(t);
        (t, b)
    } else {
        let t = Vec3::new(0.0, -n.z(), n.y()).normalize();
        let b = n.cross(t);
        (t, b)
    };
    tangent * local.x() + n * local.y() + bitangent * local.z()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_probe_data_default() {
        let d = EnvProbeData::default();
        assert_eq!(d.resolution, 32);
        assert!((d.influence_radius - 20.0).abs() < 1e-6);
        assert!(!d.capture_dynamic);
        assert_eq!(d.radiance_mip_count, 5);
    }

    #[test]
    fn cubemap_new_uniform_color_fills_all_faces() {
        let cube = Cubemap::new_with_color(4, Color::new(0.5, 0.25, 0.125, 1.0));
        assert_eq!(cube.resolution, 4);
        for face in &cube.faces {
            assert_eq!(face.len(), 4 * 4 * 4); // res² × RGBA
            for chunk in face.chunks_exact(4) {
                assert!((chunk[0] - 0.5).abs() < 1e-6);
                assert!((chunk[1] - 0.25).abs() < 1e-6);
                assert!((chunk[2] - 0.125).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn sample_returns_face_color_for_axis_directions() {
        let faces = [
            Color::RED,                     // +X
            Color::new(0.0, 1.0, 1.0, 1.0), // -X
            Color::GREEN,                   // +Y
            Color::new(1.0, 0.0, 1.0, 1.0), // -Y
            Color::BLUE,                    // +Z
            Color::new(1.0, 1.0, 0.0, 1.0), // -Z
        ];
        let cube = Cubemap::new_per_face_color(4, faces);
        let cases: [(Vec3, Color); 6] = [
            (Vec3::new(1.0, 0.0, 0.0), faces[0]),
            (Vec3::new(-1.0, 0.0, 0.0), faces[1]),
            (Vec3::new(0.0, 1.0, 0.0), faces[2]),
            (Vec3::new(0.0, -1.0, 0.0), faces[3]),
            (Vec3::new(0.0, 0.0, 1.0), faces[4]),
            (Vec3::new(0.0, 0.0, -1.0), faces[5]),
        ];
        for (dir, expected) in cases {
            let got = cube.sample(dir);
            assert!((got.r - expected.r).abs() < 1e-3);
            assert!((got.g - expected.g).abs() < 1e-3);
            assert!((got.b - expected.b).abs() < 1e-3);
        }
    }

    #[test]
    fn sample_normalises_non_unit_direction() {
        let cube = Cubemap::new_per_face_color(
            4,
            [
                Color::RED,
                Color::BLACK,
                Color::BLACK,
                Color::BLACK,
                Color::BLACK,
                Color::BLACK,
            ],
        );
        let got = cube.sample(Vec3::new(10.0, 0.0, 0.0));
        assert!((got.r - 1.0).abs() < 1e-3);
    }

    #[test]
    fn prefilter_irradiance_uniform_input_gives_uniform_output() {
        let src = Cubemap::new_with_color(8, Color::new(0.5, 0.5, 0.5, 1.0));
        let out = prefilter_irradiance(&src, 4);
        for face in &out.faces {
            for chunk in face.chunks_exact(4) {
                assert!(
                    (chunk[0] - 0.5).abs() < 5e-2,
                    "irradiance of uniform input should be input, got {}",
                    chunk[0],
                );
            }
        }
    }

    #[test]
    fn prefilter_radiance_mip_chain_lengths_decrease() {
        let src = Cubemap::new_with_color(16, Color::WHITE);
        let mips = prefilter_radiance(&src, 5);
        assert_eq!(mips.len(), 5);
        assert_eq!(mips[0].resolution, 16);
        assert_eq!(mips[1].resolution, 8);
        assert_eq!(mips[2].resolution, 4);
        assert_eq!(mips[3].resolution, 2);
        assert_eq!(mips[4].resolution, 1);
    }

    #[test]
    fn prefiltered_env_probe_struct_round_trip() {
        let src = Cubemap::new_with_color(4, Color::new(0.2, 0.3, 0.4, 1.0));
        let probe = PrefilteredEnvProbe {
            position: Vec3::new(1.0, 2.0, 3.0),
            influence_radius: 5.0,
            irradiance: prefilter_irradiance(&src, 2),
            radiance_mips: prefilter_radiance(&src, 3),
        };
        assert_eq!(probe.radiance_mips.len(), 3);
        let s = probe.irradiance.sample(Vec3::Y);
        assert!((s.r - 0.2).abs() < 5e-2);
    }

    #[test]
    fn face_uv_round_trip_for_each_face_center() {
        // Each face center maps back to the canonical axis direction.
        let expected = [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(0.0, 0.0, -1.0),
        ];
        for (face, axis) in expected.iter().enumerate() {
            let dir = face_uv_to_direction(face, 0.5, 0.5).normalize();
            assert!(
                (dir - *axis).length() < 1e-3,
                "face {face}: got {dir:?}, want {axis:?}",
            );
        }
    }

    #[test]
    fn env_probe_data_serde_round_trip() {
        let d = EnvProbeData {
            resolution: 64,
            influence_radius: 10.0,
            capture_dynamic: true,
            radiance_mip_count: 4,
        };
        let json = serde_json::to_string(&d).unwrap();
        let back: EnvProbeData = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn integrate_cosine_uniform_input_returns_input() {
        let env = Cubemap::new_with_color(8, Color::new(0.7, 0.6, 0.5, 1.0));
        let result = integrate_cosine(&env, Vec3::Y, 8);
        assert!((result.r - 0.7).abs() < 5e-2);
        assert!((result.g - 0.6).abs() < 5e-2);
        assert!((result.b - 0.5).abs() < 5e-2);
    }

    #[test]
    fn cubemap_face_views_return_six_unique_matrices() {
        let views = cubemap_face_views(Vec3::new(1.0, 2.0, 3.0));
        // Every face's forward direction lands the same world point at
        // a different transformed coordinate, so no two view matrices
        // should be identical.
        for i in 0..6 {
            for j in (i + 1)..6 {
                assert!(views[i] != views[j]);
            }
        }
    }

    #[test]
    fn cubemap_face_cameras_uses_90deg_fov() {
        let cams = cubemap_face_cameras(Vec3::ZERO, 0.1, 100.0);
        // All projections are identical (same near/far/fov).
        for cam in &cams[1..] {
            assert_eq!(cam.projection, cams[0].projection);
        }
        // Views differ per face.
        for (i, cam) in cams.iter().enumerate() {
            for (j, other) in cams.iter().enumerate() {
                if i != j {
                    assert!(cam.view != other.view);
                }
            }
        }
    }

    #[test]
    fn capture_sky_to_cubemap_populates_all_faces() {
        let atmosphere = crate::sky::AtmosphereParams::default();
        let cube = capture_sky_to_cubemap(8, &atmosphere);
        assert_eq!(cube.resolution, 8);
        // Each face must have some non-zero radiance (sky is never
        // perfectly black for the default sun direction).
        for (i, face) in cube.faces.iter().enumerate() {
            let max: f32 = face.iter().copied().fold(0.0_f32, f32::max);
            assert!(max > 0.0, "face {i} is entirely black");
        }
    }
}
