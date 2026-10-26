//! Reads a VRM 1.0 file, prints its licence fields, then poses it: each expression at full
//! weight, and the second humanoid bone moved, printing the posed positions of the first
//! primitive.
//!
//! ```sh
//! cargo run --example vrm_pose_demo                     # the test fixture
//! cargo run --example vrm_pose_demo -- path/to/model.vrm
//! ```

use alice_game_engine::vrm::read_vrm;
use alice_game_engine::vrm_pose::VrmPose;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/vrm/tiny.vrm").to_owned()
    });
    let model = read_vrm(&std::fs::read(&path)?)?;
    let m = &model.document.meta;
    println!("{} by {}", m.name, m.authors.join(", "));
    println!(
        "licence {} / avatar {} / commercial {} / redistribution {} / modification {} / credit {}",
        m.license_url,
        m.avatar_permission,
        m.commercial_usage,
        m.allow_redistribution,
        m.modification,
        m.credit_notation
    );
    let mut pose = VrmPose::new(&model)?;
    println!("rest: {:?}", pose.posed_positions(0, 0)?);
    for e in &model.document.expressions {
        pose.set_expression(&e.name, 1.0)?;
        println!(
            "{} ({}): {:?}",
            e.name,
            if e.preset { "preset" } else { "custom" },
            pose.posed_positions(0, 0)?
        );
        pose.set_expression(&e.name, 0.0)?;
    }
    if let Some((bone, node)) = model.document.bones.get(1) {
        let node = *node as usize;
        let t = model.nodes[node].translation;
        pose.set_translation(node, [t[0] + 0.5, t[1], t[2]])?;
        println!("{bone} moved by +0.5 x: {:?}", pose.posed_positions(0, 0)?);
    }
    Ok(())
}
