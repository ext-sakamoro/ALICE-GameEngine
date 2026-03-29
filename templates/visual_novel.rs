//! Visual Novel template (Shiden / Ren'Py style).
//!
//! Branching dialogue, character emotions, LLM-powered responses.
//! Reference: https://github.com/HANON-games/Shiden

use alice_game_engine::app::{AppCallbacks, HeadlessRunner};
use alice_game_engine::engine::{EngineConfig, EngineContext};
use alice_game_engine::llm::*;
use alice_game_engine::scripting::*;
use alice_game_engine::verse::*;

#[derive(Clone, PartialEq)]
struct StoryState {
    scene: String,
    affection: i32,
    flags: Vec<String>,
}

struct VisualNovel {
    state: StoryState,
    history: Vec<StoryState>,
    characters: Vec<NpcContext>,
    events: EventBus,
    dialogue_log: Vec<String>,
}

impl VisualNovel {
    fn new() -> Self {
        Self {
            state: StoryState {
                scene: "school_gate".to_string(),
                affection: 0,
                flags: Vec::new(),
            },
            history: Vec::new(),
            characters: vec![
                NpcContext::new("Sakura", "a cheerful classmate who loves painting"),
                NpcContext::new("Ren", "a quiet library assistant with a mysterious past"),
            ],
            events: EventBus::new(),
            dialogue_log: Vec::new(),
        }
    }

    fn choose(&mut self, choice: &str) {
        self.history.push(self.state.clone());

        match (self.state.scene.as_str(), choice) {
            ("school_gate", "greet_sakura") => {
                self.state.affection += 1;
                self.state.scene = "art_room".to_string();
                self.dialogue_log.push("You greeted Sakura warmly.".to_string());
                self.events.publish(Event::with_string("scene_change", "art_room"));
            }
            ("school_gate", "go_library") => {
                self.state.scene = "library".to_string();
                self.state.flags.push("met_ren".to_string());
                self.dialogue_log.push("You headed to the library.".to_string());
                self.events.publish(Event::with_string("scene_change", "library"));
            }
            ("art_room", "compliment") => {
                self.state.affection += 2;
                self.dialogue_log.push("Sakura smiled brightly.".to_string());
            }
            ("library", "ask_about_book") => {
                self.state.flags.push("book_quest".to_string());
                self.dialogue_log.push("Ren mentioned a rare book in the archives.".to_string());
            }
            _ => {
                self.dialogue_log.push(format!("(No response for '{choice}')"));
            }
        }
    }

    fn undo(&mut self) {
        if let Some(prev) = self.history.pop() {
            self.state = prev;
            self.dialogue_log.push("[Undo]".to_string());
        }
    }

    fn talk_to(&mut self, char_idx: usize, input: &str) {
        let llm = MockLlm::new("That sounds interesting! Let me think about it...");
        if let Some(npc) = self.characters.get_mut(char_idx) {
            if let Ok(reply) = npc.respond(input, &llm) {
                self.dialogue_log.push(format!("{}: {reply}", npc.name));
            }
        }
    }
}

impl AppCallbacks for VisualNovel {
    fn init(&mut self, _ctx: &mut EngineContext) {
        println!("=== Visual Novel: School Days ===");
        println!("Scene: {}", self.state.scene);
    }
    fn update(&mut self, _ctx: &mut EngineContext, _dt: f32) {}
}

fn main() {
    let mut runner = HeadlessRunner::new(EngineConfig::default());
    let mut vn = VisualNovel::new();
    runner.init(&mut vn);

    vn.choose("greet_sakura");
    vn.choose("compliment");
    vn.talk_to(0, "Your painting is beautiful!");
    vn.undo();
    vn.choose("go_library");
    vn.choose("ask_about_book");
    vn.talk_to(1, "What book are you reading?");

    println!("\n--- Dialogue Log ---");
    for line in &vn.dialogue_log {
        println!("  {line}");
    }
    println!("\nScene: {} | Affection: {} | Flags: {:?}",
        vn.state.scene, vn.state.affection, vn.state.flags);
}
