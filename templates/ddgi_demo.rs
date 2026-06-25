//! DDGI demo — fills a small probe volume with two synthetic
//! "irradiance" colours (warm interior, cool exterior) and queries a
//! few world positions to show trilinear blending.
//!
//! ```bash
//! cargo run --example ddgi_demo
//! ```

use alice_game_engine::ddgi::{DdgiConfig, DdgiVolume};
use alice_game_engine::math::Vec3;

fn main() {
    println!("=== DDGI Demo ===");

    let config = DdgiConfig {
        grid: (4, 4, 4),
        spacing: 5.0,
        origin: Vec3::ZERO,
        irradiance_resolution: 6,
        visibility_resolution: 16,
        hysteresis: 1.0, // full replace this frame for the demo
    };
    let mut v = DdgiVolume::new(config);

    println!(
        "probe grid: {:?} (= {} probes)",
        config.grid,
        v.probe_count()
    );

    // Warm at the centre, cool at the edges (Manhattan-distance falloff).
    let centre = (
        (config.grid.0 - 1) as f32 * 0.5,
        (config.grid.1 - 1) as f32 * 0.5,
        (config.grid.2 - 1) as f32 * 0.5,
    );
    let irr_n = (config.irradiance_resolution as usize).pow(2) * 3;
    for k in 0..config.grid.2 {
        for j in 0..config.grid.1 {
            for i in 0..config.grid.0 {
                let dist = (i as f32 - centre.0).abs()
                    + (j as f32 - centre.1).abs()
                    + (k as f32 - centre.2).abs();
                let max_dist = (config.grid.0 + config.grid.1 + config.grid.2) as f32 * 0.5;
                let warm = 1.0 - (dist / max_dist).clamp(0.0, 1.0);
                let mut samples = vec![0.0; irr_n];
                for tex in samples.chunks_exact_mut(3) {
                    tex[0] = warm;
                    tex[1] = warm * 0.6;
                    tex[2] = (1.0 - warm) * 0.7;
                }
                let idx = v.probe_index(i, j, k).unwrap();
                v.update_probe_irradiance(idx, &samples);
            }
        }
    }

    let probes = [
        ("centre", Vec3::new(7.5, 7.5, 7.5)),
        ("inside corner", Vec3::new(2.5, 2.5, 2.5)),
        ("near edge", Vec3::new(14.0, 1.0, 1.0)),
        ("outside", Vec3::new(-100.0, 0.0, 0.0)),
    ];

    for (label, world) in probes {
        let (r, g, b) = v.sample_irradiance(world, Vec3::Y);
        println!("  {label:>14}: ({:.3}, {:.3}, {:.3})", r, g, b,);
    }
}
