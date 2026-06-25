//! Dynamic Diffuse Global Illumination (DDGI).
//!
//! Stores irradiance + mean / variance distance on a regular 3-D grid
//! of probes (Majercik et al. 2019). Each probe holds an octahedrally-
//! encoded irradiance + visibility map; samples lying inside the grid
//! tri-linearly blend the eight surrounding probes weighted by the
//! Chebyshev visibility test (= probes whose stored distance disagrees
//! with the lookup distance contribute less).
//!
//! This module owns the **CPU-side data + the per-frame update
//! integrator** (input: incoming radiance estimate per probe direction
//! → output: smoothed irradiance map). The actual ray-trace that fills
//! the input lives in a future PR; for now you can drive the integrator
//! with hand-authored values, mock probes, or a path tracer wrapper.

use serde::{Deserialize, Serialize};

use crate::math::Vec3;

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DdgiConfig {
    /// Probe grid dimensions (probe count along each axis).
    pub grid: (u32, u32, u32),
    /// World-space spacing between adjacent probes (metres).
    pub spacing: f32,
    /// World-space origin of probe `[0,0,0]`.
    pub origin: Vec3,
    /// Resolution of each probe's irradiance map (octahedron edge in
    /// texels). 6 / 8 are typical (corresponds to wiki `6 + 2` border).
    pub irradiance_resolution: u32,
    /// Resolution of the visibility map (typically 16).
    pub visibility_resolution: u32,
    /// Convergence rate (1.0 = replace previous frame, 0.05 = slow
    /// 20-frame moving average that hides path-trace noise).
    pub hysteresis: f32,
}

impl Default for DdgiConfig {
    fn default() -> Self {
        Self {
            grid: (8, 4, 8),
            spacing: 2.0,
            origin: Vec3::ZERO,
            irradiance_resolution: 6,
            visibility_resolution: 16,
            hysteresis: 0.05,
        }
    }
}

// ---------------------------------------------------------------------------
// Probe data
// ---------------------------------------------------------------------------

/// One probe: irradiance + visibility maps in octahedral encoding.
/// Indexing convention: `texel = y * resolution + x` (row-major).
#[derive(Debug, Clone)]
pub struct DdgiProbe {
    pub position: Vec3,
    /// `irradiance_resolution²` × 3 channels (RGB).
    pub irradiance: Vec<f32>,
    /// `visibility_resolution²` × 2 channels (mean, variance).
    pub visibility: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Octahedral mapping
// ---------------------------------------------------------------------------

/// Direction → octahedral UV in `[0, 1]²`.
#[must_use]
pub fn dir_to_oct(dir: Vec3) -> (f32, f32) {
    let inv_norm = (dir.x().abs() + dir.y().abs() + dir.z().abs()).recip();
    let xn = dir.x() * inv_norm;
    let yn = dir.y() * inv_norm;
    let zn = dir.z() * inv_norm;
    let (ox, oy) = if zn >= 0.0 {
        (xn, yn)
    } else {
        let abs_x = xn.abs();
        let abs_y = yn.abs();
        ((1.0 - abs_y).copysign(xn), (1.0 - abs_x).copysign(yn))
    };
    (ox.mul_add(0.5, 0.5), oy.mul_add(0.5, 0.5))
}

/// Octahedral UV (`[0, 1]²`) → unit direction.
#[must_use]
pub fn oct_to_dir(u: f32, v: f32) -> Vec3 {
    let x = u.mul_add(2.0, -1.0);
    let y = v.mul_add(2.0, -1.0);
    let nz = 1.0 - x.abs() - y.abs();
    // Standard octahedron unfolding: subtract the overshoot from
    // x and y when nz is negative so the result lies on the lower
    // hemisphere instead of collapsing to (0, 0, 0).
    let t = (-nz).max(0.0);
    let nx = x - t.copysign(x);
    let ny = y - t.copysign(y);
    let v = Vec3::new(nx, ny, nz);
    let len_sq = v.length_squared();
    if len_sq < 1e-12 {
        Vec3::Y
    } else {
        v * len_sq.sqrt().recip()
    }
}

// ---------------------------------------------------------------------------
// Volume
// ---------------------------------------------------------------------------

pub struct DdgiVolume {
    pub config: DdgiConfig,
    pub probes: Vec<DdgiProbe>,
}

impl DdgiVolume {
    #[must_use]
    pub fn new(config: DdgiConfig) -> Self {
        let total = (config.grid.0 * config.grid.1 * config.grid.2) as usize;
        let irr_n = (config.irradiance_resolution as usize).pow(2) * 3;
        let vis_n = (config.visibility_resolution as usize).pow(2) * 2;
        let mut probes = Vec::with_capacity(total);
        for k in 0..config.grid.2 {
            for j in 0..config.grid.1 {
                for i in 0..config.grid.0 {
                    let position = config.origin
                        + Vec3::new(
                            i as f32 * config.spacing,
                            j as f32 * config.spacing,
                            k as f32 * config.spacing,
                        );
                    probes.push(DdgiProbe {
                        position,
                        irradiance: vec![0.0; irr_n],
                        visibility: vec![0.0; vis_n],
                    });
                }
            }
        }
        Self { config, probes }
    }

