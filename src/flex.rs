//! Flexbox-inspired layout (Bevy `bevy_ui` style).
//!
//! Computes child sizes + positions inside a fixed-size container
//! using the same primary axis (`FlexDirection`), justify
//! (main-axis distribution), and align (cross-axis alignment)
//! vocabulary as CSS Flexbox + Bevy's UI module. Children with
//! non-zero `grow` consume the remaining main-axis space
//! proportionally.

use serde::{Deserialize, Serialize};

use crate::math::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JustifyContent {
    FlexStart,
    Centre,
    FlexEnd,
    SpaceBetween,
    SpaceAround,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlignItems {
    FlexStart,
    Centre,
    FlexEnd,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlexNode {
    /// Preferred size on the main + cross axis (px). `0.0` means
    /// "let the layout decide" (= grow / stretch).
    pub size: Vec2,
    /// Flex-grow weight. Children share the leftover main-axis space
    /// in proportion to this value.
    pub grow: f32,
    /// Padding applied uniformly inside the node (px).
    pub padding: f32,
}

impl Default for FlexNode {
    fn default() -> Self {
        Self {
            size: Vec2::ZERO,
            grow: 1.0,
            padding: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlexLayout {
    pub direction: FlexDirection,
    pub justify: JustifyContent,
    pub align: AlignItems,
    pub gap: f32,
}

impl Default for FlexLayout {
    fn default() -> Self {
        Self {
            direction: FlexDirection::Row,
            justify: JustifyContent::FlexStart,
            align: AlignItems::FlexStart,
            gap: 0.0,
        }
    }
}

/// Position + size of a resolved child. `position` is the top-left
/// corner of the child inside the container, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedChild {
    pub position: Vec2,
    pub size: Vec2,
}

/// Resolve `children` inside a container of `container_size` (px)
/// according to `layout`. Returns one [`ResolvedChild`] per input
/// child, in the same order.
#[must_use]
pub fn solve(
    container_size: Vec2,
    layout: FlexLayout,
    children: &[FlexNode],
) -> Vec<ResolvedChild> {
    if children.is_empty() {
        return Vec::new();
    }
    let row = matches!(layout.direction, FlexDirection::Row);
    let main_total = if row {
        container_size.x()
    } else {
        container_size.y()
    };
    let cross_total = if row {
        container_size.y()
    } else {
        container_size.x()
    };

    let main_size_of = |c: &FlexNode| if row { c.size.x() } else { c.size.y() };
    let cross_size_of = |c: &FlexNode| if row { c.size.y() } else { c.size.x() };

    let total_gaps = layout.gap * (children.len().saturating_sub(1) as f32);
    let fixed_sum: f32 = children.iter().map(main_size_of).sum();
    let grow_sum: f32 = children.iter().map(|c| c.grow).sum();
    let leftover = (main_total - fixed_sum - total_gaps).max(0.0);

    let mut main_sizes: Vec<f32> = Vec::with_capacity(children.len());
    for c in children {
        let base = main_size_of(c);
        let share = if grow_sum > 0.0 {
            leftover * c.grow / grow_sum
        } else {
            0.0
        };
        main_sizes.push(base + share);
    }

    let used_main: f32 = main_sizes.iter().sum::<f32>() + total_gaps;
    let free = (main_total - used_main).max(0.0);
    let (mut cursor, between) = match layout.justify {
        JustifyContent::FlexStart => (0.0, layout.gap),
        JustifyContent::FlexEnd => (free, layout.gap),
        JustifyContent::Centre => (free * 0.5, layout.gap),
        JustifyContent::SpaceBetween => {
            let extra = if children.len() > 1 {
                free / (children.len() - 1) as f32
            } else {
                0.0
            };
            (0.0, layout.gap + extra)
        }
        JustifyContent::SpaceAround => {
            let slot = if !children.is_empty() {
                free / children.len() as f32
            } else {
                0.0
            };
            (slot * 0.5, layout.gap + slot)
        }
    };

    let mut out = Vec::with_capacity(children.len());
    for (i, c) in children.iter().enumerate() {
        let main = main_sizes[i];
        let cross_basis = cross_size_of(c);
        let cross = match layout.align {
            AlignItems::Stretch => cross_total,
            _ => cross_basis.max(0.0),
        };
        let cross_pos = match layout.align {
            AlignItems::FlexStart | AlignItems::Stretch => 0.0,
            AlignItems::Centre => (cross_total - cross) * 0.5,
            AlignItems::FlexEnd => cross_total - cross,
        };
        let (size, position) = if row {
            (Vec2::new(main, cross), Vec2::new(cursor, cross_pos))
        } else {
            (Vec2::new(cross, main), Vec2::new(cross_pos, cursor))
        };
        out.push(ResolvedChild { position, size });
        cursor += main + between;
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
    fn row_grow_distributes_leftover_space() {
        let layout = FlexLayout::default();
        let children = vec![
            FlexNode {
                size: Vec2::new(100.0, 50.0),
                grow: 1.0,
                padding: 0.0,
            },
            FlexNode {
                size: Vec2::new(100.0, 50.0),
                grow: 3.0,
                padding: 0.0,
            },
        ];
        let solved = solve(Vec2::new(600.0, 100.0), layout, &children);
        assert_eq!(solved.len(), 2);
        // Leftover = 600 - 200 = 400, split 1:3 → 100, 300 extra.
        assert!((solved[0].size.x() - 200.0).abs() < 1e-3);
        assert!((solved[1].size.x() - 400.0).abs() < 1e-3);
        assert!((solved[1].position.x() - 200.0).abs() < 1e-3);
    }

    #[test]
    fn column_justify_centre_stacks_with_offset() {
        let layout = FlexLayout {
            direction: FlexDirection::Column,
            justify: JustifyContent::Centre,
            ..FlexLayout::default()
        };
        let children = vec![
            FlexNode {
                size: Vec2::new(50.0, 50.0),
                grow: 0.0,
                padding: 0.0,
            };
            2
        ];
        let solved = solve(Vec2::new(200.0, 400.0), layout, &children);
        // Free = 400 - 100 = 300, centre → cursor starts at 150.
        assert!((solved[0].position.y() - 150.0).abs() < 1e-3);
        assert!((solved[1].position.y() - 200.0).abs() < 1e-3);
    }

    #[test]
    fn align_stretch_extends_cross_axis() {
        let layout = FlexLayout {
            align: AlignItems::Stretch,
            ..FlexLayout::default()
        };
        let children = vec![FlexNode {
            size: Vec2::new(100.0, 20.0),
            grow: 1.0,
            padding: 0.0,
        }];
        let solved = solve(Vec2::new(300.0, 80.0), layout, &children);
        assert!((solved[0].size.y() - 80.0).abs() < 1e-3);
    }

    #[test]
    fn space_between_pushes_first_and_last_to_edges() {
        let layout = FlexLayout {
            justify: JustifyContent::SpaceBetween,
            ..FlexLayout::default()
        };
        let children = vec![
            FlexNode {
                size: Vec2::new(50.0, 50.0),
                grow: 0.0,
                padding: 0.0,
            };
            3
        ];
        let solved = solve(Vec2::new(500.0, 100.0), layout, &children);
        assert!((solved[0].position.x()).abs() < 1e-3);
        assert!((solved[2].position.x() + solved[2].size.x() - 500.0).abs() < 1e-3);
    }

    #[test]
    fn gap_separates_children() {
        let layout = FlexLayout {
            gap: 10.0,
            ..FlexLayout::default()
        };
        let children = vec![
            FlexNode {
                size: Vec2::new(50.0, 50.0),
                grow: 0.0,
                padding: 0.0,
            };
            3
        ];
        let solved = solve(Vec2::new(500.0, 100.0), layout, &children);
        assert!((solved[1].position.x() - solved[0].position.x() - 60.0).abs() < 1e-3);
    }

    #[test]
    fn empty_children_returns_empty() {
        let solved = solve(Vec2::new(100.0, 100.0), FlexLayout::default(), &[]);
        assert!(solved.is_empty());
    }
}
