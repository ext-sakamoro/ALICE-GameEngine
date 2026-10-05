//! Physics Sandbox: balls dropped on a ground sphere, simulated by
//! `alice_physics` through the engine adapter (contacts, damping, sleeping).
//!
//! Run: `cargo run --example physics_sandbox --features physics`

use alice_game_engine::physics3d::{BodyDesc, PhysicsWorld};
use glam::Vec3;

fn main() {
    let mut world = PhysicsWorld::default();

    // Ground: a large static sphere whose top is at y = 0
    world.add_body(BodyDesc::fixed(Vec3::new(0.0, -1000.0, 0.0), 1000.0));

    // Drop 10 balls from increasing heights
    let balls: Vec<_> = (0..10)
        .map(|i| {
            let mut desc = BodyDesc::dynamic(
                Vec3::new(i as f32 * 2.0 - 9.0, 5.0 + i as f32 * 3.0, 0.0),
                1.0,
                0.5,
            );
            desc.restitution = 0.6;
            desc.linear_damping = 0.02;
            world.add_body(desc)
        })
        .collect();

    println!("Simulating 10 s at 60 fps...");
    for frame in 0..600 {
        world.update(1.0 / 60.0);
        if frame % 60 == 0 {
            let sleeping = balls
                .iter()
                .filter(|&&b| world.is_sleeping(b) == Some(true))
                .count();
            println!(
                "t={:.1}s  contacts={}  sleeping={}/{}",
                frame as f32 / 60.0,
                world.contacts().len(),
                sleeping,
                balls.len()
            );
        }
    }

    println!("\nFinal positions:");
    for (i, &b) in balls.iter().enumerate() {
        let p = world.position(b).unwrap_or_default();
        let sleeping = world.is_sleeping(b) == Some(true);
        println!(
            "  Ball {i}: ({:.2}, {:.2}, {:.2}) {}",
            p.x,
            p.y,
            p.z,
            if sleeping { "[sleeping]" } else { "" }
        );
    }
}
