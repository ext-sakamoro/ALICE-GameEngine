//! Local LLM integration for NPC AI, dialogue, and procedural content.
//!
//! Provides trait-based abstraction over local inference engines
//! (llama.cpp, ONNX Runtime, ALICE-Train ternary models, etc.)
//! without hard-coding any specific backend.

use serde::{Deserialize, Serialize};
use std::fmt::Write as _;

// ---------------------------------------------------------------------------
// LLM Provider trait
// ---------------------------------------------------------------------------

/// Trait for local LLM inference backends.
pub trait LlmProvider: Send + Sync {
    /// Returns the model name/identifier.
    fn model_name(&self) -> &str;

    /// Generates a completion from the given prompt.
    /// Returns the generated text.
    ///
    /// # Errors
    ///
    /// Returns an error string if inference fails.
    fn generate(&self, request: &LlmRequest) -> Result<LlmResponse, String>;

    /// Returns the maximum context length in tokens.
    fn max_tokens(&self) -> u32;

    /// Returns true if the model is loaded and ready.
    fn is_ready(&self) -> bool;
}

/// Inference request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub prompt: String,
    pub max_new_tokens: u32,
    pub temperature: f32,
    pub top_p: f32,
    pub stop_sequences: Vec<String>,
}

impl LlmRequest {
    #[must_use]
    pub fn new(prompt: &str) -> Self {
        Self {
            prompt: prompt.to_string(),
            max_new_tokens: 256,
            temperature: 0.7,
            top_p: 0.9,
            stop_sequences: Vec::new(),
        }
    }

    #[must_use]
    pub const fn with_max_tokens(mut self, tokens: u32) -> Self {
        self.max_new_tokens = tokens;
        self
    }

    #[must_use]
    pub const fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp;
        self
    }

    #[must_use]
    pub fn with_stop(mut self, stop: &str) -> Self {
        self.stop_sequences.push(stop.to_string());
        self
    }
}

/// Inference response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub tokens_generated: u32,
    pub finish_reason: FinishReason,
}

/// Why generation stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FinishReason {
    MaxTokens,
    StopSequence,
    EndOfText,
}

// ---------------------------------------------------------------------------
// NPC AI Context
// ---------------------------------------------------------------------------

/// Manages conversation context for an NPC powered by a local LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcContext {
    pub name: String,
    pub personality: String,
    pub memory: Vec<String>,
    pub max_memory: usize,
}

impl NpcContext {
    #[must_use]
    pub fn new(name: &str, personality: &str) -> Self {
        Self {
            name: name.to_string(),
            personality: personality.to_string(),
            memory: Vec::new(),
            max_memory: 20,
        }
    }

    /// Adds a conversation entry to memory.
    pub fn remember(&mut self, entry: &str) {
        self.memory.push(entry.to_string());
        if self.memory.len() > self.max_memory {
            self.memory.remove(0);
        }
    }

    /// Builds a prompt for the LLM including personality and memory.
    #[must_use]
    pub fn build_prompt(&self, player_input: &str) -> String {
        let mut prompt = format!(
            "You are {}, {}.\n\nConversation history:\n",
            self.name, self.personality
        );
        for entry in &self.memory {
            prompt.push_str(entry);
            prompt.push('\n');
        }
        let _ = write!(prompt, "Player: {player_input}\n{}: ", self.name);
        prompt
    }

    /// Generates an NPC response using the LLM provider.
    ///
    /// # Errors
    ///
    /// Returns an error string if inference fails.
    pub fn respond(&mut self, player_input: &str, llm: &dyn LlmProvider) -> Result<String, String> {
        let prompt = self.build_prompt(player_input);
        let request = LlmRequest::new(&prompt)
            .with_max_tokens(128)
            .with_temperature(0.8)
            .with_stop("\nPlayer:");
        let response = llm.generate(&request)?;
        self.remember(&format!("Player: {player_input}"));
        self.remember(&format!("{}: {}", self.name, response.text));
        Ok(response.text)
    }

    #[must_use]
    pub const fn memory_count(&self) -> usize {
        self.memory.len()
    }
}

// ---------------------------------------------------------------------------
// Procedural content generation
// ---------------------------------------------------------------------------

/// Request for procedural content generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentGenRequest {
    pub content_type: ContentType,
    pub description: String,
    pub constraints: Vec<String>,
}

/// Types of content an LLM can generate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContentType {
    Dialogue,
    QuestDescription,
    ItemName,
    LoreText,
    SdfFormula,
    AnimationScript,
}

