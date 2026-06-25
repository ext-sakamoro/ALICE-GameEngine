//! GPU BVH + radix sort demo — builds a BVH over 64 random AABBs,
//! prints node count, depth bound, and demonstrates the standalone
//! Morton-code radix sort on a small key-value array.
//!
//! ```bash
//! cargo run --example gpu_bvh_demo
//! ```

use alice_game_engine::gpu_bvh::{morton3_unit, radix_sort_u32_pairs, Aabb, Bvh, BvhNode};
use alice_game_engine::math::Vec3;

fn pseudo(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    ((*state >> 16) & 0x7FFF) as f32 / 32_767.0
}

fn main() {
    println!("=== GPU BVH + Radix Sort Demo ===");

    let mut seed = 0xCAFE_BABE_u32;
    let aabbs: Vec<Aabb> = (0..64)
        .map(|_| {
            let centre = Vec3::new(
                pseudo(&mut seed) * 10.0 - 5.0,
                pseudo(&mut seed) * 4.0,
                pseudo(&mut seed) * 10.0 - 5.0,
            );
            let half = 0.2 + pseudo(&mut seed) * 0.5;
            let extent = Vec3::new(half, half, half);
            Aabb::new(centre - extent, centre + extent)
        })
        .collect();

    let t0 = std::time::Instant::now();
    let bvh = Bvh::build(&aabbs, 4);
    let build = t0.elapsed();

    let leaves: usize = bvh.nodes.iter().filter(|n| n.is_leaf()).count();
    println!(
        "primitives: {}, leaf_size: 4 → nodes: {} (leaves: {}, interior: {})",
        aabbs.len(),
        bvh.nodes.len(),
        leaves,
        bvh.nodes.len() - leaves,
    );
    println!("build: {build:?}");
    println!(
        "scene bounds: min=({:.2},{:.2},{:.2}) max=({:.2},{:.2},{:.2})",
        bvh.scene_bounds.min.x(),
        bvh.scene_bounds.min.y(),
        bvh.scene_bounds.min.z(),
        bvh.scene_bounds.max.x(),
        bvh.scene_bounds.max.y(),
        bvh.scene_bounds.max.z(),
    );

    // Show the first 5 leaves and their primitive ranges.
    println!("\nfirst leaves:");
    let mut shown = 0;
    for (idx, node) in bvh.nodes.iter().enumerate() {
        if !node.is_leaf() {
            continue;
        }
        println!(
            "  node[{idx:>3}]: primitives [{}, {}) bounds=({:.2},{:.2},{:.2})..({:.2},{:.2},{:.2})",
            node.primitive_start,
            node.primitive_start + node.primitive_count,
            node.bounds.min.x(),
            node.bounds.min.y(),
            node.bounds.min.z(),
            node.bounds.max.x(),
            node.bounds.max.y(),
            node.bounds.max.z(),
        );
        shown += 1;
        if shown >= 5 {
            break;
        }
    }

    // Standalone radix sort: morton-code-ish keys.
    let mut pairs: Vec<(u32, &'static str)> = vec![
        (morton3_unit(Vec3::new(0.9, 0.5, 0.1)), "warm corner"),
        (morton3_unit(Vec3::new(0.1, 0.1, 0.1)), "origin"),
        (morton3_unit(Vec3::new(0.5, 0.5, 0.5)), "centre"),
        (morton3_unit(Vec3::new(0.2, 0.8, 0.4)), "tall NW"),
        (morton3_unit(Vec3::new(0.99, 0.99, 0.99)), "far corner"),
    ];
    radix_sort_u32_pairs(&mut pairs);
    println!("\nradix-sorted Morton labels:");
    for (key, label) in &pairs {
        println!("  morton=0x{:08X}  {label}", key);
    }

    // Suppress unused-import warning in trivial demo.
    let _ = BvhNode::INVALID;
}
