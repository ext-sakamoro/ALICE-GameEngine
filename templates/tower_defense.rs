//! Tower Defense template (Mindustry style).
//!
//! Enemies follow a path, towers shoot them, wave system.

use alice_game_engine::ability::*;
use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::Vec2;
use alice_game_engine::scripting::*;

struct Enemy {
    pos: Vec2,
    speed: f32,
    hp: f32,
    path_idx: usize,
    alive: bool,
}

struct Tower {
    pos: Vec2,
    range: f32,
    damage: f32,
    cooldown: f32,
    cooldown_timer: f32,
}

struct TowerDefense {
    path: Vec<Vec2>,
    enemies: Vec<Enemy>,
    towers: Vec<Tower>,
    wave: u32,
    gold: f32,
    lives: u32,
    timers: TimerManager,
}

impl TowerDefense {
    fn new() -> Self {
        let path = vec![
            Vec2::new(0.0, 300.0),
            Vec2::new(200.0, 300.0),
            Vec2::new(200.0, 100.0),
            Vec2::new(400.0, 100.0),
            Vec2::new(400.0, 500.0),
            Vec2::new(700.0, 500.0),
        ];

        let towers = vec![
            Tower { pos: Vec2::new(150.0, 200.0), range: 100.0, damage: 10.0, cooldown: 0.5, cooldown_timer: 0.0 },
            Tower { pos: Vec2::new(350.0, 200.0), range: 120.0, damage: 15.0, cooldown: 0.8, cooldown_timer: 0.0 },
            Tower { pos: Vec2::new(450.0, 400.0), range: 90.0, damage: 20.0, cooldown: 1.0, cooldown_timer: 0.0 },
        ];

        let mut timers = TimerManager::new();
        timers.add(Timer::new("spawn", 1.0, TimerMode::Repeating));

        Self { path, enemies: Vec::new(), towers, wave: 1, gold: 100.0, lives: 20, timers }
    }

    fn spawn_enemy(&mut self) {
        self.enemies.push(Enemy {
            pos: self.path[0],
            speed: 50.0 + self.wave as f32 * 5.0,
            hp: 30.0 + self.wave as f32 * 10.0,
            path_idx: 0,
            alive: true,
        });
    }
}

impl AppCallbacks for TowerDefense {
    fn init(&mut self, _ctx: &mut EngineContext) {
        println!("=== Tower Defense ===");
        println!("{} towers placed, Wave {}", self.towers.len(), self.wave);
    }

    fn update(&mut self, _ctx: &mut EngineContext, dt: f32) {
        // Spawn enemies on timer
        let fired = self.timers.update(dt);
        for name in &fired {
            if name == "spawn" && self.enemies.len() < 10 {
                self.spawn_enemy();
            }
        }

        // Move enemies along path
        for enemy in &mut self.enemies {
            if !enemy.alive { continue; }
            if enemy.path_idx + 1 >= self.path.len() {
                enemy.alive = false;
                self.lives = self.lives.saturating_sub(1);
                continue;
            }
            let target = self.path[enemy.path_idx + 1];
            let dir = target - enemy.pos;
            let dist = dir.length();
            if dist < 5.0 {
                enemy.path_idx += 1;
            } else {
                enemy.pos = enemy.pos + dir * (enemy.speed * dt / dist);
            }
        }

        // Towers shoot
        for tower in &mut self.towers {
            tower.cooldown_timer -= dt;
            if tower.cooldown_timer > 0.0 { continue; }

            if let Some(target) = self.enemies.iter_mut().find(|e| {
                e.alive && (e.pos - tower.pos).length() < tower.range
            }) {
                target.hp -= tower.damage;
                if target.hp <= 0.0 {
                    target.alive = false;
                    self.gold += 10.0;
                }
                tower.cooldown_timer = tower.cooldown;
            }
        }

        self.enemies.retain(|e| e.alive);
    }
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = TowerDefense::new();
    runner.init(&mut game);
    runner.run_frames(600, 60.0, &mut game);
    println!("Wave {} complete. Lives: {} | Gold: {:.0} | Enemies remaining: {}",
        game.wave, game.lives, game.gold, game.enemies.len());
}
