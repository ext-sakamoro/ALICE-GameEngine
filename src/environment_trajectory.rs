//! Environment trajectory trait — language world model / agent simulator
//! abstraction for NPC AI, procedural environments, and Sim RL training.
//!
//! Models the (state, action) → next state mapping formalised in
//! Qwen-AgentWorld (arXiv:2606.24597). An `EnvironmentTrajectory`
//! implementation predicts the next observation given the agent's full
//! interaction history and a candidate action. The engine can use this
//! abstraction to:
//!
//! - Drive NPC behaviour against a learned world model instead of a
//!   hand-written script.
//! - Simulate environment perturbations for agent training (Sim RL)
//!   without owning the underlying simulator.
//! - Plug in language world models (ALICE-LLM, llama.cpp, ONNX, remote
//!   APIs) through a common contract.
//!
//! Design follows the 7 plug-in principles used by other `bridge`-style
//! traits: `Send + Sync`, opaque `Vec<u8>` payloads with `kind` tags,
//! `Custom` enum variant for forward compatibility, Mock implementation
//! co-located with the trait, and one contract test exercising
//! `Box<dyn Trait>` dispatch.
//!
//! ```rust
//! use alice_game_engine::environment_trajectory::*;
//!
//! let schema = EnvironmentSchema {
//!     task_description_hash: 0x4c57_4d00,
//!     action_space_kinds: vec![1, 2, 3],
//!     stateful: false,
//! };
//! let model = MockEnvironmentTrajectory::echo(schema);
//! let action = Action::Custom { kind: 1, payload: vec![0xDE, 0xAD] };
//! let obs = model.predict_next(&[], &action);
//! match obs {
//!     Observation::Custom { kind, .. } => assert_eq!(kind, 0),
//! }
//! ```

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Action / Observation / Turn
// ---------------------------------------------------------------------------

/// Opaque agent action — `kind` tag selects the schema, `payload` carries
/// the encoded action body.
///
/// `Custom` is the only variant today; future revisions add new variants
/// without breaking existing implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    /// Generic action with a kind tag and a binary payload.
    Custom {
        /// Action kind identifier (e.g. FNV-1a hash of `tap`, `type`,
        /// `execute_bash`). Must match an entry in
        /// [`EnvironmentSchema::action_space_kinds`].
        kind: u32,
        /// Encoded action body (JSON, `MessagePack`, raw bytes — opaque
        /// to the engine).
        payload: Vec<u8>,
    },
}

impl Action {
    /// Returns the action's kind tag.
    #[must_use]
    pub const fn kind(&self) -> u32 {
        match self {
            Self::Custom { kind, .. } => *kind,
        }
    }

    /// Returns the action payload bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        match self {
            Self::Custom { payload, .. } => payload,
        }
    }
}

/// Opaque environment observation — same `kind`/`payload` design as
/// [`Action`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Observation {
    /// Generic observation with a kind tag and a binary payload.
    Custom {
        /// Observation kind identifier (e.g. FNV-1a hash of `terminal`,
        /// `ui_tree`, `json_response`).
        kind: u32,
        /// Encoded observation body.
        payload: Vec<u8>,
    },
}

impl Observation {
    /// Returns the observation's kind tag.
    #[must_use]
    pub const fn kind(&self) -> u32 {
        match self {
            Self::Custom { kind, .. } => *kind,
        }
    }

    /// Returns the observation payload bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        match self {
            Self::Custom { payload, .. } => payload,
        }
    }
}

/// A single (action, observation) pair from an interaction trajectory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    /// The agent's action at this turn.
    pub action: Action,
    /// The environment's observation after the action.
    pub observation: Observation,
}

// ---------------------------------------------------------------------------
// Environment schema
// ---------------------------------------------------------------------------

/// Static schema describing what the environment looks like.
///
/// Compresses the 5-component prompt schema from Qwen-AgentWorld
/// (`task_description`, `action_space`, `initial_state`, `demonstrations`,
/// `simulation_instruction`) into a fixed-shape descriptor the engine can
/// inspect without parsing prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSchema {
    /// FNV-1a hash of the canonical task description string. Identifies
    /// the environment family (e.g. terminal vs. browser vs. game NPC).
    pub task_description_hash: u64,
    /// Set of action `kind` tags this environment accepts. An
    /// [`EnvironmentTrajectory`] implementation must reject actions whose
    /// kind is not listed here.
    pub action_space_kinds: Vec<u32>,
    /// Whether the environment carries explicit internal state across
    /// turns. `true` for terminal / OS / GUI; `false` for stateless
    /// search-style environments where the history is the only state.
    pub stateful: bool,
}

