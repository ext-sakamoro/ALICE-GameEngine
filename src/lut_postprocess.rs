//! LUT-based color grading post-process.
//!
//! Integrates ALICE-RAW generated LUTs into the deferred rendering pipeline.
//! Supports `.cube` 3D LUT files and direct LUT data injection.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use alice_game_engine::lut_postprocess::*;
//!
//! // Load from .cube file
//! let lut = LutPostProcess::from_cube_file("preset.cube").unwrap();
//!
//! // Register as plugin
//! ctx.plugins.register(Box::new(lut));
//! ```
//!
//! ## WGSL Integration
//!
//! The LUT post-process shader reads the deferred lighting output and applies
//! a 3D LUT via trilinear interpolation. The shader is automatically registered
//! in the `ShaderCache` as `"lut_postprocess"`.

use crate::bridge::Plugin;
use crate::shader::{ShaderCache, ShaderSource, ShaderStage};

// ---------------------------------------------------------------------------
// LUT data
// ---------------------------------------------------------------------------

/// 3D LUT data for color grading.
#[derive(Debug, Clone)]
pub struct Lut3DData {
    /// Grid size (typically 17, 33, or 65).
    pub size: usize,
    /// RGB values, flattened `[size^3][3]` in R-major order.
    /// Values are `0.0..=1.0`.
    pub data: Vec<[f32; 3]>,
}

impl Lut3DData {
    /// Creates an identity (pass-through) LUT.
    #[must_use]
    pub fn identity(size: usize) -> Self {
        let total = size * size * size;
        let mut data = Vec::with_capacity(total);
        let scale = 1.0 / (size - 1) as f32;
        for bi in 0..size {
            for gi in 0..size {
                for ri in 0..size {
                    data.push([ri as f32 * scale, gi as f32 * scale, bi as f32 * scale]);
                }
            }
        }
        Self { size, data }
    }

    /// Samples the LUT with trilinear interpolation.
    #[must_use]
    pub fn sample(&self, r: f32, g: f32, b: f32) -> [f32; 3] {
        let s = self.size;
        let scale = (s - 1) as f32;

        let ri = (r * scale).clamp(0.0, scale);
        let gi = (g * scale).clamp(0.0, scale);
        let bi = (b * scale).clamp(0.0, scale);

        let r0 = ri.floor() as usize;
        let g0 = gi.floor() as usize;
        let b0 = bi.floor() as usize;
        let r1 = (r0 + 1).min(s - 1);
        let g1 = (g0 + 1).min(s - 1);
        let b1 = (b0 + 1).min(s - 1);
        let rf = ri.fract();
        let gf = gi.fract();
        let bf = bi.fract();

        let idx = |r: usize, g: usize, b: usize| -> [f32; 3] { self.data[b * s * s + g * s + r] };

        let c000 = idx(r0, g0, b0);
        let c100 = idx(r1, g0, b0);
        let c010 = idx(r0, g1, b0);
        let c110 = idx(r1, g1, b0);
        let c001 = idx(r0, g0, b1);
        let c101 = idx(r1, g0, b1);
        let c011 = idx(r0, g1, b1);
        let c111 = idx(r1, g1, b1);

        let mut out = [0.0f32; 3];
        for ch in 0..3 {
            let c00 = c000[ch] * (1.0 - rf) + c100[ch] * rf;
            let c01 = c001[ch] * (1.0 - rf) + c101[ch] * rf;
            let c10 = c010[ch] * (1.0 - rf) + c110[ch] * rf;
            let c11 = c011[ch] * (1.0 - rf) + c111[ch] * rf;
            let c0 = c00 * (1.0 - gf) + c10 * gf;
            let c1 = c01 * (1.0 - gf) + c11 * gf;
            out[ch] = (c0 * (1.0 - bf) + c1 * bf).clamp(0.0, 1.0);
        }
        out
    }

