//! Ocean demo — runs the Tessendorf FFT simulator for several time
//! steps and prints a small ASCII height map so the surface motion is
//! visible without a renderer.
//!
//! ```bash
//! cargo run --example ocean_demo
//! ```

use alice_game_engine::math::Vec2;
use alice_game_engine::ocean::{OceanConfig, OceanSimulator};

fn ascii_for(height: f32, min: f32, max: f32) -> char {
    let glyphs = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];
    if (max - min).abs() < 1e-6 {
        return glyphs[0];
    }
    let t = ((height - min) / (max - min)).clamp(0.0, 1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let idx = (t * (glyphs.len() as f32 - 1.0)).round() as usize;
    glyphs[idx.min(glyphs.len() - 1)]
}

fn print_frame(label: &str, heights: &[f32], grid_size: u32) {
    let n = grid_size as usize;
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for &h in heights {
        if h < min {
            min = h;
        }
        if h > max {
            max = h;
        }
    }
    println!("\n{label} — range [{min:.3}, {max:.3}]");
    for j in 0..n {
        let mut row = String::with_capacity(n * 2);
        for i in 0..n {
            row.push(ascii_for(heights[j * n + i], min, max));
            row.push(' ');
        }
        println!("  {row}");
    }
}

fn main() {
    println!("=== Tessendorf Ocean Demo ===");

    let config = OceanConfig {
        grid_size: 32,
        patch_size: 60.0,
        wind_direction: Vec2::new(1.0, 0.3),
        wind_speed: 18.0,
        amplitude: 0.001,
        gravity: 9.81,
    };
    let mut sim = OceanSimulator::new(config);

    let start = std::time::Instant::now();

    // Sample a coarse 16×16 region of the 32×32 simulation for printing.
    for &time in &[0.0_f32, 1.5, 3.0, 6.0] {
        let frame = sim.simulate(time);
        let n = frame.grid_size as usize;
        let coarse_n = 16;
        let stride = n / coarse_n;
        let mut coarse = Vec::with_capacity(coarse_n * coarse_n);
        for j in 0..coarse_n {
            for i in 0..coarse_n {
                coarse.push(frame.heights[(j * stride) * n + (i * stride)]);
            }
        }
        print_frame(&format!("t = {time:>4.1}s"), &coarse, coarse_n as u32);
    }

    let elapsed = start.elapsed();
    println!(
        "\n4 simulate() calls on a 32×32 grid: {elapsed:?} (≈ {:.2} ms/frame)",
        elapsed.as_secs_f32() * 1000.0 / 4.0,
    );
}
