//! Racing template (SuperTuxKart style).
//!
//! Physics-based car, track waypoints, lap counter.

use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::camera_controller::OrbitCamera;
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::Vec3;
use alice_game_engine::physics3d::*;

struct Car {
    body_idx: usize,
    throttle: f32,
    steering: f32,
    speed: f32,
}

struct RacingGame {
    physics: PhysicsWorld,
    car: Car,
    waypoints: Vec<Vec3>,
    current_wp: usize,
    lap: u32,
    total_laps: u32,
    camera: OrbitCamera,
}

impl RacingGame {
    fn new() -> Self {
        let mut physics = PhysicsWorld::new();
        physics.gravity = Vec3::new(0.0, -9.81, 0.0);

        // Ground
        physics.add_body(RigidBody::new_static(Vec3::ZERO));

        // Car
        let mut car_body = RigidBody::new(Vec3::new(0.0, 0.5, 0.0), 1200.0);
        car_body.linear_damping = 0.05;
        car_body.restitution = 0.2;
        let car_idx = physics.add_body(car_body);

        let waypoints = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(50.0, 0.0, 0.0),
            Vec3::new(50.0, 0.0, 50.0),
            Vec3::new(0.0, 0.0, 50.0),
        ];

        Self {
            physics,
            car: Car { body_idx: car_idx, throttle: 0.8, steering: 0.0, speed: 0.0 },
            waypoints,
            current_wp: 0,
            lap: 0,
            total_laps: 3,
            camera: OrbitCamera::new(Vec3::ZERO, 15.0),
        }
    }
}

impl AppCallbacks for RacingGame {
    fn init(&mut self, _ctx: &mut EngineContext) {
        println!("=== Racing: {} laps ===", self.total_laps);
    }

    fn update(&mut self, _ctx: &mut EngineContext, dt: f32) {
        // AI steering toward next waypoint
        let car_pos = self.physics.bodies[self.car.body_idx].position;
        let target = self.waypoints[self.current_wp];
        let to_target = target - car_pos;
        let dist = to_target.length();

        if dist < 5.0 {
            self.current_wp += 1;
            if self.current_wp >= self.waypoints.len() {
                self.current_wp = 0;
                self.lap += 1;
                if self.lap <= self.total_laps {
                    println!("  Lap {} completed!", self.lap);
                }
            }
        }

        // Apply force toward waypoint
        if dist > 0.1 {
            let dir = to_target * dist.recip();
            let force = dir * self.car.throttle * 5000.0;
            self.physics.bodies[self.car.body_idx].apply_force(force);
        }

        self.physics.step(dt);
        self.car.speed = self.physics.bodies[self.car.body_idx].velocity.length();

        // Camera follows car
        self.camera.target = car_pos;
    }
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = RacingGame::new();
    runner.init(&mut game);
    runner.run_frames(1800, 60.0, &mut game); // 30 seconds
    let pos = game.physics.bodies[game.car.body_idx].position;
    println!("Laps: {}/{} | Speed: {:.1} | Pos: ({:.1}, {:.1}, {:.1})",
        game.lap, game.total_laps, game.car.speed, pos.x(), pos.y(), pos.z());
}
