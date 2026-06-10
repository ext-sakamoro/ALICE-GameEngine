//! RPG template — turn-based battle + event-script dialogue + treasure chest.
//!
//! Copy this file into your own crate to start a role-playing game.
//!
//! The template drives three layers together:
//!   1. [`alice_game_engine::scripting::EventScript`] — no-code event flow
//!      (NPC dialogue → quest accepted → battle trigger → reward).
//!   2. [`alice_game_engine::battle::TurnBattleRunner`] — speed-ordered
//!      turn battle between an ally party and an enemy party.
//!   3. [`alice_game_engine::ability::AttributeSet`] — HP / MP / ATK / DEF /
//!      Speed for each battler, plus a `gold` purse on the hero.
//!
//! Run: `cargo run --example rpg` (after registering as an example).

use alice_game_engine::ability::{Attribute, AttributeSet};
use alice_game_engine::battle::{
    BattleAction, BattleCommand, BattleResult, Battler, Party, RandomAi, Team, TurnBattleRunner,
};
use alice_game_engine::scripting::{
    BeginBattleCommand, ChangeAttrCommand, CommandStatus, EventContext, EventScript,
    GiveItemCommand, MessageCommand, ScriptVars,
};

fn make_hero() -> Battler {
    let mut attrs = AttributeSet::new();
    attrs.add(Attribute::new("hp", 60.0, 0.0, 60.0));
    attrs.add(Attribute::new("mp", 30.0, 0.0, 30.0));
    attrs.add(Attribute::new("atk", 14.0, 0.0, 999.0));
    attrs.add(Attribute::new("def", 4.0, 0.0, 999.0));
    attrs.add(Attribute::new("speed", 12.0, 0.0, 999.0));
    attrs.add(Attribute::new("gold", 0.0, 0.0, 9_999.0));
    Battler::new("Hero", attrs, Team::Ally)
}

fn make_slime() -> Battler {
    let mut attrs = AttributeSet::new();
    attrs.add(Attribute::new("hp", 25.0, 0.0, 25.0));
    attrs.add(Attribute::new("atk", 6.0, 0.0, 999.0));
    attrs.add(Attribute::new("def", 1.0, 0.0, 999.0));
    attrs.add(Attribute::new("speed", 5.0, 0.0, 999.0));
    Battler::new("Slime", attrs, Team::Enemy)
}

/// Build the opening event: NPC tells you to slay a slime in the caves.
fn opening_quest() -> EventScript {
    let mut script = EventScript::new();
    script.push(Box::new(MessageCommand::new(
        "Elder",
        "Welcome, traveler. A slime has made the cave its home.",
    )));
    script.push(Box::new(MessageCommand::new(
        "Elder",
        "Defeat it, and the village will reward you.",
    )));
    script.push(Box::new(BeginBattleCommand::new("cave_slime")));
    script
}

/// Build the reward event: gold + potion + closing line.
fn reward_quest() -> EventScript {
    let mut script = EventScript::new();
    script.push(Box::new(MessageCommand::new(
        "Elder",
        "You have done well. Take this as our thanks.",
    )));
    script.push(Box::new(ChangeAttrCommand::new("gold", 50.0)));
    script.push(Box::new(GiveItemCommand::new("potion", 2)));
    script.push(Box::new(MessageCommand::new(
        "Elder",
        "Safe travels, hero.",
    )));
    script
}

fn run_battle(hero: &mut Battler) -> BattleResult {
    let allies = Party::new(vec![hero.clone()]);
    let enemies = Party::new(vec![make_slime()]);
    let mut runner = TurnBattleRunner::new(allies, enemies);
    let mut ai = RandomAi::new(0xA11CE);
    println!("\n=== Battle: Hero vs Slime ===");

    let mut result = BattleResult::Ongoing;
    while result == BattleResult::Ongoing {
        let cmds = vec![BattleCommand {
            actor_idx: 0,
            action: BattleAction::Attack { target_idx: 0 },
        }];
        result = runner.run_turn(cmds, &mut ai);
    }

    // Replay log + sync hero HP back to the outside world.
    for line in runner.log() {
        println!("  {line}");
    }
    *hero = runner.allies.battlers.remove(0);
    println!("=== Battle result: {result:?} ===\n");
    result
}

fn run_script(script: &mut EventScript, vars: &mut ScriptVars, attrs: Option<&mut AttributeSet>) {
    let mut log = Vec::new();
    let mut attrs_ref = attrs;
    while !script.is_done() {
        let mut ctx = EventContext {
            vars,
            attrs: attrs_ref.as_deref_mut(),
            log: &mut log,
            elapsed_ticks: 0,
        };
        if matches!(script.step(&mut ctx), CommandStatus::Failed(_)) {
            break;
        }
    }
    for line in log {
        println!("  {line}");
    }
}

fn main() {
    let mut hero = make_hero();
    let mut vars = ScriptVars::new();

    println!("=== Opening quest ===");
    let mut opening = opening_quest();
    run_script(&mut opening, &mut vars, Some(&mut hero.attrs));

    if vars.get_string("pending_battle") == Some("cave_slime") {
        vars.set_string("pending_battle", ""); // clear before transitioning
        let result = run_battle(&mut hero);
        if result == BattleResult::Win {
            println!("=== Reward quest ===");
            let mut reward = reward_quest();
            run_script(&mut reward, &mut vars, Some(&mut hero.attrs));
        }
    }

    println!("\n=== Final state ===");
    println!("  Hero HP : {}", hero.attrs.value("hp"));
    println!("  Hero MP : {}", hero.attrs.value("mp"));
    println!("  Gold    : {}", hero.attrs.value("gold"));
    println!("  Potion  : {}", vars.get_int("item:potion").unwrap_or(0));
}
