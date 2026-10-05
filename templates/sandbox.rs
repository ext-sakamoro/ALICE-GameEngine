//! Physics Sandbox template with SDF objects.
//!
//! Copy this file to start a physics playground. Bodies live in
//! `ctx.physics`, which the engine advances every frame.
//!
//! ```bash
//! cargo run --example sandbox --features physics
//! ```

use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::Vec3;
use alice_game_engine::physics3d::{BodyDesc, BodyHandle};
use alice_game_engine::scene_graph::*;

#[derive(Default)]
struct Sandbox {
    bodies: Vec<BodyHandle>,
    projectile_hits: u32,
}

impl AppCallbacks for Sandbox {
    fn init(&mut self, ctx: &mut EngineContext) {
        ctx.scene
            .add(Node::new("camera", NodeKind::Camera(CameraData::default())));
        ctx.scene
            .add(Node::new("sun", NodeKind::Light(LightData::default())));

        // SDF ground (render side)
        ctx.scene.add(Node::new(
            "ground",
            NodeKind::Sdf(SdfData {
                sdf_json: r#"{"Primitive":{"Plane":{"normal":[0,1,0],"offset":0}}}"#.to_string(),
                half_extents: Vec3::new(50.0, 1.0, 50.0),
                generate_collider: true,
            }),
        ));

        // Physics ground: a large static sphere whose top is at y = 0
        ctx.physics
            .add_body(BodyDesc::fixed(glam::Vec3::new(0.0, -1000.0, 0.0), 1000.0));

        // Stack of balls
        for y in 0..5 {
            let mut body =
                BodyDesc::dynamic(glam::Vec3::new(0.0, 0.5 + y as f32 * 1.0, 0.0), 1.0, 0.5);
            body.restitution = 0.3;
            self.bodies.push(ctx.physics.add_body(body));
        }

        // Projectile
        let mut bullet = BodyDesc::dynamic(glam::Vec3::new(-10.0, 3.0, 0.0), 2.0, 0.3);
        bullet.velocity = glam::Vec3::new(20.0, 5.0, 0.0);
        self.bodies.push(ctx.physics.add_body(bullet));

        println!("Sandbox: {} physics bodies", ctx.physics.body_count());
    }

    fn update(&mut self, ctx: &mut EngineContext, _dt: f32) {
        // Count the fixed steps in which the projectile touches something
        if let Some(&bullet) = self.bodies.last() {
            if ctx
                .physics
                .contacts()
                .iter()
                .any(|c| c.body_a == bullet || c.body_b == bullet)
            {
                self.projectile_hits += 1;
            }
        }
    }
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = Sandbox::default();
    runner.init(&mut game);
    runner.run_frames(600, 60.0, &mut game);

    let physics = &runner.engine.context.physics;
    println!(
        "After 10s ({} frames with a projectile contact):",
        game.projectile_hits
    );
    for (i, &body) in game.bodies.iter().enumerate() {
        let p = physics.position(body).unwrap_or_default();
        let sleeping = physics.is_sleeping(body) == Some(true);
        println!(
            "  Body {i}: ({:.1}, {:.1}, {:.1}) {}",
            p.x,
            p.y,
            p.z,
            if sleeping { "[sleeping]" } else { "" }
        );
    }
}