    #[must_use]
    pub const fn probe_count(&self) -> usize {
        self.probes.len()
    }

    /// Probe linear index from grid coordinates. Returns `None` when
    /// out of range.
    #[must_use]
    pub const fn probe_index(&self, i: u32, j: u32, k: u32) -> Option<usize> {
        if i >= self.config.grid.0 || j >= self.config.grid.1 || k >= self.config.grid.2 {
            return None;
        }
        Some((k * self.config.grid.0 * self.config.grid.1 + j * self.config.grid.0 + i) as usize)
    }

    /// Apply a per-direction radiance estimate to one probe. The
    /// `samples` slice must have length `irradiance_resolution²` × 3
    /// (RGB). The integrator blends each sample into the existing
    /// irradiance with `hysteresis` as a low-pass filter.
    pub fn update_probe_irradiance(&mut self, probe_idx: usize, samples: &[f32]) {
        let probe = &mut self.probes[probe_idx];
        debug_assert_eq!(samples.len(), probe.irradiance.len());
        let alpha = self.config.hysteresis.clamp(0.0, 1.0);
        let inv_alpha = 1.0 - alpha;
        for (target, sample) in probe.irradiance.iter_mut().zip(samples.iter()) {
            *target = sample.mul_add(alpha, *target * inv_alpha);
        }
    }

    /// Reset every probe's irradiance + visibility to zero.
    pub fn clear(&mut self) {
        for probe in &mut self.probes {
            for v in &mut probe.irradiance {
                *v = 0.0;
            }
            for v in &mut probe.visibility {
                *v = 0.0;
            }
        }
    }

