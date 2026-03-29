//! FPS (First-Person Shooter) template.
//!
//! Copy this file to start a 3D first-person game.

use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::camera_controller::FpsCamera;
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::{Quat, Vec3};
use alice_game_engine::physics3d::*;
use alice_game_engine::scene_graph::*;

struct FpsGame {
    camera: FpsCamera,
    physics: PhysicsWorld,
    player_body: usize,
    ammo: u32,
    score: u32,
}

impl FpsGame {
    fn new() -> Self {
        let mut physics = PhysicsWorld::new();
        let player = physics.add_body(RigidBody::new(Vec3::new(0.0, 1.8, 0.0), 80.0));
        physics.bodies[player].linear_damping = 0.1;

        // Ground
        physics.add_body(RigidBody::new_static(Vec3::ZERO));

        // Target boxes
        for i in 0..5 {
            let mut target = RigidBody::new(Vec3::new(i as f32 * 5.0 - 10.0, 1.0, -20.0), 5.0);
            target.restitution = 0.5;
            physics.add_body(target);
        }

        Self {
            camera: FpsCamera::new(Vec3::new(0.0, 1.8, 0.0)),
            physics,
            player_body: player,
            ammo: 30,
            score: 0,
        }
    }
}

impl AppCallbacks for FpsGame {
    fn init(&mut self, ctx: &mut EngineContext) {
        ctx.scene.add(Node::new("camera", NodeKind::Camera(CameraData::default())));
        ctx.scene.add(Node::new("sun", NodeKind::Light(LightData {
            variant: LightVariant::Directional,
            intensity: 1.5,
            ..LightData::default()
        })));

        // Floor mesh
        let mut floor = Node::new("floor", NodeKind::Mesh(MeshData::default()));
        floor.local_transform.scale = Vec3::new(50.0, 0.1, 50.0);
        ctx.scene.add(floor);

        println!("FPS Game — {} targets, {} ammo", 5, self.ammo);
    }

    fn update(&mut self, _ctx: &mut EngineContext, dt: f32) {
        // Simulate movement
        self.camera.move_local(0.5, 0.0, 0.0, dt);
        self.camera.look(dt * 0.3, 0.0);

        // Physics
        self.physics.step(dt);

        // Sync camera to physics body
        self.physics.bodies[self.player_body].position = self.camera.position;
    }
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = FpsGame::new();
    runner.init(&mut game);
    runner.run_frames(300, 60.0, &mut game);
    println!(
        "Camera: ({:.1}, {:.1}, {:.1}) | Score: {} | Ammo: {}",
        game.camera.position.x(), game.camera.position.y(), game.camera.position.z(),
        game.score, game.ammo
    );
}
