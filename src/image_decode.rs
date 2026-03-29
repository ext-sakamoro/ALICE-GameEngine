//! Image decoding: BMP parser + trait for external decoders (PNG/JPG via image crate).
//!
//! ```rust
//! use alice_game_engine::image_decode::*;
//!
//! // BMP decoding (built-in, no external deps)
//! // For PNG/JPG, implement ImageDecoder trait with the `image` crate.
//! let decoder_count = 0; // placeholder
//! assert_eq!(decoder_count, 0);
//! ```

// ---------------------------------------------------------------------------
// ImageDecoder trait — for external crate injection
// ---------------------------------------------------------------------------

/// Trait for image decoders (implement with `image` crate for PNG/JPG).
pub trait ImageDecoder: Send + Sync {
    /// Decodes image bytes into RGBA8 pixel data.
    ///
    /// # Errors
    ///
    /// Returns error on decode failure.
    fn decode(&self, data: &[u8]) -> Result<DecodedImage, String>;

    /// Returns supported file extensions.
    fn supported_extensions(&self) -> &[&str];
}

/// Decoded image data.
#[derive(Debug, Clone)]
pub struct DecodedImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl DecodedImage {
    #[must_use]
    pub const fn byte_size(&self) -> usize {
        self.pixels.len()
    }

    #[must_use]
    pub const fn pixel_count(&self) -> u32 {
        self.width * self.height
    }
}

// ---------------------------------------------------------------------------
// BMP decoder (built-in, no external deps)
// ---------------------------------------------------------------------------

/// Decodes an uncompressed 24-bit or 32-bit BMP to RGBA8.
///
/// # Errors
///
/// Returns error if the BMP format is unsupported or data is too short.
pub fn decode_bmp(data: &[u8]) -> Result<DecodedImage, String> {
    if data.len() < 54 {
        return Err("BMP too short".to_string());
    }
    if data[0] != b'B' || data[1] != b'M' {
        return Err("Not a BMP file".to_string());
    }

    let pixel_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]) as usize;
    let width = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    let height = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    let bpp = u16::from_le_bytes([data[28], data[29]]);

    if width <= 0 || height == 0 {
        return Err("Invalid BMP dimensions".to_string());
    }
    if bpp != 24 && bpp != 32 {
        return Err(format!("Unsupported BMP bpp: {bpp}"));
    }

    #[allow(clippy::cast_sign_loss)]
    let w = width as u32;
    let abs_h = height.unsigned_abs();
    let bottom_up = height > 0;
    let bytes_per_pixel = (bpp / 8) as usize;
    let row_stride = (w as usize * bytes_per_pixel).div_ceil(4) * 4;

    let mut pixels = vec![0u8; (w * abs_h * 4) as usize];

    for y in 0..abs_h {
        let src_y = if bottom_up { abs_h - 1 - y } else { y };
        let src_offset = pixel_offset + src_y as usize * row_stride;

        for x in 0..w {
            let si = src_offset + x as usize * bytes_per_pixel;
            let di = ((y * w + x) * 4) as usize;

            if si + bytes_per_pixel > data.len() || di + 3 >= pixels.len() {
                continue;
            }

            // BMP stores BGR(A)
            pixels[di] = data[si + 2]; // R
            pixels[di + 1] = data[si + 1]; // G
            pixels[di + 2] = data[si]; // B
            pixels[di + 3] = if bpp == 32 { data[si + 3] } else { 255 };
        }
    }

    Ok(DecodedImage {
        width: w,
        height: abs_h,
        pixels,
    })
}

/// Detects image format from file header bytes.
#[must_use]
pub fn detect_image_format(data: &[u8]) -> ImageFormat {
    if data.len() >= 2 && data[0] == b'B' && data[1] == b'M' {
        return ImageFormat::Bmp;
    }
    if data.len() >= 8 && data[0..4] == [0x89, b'P', b'N', b'G'] {
        return ImageFormat::Png;
    }
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
        return ImageFormat::Jpg;
    }
    ImageFormat::Unknown
}

/// Known image formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Bmp,
    Png,
    Jpg,
    Unknown,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bmp_24(width: u32, height: u32) -> Vec<u8> {
        let row_stride = ((width * 3 + 3) / 4) * 4;
        let pixel_size = row_stride * height;
        let file_size = 54 + pixel_size;

        let mut data = vec![0u8; file_size as usize];
        data[0] = b'B';
        data[1] = b'M';
        data[2..6].copy_from_slice(&(file_size).to_le_bytes());
        data[10..14].copy_from_slice(&54_u32.to_le_bytes());
        data[14..18].copy_from_slice(&40_u32.to_le_bytes()); // header size
        data[18..22].copy_from_slice(&(width as i32).to_le_bytes());
        data[22..26].copy_from_slice(&(height as i32).to_le_bytes());
        data[26..28].copy_from_slice(&1_u16.to_le_bytes()); // planes
        data[28..30].copy_from_slice(&24_u16.to_le_bytes()); // bpp

        // Fill with red (BGR = 00 00 FF)
        for y in 0..height {
            for x in 0..width {
                let offset = 54 + (y * row_stride + x * 3) as usize;
                data[offset] = 0; // B
                data[offset + 1] = 0; // G
                data[offset + 2] = 255; // R
            }
        }
        data
    }

    #[test]
    fn decode_bmp_24bit() {
        let bmp = make_bmp_24(4, 4);
        let img = decode_bmp(&bmp).unwrap();
        assert_eq!(img.width, 4);
        assert_eq!(img.height, 4);
        assert_eq!(img.pixel_count(), 16);
        assert_eq!(img.pixels[0], 255); // R
        assert_eq!(img.pixels[3], 255); // A
    }

    #[test]
    fn decode_bmp_invalid() {
        assert!(decode_bmp(b"not bmp").is_err());
    }

    #[test]
    fn decode_bmp_too_short() {
        assert!(decode_bmp(&[b'B', b'M']).is_err());
    }

    #[test]
    fn detect_bmp() {
        assert_eq!(detect_image_format(&[b'B', b'M', 0, 0]), ImageFormat::Bmp);
    }

    #[test]
    fn detect_png() {
        assert_eq!(
            detect_image_format(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
            ImageFormat::Png
        );
    }

    #[test]
    fn detect_jpg() {
        assert_eq!(detect_image_format(&[0xFF, 0xD8, 0xFF]), ImageFormat::Jpg);
    }

    #[test]
    fn detect_unknown() {
        assert_eq!(detect_image_format(&[0, 0, 0]), ImageFormat::Unknown);
    }

    #[test]
    fn decoded_image_byte_size() {
        let img = DecodedImage {
            width: 2,
            height: 2,
            pixels: vec![0; 16],
        };
        assert_eq!(img.byte_size(), 16);
    }
}
