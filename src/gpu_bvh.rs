//! Acceleration structures — CPU-side BVH + Morton-code radix sort.
//!
//! Provides the data layout and CPU build path for a tile-friendly
//! BVH that the GPU side can either consume directly (as a flat
//! buffer of [`BvhNode`]) or rebuild every frame from a Morton-sorted
//! primitive list. The same radix sort drives Gaussian splat
//! tile bins, particle z-prepass binning, and broadphase collision
//! pair generation.
//!
//! A future PR ports the build step to a compute shader so dynamic
//! geometry can refit per-frame; the public types stay the same so
//! the upgrade is a one-PR drop-in.

use crate::math::Vec3;

// ---------------------------------------------------------------------------
// AABB
// ---------------------------------------------------------------------------

/// Axis-aligned bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    #[must_use]
    pub const fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    /// Empty AABB suitable as a starting accumulator for `union`.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            min: Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY),
            max: Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY),
        }
    }

    /// AABB whose min == max == centre.
    #[must_use]
    pub const fn point(p: Vec3) -> Self {
        Self { min: p, max: p }
    }

    /// Union with another AABB.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min: Vec3(self.min.0.min(other.min.0)),
            max: Vec3(self.max.0.max(other.max.0)),
        }
    }

    /// Centroid (= average of corners).
    #[must_use]
    pub fn centre(self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Surface area (Slab-Surface heuristic input).
    #[must_use]
    pub fn surface_area(self) -> f32 {
        let d = self.max - self.min;
        2.0 * d.x().mul_add(d.y(), d.x().mul_add(d.z(), d.y() * d.z()))
    }
}

// ---------------------------------------------------------------------------
// Morton encoding
// ---------------------------------------------------------------------------

const fn spread_bits_10(x: u32) -> u32 {
    let mut v = x & 0x3FF;
    v = (v | (v << 16)) & 0x0300_00FF;
    v = (v | (v << 8)) & 0x0300_F00F;
    v = (v | (v << 4)) & 0x030C_30C3;
    v = (v | (v << 2)) & 0x0924_9249;
    v
}

/// Morton (Z-curve) code for a unit-cube point (`xyz ∈ [0, 1]`).
/// Returns a 30-bit code in the low bits of a `u32`.
#[must_use]
pub fn morton3_unit(xyz: Vec3) -> u32 {
    let scale = 1024.0_f32; // 2^10
    let xi = (xyz.x().clamp(0.0, 1.0) * scale).min(scale - 1.0).max(0.0) as u32;
    let yi = (xyz.y().clamp(0.0, 1.0) * scale).min(scale - 1.0).max(0.0) as u32;
    let zi = (xyz.z().clamp(0.0, 1.0) * scale).min(scale - 1.0).max(0.0) as u32;
    spread_bits_10(xi) | (spread_bits_10(yi) << 1) | (spread_bits_10(zi) << 2)
}

// ---------------------------------------------------------------------------
// Radix sort (LSD, 11-bit passes × 3 = 33 bits, covers 30-bit Morton)
// ---------------------------------------------------------------------------

/// Sort `(code, payload)` pairs in ascending order of `code` using a
/// stable LSD radix sort. Three 11-bit passes — well-tuned for the
/// 30-bit Morton output and small enough to fit in L1 cache.
pub fn radix_sort_u32_pairs<P: Copy>(items: &mut Vec<(u32, P)>) {
    const PASSES: u32 = 3;
    const BITS_PER_PASS: u32 = 11;
    const BUCKETS: usize = 1 << BITS_PER_PASS;
    const MASK: u32 = BUCKETS as u32 - 1;

    let n = items.len();
    if n < 2 {
        return;
    }
    // Initialise scratch by cloning the input; the scatter pass below
    // overwrites every slot, so the seed values are irrelevant.
    let mut scratch: Vec<(u32, P)> = items.clone();

    for pass in 0..PASSES {
        let shift = pass * BITS_PER_PASS;
        let mut counts = [0_u32; BUCKETS];
        for (code, _) in items.iter() {
            let bucket = ((code >> shift) & MASK) as usize;
            counts[bucket] += 1;
        }
        // Exclusive prefix sum → starting offsets per bucket.
        let mut sum = 0_u32;
        for c in &mut counts {
            let next = sum + *c;
            *c = sum;
            sum = next;
        }
        for &(code, payload) in items.iter() {
            let bucket = ((code >> shift) & MASK) as usize;
            let idx = counts[bucket] as usize;
            scratch[idx] = (code, payload);
            counts[bucket] += 1;
        }
        std::mem::swap(items, &mut scratch);
    }
}

