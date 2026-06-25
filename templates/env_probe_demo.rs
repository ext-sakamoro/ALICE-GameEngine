//! Environment probe demo — builds a synthetic "sky" cubemap with
//! distinct face colors, runs the irradiance and radiance prefilters,
//! samples the results, and prints sanity values.
//!
//! ```bash
//! cargo run --example env_probe_demo
//! ```

use alice_game_engine::env_probe::{
    prefilter_irradiance, prefilter_radiance, Cubemap, PrefilteredEnvProbe,
};
use alice_game_engine::math::{Color, Vec3};

fn main() {
    println!("=== EnvProbe Demo ===");

    // Source "sky" — bright sun overhead, ground darker.
    let source = Cubemap::new_per_face_color(
        16,
        [
            Color::new(0.6, 0.5, 0.4, 1.0),   // +X warm side
            Color::new(0.4, 0.5, 0.6, 1.0),   // -X cool side
            Color::new(1.5, 1.4, 1.0, 1.0),   // +Y sky / sun (HDR)
            Color::new(0.1, 0.08, 0.06, 1.0), // -Y ground
            Color::new(0.5, 0.55, 0.6, 1.0),
            Color::new(0.55, 0.5, 0.45, 1.0),
        ],
    );

    let t0 = std::time::Instant::now();
    let irradiance = prefilter_irradiance(&source, 8);
    let t_irr = t0.elapsed();

    let t0 = std::time::Instant::now();
    let radiance = prefilter_radiance(&source, 5);
    let t_rad = t0.elapsed();

    let probe = PrefilteredEnvProbe {
        position: Vec3::new(0.0, 1.7, 0.0),
        influence_radius: 20.0,
        irradiance,
        radiance_mips: radiance,
    };

    println!("source cubemap: 16×16, 6 faces");
    println!("irradiance: 8×8, built in {t_irr:?}");
    println!(
        "radiance mip chain: {} levels, built in {t_rad:?}",
        probe.radiance_mips.len()
    );

    println!("\nirradiance samples (low-frequency, diffuse IBL):");
    for (label, dir) in [
        ("up    ", Vec3::Y),
        ("down  ", Vec3::new(0.0, -1.0, 0.0)),
        ("+X    ", Vec3::X),
        ("+Z    ", Vec3::Z),
    ] {
        let c = probe.irradiance.sample(dir);
        println!("  {label}: ({:>5.3}, {:>5.3}, {:>5.3})", c.r, c.g, c.b);
    }

    println!("\nradiance specular samples (roughness 0.0 vs 1.0):");
    let dir = Vec3::Y;
    let smooth = probe.radiance_mips[0].sample(dir);
    let rough = probe.radiance_mips[probe.radiance_mips.len() - 1].sample(dir);
    println!(
        "  smooth (mip 0): ({:>5.3}, {:>5.3}, {:>5.3})",
        smooth.r, smooth.g, smooth.b,
    );
    println!(
        "  rough  (mip last): ({:>5.3}, {:>5.3}, {:>5.3})",
        rough.r, rough.g, rough.b,
    );
}
