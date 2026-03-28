//! Texture management: CPU image data, GPU upload descriptors, mipmaps.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// TextureFormat
// ---------------------------------------------------------------------------

/// Pixel format for texture data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PixelFormat {
    R8,
    Rg8,
    Rgba8,
    Rgba8Srgb,
    R16Float,
    Rgba16Float,
    Rgba32Float,
    Depth32,
}

impl PixelFormat {
    /// Bytes per pixel.
    #[must_use]
    #[allow(clippy::match_same_arms)]
    pub const fn bytes_per_pixel(self) -> u32 {
        match self {
            Self::R8 => 1,
            Self::Rg8 => 2,
            Self::Rgba8 | Self::Rgba8Srgb => 4,
            Self::R16Float => 2,
            Self::Rgba16Float => 8,
            Self::Rgba32Float => 16,
            Self::Depth32 => 4,
        }
    }

    /// Number of channels.
    #[must_use]
    pub const fn channels(self) -> u32 {
        match self {
            Self::R8 | Self::R16Float | Self::Depth32 => 1,
            Self::Rg8 => 2,
            Self::Rgba8 | Self::Rgba8Srgb | Self::Rgba16Float | Self::Rgba32Float => 4,
        }
    }
}

// ---------------------------------------------------------------------------
// TextureAsset — CPU-side image
// ---------------------------------------------------------------------------

/// A CPU-side texture asset.
#[derive(Debug, Clone)]
pub struct TextureAsset {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub data: Vec<u8>,
    pub generate_mipmaps: bool,
}

impl TextureAsset {
    /// Creates a texture from raw pixel data.
    #[must_use]
    pub fn new(name: &str, width: u32, height: u32, format: PixelFormat, data: Vec<u8>) -> Self {
        Self {
            name: name.to_string(),
            width,
            height,
            format,
            data,
            generate_mipmaps: true,
        }
    }

    /// Creates a 1x1 solid color texture.
    #[must_use]
    pub fn solid_color(name: &str, r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            name: name.to_string(),
            width: 1,
            height: 1,
            format: PixelFormat::Rgba8,
            data: vec![r, g, b, a],
            generate_mipmaps: false,
        }
    }

    /// Creates a checkerboard pattern for debugging.
    #[must_use]
    pub fn checkerboard(name: &str, size: u32, cell_size: u32) -> Self {
        let mut data = Vec::with_capacity((size * size * 4) as usize);
        for y in 0..size {
            for x in 0..size {
                let is_white = ((x / cell_size) + (y / cell_size)).is_multiple_of(2);
                let v = if is_white { 255 } else { 64 };
                data.extend_from_slice(&[v, v, v, 255]);
            }
        }
        Self {
            name: name.to_string(),
            width: size,
            height: size,
            format: PixelFormat::Rgba8,
            data,
            generate_mipmaps: true,
        }
    }

    /// Returns the total size in bytes.
    #[must_use]
    pub const fn byte_size(&self) -> usize {
        self.data.len()
    }

    /// Returns the expected byte size based on dimensions and format.
    #[must_use]
    pub fn expected_byte_size(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height) * u64::from(self.format.bytes_per_pixel())
    }

    /// Returns the number of mipmap levels.
    #[must_use]
    pub const fn mip_levels(&self) -> u32 {
        if !self.generate_mipmaps {
            return 1;
        }
        mip_level_count(self.width, self.height)
    }

    /// Returns a pixel at (x, y) for RGBA8 format.
    #[must_use]
    pub fn pixel_rgba8(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if self.format != PixelFormat::Rgba8 && self.format != PixelFormat::Rgba8Srgb {
            return None;
        }
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        if idx + 3 >= self.data.len() {
            return None;
        }
        Some([
            self.data[idx],
            self.data[idx + 1],
            self.data[idx + 2],
            self.data[idx + 3],
        ])
    }
}

// ---------------------------------------------------------------------------
// Mipmap helpers
// ---------------------------------------------------------------------------

/// Computes the number of mipmap levels for a given resolution.
#[must_use]
pub const fn mip_level_count(width: u32, height: u32) -> u32 {
    let max_dim = if width > height { width } else { height };
    if max_dim == 0 {
        return 1;
    }
    u32::BITS - max_dim.leading_zeros()
}

/// Returns the dimensions of a specific mip level.
#[must_use]
pub fn mip_dimensions(width: u32, height: u32, level: u32) -> (u32, u32) {
    let w = (width >> level).max(1);
    let h = (height >> level).max(1);
    (w, h)
}

// ---------------------------------------------------------------------------
// GpuTextureDesc — descriptor for GPU upload
// ---------------------------------------------------------------------------

/// Describes a GPU texture (without actual GPU resources).
#[derive(Debug, Clone)]
pub struct GpuTextureDesc {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    pub mip_levels: u32,
    pub sample_count: u32,
}

impl GpuTextureDesc {
    #[must_use]
    pub fn from_asset(asset: &TextureAsset) -> Self {
        Self {
            name: asset.name.clone(),
            width: asset.width,
            height: asset.height,
            format: asset.format,
            mip_levels: asset.mip_levels(),
            sample_count: 1,
        }
    }

    /// Estimated VRAM in bytes (all mip levels).
    #[must_use]
    pub fn estimated_vram(&self) -> u64 {
        let bpp = u64::from(self.format.bytes_per_pixel());
        let mut total = 0u64;
        for level in 0..self.mip_levels {
            let (w, h) = mip_dimensions(self.width, self.height, level);
            total += u64::from(w) * u64::from(h) * bpp;
        }
        total
    }
}