impl ContentGenRequest {
    #[must_use]
    pub fn new(content_type: ContentType, description: &str) -> Self {
        Self {
            content_type,
            description: description.to_string(),
            constraints: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_constraint(mut self, constraint: &str) -> Self {
        self.constraints.push(constraint.to_string());
        self
    }

    /// Builds a prompt for the LLM to generate this content.
    #[must_use]
    pub fn to_prompt(&self) -> String {
        let type_str = match self.content_type {
            ContentType::Dialogue => "dialogue line",
            ContentType::QuestDescription => "quest description",
            ContentType::ItemName => "item name",
            ContentType::LoreText => "lore text",
            ContentType::SdfFormula => "SDF formula in JSON",
            ContentType::AnimationScript => "animation keyframe script",
        };
        let mut prompt = format!("Generate a {type_str}: {}\n", self.description);
        for c in &self.constraints {
            let _ = writeln!(prompt, "Constraint: {c}");
        }
        prompt.push_str("Output:\n");
        prompt
    }
}

// ---------------------------------------------------------------------------
// Mock LLM for testing
// ---------------------------------------------------------------------------

/// A deterministic mock LLM for testing (no actual inference).
pub struct MockLlm {
    pub response_text: String,
}

impl MockLlm {
    #[must_use]
    pub fn new(response: &str) -> Self {
        Self {
            response_text: response.to_string(),
        }
    }
}

impl LlmProvider for MockLlm {
    fn model_name(&self) -> &'static str {
        "mock-llm"
    }

    fn generate(&self, _request: &LlmRequest) -> Result<LlmResponse, String> {
        Ok(LlmResponse {
            text: self.response_text.clone(),
            tokens_generated: self.response_text.split_whitespace().count() as u32,
            finish_reason: FinishReason::EndOfText,
        })
    }

    fn max_tokens(&self) -> u32 {
        4096
    }

    fn is_ready(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_request_builder() {
        let req = LlmRequest::new("Hello")
            .with_max_tokens(100)
            .with_temperature(0.5)
            .with_stop("\n");
        assert_eq!(req.max_new_tokens, 100);
        assert_eq!(req.temperature, 0.5);
        assert_eq!(req.stop_sequences.len(), 1);
    }

    #[test]
    fn mock_llm_generate() {
        let llm = MockLlm::new("I am a guard.");
        let req = LlmRequest::new("Who are you?");
        let resp = llm.generate(&req).unwrap();
        assert_eq!(resp.text, "I am a guard.");
        assert_eq!(resp.finish_reason, FinishReason::EndOfText);
    }

    #[test]
    fn mock_llm_ready() {
        let llm = MockLlm::new("test");
        assert!(llm.is_ready());
        assert_eq!(llm.model_name(), "mock-llm");
        assert_eq!(llm.max_tokens(), 4096);
    }

    #[test]
    fn npc_context_new() {
        let npc = NpcContext::new("Guard", "a stern castle guard");
        assert_eq!(npc.name, "Guard");
        assert_eq!(npc.memory_count(), 0);
    }

    #[test]
    fn npc_remember() {
        let mut npc = NpcContext::new("Guard", "stern");
        npc.remember("Player: Hello");
        npc.remember("Guard: Halt!");
        assert_eq!(npc.memory_count(), 2);
    }

    #[test]
    fn npc_memory_limit() {
        let mut npc = NpcContext::new("Guard", "stern");
        npc.max_memory = 3;
        for i in 0..5 {
            npc.remember(&format!("Entry {i}"));
        }
        assert_eq!(npc.memory_count(), 3);
    }

    #[test]
    fn npc_build_prompt() {
        let mut npc = NpcContext::new("Guard", "a stern castle guard");
        npc.remember("Player: Hello");
        let prompt = npc.build_prompt("What is this place?");
        assert!(prompt.contains("Guard"));
        assert!(prompt.contains("stern castle guard"));
        assert!(prompt.contains("Player: Hello"));
        assert!(prompt.contains("What is this place?"));
    }

    #[test]
    fn npc_respond() {
        let llm = MockLlm::new("This is the castle entrance.");
        let mut npc = NpcContext::new("Guard", "stern");
        let reply = npc.respond("Where am I?", &llm).unwrap();
        assert_eq!(reply, "This is the castle entrance.");
        assert_eq!(npc.memory_count(), 2);
    }

    #[test]
    fn content_gen_prompt() {
        let req = ContentGenRequest::new(ContentType::QuestDescription, "Find the lost sword")
            .with_constraint("Max 50 words")
            .with_constraint("Medieval setting");
        let prompt = req.to_prompt();
        assert!(prompt.contains("quest description"));
        assert!(prompt.contains("Find the lost sword"));
        assert!(prompt.contains("Max 50 words"));
    }

    #[test]
    fn content_gen_sdf() {
        let req = ContentGenRequest::new(ContentType::SdfFormula, "A twisted tower");
        let prompt = req.to_prompt();
        assert!(prompt.contains("SDF formula"));
    }

    #[test]
    fn finish_reason_eq() {
        assert_eq!(FinishReason::MaxTokens, FinishReason::MaxTokens);
        assert_ne!(FinishReason::EndOfText, FinishReason::StopSequence);
    }

    #[test]
    fn content_type_variants() {
        let types = [
            ContentType::Dialogue,
            ContentType::QuestDescription,
            ContentType::ItemName,
            ContentType::LoreText,
            ContentType::SdfFormula,
            ContentType::AnimationScript,
        ];
        assert_eq!(types.len(), 6);
    }
}
