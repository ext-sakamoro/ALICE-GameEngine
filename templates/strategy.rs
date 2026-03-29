//! Turn-Based Strategy template (Wesnoth / Freeciv style).
//!
//! Hex-ish grid, unit abilities, A* movement, fog of war.

use alice_game_engine::ability::*;
use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::math::Vec2;
use alice_game_engine::scene2d::*;

#[derive(Clone)]
struct Unit {
    name: String,
    pos: (u32, u32),
    attrs: AttributeSet,
    move_range: u32,
    team: u8,
}

impl Unit {
    fn new(name: &str, pos: (u32, u32), hp: f32, atk: f32, team: u8) -> Self {
        let mut attrs = AttributeSet::new();
        attrs.add(Attribute::new("hp", hp, 0.0, hp));
        attrs.add(Attribute::new("atk", atk, 0.0, 999.0));
        Self { name: name.to_string(), pos, attrs, move_range: 3, team }
    }

    fn is_alive(&self) -> bool {
        self.attrs.value("hp") > 0.0
    }
}

struct StrategyGame {
    tilemap: TileMap,
    units: Vec<Unit>,
    turn: u32,
    current_team: u8,
}

impl StrategyGame {
    fn new() -> Self {
        let mut tilemap = TileMap::new(12, 12, 48.0);
        // Mountains (impassable)
        for x in 4..8 { tilemap.set(x, 5, TileDef { id: 2, solid: true }); }

        let units = vec![
            Unit::new("Knight", (1, 1), 100.0, 25.0, 0),
            Unit::new("Archer", (2, 1), 60.0, 35.0, 0),
            Unit::new("Mage", (1, 2), 50.0, 45.0, 0),
            Unit::new("Orc", (10, 10), 120.0, 20.0, 1),
            Unit::new("Goblin", (9, 10), 40.0, 15.0, 1),
            Unit::new("Troll", (10, 9), 200.0, 30.0, 1),
        ];

        Self { tilemap, units, turn: 0, current_team: 0 }
    }

    fn attack(&mut self, attacker_idx: usize, target_idx: usize) {
        let atk = self.units[attacker_idx].attrs.value("atk");
        self.units[target_idx].attrs.modify("hp", -atk);
        let target_name = self.units[target_idx].name.clone();
        let attacker_name = &self.units[attacker_idx].name;
        println!("  {} attacks {} for {:.0} damage (HP: {:.0})",
            attacker_name, target_name, atk, self.units[target_idx].attrs.value("hp"));
    }

    fn end_turn(&mut self) {
        self.current_team = 1 - self.current_team;
        self.turn += 1;
        self.units.retain(|u| u.is_alive());
    }
}

impl AppCallbacks for StrategyGame {
    fn init(&mut self, _ctx: &mut EngineContext) {
        println!("=== Turn-Based Strategy ===");
        println!("{} units on the field", self.units.len());
    }
    fn update(&mut self, _ctx: &mut EngineContext, _dt: f32) {}
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = StrategyGame::new();
    runner.init(&mut game);

    // Simulate 3 turns of combat
    for _ in 0..3 {
        println!("\n--- Turn {} (Team {}) ---", game.turn, game.current_team);
        let team = game.current_team;
        let attackers: Vec<usize> = game.units.iter().enumerate()
            .filter(|(_, u)| u.team == team).map(|(i, _)| i).collect();
        let targets: Vec<usize> = game.units.iter().enumerate()
            .filter(|(_, u)| u.team != team && u.is_alive()).map(|(i, _)| i).collect();

        for &ai in &attackers {
            if let Some(&ti) = targets.first() {
                if ti < game.units.len() && game.units[ti].is_alive() {
                    game.attack(ai, ti);
                }
            }
        }
        game.end_turn();
    }

    println!("\nSurvivors:");
    for u in &game.units {
        println!("  {} (Team {}) HP: {:.0}", u.name, u.team, u.attrs.value("hp"));
    }
}
