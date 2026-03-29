//! NPC Chat: demonstrates LLM NPC dialogue with MockLlm.
//!
//! Run: `cargo run --example npc_chat`

use alice_game_engine::llm::*;

fn main() {
    // MockLlm returns predefined responses (replace with real LLM for production)
    let responses = [
        "Welcome to the village, traveler.",
        "The dragon was last seen near the mountain pass.",
        "Take this sword. You'll need it.",
        "Be careful out there.",
    ];
    let mut response_idx = 0;

    let mut guard = NpcContext::new(
        "Guard",
        "a weathered village guard who has seen many travelers",
    );
    let mut merchant = NpcContext::new("Merchant", "a cheerful merchant selling potions and gear");

    let player_lines = [
        ("Guard", "Hello, who are you?"),
        ("Guard", "Have you seen anything strange lately?"),
        ("Merchant", "What do you have for sale?"),
        ("Guard", "Any advice for the road ahead?"),
    ];

    for (npc_name, player_input) in &player_lines {
        let llm = MockLlm::new(responses[response_idx % responses.len()]);
        response_idx += 1;

        let npc = if *npc_name == "Guard" {
            &mut guard
        } else {
            &mut merchant
        };

        match npc.respond(player_input, &llm) {
            Ok(reply) => {
                println!("Player → {npc_name}: {player_input}");
                println!("{npc_name}: {reply}");
                println!();
            }
            Err(e) => println!("Error: {e}"),
        }
    }

    println!("Guard memory: {} entries", guard.memory_count());
    println!("Merchant memory: {} entries", merchant.memory_count());

    // Procedural content generation
    let quest = ContentGenRequest::new(ContentType::QuestDescription, "Find the lost artifact")
        .with_constraint("Medieval fantasy setting")
        .with_constraint("Max 3 sentences");
    println!("\nQuest prompt:\n{}", quest.to_prompt());
}