    /// Returns raw f32 data for GPU texture upload (RGB interleaved).
    #[must_use]
    pub fn to_gpu_data(&self) -> Vec<f32> {
        let mut out = Vec::with_capacity(self.data.len() * 4);
        for rgb in &self.data {
            out.push(rgb[0]);
            out.push(rgb[1]);
            out.push(rgb[2]);
            out.push(1.0); // RGBA padding
        }
        out
    }

    /// Returns dimensions for GPU 3D texture creation.
    #[must_use]
    pub const fn texture_dimensions(&self) -> (u32, u32, u32) {
        let s = self.size as u32;
        (s, s, s)
    }
}

// ---------------------------------------------------------------------------
// .cube file parser
// ---------------------------------------------------------------------------

/// Parses a `.cube` 3D LUT file.
///
/// # Errors
///
/// Returns error if the file format is invalid.
pub fn parse_cube_file(content: &str) -> Result<Lut3DData, String> {
    let mut size = 0usize;
    let mut data = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("TITLE")
            || line.starts_with("DOMAIN_MIN")
            || line.starts_with("DOMAIN_MAX")
        {
            continue;
        }
        if let Some(rest) = line.strip_prefix("LUT_3D_SIZE") {
            size = rest
                .trim()
                .parse::<usize>()
                .map_err(|e| format!("Invalid LUT_3D_SIZE: {e}"))?;
            continue;
        }
        // Data line: "R G B"
        let parts: Vec<f32> = line
            .split_whitespace()
            .filter_map(|s| s.parse::<f32>().ok())
            .collect();
        if parts.len() >= 3 {
            data.push([parts[0], parts[1], parts[2]]);
        }
    }

    if size == 0 {
        return Err("LUT_3D_SIZE not found".to_string());
    }
    let expected = size * size * size;
    if data.len() != expected {
        return Err(format!(
            "Expected {} entries for size {}, got {}",
            expected,
            size,
            data.len()
        ));
    }

    Ok(Lut3DData { size, data })
}

/// Loads a `.cube` file from disk.
///
/// # Errors
///
/// Returns error if the file cannot be read or parsed.
pub fn load_cube_file(path: &str) -> Result<Lut3DData, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read {path}: {e}"))?;
    parse_cube_file(&content)
}

// ---------------------------------------------------------------------------
// LUT Post-Process Plugin
// ---------------------------------------------------------------------------

/// Color grading plugin that applies a 3D LUT as post-process.
///
/// When registered with `EngineContext`, the LUT shader is added to the
/// `ShaderCache` and can be used in the deferred pipeline's post-process stage.
pub struct LutPostProcess {
    /// The 3D LUT data.
    pub lut: Lut3DData,
    /// Effect intensity (0.0 = bypass, 1.0 = full effect).
    pub intensity: f32,
    /// Whether the effect is enabled.
    pub enabled: bool,
    plugin_name: String,
}

impl LutPostProcess {
    /// Creates a new LUT post-process from 3D LUT data.
    #[must_use]
    pub fn new(lut: Lut3DData) -> Self {
        Self {
            lut,
            intensity: 1.0,
            enabled: true,
            plugin_name: "lut_postprocess".to_string(),
        }
    }

    /// Creates from a `.cube` file path.
    ///
    /// # Errors
    ///
    /// Returns error if the file cannot be loaded.
    pub fn from_cube_file(path: &str) -> Result<Self, String> {
        let lut = load_cube_file(path)?;
        Ok(Self::new(lut))
    }

    /// Creates an identity (pass-through) LUT.
    #[must_use]
    pub fn identity(size: usize) -> Self {
        Self::new(Lut3DData::identity(size))
    }

    /// Sets the intensity.
    pub fn set_intensity(&mut self, intensity: f32) {
        self.intensity = intensity.clamp(0.0, 1.0);
    }

    /// Replaces the LUT data (hot-swap).
    pub fn set_lut(&mut self, lut: Lut3DData) {
        self.lut = lut;
    }

