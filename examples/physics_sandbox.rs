//! Physics Sandbox: demonstrates Verlet integration, collision, damping, sleeping.
//!
//! Run: `cargo run --example physics_sandbox --features full`

use alice_game_engine::math::Vec3;
use alice_game_engine::physics3d::*;

fn main() {
    let mut world = PhysicsWorld::new();

    // Ground
    world.add_body(RigidBody::new_static(Vec3::ZERO));

    // Drop 10 balls from increasing heights
    for i in 0..10 {
        let mut body = RigidBody::new(Vec3::new(i as f32 * 2.0, 5.0 + i as f32 * 3.0, 0.0), 1.0);
        body.restitution = 0.6;
        body.linear_damping = 0.02;
        world.add_body(body);
    }

    println!("Simulating 600 frames...");

    for frame in 0..600 {
        world.step(1.0 / 60.0);

        if frame % 60 == 0 {
            let sleeping = world.bodies.iter().filter(|b| b.sleeping).count();
            let contacts = world.contacts.len();
            println!(
                "t={:.1}s  contacts={}  sleeping={}/{}",
                frame as f32 / 60.0,
                contacts,
                sleeping,
                world.body_count()
            );
        }
    }

    println!("\nFinal positions:");
    for (i, body) in world.bodies.iter().enumerate().skip(1) {
        println!(
            "  Ball {}: ({:.2}, {:.2}, {:.2}) {}",
            i,
            body.position.x(),
            body.position.y(),
            body.position.z(),
            if body.sleeping { "[sleeping]" } else { "" }
        );
    }
}
