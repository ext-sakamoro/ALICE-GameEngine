//! GUI theme — Bevy `bevy_ui` style colour palette + spacing scale.
//!
//! A theme is a stateless bundle of colours, spacings, and font
//! sizes that any widget can read to stay visually consistent.
//! Presets cover the two most common cases (Dark / Light); custom
//! themes can be authored by constructing a [`UiTheme`] directly.

use serde::{Deserialize, Serialize};

use crate::math::Color;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct UiTheme {
    pub background: Color,
    pub surface: Color,
    pub primary: Color,
    pub accent: Color,
    pub text: Color,
    pub text_dim: Color,
    pub border: Color,
    pub spacing_small: f32,
    pub spacing_medium: f32,
    pub spacing_large: f32,
    pub corner_radius: f32,
    pub font_size_small: f32,
    pub font_size_body: f32,
    pub font_size_heading: f32,
}

impl UiTheme {
    /// Classic dark theme (= IDE / editor default).
    #[must_use]
    pub fn dark() -> Self {
        Self {
            background: Color::new(0.10, 0.10, 0.12, 1.0),
            surface: Color::new(0.16, 0.16, 0.18, 1.0),
            primary: Color::new(0.30, 0.55, 0.95, 1.0),
            accent: Color::new(0.95, 0.60, 0.30, 1.0),
            text: Color::new(0.92, 0.92, 0.92, 1.0),
            text_dim: Color::new(0.65, 0.65, 0.65, 1.0),
            border: Color::new(0.25, 0.25, 0.28, 1.0),
            spacing_small: 4.0,
            spacing_medium: 8.0,
            spacing_large: 16.0,
            corner_radius: 4.0,
            font_size_small: 12.0,
            font_size_body: 14.0,
            font_size_heading: 20.0,
        }
    }

    /// Light theme (= material design defaults).
    #[must_use]
    pub fn light() -> Self {
        Self {
            background: Color::new(0.98, 0.98, 0.98, 1.0),
            surface: Color::new(1.00, 1.00, 1.00, 1.0),
            primary: Color::new(0.20, 0.45, 0.85, 1.0),
            accent: Color::new(0.85, 0.40, 0.20, 1.0),
            text: Color::new(0.10, 0.10, 0.12, 1.0),
            text_dim: Color::new(0.40, 0.40, 0.42, 1.0),
            border: Color::new(0.80, 0.80, 0.82, 1.0),
            spacing_small: 4.0,
            spacing_medium: 8.0,
            spacing_large: 16.0,
            corner_radius: 4.0,
            font_size_small: 12.0,
            font_size_body: 14.0,
            font_size_heading: 20.0,
        }
    }
}

impl Default for UiTheme {
    fn default() -> Self {
        Self::dark()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_default_has_dark_background() {
        let t = UiTheme::default();
        assert!(t.background.r < 0.2 && t.background.g < 0.2);
    }

    #[test]
    fn light_has_bright_background() {
        let t = UiTheme::light();
        assert!(t.background.r > 0.9 && t.background.g > 0.9);
    }

    #[test]
    fn spacings_form_increasing_scale() {
        let t = UiTheme::default();
        assert!(t.spacing_small < t.spacing_medium);
        assert!(t.spacing_medium < t.spacing_large);
        assert!(t.font_size_small < t.font_size_body);
        assert!(t.font_size_body < t.font_size_heading);
    }

    #[test]
    fn theme_serde_round_trip() {
        let t = UiTheme::light();
        let j = serde_json::to_string(&t).unwrap();
        let back: UiTheme = serde_json::from_str(&j).unwrap();
        assert!((back.background.r - t.background.r).abs() < 1e-6);
    }
}
