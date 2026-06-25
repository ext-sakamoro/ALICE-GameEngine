//! Volumetric clouds demo — marches rays through the cloud layer for a
//! grid of view directions and prints transmittance as ASCII art.
//!
//! ```bash
//! cargo run --example volumetric_clouds_demo
//! ```

use alice_game_engine::math::{Vec2, Vec3};
use alice_game_engine::volumetric_clouds::{march_cloud_ray, VolumetricCloudConfig};

fn glyph_for_transmittance(t: f32) -> char {
    let glyphs = ['@', '%', '#', '*', '+', '=', '-', ':', '.', ' '];
    let idx = (t.clamp(0.0, 1.0) * (glyphs.len() as f32 - 1.0)).round() as usize;
    glyphs[idx.min(glyphs.len() - 1)]
}

fn print_sky(label: &str, config: &VolumetricCloudConfig, time: f32) {
    println!("\n{label} (t = {time:.1}s)");
    let n = 32_u32;
    let observer = Vec3::new(0.0, 0.0, 0.0);
    for j in 0..n / 2 {
        let mut row = String::with_capacity(n as usize * 2);
        // Top half of the sky (y ≥ 0).
        for i in 0..n {
            let u = (i as f32 + 0.5) / n as f32 * 2.0 - 1.0; // -1..1
            let v = (j as f32 + 0.5) / (n as f32 / 2.0); // 0..1
            let dir = Vec3::new(u, v, -1.0).normalize();
            let r = march_cloud_ray(observer, dir, time, config);
            row.push(glyph_for_transmittance(r.transmittance));
            row.push(' ');
        }
        println!("  {row}");
    }
}

fn main() {
    println!("=== Volumetric Clouds Demo ===");

    let mut config = VolumetricCloudConfig {
        coverage: 0.6,
        density_scale: 1.4,
        wind: Vec2::new(40.0, 5.0),
        base_height: 1500.0,
        max_height: 3500.0,
        step_count: 24,
        ..VolumetricCloudConfig::default()
    };

    let start = std::time::Instant::now();
    print_sky(
        "low coverage (0.3)",
        &VolumetricCloudConfig {
            coverage: 0.3,
            ..config
        },
        0.0,
    );
    print_sky("medium coverage (0.6)", &config, 0.0);
    config.coverage = 0.9;
    print_sky("overcast (0.9)", &config, 0.0);
    print_sky(
        "medium with wind, t=20s",
        &VolumetricCloudConfig {
            coverage: 0.6,
            ..config
        },
        20.0,
    );
    let elapsed = start.elapsed();

    println!("\n4 × 32×16 ray-marches with 24 steps each: {elapsed:?}",);
}