impl EnvironmentSchema {
    /// Returns `true` if `kind` is a valid action kind for this schema.
    #[must_use]
    pub fn accepts(&self, kind: u32) -> bool {
        self.action_space_kinds.contains(&kind)
    }
}

// ---------------------------------------------------------------------------
// EnvironmentTrajectory trait
// ---------------------------------------------------------------------------

/// Trait for language world models and environment simulators.
///
/// Implementations predict `Observation_{t+1}` given the interaction
/// history `Turn_{≤t}` and the current `Action_t`. The trait is the
/// engine-side contract; concrete implementations live in external
/// crates (e.g. ALICE-LLM, ALICE-Cognitive, ALICE-Metaverse,
/// `llama-cpp-2`).
///
/// Implementations must be `Send + Sync` so the engine can drive
/// inference from multiple threads or job system workers.
pub trait EnvironmentTrajectory: Send + Sync {
    /// Predicts the next observation given the full interaction history
    /// and the agent's current action.
    ///
    /// `history` contains every (action, observation) turn produced so
    /// far in the trajectory; `action` is the agent's new action whose
    /// resulting observation should be predicted.
    fn predict_next(&self, history: &[Turn], action: &Action) -> Observation;

    /// Returns the static schema describing this environment.
    fn schema(&self) -> &EnvironmentSchema;

