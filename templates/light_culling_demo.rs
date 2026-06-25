//! Tiled light culling demo — places 128 random point lights in a small
//! world volume, runs the [`TiledLightCuller`] against a fixed camera,
//! and prints the resulting per-tile distribution + average lights per
//! covered tile.
//!
//! ```bash
//! cargo run --example light_culling_demo
//! ```

use alice_game_engine::light_culling::{LightCullingConfig, TiledLightCuller};
use alice_game_engine::math::{Color, Mat4, Vec3};
use alice_game_engine::renderer::LightRenderData;
use alice_game_engine::scene_graph::LightVariant;

fn random_light(seed: &mut u32) -> LightRenderData {
    // Tiny PCG-style hash for deterministic output without dev-dep.
    #[allow(clippy::cast_precision_loss)]
    let mut r = || {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f32::from(((*seed >> 16) & 0xFFFF) as u16) / 65_535.0
    };
    let x = r() * 20.0 - 10.0;
    let y = r() * 6.0;
    let z = r() * 20.0 - 10.0;
    LightRenderData {
        position: Vec3::new(x, y, z),
        direction: Vec3::new(0.0, -1.0, 0.0),
        color: Color::WHITE,
        intensity: 1.0,
        variant: LightVariant::Point { radius: 4.0 },
        cast_shadows: false,
    }
}

fn main() {
    println!("=== Tiled Light Culling Demo ===");

    let screen_w = 1920_u32;
    let screen_h = 1080_u32;
    let config = LightCullingConfig::default();
    let culler = TiledLightCuller::new(config, screen_w, screen_h);

    println!(
        "screen: {screen_w}×{screen_h}, tile: {}×{} → {} tiles ({}×{} = {} px²)",
        culler.tile_count_x,
        culler.tile_count_y,
        culler.tile_count_x * culler.tile_count_y,
        config.tile_size,
        config.tile_size,
        config.tile_size * config.tile_size,
    );

    // Build 128 random point lights + one directional sun.
    let mut seed = 0x4d2_u32;
    let mut lights = Vec::with_capacity(129);
    for _ in 0..128 {
        lights.push(random_light(&mut seed));
    }
    lights.push(LightRenderData {
        position: Vec3::ZERO,
        direction: Vec3::new(0.3, -1.0, 0.2),
        color: Color::WHITE,
        intensity: 1.0,
        variant: LightVariant::Directional,
        cast_shadows: true,
    });

    let view = Mat4::look_at(Vec3::new(0.0, 8.0, 18.0), Vec3::ZERO, Vec3::Y);
    let proj = Mat4::perspective(std::f32::consts::FRAC_PI_3, 16.0 / 9.0, 0.1, 100.0);
    let start = std::time::Instant::now();
    let result = culler.cull(&lights, view, proj);
    let elapsed = start.elapsed();

    let total_tiles = result.tiles.len();
    let covered: usize = result.tiles.iter().filter(|t| !t.is_empty()).count();
    let pairs = result.total_light_tile_pairs();
    #[allow(clippy::cast_precision_loss)]
    let avg = if covered == 0 {
        0.0
    } else {
        pairs as f32 / covered as f32
    };
    let max_per_tile = result.tiles.iter().map(Vec::len).max().unwrap_or(0);

    println!(
        "lights: 128 point + 1 directional ({} indices in directional list)",
        result.directional.len(),
    );
    println!("covered tiles: {covered} / {total_tiles}");
    println!("light×tile pairs: {pairs}");
    println!("avg lights per covered tile: {avg:.2}");
    println!("max lights in any single tile: {max_per_tile}");
    println!("cull pass: {elapsed:?}");
}
