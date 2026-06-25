//! 3D Gaussian Splatting demo — places 200 random splats in a cube,
//! prepares one frame, and prints depth-sort + cull statistics.
//!
//! ```bash
//! cargo run --example gaussian_splat_demo
//! ```

use alice_game_engine::gaussian_splat::{GaussianCloud, Splat};
use alice_game_engine::math::{Color, Mat4, Vec3};

fn pseudo_rand(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    ((*state >> 16) & 0x7FFF) as f32 / 32_767.0
}

fn main() {
    println!("=== 3D Gaussian Splatting Demo ===");

    let mut cloud = GaussianCloud::new();
    let mut seed = 0xA5A5_A5A5_u32;

    for _ in 0..200 {
        let p = Vec3::new(
            pseudo_rand(&mut seed) * 4.0 - 2.0,
            pseudo_rand(&mut seed) * 4.0 - 2.0,
            pseudo_rand(&mut seed) * 4.0 - 2.0,
        );
        let radius = 0.05 + pseudo_rand(&mut seed) * 0.1;
        let color = Color::new(
            pseudo_rand(&mut seed),
            pseudo_rand(&mut seed),
            pseudo_rand(&mut seed),
            1.0,
        );
        let opacity = 0.4 + pseudo_rand(&mut seed) * 0.6;
        cloud.add(Splat::isotropic(p, radius, color, opacity));
    }

    println!("splats authored: {}", cloud.len());

    let view = Mat4::look_at(Vec3::new(0.0, 0.0, 8.0), Vec3::ZERO, Vec3::Y);
    let proj = Mat4::perspective(std::f32::consts::FRAC_PI_3, 16.0 / 9.0, 0.1, 100.0);

    let t0 = std::time::Instant::now();
    let projected = cloud.prepare_frame(view, proj);
    let elapsed = t0.elapsed();

    println!("projected (visible after cull): {}", projected.len());
    println!("prepare_frame: {elapsed:?}");

    if let (Some(first), Some(last)) = (projected.first(), projected.last()) {
        println!(
            "back of list (farthest): source={} depth={:.2} ndc=({:.2}, {:.2}) radius={:.3}",
            first.source, first.depth, first.ndc.0, first.ndc.1, first.radius,
        );
        println!(
            "front of list (nearest): source={} depth={:.2} ndc=({:.2}, {:.2}) radius={:.3}",
            last.source, last.depth, last.ndc.0, last.ndc.1, last.radius,
        );
    }

    // Verify monotonic depth ordering.
    let mut sorted = true;
    for w in projected.windows(2) {
        if w[0].depth < w[1].depth {
            sorted = false;
            break;
        }
    }
    println!("back-to-front sort holds: {sorted}");
}