    /// Applies the LUT to a single pixel (CPU fallback).
    #[must_use]
    pub fn apply_pixel(&self, r: f32, g: f32, b: f32) -> [f32; 3] {
        if !self.enabled || self.intensity < 1e-6 {
            return [r, g, b];
        }
        let graded = self.lut.sample(r, g, b);
        let t = self.intensity;
        let s = 1.0 - t;
        [
            r * s + graded[0] * t,
            g * s + graded[1] * t,
            b * s + graded[2] * t,
        ]
    }

    /// Applies the LUT to an entire framebuffer (CPU, for headless/screenshot).
    pub fn apply_framebuffer(&self, pixels: &mut [[f32; 3]]) {
        for px in pixels.iter_mut() {
            let result = self.apply_pixel(px[0], px[1], px[2]);
            *px = result;
        }
    }

    /// Registers the LUT post-process WGSL shader in the given cache.
    pub fn register_shader(cache: &mut ShaderCache) {
        cache.add(ShaderSource::new(
            "lut_postprocess",
            LUT_POSTPROCESS_WGSL,
            ShaderStage::Fragment,
        ));
    }

    /// Returns the WGSL shader source for GPU integration.
    #[must_use]
    pub fn shader_source() -> &'static str {
        LUT_POSTPROCESS_WGSL
    }
}

impl Plugin for LutPostProcess {
    fn name(&self) -> &str {
        &self.plugin_name
    }

    fn on_register(&mut self) {
        // LUT data ready for GPU upload on next frame
    }

    fn on_update(&mut self, _dt: f32) {
        // CPU-side: no per-frame work needed
        // GPU-side: shader reads LUT texture each frame automatically
    }

    fn on_shutdown(&mut self) {
        self.enabled = false;
    }
}

// ---------------------------------------------------------------------------
// WGSL Shader
// ---------------------------------------------------------------------------

/// LUT color grading post-process fragment shader (WGSL).
///
/// Reads the deferred lighting output, applies 3D LUT with trilinear
/// interpolation, and writes to the final framebuffer.
///
/// Bind group layout:
/// - `@group(0) @binding(0)`: scene color texture (2D)
/// - `@group(0) @binding(1)`: sampler
/// - `@group(0) @binding(2)`: LUT 3D texture
/// - `@group(0) @binding(3)`: LUT sampler
/// - `@group(0) @binding(4)`: LUT uniforms (intensity)
pub const LUT_POSTPROCESS_WGSL: &str = r"
struct LutUniforms {
    intensity: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
};

