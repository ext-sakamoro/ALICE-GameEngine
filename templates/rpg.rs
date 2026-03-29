//! RPG template with abilities, NPC dialogue, and tile map.
//!
//! Copy this file to start a role-playing game.

use alice_game_engine::ability::*;
use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::llm::*;
use alice_game_engine::math::Vec2;
use alice_game_engine::scene2d::*;
use alice_game_engine::verse::*;

struct RpgGame {
    player_pos: Vec2,
    attrs: AttributeSet,
    ability_sys: AbilitySystem,
    npcs: Vec<NpcContext>,
    tilemap: TileMap,
    turn: u32,
}

impl RpgGame {
    fn new() -> Self {
        let mut attrs = AttributeSet::new();
        attrs.add(Attribute::new("hp", 100.0, 0.0, 100.0));
        attrs.add(Attribute::new("mp", 50.0, 0.0, 50.0));
        attrs.add(Attribute::new("gold", 100.0, 0.0, 9999.0));

        let mut ability_sys = AbilitySystem::new();
        ability_sys.add_ability(Ability::new(
            "heal",
            3,
            "mp",
            15.0,
            GameplayEffect::instant("heal", vec![AttributeModifier::flat("hp", 30.0)]),
        ));
        ability_sys.add_ability(Ability::new(
            "fireball",
            5,
            "mp",
            25.0,
            GameplayEffect::instant("damage", vec![AttributeModifier::flat("hp", -40.0)]),
        ));

        let mut tilemap = TileMap::new(16, 16, 32.0);
        for x in 0..16 {
            tilemap.set(x, 0, TileDef { id: 1, solid: true });
            tilemap.set(x, 15, TileDef { id: 1, solid: true });
        }
        for y in 0..16 {
            tilemap.set(0, y, TileDef { id: 1, solid: true });
            tilemap.set(15, y, TileDef { id: 1, solid: true });
        }

        Self {
            player_pos: Vec2::new(256.0, 256.0),
            attrs,
            ability_sys,
            npcs: vec![
                NpcContext::new("Elder", "a wise village elder who knows ancient secrets"),
                NpcContext::new("Merchant", "a traveling merchant with rare potions"),
            ],
            tilemap,
            turn: 0,
        }
    }
}

impl AppCallbacks for RpgGame {
    fn init(&mut self, _ctx: &mut EngineContext) {
        println!("RPG World initialized.");
        println!("  HP: {} | MP: {} | Gold: {}",
            self.attrs.value("hp"), self.attrs.value("mp"), self.attrs.value("gold"));
    }

    fn update(&mut self, _ctx: &mut EngineContext, _dt: f32) {
        self.turn += 1;
        self.ability_sys.tick(&mut self.attrs);

        // Every 60 frames: try an action
        if self.turn % 60 == 0 {
            // Purchase with transaction (rollback if not enough gold)
            let mut gold = self.attrs.value("gold") as i32;
            let bought = Transaction::execute(&mut gold, |g| {
                *g -= 30;
                decide(*g >= 0)
            });
            if bought.is_ok() {
                self.attrs.modify("gold", -30.0);
                println!("Turn {}: Bought potion. Gold: {}", self.turn, self.attrs.value("gold"));
            }
        }

        if self.turn % 120 == 0 {
            self.ability_sys.activate("heal", &mut self.attrs);
            println!("Turn {}: Cast heal. HP: {}", self.turn, self.attrs.value("hp"));
        }
    }
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = RpgGame::new();
    runner.init(&mut game);
    runner.run_frames(600, 60.0, &mut game);

    // NPC dialogue demo
    let llm = MockLlm::new("The ancient artifact lies in the northern caves.");
    let reply = game.npcs[0].respond("Where is the treasure?", &llm).unwrap();
    println!("\nElder: {reply}");
    println!("Final — HP: {} | MP: {} | Gold: {}",
        game.attrs.value("hp"), game.attrs.value("mp"), game.attrs.value("gold"));
}
