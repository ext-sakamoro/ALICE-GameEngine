//! Space Trader template (Oolite / Elite style).
//!
//! 3D space navigation, cargo trading, NPC encounters.

use alice_game_engine::ability::*;
use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::llm::*;
use alice_game_engine::math::Vec3;
use alice_game_engine::scripting::*;

#[derive(Clone)]
struct Planet {
    name: String,
    position: Vec3,
    prices: Vec<(String, f32)>,
}

struct SpaceTrader {
    ship_pos: Vec3,
    ship_vel: Vec3,
    credits: f32,
    cargo: Vec<(String, u32)>,
    fuel: f32,
    planets: Vec<Planet>,
    current_planet: Option<usize>,
    npc: NpcContext,
    events: EventBus,
}

impl SpaceTrader {
    fn new() -> Self {
        let planets = vec![
            Planet {
                name: "Terra".to_string(),
                position: Vec3::ZERO,
                prices: vec![("Food".to_string(), 10.0), ("Ore".to_string(), 50.0), ("Tech".to_string(), 200.0)],
            },
            Planet {
                name: "Mars Station".to_string(),
                position: Vec3::new(100.0, 0.0, 50.0),
                prices: vec![("Food".to_string(), 25.0), ("Ore".to_string(), 30.0), ("Tech".to_string(), 150.0)],
            },
            Planet {
                name: "Outer Rim".to_string(),
                position: Vec3::new(-80.0, 20.0, 120.0),
                prices: vec![("Food".to_string(), 40.0), ("Ore".to_string(), 20.0), ("Tech".to_string(), 300.0)],
            },
        ];

        Self {
            ship_pos: Vec3::ZERO,
            ship_vel: Vec3::ZERO,
            credits: 1000.0,
            cargo: vec![("Food".to_string(), 10)],
            fuel: 100.0,
            planets,
            current_planet: Some(0),
            npc: NpcContext::new("Trader Zyx", "a shrewd alien merchant with tentacles"),
            events: EventBus::new(),
        }
    }

    fn buy(&mut self, item: &str, qty: u32) {
        if let Some(pi) = self.current_planet {
            if let Some((_, price)) = self.planets[pi].prices.iter().find(|(n, _)| n == item) {
                let cost = price * qty as f32;
                if self.credits >= cost {
                    self.credits -= cost;
                    if let Some(c) = self.cargo.iter_mut().find(|(n, _)| n == item) {
                        c.1 += qty;
                    } else {
                        self.cargo.push((item.to_string(), qty));
                    }
                    println!("  Bought {qty}x {item} for {cost:.0} credits");
                }
            }
        }
    }

    fn sell(&mut self, item: &str, qty: u32) {
        if let Some(pi) = self.current_planet {
            if let Some(c) = self.cargo.iter_mut().find(|(n, _)| n == item) {
                let sell_qty = qty.min(c.1);
                if let Some((_, price)) = self.planets[pi].prices.iter().find(|(n, _)| n == item) {
                    let revenue = price * sell_qty as f32;
                    self.credits += revenue;
                    c.1 -= sell_qty;
                    println!("  Sold {sell_qty}x {item} for {revenue:.0} credits");
                }
            }
        }
    }

    fn travel_to(&mut self, planet_idx: usize) {
        let target = self.planets[planet_idx].position;
        let dist = (target - self.ship_pos).length();
        let fuel_cost = dist * 0.1;
        if self.fuel >= fuel_cost {
            self.fuel -= fuel_cost;
            self.ship_pos = target;
            self.current_planet = Some(planet_idx);
            let name = &self.planets[planet_idx].name;
            println!("  Traveled to {name} (fuel: {:.0}, dist: {:.0})", self.fuel, dist);
            self.events.publish(Event::with_string("arrived", name));
        } else {
            println!("  Not enough fuel!");
        }
    }
}

impl AppCallbacks for SpaceTrader {
    fn init(&mut self, _ctx: &mut EngineContext) {
        println!("=== Space Trader ===");
        println!("Credits: {:.0} | Fuel: {:.0}", self.credits, self.fuel);
    }
    fn update(&mut self, _ctx: &mut EngineContext, _dt: f32) {}
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut game = SpaceTrader::new();
    runner.init(&mut game);

    // Trading run
    game.buy("Ore", 5);
    game.travel_to(1);
    game.sell("Ore", 5);
    game.buy("Food", 8);
    game.travel_to(2);
    game.sell("Food", 8);

    // NPC encounter
    let llm = MockLlm::new("I have rare crystals. 500 credits for 3 units.");
    let reply = game.npc.respond("What are you selling?", &llm).unwrap();
    println!("\nTrader Zyx: {reply}");

    println!("\nFinal — Credits: {:.0} | Fuel: {:.0} | Cargo: {:?}",
        game.credits, game.fuel, game.cargo);
}