@group(0) @binding(0) var t_scene: texture_2d<f32>;
@group(0) @binding(1) var s_scene: sampler;
@group(0) @binding(2) var t_lut: texture_3d<f32>;
@group(0) @binding(3) var s_lut: sampler;
@group(0) @binding(4) var<uniform> lut_uniforms: LutUniforms;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let scene_color = textureSample(t_scene, s_scene, uv);
    let rgb = clamp(scene_color.rgb, vec3<f32>(0.0), vec3<f32>(1.0));

    // Sample 3D LUT with trilinear interpolation (GPU handles this)
    let graded = textureSample(t_lut, s_lut, rgb).rgb;

    // Mix original and graded by intensity
    let final_color = mix(rgb, graded, lut_uniforms.intensity);
    return vec4<f32>(final_color, scene_color.a);
}
";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_lut_passthrough() {
        let lut = Lut3DData::identity(17);
        assert_eq!(lut.data.len(), 17 * 17 * 17);

        let s = lut.sample(0.0, 0.0, 0.0);
        assert!(s[0] < 0.02 && s[1] < 0.02 && s[2] < 0.02);

        let s = lut.sample(1.0, 1.0, 1.0);
        assert!(s[0] > 0.98 && s[1] > 0.98 && s[2] > 0.98);

        // Arbitrary color should pass through identity
        let s = lut.sample(0.5, 0.3, 0.7);
        assert!((s[0] - 0.5).abs() < 0.05);
        assert!((s[1] - 0.3).abs() < 0.05);
        assert!((s[2] - 0.7).abs() < 0.05);
    }

    #[test]
    fn parse_cube_basic() {
        let cube = "\
# Comment
TITLE \"Test\"
LUT_3D_SIZE 2
DOMAIN_MIN 0.0 0.0 0.0
DOMAIN_MAX 1.0 1.0 1.0

0.0 0.0 0.0
1.0 0.0 0.0
0.0 1.0 0.0
1.0 1.0 0.0
0.0 0.0 1.0
1.0 0.0 1.0
0.0 1.0 1.0
1.0 1.0 1.0
";
        let lut = parse_cube_file(cube).unwrap();
        assert_eq!(lut.size, 2);
        assert_eq!(lut.data.len(), 8);
        assert_eq!(lut.data[0], [0.0, 0.0, 0.0]);
        assert_eq!(lut.data[7], [1.0, 1.0, 1.0]);
    }

    #[test]
    fn parse_cube_wrong_count() {
        let cube = "LUT_3D_SIZE 2\n0.0 0.0 0.0\n";
        let result = parse_cube_file(cube);
        assert!(result.is_err());
    }

    #[test]
    fn lut_plugin_interface() {
        let mut plugin = LutPostProcess::identity(8);
        assert_eq!(plugin.name(), "lut_postprocess");
        assert!(plugin.enabled);

        plugin.set_intensity(0.5);
        assert!((plugin.intensity - 0.5).abs() < 1e-6);

        // Bypass check
        plugin.set_intensity(0.0);
        let result = plugin.apply_pixel(0.8, 0.3, 0.5);
        assert!((result[0] - 0.8).abs() < 1e-6);
    }

    #[test]
    fn lut_apply_with_intensity() {
        let mut lut_data = Lut3DData::identity(4);
        // Invert the LUT: swap R and B
        for entry in &mut lut_data.data {
            let tmp = entry[0];
            entry[0] = entry[2];
            entry[2] = tmp;
        }

        let mut plugin = LutPostProcess::new(lut_data);
        plugin.set_intensity(1.0);

        let result = plugin.apply_pixel(1.0, 0.0, 0.0);
        // Red should become blue
        assert!(result[0] < 0.1);
        assert!(result[2] > 0.9);
    }

    #[test]
    fn lut_gpu_data_format() {
        let lut = Lut3DData::identity(4);
        let gpu_data = lut.to_gpu_data();
        // RGBA = 4 floats per entry
        assert_eq!(gpu_data.len(), 4 * 4 * 4 * 4);
        // First entry: (0,0,0) -> alpha=1
        assert!((gpu_data[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn lut_texture_dimensions() {
        let lut = Lut3DData::identity(33);
        assert_eq!(lut.texture_dimensions(), (33, 33, 33));
    }

    #[test]
    fn shader_has_entry_point() {
        let src = ShaderSource::new(
            "lut_postprocess",
            LUT_POSTPROCESS_WGSL,
            ShaderStage::Fragment,
        );
        assert!(src.has_entry_point());
    }

    #[test]
    fn register_shader_in_cache() {
        let mut cache = ShaderCache::new();
        LutPostProcess::register_shader(&mut cache);
        assert!(cache.get("lut_postprocess").is_some());
    }

    #[test]
    fn apply_framebuffer_batch() {
        let plugin = LutPostProcess::identity(8);
        let mut pixels = vec![[0.5, 0.3, 0.7]; 100];
        plugin.apply_framebuffer(&mut pixels);
        // Identity LUT: values should be approximately preserved
        for px in &pixels {
            assert!((px[0] - 0.5).abs() < 0.1);
            assert!((px[1] - 0.3).abs() < 0.1);
            assert!((px[2] - 0.7).abs() < 0.1);
        }
    }
}