// ---------------------------------------------------------------------------
// SamplerDesc
// ---------------------------------------------------------------------------

/// Texture sampler configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterMode {
    Nearest,
    Linear,
}

/// Address mode for texture wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressMode {
    Repeat,
    ClampToEdge,
    MirrorRepeat,
}

/// Sampler descriptor.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SamplerDesc {
    pub min_filter: FilterMode,
    pub mag_filter: FilterMode,
    pub mipmap_filter: FilterMode,
    pub address_u: AddressMode,
    pub address_v: AddressMode,
}

impl Default for SamplerDesc {
    fn default() -> Self {
        Self {
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Linear,
            address_u: AddressMode::Repeat,
            address_v: AddressMode::Repeat,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_format_bpp() {
        assert_eq!(PixelFormat::R8.bytes_per_pixel(), 1);
        assert_eq!(PixelFormat::Rgba8.bytes_per_pixel(), 4);
        assert_eq!(PixelFormat::Rgba16Float.bytes_per_pixel(), 8);
        assert_eq!(PixelFormat::Rgba32Float.bytes_per_pixel(), 16);
    }

    #[test]
    fn pixel_format_channels() {
        assert_eq!(PixelFormat::R8.channels(), 1);
        assert_eq!(PixelFormat::Rg8.channels(), 2);
        assert_eq!(PixelFormat::Rgba8.channels(), 4);
    }

    #[test]
    fn texture_asset_solid() {
        let tex = TextureAsset::solid_color("white", 255, 255, 255, 255);
        assert_eq!(tex.width, 1);
        assert_eq!(tex.byte_size(), 4);
        assert_eq!(tex.mip_levels(), 1);
    }

    #[test]
    fn texture_asset_checkerboard() {
        let tex = TextureAsset::checkerboard("check", 64, 8);
        assert_eq!(tex.width, 64);
        assert_eq!(tex.byte_size(), 64 * 64 * 4);
        assert!(tex.mip_levels() > 1);
    }

    #[test]
    fn texture_asset_pixel_rgba8() {
        let tex = TextureAsset::solid_color("red", 255, 0, 0, 255);
        let px = tex.pixel_rgba8(0, 0).unwrap();
        assert_eq!(px, [255, 0, 0, 255]);
    }

    #[test]
    fn texture_asset_pixel_out_of_bounds() {
        let tex = TextureAsset::solid_color("x", 0, 0, 0, 0);
        assert!(tex.pixel_rgba8(5, 5).is_none());
    }

    #[test]
    fn texture_expected_size() {
        let tex = TextureAsset::new("t", 128, 128, PixelFormat::Rgba8, vec![0; 128 * 128 * 4]);
        assert_eq!(tex.expected_byte_size(), 128 * 128 * 4);
    }

    #[test]
    fn mip_level_count_1x1() {
        assert_eq!(mip_level_count(1, 1), 1);
    }

    #[test]
    fn mip_level_count_256() {
        assert_eq!(mip_level_count(256, 256), 9); // 256 -> 128 -> ... -> 1
    }

    #[test]
    fn mip_level_count_non_power_of_two() {
        assert!(mip_level_count(300, 200) > 1);
    }

    #[test]
    fn mip_dimensions_level0() {
        assert_eq!(mip_dimensions(256, 128, 0), (256, 128));
    }

    #[test]
    fn mip_dimensions_level1() {
        assert_eq!(mip_dimensions(256, 128, 1), (128, 64));
    }

    #[test]
    fn mip_dimensions_clamp_to_1() {
        assert_eq!(mip_dimensions(4, 4, 10), (1, 1));
    }

    #[test]
    fn gpu_texture_desc_from_asset() {
        let tex = TextureAsset::checkerboard("check", 64, 8);
        let desc = GpuTextureDesc::from_asset(&tex);
        assert_eq!(desc.width, 64);
        assert!(desc.mip_levels > 1);
    }

    #[test]
    fn gpu_texture_desc_vram() {
        let desc = GpuTextureDesc {
            name: "test".to_string(),
            width: 256,
            height: 256,
            format: PixelFormat::Rgba8,
            mip_levels: 1,
            sample_count: 1,
        };
        assert_eq!(desc.estimated_vram(), 256 * 256 * 4);
    }

    #[test]
    fn gpu_texture_desc_vram_mipmaps() {
        let desc = GpuTextureDesc {
            name: "test".to_string(),
            width: 256,
            height: 256,
            format: PixelFormat::Rgba8,
            mip_levels: mip_level_count(256, 256),
            sample_count: 1,
        };
        // Sum of all mip levels should be > base but < 2x base
        let base = 256u64 * 256 * 4;
        assert!(desc.estimated_vram() > base);
        assert!(desc.estimated_vram() < base * 2);
    }

    #[test]
    fn sampler_desc_default() {
        let s = SamplerDesc::default();
        assert_eq!(s.min_filter, FilterMode::Linear);
        assert_eq!(s.address_u, AddressMode::Repeat);
    }

    #[test]
    fn checkerboard_pattern() {
        let tex = TextureAsset::checkerboard("c", 4, 2);
        let p00 = tex.pixel_rgba8(0, 0).unwrap();
        let p20 = tex.pixel_rgba8(2, 0).unwrap();
        // (0,0) is cell (0,0) = white, (2,0) is cell (1,0) = dark
        assert_eq!(p00[0], 255);
        assert_eq!(p20[0], 64);
    }
}
