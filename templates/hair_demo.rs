//! Hair / grass demo — 64 grass-blade strands swayed by wind. Prints
//! tip displacement before / after one second of simulation.
//!
//! ```bash
//! cargo run --example hair_demo
//! ```

use alice_game_engine::hair::{HairConfig, HairSystem};
use alice_game_engine::math::Vec3;

fn main() {
    println!("=== Hair / Grass Demo ===");

    let mut hair = HairSystem::new(HairConfig {
        segments: 6,
        length: 0.4,
        wind_strength: 0.6,
        gravity: 9.81,
        stiffness: 0.85,
        lod_distance: 30.0,
    });

    // Plant a 8×8 grid of blades.
    for x in 0..8 {
        for z in 0..8 {
            let anchor = Vec3::new((x as f32) * 0.5, 0.0, (z as f32) * 0.5);
            hair.add_strand(anchor, Vec3::Y);
        }
    }

    println!(
        "strands: {} (8×8 grid, segments={}, length={:.2} m)",
        hair.strand_count(),
        hair.config.segments,
        hair.config.length,
    );

    let initial_tip = hair.strands()[0].points.last().copied().unwrap();
    let wind = Vec3::new(5.0, 0.0, 2.0);
    let dt = 1.0 / 60.0;

    let start = std::time::Instant::now();
    for _ in 0..60 {
        hair.simulate(dt, wind);
    }
    let elapsed = start.elapsed();

    let after_tip = hair.strands()[0].points.last().copied().unwrap();
    let displacement = (after_tip - initial_tip).length();
    let per_frame_ns = elapsed.as_nanos() / 60;

    println!("\nwind: {wind:?}");
    println!(
        "1 second of simulation ({:?} total, ~{per_frame_ns} ns/frame for 64 strands):",
        elapsed
    );
    println!("  tip 0 displacement: {displacement:.4} m");
    println!(
        "  tip 0 final: ({:.3}, {:.3}, {:.3})",
        after_tip.x(),
        after_tip.y(),
        after_tip.z(),
    );

    // LOD demo
    let close = Vec3::new(2.0, 0.0, 2.0);
    let far = Vec3::new(50.0, 0.0, 50.0);
    println!("\nLOD cutoff {:.1} m:", hair.config.lod_distance,);
    println!(
        "  camera at {close:?}: past LOD = {}",
        hair.past_lod(close, Vec3::ZERO)
    );
    println!(
        "  camera at {far:?}: past LOD = {}",
        hair.past_lod(far, Vec3::ZERO)
    );
}
