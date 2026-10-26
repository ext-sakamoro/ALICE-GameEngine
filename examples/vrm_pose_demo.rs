//! Reads a VRM file (1.0 or 0.x), prints its licence fields, then poses it: each expression at full
//! weight, and the second humanoid bone moved, printing the posed positions of the first
//! primitive.
//!
//! ```sh
//! cargo run --example vrm_pose_demo                     # the test fixture
//! cargo run --example vrm_pose_demo -- path/to/model.vrm
//! ```

use alice_game_engine::vrm::{read_vrm, Licence};
use alice_game_engine::vrm_pose::VrmPose;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/vrm/tiny.vrm").to_owned()
    });
    let model = read_vrm(&std::fs::read(&path)?)?;
    match &model.document.meta {
        Licence::Vrm1(m) => {
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
        }
        Licence::Vrm0(m) => {
            let s = |v: &Option<String>| v.clone().unwrap_or_else(|| "-".into());
            println!("{} by {} (VRM 0.x)", s(&m.title), s(&m.author));
            println!(
                "licence {} / users {} / commercial {} / violent {} / sexual {}",
                s(&m.license_name),
                s(&m.allowed_user_name),
                s(&m.commercial_usage_name),
                s(&m.violent_usage_name),
                s(&m.sexual_usage_name)
            );
        }
    }
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
