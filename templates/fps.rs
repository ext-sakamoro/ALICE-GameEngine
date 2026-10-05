//! FPS (First-Person Shooter) template.
//!
//! Copy this file to start a 3D first-person game. Physics bodies live in
//! `ctx.physics`, which the engine advances every frame.
//!
//! ```bash
//! cargo run --example fps --features physics
//! ```

use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::camera_controller::FpsCamera;
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::Vec3;
use alice_game_engine::physics3d::{BodyDesc, BodyHandle, BodyKind};
use alice_game_engine::scene_graph::*;

struct FpsGame {
    camera: FpsCamera,
    player_body: Option<BodyHandle>,
    targets: Vec<BodyHandle>,
    ammo: u32,
    score: u32,
}

impl FpsGame {
    fn new() -> Self {
        Self {
            camera: FpsCamera::new(Vec3::new(0.0, 1.8, 0.0)),
            player_body: None,
            targets: Vec::new(),
            ammo: 30,
            score: 0,
        }
    }
}

impl AppCallbacks for FpsGame {
    fn init(&mut self, ctx: &mut EngineContext) {
        ctx.scene
            .add(Node::new("camera", NodeKind::Camera(CameraData::default())));
        ctx.scene.add(Node::new(
            "sun",
            NodeKind::Light(LightData {
                variant: LightVariant::Directional,
                intensity: 1.5,
                ..LightData::default()
            }),
        ));

        // Floor mesh
        let mut floor = Node::new("floor", NodeKind::Mesh(MeshData::default()));
        floor.local_transform.scale = Vec3::new(50.0, 0.1, 50.0);
        ctx.scene.add(floor);

        // Ground: a large static sphere whose top is at y = 0
        ctx.physics
            .add_body(BodyDesc::fixed(glam::Vec3::new(0.0, -1000.0, 0.0), 1000.0));

        // Player: kinematic capsule stand-in driven by the camera
        let mut player = BodyDesc::fixed(self.camera.position.0, 0.4);
        player.kind = BodyKind::Kinematic;
        self.player_body = Some(ctx.physics.add_body(player));

        // Targets
        for i in 0..5 {
            let mut target =
                BodyDesc::dynamic(glam::Vec3::new(i as f32 * 5.0 - 10.0, 1.0, -20.0), 5.0, 0.5);
            target.restitution = 0.5;
            self.targets.push(ctx.physics.add_body(target));
        }

        println!(
            "FPS Game — {} targets, {} ammo",
            self.targets.len(),
            self.ammo
        );
    }

    fn update(&mut self, ctx: &mut EngineContext, dt: f32) {
        // Simulate movement
        self.camera.move_local(0.5, 0.0, 0.0, dt);
        self.camera.look(dt * 0.3, 0.0);

        // The player body follows the camera on the next fixed step
        if let Some(player) = self.player_body {
            ctx.physics
                .set_kinematic_target(player, self.camera.position.0, glam::Quat::IDENTITY);
        }

        // Fire once per second at the first target still standing
        if ctx.time.frame_count.is_multiple_of(60) && self.ammo > 0 {
            self.ammo -= 1;
            if let Some(&target) = self.targets.first() {
                ctx.physics
                    .apply_impulse(target, glam::Vec3::new(0.0, 5.0, -20.0));
                self.score += 1;
            }
        }
    }
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = FpsGame::new();
    runner.init(&mut game);
    runner.run_frames(300, 60.0, &mut game);
    println!(
        "Camera: ({:.1}, {:.1}, {:.1}) | Score: {} | Ammo: {}",
        game.camera.position.x(),
        game.camera.position.y(),
        game.camera.position.z(),
        game.score,
        game.ammo
    );
}