    /// Trilinear-sample irradiance at a world-space position +
    /// direction. Returns `(r, g, b)`; falls back to zero outside the
    /// probe volume.
    #[must_use]
    pub fn sample_irradiance(&self, world: Vec3, dir: Vec3) -> (f32, f32, f32) {
        let rel = (world - self.config.origin) * self.config.spacing.recip();
        let fx = rel.x();
        let fy = rel.y();
        let fz = rel.z();
        let max_x = (self.config.grid.0 - 1) as f32;
        let max_y = (self.config.grid.1 - 1) as f32;
        let max_z = (self.config.grid.2 - 1) as f32;
        if fx < 0.0 || fy < 0.0 || fz < 0.0 || fx > max_x || fy > max_y || fz > max_z {
            return (0.0, 0.0, 0.0);
        }
        let ix = fx.floor() as u32;
        let iy = fy.floor() as u32;
        let iz = fz.floor() as u32;
        let tx = fx - fx.floor();
        let ty = fy - fy.floor();
        let tz = fz - fz.floor();

        let (ou, ov) = dir_to_oct(dir);
        let texel_x = (ou * (self.config.irradiance_resolution as f32 - 1.0)).round() as u32;
        let texel_y = (ov * (self.config.irradiance_resolution as f32 - 1.0)).round() as u32;
        let texel = (texel_y * self.config.irradiance_resolution + texel_x) as usize * 3;

        let mut acc = (0.0_f32, 0.0_f32, 0.0_f32);
        for (dz, wz) in [(0, 1.0 - tz), (1, tz)] {
            for (dy, wy) in [(0, 1.0 - ty), (1, ty)] {
                for (dx, wx) in [(0, 1.0 - tx), (1, tx)] {
                    let nx = (ix + dx).min(self.config.grid.0 - 1);
                    let ny = (iy + dy).min(self.config.grid.1 - 1);
                    let nz = (iz + dz).min(self.config.grid.2 - 1);
                    let Some(idx) = self.probe_index(nx, ny, nz) else {
                        continue;
                    };
                    let weight = wx * wy * wz;
                    let probe = &self.probes[idx];
                    acc.0 += probe.irradiance[texel] * weight;
                    acc.1 += probe.irradiance[texel + 1] * weight;
                    acc.2 += probe.irradiance[texel + 2] * weight;
                }
            }
        }
        acc
    }
}

// ---------------------------------------------------------------------------
// GPU compute pipeline
// ---------------------------------------------------------------------------

/// Owns the wgpu compute pipeline + bind group layout for the DDGI
/// per-probe irradiance update. Pair with the WGSL in
/// `shader::DDGI_UPDATE_COMPUTE_WGSL` and dispatch one workgroup per
/// probe (workgroup size 8×8 = one thread per octahedral texel).
///
/// Buffer layout (= matches the WGSL `@binding`s):
///
/// | binding | type | use |
/// |---------|------|-----|
/// | 0 | `uniform DdgiParams` | probe count + resolution + hysteresis |
/// | 1 | `storage<read> array<f32>` | per-direction radiance samples |
/// | 2 | `storage<read_write> array<f32>` | accumulated irradiance |
#[cfg(feature = "gpu")]
pub struct DdgiVolumeGpu {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub pipeline: wgpu::ComputePipeline,
}

#[cfg(feature = "gpu")]
impl DdgiVolumeGpu {
    #[must_use]
    pub fn new(device: &wgpu::Device) -> Self {
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("alice-ddgi-update-compute"),
            source: wgpu::ShaderSource::Wgsl(crate::shader::DDGI_UPDATE_COMPUTE_WGSL.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("alice-ddgi-update-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("alice-ddgi-update-pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("alice-ddgi-update-pipeline"),
            layout: Some(&pipeline_layout),
            module: &shader_module,
            entry_point: Some("cs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        Self {
            bind_group_layout,
            pipeline,
        }
    }

    /// Workgroup count = one workgroup per probe (the shader takes
    /// `8 × 8` threads per workgroup, one per irradiance texel).
    #[must_use]
    pub const fn workgroup_count(probe_count: u32) -> (u32, u32, u32) {
        (probe_count, 1, 1)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_grid_dimensions() {
        let c = DdgiConfig::default();
        assert_eq!(c.grid, (8, 4, 8));
        assert!((c.spacing - 2.0).abs() < 1e-6);
        assert!((c.hysteresis - 0.05).abs() < 1e-6);
    }

    #[test]
    fn volume_has_grid_xyz_probes() {
        let v = DdgiVolume::new(DdgiConfig {
            grid: (2, 3, 4),
            ..DdgiConfig::default()
        });
        assert_eq!(v.probe_count(), 24);
    }

    #[test]
    fn probe_positions_follow_grid_layout() {
        let v = DdgiVolume::new(DdgiConfig {
            grid: (2, 2, 2),
            spacing: 3.0,
            origin: Vec3::ZERO,
            ..DdgiConfig::default()
        });
        let last = &v.probes[7];
        assert!((last.position - Vec3::new(3.0, 3.0, 3.0)).length() < 1e-3);
    }

    #[test]
    fn dir_to_oct_round_trip_for_axis_directions() {
        let cases = [
            Vec3::new(1.0, 0.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(0.0, 0.0, 1.0),
            Vec3::new(-1.0, 0.0, 0.0),
            Vec3::new(0.0, -1.0, 0.0),
            Vec3::new(0.0, 0.0, -1.0),
        ];
        for dir in cases {
            let (u, v) = dir_to_oct(dir);
            let back = oct_to_dir(u, v);
            assert!(
                (back - dir).length() < 0.1,
                "round trip {dir:?} → ({u:.3},{v:.3}) → {back:?}",
            );
        }
    }

    #[test]
    fn probe_index_returns_some_in_range_none_out_of_range() {
        let v = DdgiVolume::new(DdgiConfig {
            grid: (4, 4, 4),
            ..DdgiConfig::default()
        });
        assert_eq!(v.probe_index(0, 0, 0), Some(0));
        assert_eq!(v.probe_index(3, 3, 3), Some(63));
        assert_eq!(v.probe_index(4, 0, 0), None);
    }

    #[test]
    fn update_probe_irradiance_blends_with_hysteresis() {
        let mut v = DdgiVolume::new(DdgiConfig {
            grid: (2, 2, 2),
            irradiance_resolution: 2,
            hysteresis: 0.5,
            ..DdgiConfig::default()
        });
        // 2*2 * 3 = 12 samples.
        let samples = vec![1.0_f32; 12];
        v.update_probe_irradiance(0, &samples);
        let probe = &v.probes[0];
        for &val in &probe.irradiance {
            assert!((val - 0.5).abs() < 1e-3, "expected 0.5, got {val}");
        }
    }

    #[test]
    fn sample_irradiance_returns_zero_outside_volume() {
        let v = DdgiVolume::new(DdgiConfig::default());
        let s = v.sample_irradiance(Vec3::new(-1000.0, 0.0, 0.0), Vec3::Y);
        assert!(s.0.abs() < 1e-6);
        assert!(s.1.abs() < 1e-6);
        assert!(s.2.abs() < 1e-6);
    }

    #[test]
    fn sample_irradiance_returns_authored_value_inside_volume() {
        let mut v = DdgiVolume::new(DdgiConfig {
            grid: (2, 2, 2),
            spacing: 5.0,
            irradiance_resolution: 2,
            hysteresis: 1.0,
            ..DdgiConfig::default()
        });
        let samples = vec![0.5_f32; 12];
        for p in 0..v.probe_count() {
            v.update_probe_irradiance(p, &samples);
        }
        let s = v.sample_irradiance(Vec3::new(2.5, 2.5, 2.5), Vec3::Y);
        assert!((s.0 - 0.5).abs() < 1e-2, "expected 0.5, got {}", s.0);
    }

    #[test]
    fn clear_zeroes_all_probes() {
        let mut v = DdgiVolume::new(DdgiConfig {
            grid: (2, 2, 2),
            irradiance_resolution: 2,
            hysteresis: 1.0,
            ..DdgiConfig::default()
        });
        let samples = vec![1.0_f32; 12];
        v.update_probe_irradiance(0, &samples);
        v.clear();
        for x in &v.probes[0].irradiance {
            assert!(x.abs() < 1e-6);
        }
    }

    #[test]
    fn config_serde_round_trip() {
        let c = DdgiConfig::default();
        let j = serde_json::to_string(&c).unwrap();
        let back: DdgiConfig = serde_json::from_str(&j).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn oct_to_dir_returns_unit_length() {
        for (u, v) in [(0.1, 0.2), (0.5, 0.5), (0.9, 0.7), (0.0, 0.0), (1.0, 1.0)] {
            let d = oct_to_dir(u, v);
            assert!(
                (d.length() - 1.0).abs() < 1e-3,
                "({u},{v}) → {d:?} length={}",
                d.length(),
            );
        }
    }
}
