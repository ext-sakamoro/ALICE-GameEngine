//! Visual Novel template — branching dialogue + flag-driven scenes, written
//! entirely as an [`EventScript`] of high-level commands. Demonstrates how
//! the no-code RPG event runtime composes into a kinetic-novel structure
//! without any extra subsystems.
//!
//! ```bash
//! cargo run --example visual_novel
//! ```

use alice_game_engine::ability::{Attribute, AttributeSet};
use alice_game_engine::scripting::{
    BeginBattleCommand, BranchCommand, ChangeAttrCommand, ChoiceCommand, CommandStatus, Comparison,
    CutsceneCommand, CutsceneLine, EventContext, EventScript, GiveItemCommand, IfVarCommand,
    LlmDialogueCommand, MessageCommand, ScriptVars, SetSwitchCommand,
};

fn make_player_attrs() -> AttributeSet {
    let mut a = AttributeSet::new();
    a.add(Attribute::new("affection_sakura", 0.0, 0.0, 100.0));
    a.add(Attribute::new("affection_aoi", 0.0, 0.0, 100.0));
    a
}

/// Prologue cutscene — sets the scene and introduces the heroine choice.
fn build_prologue() -> EventScript {
    let mut s = EventScript::new();
    s.push(Box::new(CutsceneCommand::new(vec![
        CutsceneLine::new("Narrator", "Spring. The first day at Hoshikawa Academy.", 0),
        CutsceneLine::new(
            "Narrator",
            "You stand at the school gate, blossoms drifting.",
            0,
        ),
        CutsceneLine::new("Sakura", "You're the new transfer? Welcome!", 0),
        CutsceneLine::new("Aoi", "...the library is that way.", 0),
    ])));
    s
}

/// Heroine route fork — Choice + IfVar branching.
fn build_heroine_choice() -> EventScript {
    let mut s = EventScript::new();
    s.push(Box::new(ChoiceCommand::pick(
        "Who will you walk to class with?",
        vec!["Sakura".into(), "Aoi".into()],
        "route_pick",
        0,
    )));
    s.push(Box::new(IfVarCommand::new(
        "route_pick",
        Comparison::Eq,
        0,
        Box::new(SetSwitchCommand::new("route_sakura", true)),
        Box::new(SetSwitchCommand::new("route_aoi", true)),
    )));
    // Per-route message via Branch on the bool switch we just set.
    s.push(Box::new(BranchCommand::new(
        "route_sakura",
        Box::new(MessageCommand::new(
            "Sakura",
            "Let's walk together! I'll show you the long way around.",
        )),
        Box::new(MessageCommand::new(
            "Aoi",
            "...this way is faster. Try to keep up.",
        )),
    )));
    // Affection +10 on the chosen route.
    s.push(Box::new(BranchCommand::new(
        "route_sakura",
        Box::new(ChangeAttrCommand::new("affection_sakura", 10.0)),
        Box::new(ChangeAttrCommand::new("affection_aoi", 10.0)),
    )));
    s
}

/// Mid-game beat — LLM-backed line + quest hand-off + (skippable) battle.
fn build_mid_chapter() -> EventScript {
    let mut s = EventScript::new();
    s.push(Box::new(LlmDialogueCommand::canned(
        "Sakura",
        "What do you think about the festival next week?",
        "I-I haven't really thought about it... do you have plans?",
    )));
    s.push(Box::new(ChoiceCommand::pick(
        "Invite her to the festival?",
        vec!["Yes, together".into(), "Maybe later".into()],
        "invite_pick",
        0,
    )));
    s.push(Box::new(IfVarCommand::new(
        "invite_pick",
        Comparison::Eq,
        0,
        Box::new(ChangeAttrCommand::new("affection_sakura", 20.0)),
        Box::new(MessageCommand::new("Sakura", "...okay. Some other time.")),
    )));
    // Hidden battle — only triggered if the player chose to skip via flag.
    s.push(Box::new(IfVarCommand::new(
        "invite_pick",
        Comparison::Eq,
        1,
        Box::new(MessageCommand::new(
            "Narrator",
            "The rest of the week passes quietly.",
        )),
        Box::new(BeginBattleCommand::new("festival_mishap")),
    )));
    s
}

/// Ending picker — driven by the affection attribute.
fn build_ending() -> EventScript {
    let mut s = EventScript::new();
    s.push(Box::new(MessageCommand::new(
        "Narrator",
        "Spring becomes summer. The story of this year approaches its close.",
    )));
    s.push(Box::new(GiveItemCommand::new("memory_album", 1)));
    s
}

fn run_script(script: &mut EventScript, vars: &mut ScriptVars, attrs: &mut AttributeSet) {
    let mut log = Vec::new();
    while !script.is_done() {
        let mut ctx = EventContext {
            vars,
            attrs: Some(attrs),
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
    let mut vars = ScriptVars::new();
    let mut attrs = make_player_attrs();

    println!("=== Prologue ===");
    let mut script = build_prologue();
    run_script(&mut script, &mut vars, &mut attrs);

    println!("\n=== Route choice ===");
    let mut script = build_heroine_choice();
    run_script(&mut script, &mut vars, &mut attrs);

    println!("\n=== Mid chapter ===");
    let mut script = build_mid_chapter();
    run_script(&mut script, &mut vars, &mut attrs);

    println!("\n=== Ending ===");
    let mut script = build_ending();
    run_script(&mut script, &mut vars, &mut attrs);

    println!(
        "\n=== Final affection ===\nSakura: {:.0}  /  Aoi: {:.0}",
        attrs.value("affection_sakura"),
        attrs.value("affection_aoi"),
    );
    println!(
        "Memory album: {}",
        vars.get_int("item:memory_album").unwrap_or(0)
    );
}
