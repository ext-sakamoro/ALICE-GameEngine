//! Text rendering: bitmap font, glyph layout, text mesh generation.
//!
//! For production font rendering, inject ALICE-Font via `bridge::FontProvider`.
//!
//! ```rust
//! use alice_game_engine::text::*;
//!
//! let font = BitmapFont::ascii_default();
//! let layout = font.layout_text("Hello", 16.0);
//! assert!(!layout.glyphs.is_empty());
//! ```

use crate::math::Vec2;

// ---------------------------------------------------------------------------
// Glyph
// ---------------------------------------------------------------------------

/// A positioned glyph in a text layout.
#[derive(Debug, Clone, Copy)]
pub struct GlyphInstance {
    pub char_code: u32,
    pub position: Vec2,
    pub size: Vec2,
    pub uv_min: Vec2,
    pub uv_max: Vec2,
}

// ---------------------------------------------------------------------------
// BitmapFont
// ---------------------------------------------------------------------------

/// Simple bitmap font (ASCII grid layout).
#[derive(Debug, Clone)]
pub struct BitmapFont {
    pub name: String,
    pub cell_width: f32,
    pub cell_height: f32,
    pub cols: u32,
    pub first_char: u32,
    pub last_char: u32,
}

impl BitmapFont {
    /// Creates a default ASCII bitmap font (16x16 grid, chars 32-127).
    #[must_use]
    pub fn ascii_default() -> Self {
        Self {
            name: "ascii_default".to_string(),
            cell_width: 8.0,
            cell_height: 16.0,
            cols: 16,
            first_char: 32,
            last_char: 127,
        }
    }

    /// Lays out a string into positioned glyphs.
    #[must_use]
    pub fn layout_text(&self, text: &str, font_size: f32) -> TextLayout {
        let scale = font_size * self.cell_height.recip();
        let glyph_w = self.cell_width * scale;
        let glyph_h = self.cell_height * scale;
        let inv_cols = (self.cols as f32).recip();
        let rows = ((self.last_char - self.first_char) / self.cols + 1) as f32;
        let inv_rows = rows.recip();

        let mut glyphs = Vec::new();
        let mut x = 0.0_f32;

        for ch in text.chars() {
            let code = ch as u32;
            if code < self.first_char || code > self.last_char {
                x += glyph_w;
                continue;
            }
            let idx = code - self.first_char;
            let col = idx % self.cols;
            let row = idx / self.cols;

            glyphs.push(GlyphInstance {
                char_code: code,
                position: Vec2::new(x, 0.0),
                size: Vec2::new(glyph_w, glyph_h),
                uv_min: Vec2::new(col as f32 * inv_cols, row as f32 * inv_rows),
                uv_max: Vec2::new((col + 1) as f32 * inv_cols, (row + 1) as f32 * inv_rows),
            });
            x += glyph_w;
        }

        TextLayout {
            glyphs,
            width: x,
            height: glyph_h,
        }
    }
}

/// Result of text layout.
#[derive(Debug, Clone)]
pub struct TextLayout {
    pub glyphs: Vec<GlyphInstance>,
    pub width: f32,
    pub height: f32,
}

impl TextLayout {
    #[must_use]
    pub const fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }
}

// ---------------------------------------------------------------------------
// Bridge: FontProvider trait for ALICE-Font
// ---------------------------------------------------------------------------

/// Trait for external font providers (e.g. ALICE-Font SDF glyph renderer).
pub trait FontProvider: Send + Sync {
    /// Returns glyph metrics for a character at the given font size.
    fn glyph_metrics(&self, char_code: u32, font_size: f32) -> Option<GlyphMetrics>;

    /// Returns the kerning distance between two characters.
    fn kerning(&self, left: u32, right: u32, font_size: f32) -> f32;
}

/// Glyph metrics from a font provider.
#[derive(Debug, Clone, Copy)]
pub struct GlyphMetrics {
    pub advance: f32,
    pub width: f32,
    pub height: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_font_layout() {
        let font = BitmapFont::ascii_default();
        let layout = font.layout_text("Hi", 16.0);
        assert_eq!(layout.glyph_count(), 2);
        assert!(layout.width > 0.0);
    }

    #[test]
    fn empty_text() {
        let font = BitmapFont::ascii_default();
        let layout = font.layout_text("", 16.0);
        assert_eq!(layout.glyph_count(), 0);
        assert_eq!(layout.width, 0.0);
    }

    #[test]
    fn glyph_positions_sequential() {
        let font = BitmapFont::ascii_default();
        let layout = font.layout_text("ABC", 16.0);
        assert!(layout.glyphs[1].position.x() > layout.glyphs[0].position.x());
        assert!(layout.glyphs[2].position.x() > layout.glyphs[1].position.x());
    }

    #[test]
    fn uv_in_range() {
        let font = BitmapFont::ascii_default();
        let layout = font.layout_text("Z", 16.0);
        let g = &layout.glyphs[0];
        assert!(g.uv_min.x() >= 0.0 && g.uv_min.x() <= 1.0);
        assert!(g.uv_max.y() >= 0.0 && g.uv_max.y() <= 1.0);
    }

    #[test]
    fn font_size_scales() {
        let font = BitmapFont::ascii_default();
        let small = font.layout_text("A", 8.0);
        let big = font.layout_text("A", 32.0);
        assert!(big.glyphs[0].size.x() > small.glyphs[0].size.x());
    }

    #[test]
    fn non_ascii_skipped() {
        let font = BitmapFont::ascii_default();
        let layout = font.layout_text("A\u{2603}B", 16.0); // snowman
        assert_eq!(layout.glyph_count(), 2); // only A and B
    }

    #[test]
    fn glyph_metrics_struct() {
        let m = GlyphMetrics {
            advance: 8.0,
            width: 7.0,
            height: 12.0,
            bearing_x: 1.0,
            bearing_y: 10.0,
        };
        assert_eq!(m.advance, 8.0);
    }
}
