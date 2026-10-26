//! Poses a VRM model (an expression at full weight), draws its first primitive with the toon
//! shader into an offscreen 256 x 256 image and writes it as a binary PPM.
//!
//! ```sh
//! cargo run --example vrm_toon_offscreen --features gpu -- out.ppm [model.vrm] [expression]
//! ```

use alice_game_engine::vrm::read_vrm;
use alice_game_engine::vrm_pose::VrmPose;
use alice_game_engine::vrm_render::{ToonParams, ToonRenderer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "vrm_toon.ppm".to_owned());
    let path = args.next().unwrap_or_else(|| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/vrm/tiny.vrm").to_owned()
    });
    let expression = args.next();
    let model = read_vrm(&std::fs::read(&path)?)?;
    let mut pose = VrmPose::new(&model)?;
    if let Some(e) = &expression {
        pose.set_expression(e, 1.0)?;
    }
    let prim = &model.meshes[0][0];
    let positions = pose.posed_positions(0, 0)?;
    let normals = prim.normals.clone().ok_or("the primitive has no normals")?;
    let indices = prim.indices.clone().ok_or("the primitive has no indices")?;
    let (w, h) = (256u32, 256u32);
    let image = ToonRenderer::new_headless(w, h)?.draw(
        &positions,
        &normals,
        &indices,
        &ToonParams {
            view_proj: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
            ],
            base_color: [0.95, 0.8, 0.75, 1.0],
            shade_color: [0.6, 0.45, 0.5, 1.0],
            light_dir: [0.3, 0.5, 1.0],
            threshold: 0.2,
            clear: [0.05, 0.05, 0.08, 1.0],
        },
    )?;
    let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
    ppm.extend(image.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]));
    std::fs::write(&out, ppm)?;
    println!(
        "wrote {out} ({w} x {h}, expression {})",
        expression.as_deref().unwrap_or("none")
    );
    Ok(())
}
