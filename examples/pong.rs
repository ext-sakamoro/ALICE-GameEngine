//! Pong: 2D game using scene2d, input, and physics.
//!
//! Run: `cargo run --example pong --features full`

use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::Vec2;
use alice_game_engine::scene2d::*;

struct PongGame {
    paddle_y: f32,
    ball_pos: Vec2,
    ball_vel: Vec2,
    score: u32,
}

impl PongGame {
    fn new() -> Self {
        Self {
            paddle_y: 300.0,
            ball_pos: Vec2::new(400.0, 300.0),
            ball_vel: Vec2::new(200.0, 150.0),
            score: 0,
        }
    }
}

impl AppCallbacks for PongGame {
    fn init(&mut self, _ctx: &mut EngineContext) {
        println!("Pong started. Ball at (400, 300).");
    }

    fn update(&mut self, _ctx: &mut EngineContext, dt: f32) {
        // Move ball
        self.ball_pos = self.ball_pos + self.ball_vel * dt;

        // Bounce off top/bottom walls
        if self.ball_pos.y() <= 0.0 || self.ball_pos.y() >= 600.0 {
            self.ball_vel = Vec2::new(self.ball_vel.x(), -self.ball_vel.y());
        }

        // Bounce off right wall
        if self.ball_pos.x() >= 800.0 {
            self.ball_vel = Vec2::new(-self.ball_vel.x(), self.ball_vel.y());
        }

        // Paddle collision (left side)
        if self.ball_pos.x() <= 30.0
            && self.ball_pos.y() > self.paddle_y - 40.0
            && self.ball_pos.y() < self.paddle_y + 40.0
        {
            self.ball_vel = Vec2::new(-self.ball_vel.x(), self.ball_vel.y());
            self.score += 1;
        }

        // Ball out of bounds (left)
        if self.ball_pos.x() < 0.0 {
            self.ball_pos = Vec2::new(400.0, 300.0);
        }

        // Simple AI paddle
        if self.ball_pos.y() > self.paddle_y {
            self.paddle_y += 180.0 * dt;
        } else {
            self.paddle_y -= 180.0 * dt;
        }
    }
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = PongGame::new();
    runner.init(&mut game);
    runner.run_frames(600, 60.0, &mut game);
    println!(
        "Score: {} | Ball: ({:.0}, {:.0}) | Paddle Y: {:.0}",
        game.score,
        game.ball_pos.x(),
        game.ball_pos.y(),
        game.paddle_y
    );
}