    /// Returns `true` if this implementation is loaded and ready to
    /// predict.
    fn is_ready(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Mock implementation
// ---------------------------------------------------------------------------

/// Mock environment trajectory for tests and offline development.
///
/// Three behaviours:
///
/// - `echo`: returns the last observation from history (or an empty
///   `Observation::Custom { kind: 0, .. }` if history is empty).
/// - `fixed`: returns a fixed observation regardless of history or
///   action.
/// - `xor`: returns the byte-wise XOR of the action payload and a key
///   for deterministic but action-dependent output.
pub struct MockEnvironmentTrajectory {
    schema: EnvironmentSchema,
    behaviour: MockBehaviour,
}

enum MockBehaviour {
    Echo,
    Fixed(Observation),
    Xor(Vec<u8>),
}

impl MockEnvironmentTrajectory {
    /// Mock that echoes the last observation in history.
    #[must_use]
    pub const fn echo(schema: EnvironmentSchema) -> Self {
        Self {
            schema,
            behaviour: MockBehaviour::Echo,
        }
    }

    /// Mock that always returns `observation`.
    #[must_use]
    pub const fn fixed(schema: EnvironmentSchema, observation: Observation) -> Self {
        Self {
            schema,
            behaviour: MockBehaviour::Fixed(observation),
        }
    }

    /// Mock that returns `payload XOR key` as the observation payload.
    #[must_use]
    pub const fn xor(schema: EnvironmentSchema, key: Vec<u8>) -> Self {
        Self {
            schema,
            behaviour: MockBehaviour::Xor(key),
        }
    }
}

impl EnvironmentTrajectory for MockEnvironmentTrajectory {
    fn predict_next(&self, history: &[Turn], action: &Action) -> Observation {
        match &self.behaviour {
            MockBehaviour::Echo => history.last().map_or(
                Observation::Custom {
                    kind: 0,
                    payload: Vec::new(),
                },
                |turn| turn.observation.clone(),
            ),
            MockBehaviour::Fixed(obs) => obs.clone(),
            MockBehaviour::Xor(key) => {
                let action_payload = action.payload();
                let len = action_payload.len();
                let mut payload = Vec::with_capacity(len);
                for i in 0..len {
                    payload.push(action_payload[i] ^ key[i % key.len().max(1)]);
                }
                Observation::Custom {
                    kind: action.kind(),
                    payload,
                }
            }
        }
    }

    fn schema(&self) -> &EnvironmentSchema {
        &self.schema
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_schema() -> EnvironmentSchema {
        EnvironmentSchema {
            task_description_hash: 0x1234_5678_9ABC_DEF0,
            action_space_kinds: vec![1, 2, 3, 7],
            stateful: false,
        }
    }

    #[test]
    fn schema_accepts_only_declared_kinds() {
        let schema = sample_schema();
        assert!(schema.accepts(1));
        assert!(schema.accepts(7));
        assert!(!schema.accepts(0));
        assert!(!schema.accepts(8));
    }

    #[test]
    fn action_observation_payload_round_trip() {
        let action = Action::Custom {
            kind: 7,
            payload: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };
        assert_eq!(action.kind(), 7);
        assert_eq!(action.payload(), &[0xDE, 0xAD, 0xBE, 0xEF]);

        let obs = Observation::Custom {
            kind: 42,
            payload: vec![0x01, 0x02],
        };
        assert_eq!(obs.kind(), 42);
        assert_eq!(obs.payload(), &[0x01, 0x02]);
    }

    #[test]
    fn mock_echo_returns_empty_for_empty_history() {
        let model = MockEnvironmentTrajectory::echo(sample_schema());
        let action = Action::Custom {
            kind: 1,
            payload: vec![],
        };
        let obs = model.predict_next(&[], &action);
        assert_eq!(obs.kind(), 0);
        assert!(obs.payload().is_empty());
    }

    #[test]
    fn mock_echo_returns_last_observation() {
        let model = MockEnvironmentTrajectory::echo(sample_schema());
        let history = vec![
            Turn {
                action: Action::Custom {
                    kind: 1,
                    payload: vec![1],
                },
                observation: Observation::Custom {
                    kind: 10,
                    payload: vec![10],
                },
            },
            Turn {
                action: Action::Custom {
                    kind: 2,
                    payload: vec![2],
                },
                observation: Observation::Custom {
                    kind: 20,
                    payload: vec![20, 21],
                },
            },
        ];
        let action = Action::Custom {
            kind: 3,
            payload: vec![],
        };
        let obs = model.predict_next(&history, &action);
        assert_eq!(obs.kind(), 20);
        assert_eq!(obs.payload(), &[20, 21]);
    }

    #[test]
    fn mock_fixed_ignores_history_and_action() {
        let fixed = Observation::Custom {
            kind: 99,
            payload: vec![0xFF],
        };
        let model = MockEnvironmentTrajectory::fixed(sample_schema(), fixed);
        let action = Action::Custom {
            kind: 1,
            payload: vec![0x42],
        };
        let obs = model.predict_next(&[], &action);
        assert_eq!(obs.kind(), 99);
        assert_eq!(obs.payload(), &[0xFF]);
    }

    #[test]
    fn mock_xor_returns_action_xor_key() {
        let model = MockEnvironmentTrajectory::xor(sample_schema(), vec![0x0F, 0xF0]);
        let action = Action::Custom {
            kind: 5,
            payload: vec![0xAA, 0x55, 0xAA, 0x55],
        };
        let obs = model.predict_next(&[], &action);
        assert_eq!(obs.kind(), 5);
        assert_eq!(obs.payload(), &[0xA5, 0xA5, 0xA5, 0xA5]);
    }

    #[test]
    fn trait_object_dispatchable_via_box_dyn() {
        let schema = sample_schema();
        let model = MockEnvironmentTrajectory::echo(schema);
        let dynamic: Box<dyn EnvironmentTrajectory> = Box::new(model);

        let action = Action::Custom {
            kind: 1,
            payload: vec![0xCA, 0xFE],
        };
        let obs = dynamic.predict_next(&[], &action);
        match obs {
            Observation::Custom { kind, payload } => {
                assert_eq!(kind, 0);
                assert!(payload.is_empty());
            }
        }
        assert_eq!(
            dynamic.schema().task_description_hash,
            0x1234_5678_9ABC_DEF0
        );
        assert!(dynamic.is_ready());
    }

    #[test]
    fn schema_is_stateful_flag_distinguishes_terminal_from_search() {
        let mut schema = sample_schema();
        assert!(!schema.stateful);
        schema.stateful = true;
        assert!(schema.stateful);
    }
}
