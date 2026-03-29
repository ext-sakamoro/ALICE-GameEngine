//! 2D Platformer template.
//!
//! Copy this file to start a side-scrolling action game.
//!
//! ```bash
//! cp templates/platformer.rs examples/my_game.rs
//! cargo run --example my_game --features full
//! ```

use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::Vec2;
use alice_game_engine::scene2d::*;

const GRAVITY: f32 = -600.0;
const JUMP_SPEED: f32 = 400.0;
const MOVE_SPEED: f32 = 200.0;
const GROUND_Y: f32 = 50.0;

struct Platformer {
    player_pos: Vec2,
    player_vel: Vec2,
    on_ground: bool,
    score: u32,
    coins: Vec<Vec2>,
}

impl Platformer {
    fn new() -> Self {
        Self {
            player_pos: Vec2::new(100.0, GROUND_Y),
            player_vel: Vec2::ZERO,
            on_ground: true,
            score: 0,
            coins: vec![
                Vec2::new(200.0, 100.0),
                Vec2::new(350.0, 150.0),
                Vec2::new(500.0, 80.0),
            ],
        }
    }
}

impl AppCallbacks for Platformer {
    fn init(&mut self, _ctx: &mut EngineContext) {
        println!("Platformer started. Collect {} coins!", self.coins.len());
    }

    fn update(&mut self, _ctx: &mut EngineContext, dt: f32) {
        // Gravity
        self.player_vel = self.player_vel + Vec2::new(0.0, GRAVITY * dt);

        // Simple AI: move right and jump periodically
        self.player_vel = Vec2::new(MOVE_SPEED, self.player_vel.y());
        if self.on_ground && self.player_pos.x() % 200.0 < 5.0 {
            self.player_vel = Vec2::new(self.player_vel.x(), JUMP_SPEED);
            self.on_ground = false;
        }

        // Integrate
        self.player_pos = self.player_pos + self.player_vel * dt;

        // Ground collision
        if self.player_pos.y() <= GROUND_Y {
            self.player_pos = Vec2::new(self.player_pos.x(), GROUND_Y);
            self.player_vel = Vec2::new(self.player_vel.x(), 0.0);
            self.on_ground = true;
        }

        // Coin collection
        self.coins.retain(|coin| {
            let dist = (*coin - self.player_pos).length();
            if dist < 20.0 {
                self.score += 1;
                false
            } else {
                true
            }
        });
    }
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = Platformer::new();
    runner.init(&mut game);
    runner.run_frames(600, 60.0, &mut game);
    println!(
        "Score: {} | Position: ({:.0}, {:.0}) | Coins left: {}",
        game.score,
        game.player_pos.x(),
        game.player_pos.y(),
        game.coins.len()
    );
}