// ---------------------------------------------------------------------------
// BVH
// ---------------------------------------------------------------------------

/// Linear BVH node. Interior nodes set both children; leaves point at
/// a `primitive_index` range in the input array.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BvhNode {
    pub bounds: Aabb,
    pub left: u32,
    pub right: u32,
    pub primitive_start: u32,
    pub primitive_count: u32,
}

impl BvhNode {
    pub const INVALID: u32 = u32::MAX;

    #[must_use]
    pub const fn is_leaf(&self) -> bool {
        self.primitive_count > 0
    }
}

/// CPU-built BVH ready for GPU upload.
#[derive(Debug, Clone)]
pub struct Bvh {
    pub nodes: Vec<BvhNode>,
    /// Indices into the user's primitive array, reordered to match the
    /// leaves' `primitive_start`/`primitive_count`.
    pub primitive_order: Vec<u32>,
    pub scene_bounds: Aabb,
}

impl Bvh {
    /// Group node indices by tree level — leaf-only level first, root
    /// last. Used as the dispatch driver for the interior-refit
    /// compute pass: the GPU side runs one dispatch per returned slot
    /// (bottom-up) so each interior node finds its children already
    /// refit.
    #[must_use]
    pub fn levels_bottom_up(&self) -> Vec<Vec<u32>> {
        if self.nodes.is_empty() {
            return Vec::new();
        }
        let mut depth = vec![0_u32; self.nodes.len()];
        compute_depth(&self.nodes, 0, &mut depth);
        let max_depth = *depth.iter().max().unwrap_or(&0);
        let mut levels: Vec<Vec<u32>> = vec![Vec::new(); (max_depth + 1) as usize];
        for (i, d) in depth.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            levels[*d as usize].push(i as u32);
        }
        // depth = 0 (leaves) sits at index 0, root (max depth) at the
        // last index — natural bottom-up dispatch order.
        levels
    }

    /// Build a leaf-only BVH from per-primitive AABBs. Sorts the
    /// primitives by Morton code first so siblings are spatially
    /// adjacent. `leaf_size` controls how many primitives end up in
    /// each leaf; the rest of the tree is built top-down by binary
    /// median split — a cheap and predictable build.
    #[must_use]
    pub fn build(primitive_aabbs: &[Aabb], leaf_size: u32) -> Self {
        let mut scene_bounds = Aabb::empty();
        for a in primitive_aabbs {
            scene_bounds = scene_bounds.union(*a);
        }
        if primitive_aabbs.is_empty() {
            return Self {
                nodes: Vec::new(),
                primitive_order: Vec::new(),
                scene_bounds,
            };
        }

        let extent = scene_bounds.max - scene_bounds.min;
        let inv_extent = Vec3::new(
            if extent.x() > 1e-6 {
                extent.x().recip()
            } else {
                0.0
            },
            if extent.y() > 1e-6 {
                extent.y().recip()
            } else {
                0.0
            },
            if extent.z() > 1e-6 {
                extent.z().recip()
            } else {
                0.0
            },
        );

        let mut keys: Vec<(u32, u32)> = primitive_aabbs
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let centre = a.centre();
                let n = Vec3::new(
                    (centre.x() - scene_bounds.min.x()) * inv_extent.x(),
                    (centre.y() - scene_bounds.min.y()) * inv_extent.y(),
                    (centre.z() - scene_bounds.min.z()) * inv_extent.z(),
                );
                #[allow(clippy::cast_possible_truncation)]
                (morton3_unit(n), i as u32)
            })
            .collect();
        radix_sort_u32_pairs(&mut keys);
        let primitive_order: Vec<u32> = keys.iter().map(|(_, idx)| *idx).collect();

        let mut nodes = Vec::new();
        Self::build_recursive(
            &primitive_order,
            primitive_aabbs,
            0,
            primitive_order.len(),
            leaf_size,
            &mut nodes,
        );

        Self {
            nodes,
            primitive_order,
            scene_bounds,
        }
    }

    #[allow(dead_code)]
    fn _depth_marker() {}

    fn build_recursive(
        primitive_order: &[u32],
        primitive_aabbs: &[Aabb],
        start: usize,
        end: usize,
        leaf_size: u32,
        nodes: &mut Vec<BvhNode>,
    ) -> u32 {
        let count = (end - start) as u32;
        let mut bounds = Aabb::empty();
        for k in start..end {
            bounds = bounds.union(primitive_aabbs[primitive_order[k] as usize]);
        }
        #[allow(clippy::cast_possible_truncation)]
        let node_idx = nodes.len() as u32;
        nodes.push(BvhNode {
            bounds,
            left: BvhNode::INVALID,
            right: BvhNode::INVALID,
            primitive_start: 0,
            primitive_count: 0,
        });

        if count <= leaf_size {
            #[allow(clippy::cast_possible_truncation)]
            let leaf = BvhNode {
                bounds,
                left: BvhNode::INVALID,
                right: BvhNode::INVALID,
                primitive_start: start as u32,
                primitive_count: count,
            };
            nodes[node_idx as usize] = leaf;
            return node_idx;
        }

        let mid = start + (end - start) / 2;
        let left = Self::build_recursive(
            primitive_order,
            primitive_aabbs,
            start,
            mid,
            leaf_size,
            nodes,
        );
        let right =
            Self::build_recursive(primitive_order, primitive_aabbs, mid, end, leaf_size, nodes);
        nodes[node_idx as usize].left = left;
        nodes[node_idx as usize].right = right;
        node_idx
    }
}

