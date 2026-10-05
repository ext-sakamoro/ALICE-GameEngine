//! Racing template (SuperTuxKart style).
//!
//! Physics-based car, track waypoints, lap counter. The car is a body in
//! `ctx.physics`, which the engine advances every frame.
//!
//! ```bash
//! cargo run --example racing --features physics
//! ```

use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::camera_controller::OrbitCamera;
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::Vec3;
use alice_game_engine::physics3d::{BodyDesc, BodyHandle};

struct Car {
    body: Option<BodyHandle>,
    throttle: f32,
    speed: f32,
}

struct RacingGame {
    car: Car,
    waypoints: Vec<glam::Vec3>,
    current_wp: usize,
    lap: u32,
    total_laps: u32,
    camera: OrbitCamera,
}

impl RacingGame {
    fn new() -> Self {
        Self {
            car: Car {
                body: None,
                throttle: 0.8,
                speed: 0.0,
            },
            waypoints: vec![
                glam::Vec3::new(0.0, 0.5, 0.0),
                glam::Vec3::new(50.0, 0.5, 0.0),
                glam::Vec3::new(50.0, 0.5, 50.0),
                glam::Vec3::new(0.0, 0.5, 50.0),
            ],
            current_wp: 1,
            lap: 0,
            total_laps: 3,
            camera: OrbitCamera::new(Vec3::ZERO, 15.0),
        }
    }
}

impl AppCallbacks for RacingGame {
    fn init(&mut self, ctx: &mut EngineContext) {
        // Ground: a large static sphere whose top is at y = 0
        ctx.physics.add_body(BodyDesc::fixed(
            glam::Vec3::new(25.0, -1000.0, 25.0),
            1000.0,
        ));

        let mut car = BodyDesc::dynamic(glam::Vec3::new(0.0, 0.5, 0.0), 1200.0, 0.5);
        car.linear_damping = 0.05;
        car.restitution = 0.2;
        self.car.body = Some(ctx.physics.add_body(car));
        println!("=== Racing: {} laps ===", self.total_laps);
    }

    fn update(&mut self, ctx: &mut EngineContext, _dt: f32) {
        let Some(car) = self.car.body else { return };
        let Some(car_pos) = ctx.physics.position(car) else {
            return;
        };

        // AI steering toward the next waypoint (horizontal distance)
        let mut to_target = self.waypoints[self.current_wp] - car_pos;
        to_target.y = 0.0;
        let dist = to_target.length();
        if dist < 5.0 {
            self.current_wp = (self.current_wp + 1) % self.waypoints.len();
            if self.current_wp == 1 {
                self.lap += 1;
                if self.lap <= self.total_laps {
                    println!("  Lap {} completed!", self.lap);
                }
            }
        }

        // Engine force toward the waypoint (acts over the next fixed step)
        if dist > 0.1 {
            let force = to_target / dist * self.car.throttle * 5000.0;
            ctx.physics.apply_force(car, force);
        }

        self.car.speed = ctx.physics.velocity(car).map_or(0.0, glam::Vec3::length);

        // Camera follows the car
        self.camera.target = Vec3(car_pos);
    }
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = RacingGame::new();
    runner.init(&mut game);
    runner.run_frames(1800, 60.0, &mut game); // 30 seconds
    let pos = game
        .car
        .body
        .and_then(|b| runner.engine.context.physics.position(b))
        .unwrap_or_default();
    println!(
        "Laps: {}/{} | Speed: {:.1} | Pos: ({:.1}, {:.1}, {:.1})",
        game.lap, game.total_laps, game.car.speed, pos.x, pos.y, pos.z
    );
}
