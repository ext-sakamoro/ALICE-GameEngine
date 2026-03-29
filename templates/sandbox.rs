//! Physics Sandbox template with SDF objects.
//!
//! Copy this file to start a physics playground.

use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::Vec3;
use alice_game_engine::physics3d::*;
use alice_game_engine::scene_graph::*;
use alice_game_engine::sdf_assets;

struct Sandbox {
    physics: PhysicsWorld,
}

impl Sandbox {
    fn new() -> Self {
        let mut physics = PhysicsWorld::new();

        // Ground plane
        physics.add_body(RigidBody::new_static(Vec3::ZERO));

        // Stack of boxes
        for y in 0..5 {
            let mut body = RigidBody::new(Vec3::new(0.0, 1.0 + y as f32 * 2.0, 0.0), 1.0);
            body.restitution = 0.3;
            physics.add_body(body);
        }

        // Projectile
        let mut bullet = RigidBody::new(Vec3::new(-10.0, 3.0, 0.0), 2.0);
        bullet.velocity = Vec3::new(20.0, 5.0, 0.0);
        bullet.linear_damping = 0.0;
        physics.add_body(bullet);

        Self { physics }
    }
}

impl AppCallbacks for Sandbox {
    fn init(&mut self, ctx: &mut EngineContext) {
        ctx.scene.add(Node::new("camera", NodeKind::Camera(CameraData::default())));
        ctx.scene.add(Node::new("sun", NodeKind::Light(LightData::default())));

        // SDF ground
        ctx.scene.add(Node::new("ground", NodeKind::Sdf(SdfData {
            sdf_json: r#"{"Primitive":{"Plane":{"normal":[0,1,0],"offset":0}}}"#.to_string(),
            half_extents: Vec3::new(50.0, 1.0, 50.0),
            generate_collider: true,
        })));

        println!("Sandbox: {} physics bodies", self.physics.body_count());
    }

    fn update(&mut self, _ctx: &mut EngineContext, dt: f32) {
        self.physics.step(dt);
    }
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = Sandbox::new();
    runner.init(&mut game);
    runner.run_frames(600, 60.0, &mut game);

    println!("After 10s:");
    for (i, body) in game.physics.bodies.iter().enumerate().skip(1) {
        println!("  Body {i}: ({:.1}, {:.1}, {:.1}) {}",
            body.position.x(), body.position.y(), body.position.z(),
            if body.sleeping { "[sleeping]" } else { "" });
    }
}
