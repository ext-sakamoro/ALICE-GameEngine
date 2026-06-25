//! CSS-Grid style layout solver.
//!
//! Sibling to [`crate::flex`] for 2D table-style layouts that flex
//! cannot express directly: header bars with named columns, photo
//! galleries, dashboard tiles. `TrackSize::Auto` shares leftover
//! space equally between flexible tracks; `TrackSize::Fixed(px)`
//! is rigid; `TrackSize::Fraction(weight)` mirrors CSS `fr`.

use serde::{Deserialize, Serialize};

use crate::math::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TrackSize {
    Fixed(f32),
    Fraction(f32),
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridCell {
    pub column: u32,
    pub row: u32,
    pub column_span: u32,
    pub row_span: u32,
}

impl GridCell {
    #[must_use]
    pub const fn one(column: u32, row: u32) -> Self {
        Self {
            column,
            row,
            column_span: 1,
            row_span: 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridLayout {
    pub columns: Vec<TrackSize>,
    pub rows: Vec<TrackSize>,
    pub gap: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedCell {
    pub position: Vec2,
    pub size: Vec2,
}

#[must_use]
pub fn solve(container_size: Vec2, layout: &GridLayout, cells: &[GridCell]) -> Vec<ResolvedCell> {
    let column_sizes = solve_tracks(container_size.x(), &layout.columns, layout.gap);
    let row_sizes = solve_tracks(container_size.y(), &layout.rows, layout.gap);
    let column_offsets = cumulative_offsets(&column_sizes, layout.gap);
    let row_offsets = cumulative_offsets(&row_sizes, layout.gap);

    cells
        .iter()
        .map(|c| {
            let x = column_offsets
                .get(c.column as usize)
                .copied()
                .unwrap_or(0.0);
            let y = row_offsets.get(c.row as usize).copied().unwrap_or(0.0);
            let last_col = (c.column + c.column_span.max(1) - 1) as usize;
            let last_row = (c.row + c.row_span.max(1) - 1) as usize;
            let end_x = column_offsets.get(last_col).copied().unwrap_or(0.0)
                + column_sizes.get(last_col).copied().unwrap_or(0.0);
            let end_y = row_offsets.get(last_row).copied().unwrap_or(0.0)
                + row_sizes.get(last_row).copied().unwrap_or(0.0);
            ResolvedCell {
                position: Vec2::new(x, y),
                size: Vec2::new((end_x - x).max(0.0), (end_y - y).max(0.0)),
            }
        })
        .collect()
}

fn solve_tracks(total: f32, tracks: &[TrackSize], gap: f32) -> Vec<f32> {
    if tracks.is_empty() {
        return Vec::new();
    }
    let total_gaps = gap * (tracks.len().saturating_sub(1) as f32);
    let fixed_sum: f32 = tracks
        .iter()
        .map(|t| match t {
            TrackSize::Fixed(px) => *px,
            _ => 0.0,
        })
        .sum();
    let fraction_sum: f32 = tracks
        .iter()
        .map(|t| match t {
            TrackSize::Fraction(f) => *f,
            _ => 0.0,
        })
        .sum();
    let auto_count = tracks
        .iter()
        .filter(|t| matches!(t, TrackSize::Auto))
        .count() as f32;
    let leftover = (total - fixed_sum - total_gaps).max(0.0);
    // Auto tracks split the leftover before fractions claim it (Bevy's
    // grid follows the same rule).
    let auto_share = if auto_count > 0.0 {
        leftover / (auto_count + fraction_sum.max(0.0))
    } else {
        0.0
    };
    let fraction_share = if fraction_sum > 0.0 {
        (leftover - auto_share * auto_count) / fraction_sum
    } else {
        0.0
    };
    tracks
        .iter()
        .map(|t| match t {
            TrackSize::Fixed(px) => *px,
            TrackSize::Auto => auto_share,
            TrackSize::Fraction(f) => fraction_share * f,
        })
        .collect()
}

fn cumulative_offsets(sizes: &[f32], gap: f32) -> Vec<f32> {
    let mut out = Vec::with_capacity(sizes.len());
    let mut cursor = 0.0;
    for (i, s) in sizes.iter().enumerate() {
        out.push(cursor);
        cursor += s;
        if i + 1 < sizes.len() {
            cursor += gap;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_tracks_keep_size() {
        let layout = GridLayout {
            columns: vec![TrackSize::Fixed(100.0), TrackSize::Fixed(200.0)],
            rows: vec![TrackSize::Fixed(50.0)],
            gap: 0.0,
        };
        let cells = vec![GridCell::one(0, 0), GridCell::one(1, 0)];
        let solved = solve(Vec2::new(500.0, 200.0), &layout, &cells);
        assert!((solved[0].size.x() - 100.0).abs() < 1e-3);
        assert!((solved[1].size.x() - 200.0).abs() < 1e-3);
        assert!((solved[1].position.x() - 100.0).abs() < 1e-3);
    }

    #[test]
    fn fraction_tracks_split_leftover() {
        let layout = GridLayout {
            columns: vec![TrackSize::Fraction(1.0), TrackSize::Fraction(3.0)],
            rows: vec![TrackSize::Fixed(50.0)],
            gap: 0.0,
        };
        let cells = vec![GridCell::one(0, 0), GridCell::one(1, 0)];
        let solved = solve(Vec2::new(400.0, 100.0), &layout, &cells);
        assert!((solved[0].size.x() - 100.0).abs() < 1e-3);
        assert!((solved[1].size.x() - 300.0).abs() < 1e-3);
    }

    #[test]
    fn auto_tracks_split_evenly() {
        let layout = GridLayout {
            columns: vec![TrackSize::Auto, TrackSize::Auto, TrackSize::Auto],
            rows: vec![TrackSize::Fixed(50.0)],
            gap: 0.0,
        };
        let cells = vec![
            GridCell::one(0, 0),
            GridCell::one(1, 0),
            GridCell::one(2, 0),
        ];
        let solved = solve(Vec2::new(300.0, 100.0), &layout, &cells);
        for c in &solved {
            assert!((c.size.x() - 100.0).abs() < 1e-3);
        }
    }

    #[test]
    fn gap_separates_columns() {
        let layout = GridLayout {
            columns: vec![TrackSize::Fixed(100.0); 2],
            rows: vec![TrackSize::Fixed(50.0)],
            gap: 20.0,
        };
        let cells = vec![GridCell::one(0, 0), GridCell::one(1, 0)];
        let solved = solve(Vec2::new(300.0, 100.0), &layout, &cells);
        assert!((solved[1].position.x() - 120.0).abs() < 1e-3);
    }

    #[test]
    fn column_span_covers_multiple_tracks() {
        let layout = GridLayout {
            columns: vec![TrackSize::Fixed(50.0); 4],
            rows: vec![TrackSize::Fixed(50.0)],
            gap: 0.0,
        };
        let cells = vec![GridCell {
            column: 1,
            row: 0,
            column_span: 2,
            row_span: 1,
        }];
        let solved = solve(Vec2::new(200.0, 100.0), &layout, &cells);
        assert!((solved[0].position.x() - 50.0).abs() < 1e-3);
        assert!((solved[0].size.x() - 100.0).abs() < 1e-3);
    }

    #[test]
    fn empty_layout_returns_empty() {
        let layout = GridLayout {
            columns: Vec::new(),
            rows: Vec::new(),
            gap: 0.0,
        };
        let solved = solve(Vec2::new(100.0, 100.0), &layout, &[]);
        assert!(solved.is_empty());
    }
}