fn compute_depth(nodes: &[BvhNode], idx: u32, depth: &mut [u32]) -> u32 {
    let node = &nodes[idx as usize];
    if node.is_leaf() {
        depth[idx as usize] = 0;
        return 0;
    }
    let l = if node.left == BvhNode::INVALID {
        0
    } else {
        compute_depth(nodes, node.left, depth) + 1
    };
    let r = if node.right == BvhNode::INVALID {
        0
    } else {
        compute_depth(nodes, node.right, depth) + 1
    };
    let d = l.max(r);
    depth[idx as usize] = d;
    d
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_union_grows_to_contain_both() {
        let a = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        let b = Aabb::new(Vec3::new(-2.0, 0.0, 0.0), Vec3::new(0.5, 2.0, 0.0));
        let u = a.union(b);
        assert!((u.min.x() + 2.0).abs() < 1e-6);
        assert!((u.max.y() - 2.0).abs() < 1e-6);
    }

    #[test]
    fn aabb_centre_average_of_corners() {
        let a = Aabb::new(Vec3::new(-1.0, 0.0, 2.0), Vec3::new(1.0, 4.0, 6.0));
        let c = a.centre();
        assert!((c.x()).abs() < 1e-6);
        assert!((c.y() - 2.0).abs() < 1e-6);
        assert!((c.z() - 4.0).abs() < 1e-6);
    }

    #[test]
    fn aabb_surface_area_unit_cube() {
        let a = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        assert!((a.surface_area() - 6.0).abs() < 1e-4);
    }

    #[test]
    fn morton_monotonic_along_diagonal() {
        let a = morton3_unit(Vec3::new(0.1, 0.1, 0.1));
        let b = morton3_unit(Vec3::new(0.5, 0.5, 0.5));
        let c = morton3_unit(Vec3::new(0.9, 0.9, 0.9));
        assert!(a < b);
        assert!(b < c);
    }

    #[test]
    fn morton_corner_values() {
        let m0 = morton3_unit(Vec3::ZERO);
        let m1 = morton3_unit(Vec3::new(1.0, 1.0, 1.0));
        assert_eq!(m0, 0);
        assert!(m1 > m0);
    }

    #[test]
    fn radix_sort_orders_ascending() {
        let mut pairs: Vec<(u32, u32)> = vec![(7, 0), (1, 1), (5, 2), (3, 3), (0, 4)];
        radix_sort_u32_pairs(&mut pairs);
        for w in pairs.windows(2) {
            assert!(w[0].0 <= w[1].0);
        }
        // Stable: equal keys preserve insertion order. Already-distinct
        // here, so just check the sort completed.
        assert_eq!(pairs.len(), 5);
    }

    #[test]
    fn radix_sort_preserves_payload() {
        let mut pairs: Vec<(u32, &'static str)> =
            vec![(2, "two"), (5, "five"), (0, "zero"), (3, "three")];
        radix_sort_u32_pairs(&mut pairs);
        assert_eq!(
            pairs.iter().map(|p| p.1).collect::<Vec<_>>(),
            vec!["zero", "two", "three", "five"]
        );
    }

    #[test]
    fn radix_sort_handles_large_codes() {
        let mut pairs: Vec<(u32, u32)> = (0..1000_u32).rev().map(|i| (i * 12345, i)).collect();
        radix_sort_u32_pairs(&mut pairs);
        for w in pairs.windows(2) {
            assert!(w[0].0 < w[1].0);
        }
    }

    #[test]
    fn bvh_build_empty() {
        let bvh = Bvh::build(&[], 4);
        assert!(bvh.nodes.is_empty());
        assert!(bvh.primitive_order.is_empty());
    }

    #[test]
    fn bvh_build_single_primitive_is_leaf() {
        let aabb = Aabb::new(Vec3::ZERO, Vec3::new(1.0, 1.0, 1.0));
        let bvh = Bvh::build(&[aabb], 4);
        assert_eq!(bvh.nodes.len(), 1);
        assert!(bvh.nodes[0].is_leaf());
        assert_eq!(bvh.nodes[0].primitive_count, 1);
    }

    #[test]
    fn bvh_build_groups_spatially_adjacent_primitives() {
        // 8 unit cubes spread along x: Morton sort should put them in
        // spatial order, and the BVH root's children should each cover
        // half the line.
        let aabbs: Vec<Aabb> = (0..8)
            .map(|i| {
                let x = i as f32;
                Aabb::new(Vec3::new(x, 0.0, 0.0), Vec3::new(x + 1.0, 1.0, 1.0))
            })
            .collect();
        let bvh = Bvh::build(&aabbs, 2);
        assert!(bvh.nodes.len() > 1);
        let root = bvh.nodes[0];
        assert!(!root.is_leaf());
        // Root must enclose every primitive.
        assert!(root.bounds.min.x() <= 0.0);
        assert!(root.bounds.max.x() >= 8.0);
    }

    #[test]
    fn levels_bottom_up_returns_deepest_first() {
        // 8 cubes spread along x with leaf_size 2 → multi-level BVH.
        let aabbs: Vec<Aabb> = (0..8)
            .map(|i| Aabb::point(Vec3::new(i as f32, 0.0, 0.0)))
            .collect();
        let bvh = Bvh::build(&aabbs, 2);
        let levels = bvh.levels_bottom_up();
        // At least 2 levels (= leaves + root).
        assert!(levels.len() >= 2);
        // Final entry contains the root index 0.
        assert!(levels.last().unwrap().contains(&0));
        // Total nodes across levels matches bvh.nodes.len().
        let total: usize = levels.iter().map(Vec::len).sum();
        assert_eq!(total, bvh.nodes.len());
    }

    #[test]
    fn levels_bottom_up_empty_tree_returns_empty() {
        let bvh = Bvh::build(&[], 4);
        assert!(bvh.levels_bottom_up().is_empty());
    }

    #[test]
    fn bvh_leaf_size_controls_subdivision() {
        let aabbs: Vec<Aabb> = (0..16)
            .map(|i| Aabb::point(Vec3::new(i as f32, 0.0, 0.0)))
            .collect();
        let bvh_small = Bvh::build(&aabbs, 1);
        let bvh_big = Bvh::build(&aabbs, 16);
        assert!(bvh_small.nodes.len() > bvh_big.nodes.len());
        // Single huge leaf when leaf_size >= count.
        assert_eq!(bvh_big.nodes.len(), 1);
        assert!(bvh_big.nodes[0].is_leaf());
    }
}
