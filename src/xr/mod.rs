//! # `XR` — Pure-Rust extended-reality abstraction
//!
//! ALICE-GameEngine's `XR` layer is a Rust-native abstraction over
//! head-mounted displays and motion controllers. **No `OpenXR` dependency** —
//! the engine defines its own [`XrProvider`] trait so that any backend
//! (stereo window simulator, future SteamVR/Quest FFI, network bridge…) can
//! be plugged in without leaking external runtime types.
//!
//! ## Layers
//!
//! ```text
//!   game logic              ──┐
//!                              ▼
//!   XrProvider trait        ──┐
//!                              ▼
//!   ┌────────────┬──────────────┬─────────────┐
//!   │ MockProvider│ StereoWindow │ (future)    │
//!   │  (testing)  │   (dev/sim)  │  Quest /    │
//!   │             │              │  SteamVR    │
//!   └────────────┴──────────────┴─────────────┘
//! ```
//!
//! ## Quick start
//!
//! ```no_run
//! # #[cfg(feature = "window")] {
//! use alice_game_engine::xr::{run_xr_windowed, StereoWindowProvider};
//! use alice_game_engine::engine::EngineConfig;
//! use alice_game_engine::window::WindowConfig;
//! use alice_game_engine::app::AppCallbacks;
//!
//! struct MyVrGame;
//! impl AppCallbacks for MyVrGame {
//!     fn init(&mut self, _ctx: &mut alice_game_engine::engine::EngineContext) {}
//!     fn update(&mut self, _ctx: &mut alice_game_engine::engine::EngineContext, _dt: f32) {}
//! }
//!
//! run_xr_windowed(
//!     WindowConfig::default(),
//!     EngineConfig::default(),
//!     Box::new(MyVrGame),
//! ).unwrap();
//! # }
//! ```
//!
//! ## Modules
//!
//! - [`types`] — Data types ([`XrConfig`], [`XrPose`], [`XrViewState`], [`XrAction`], [`XrHaptics`], …)
//! - [`provider`] — [`XrProvider`] trait
//! - [`mock`] — [`MockProvider`] for unit tests
//! - [`stereo_window`] (feature `window`) — [`StereoWindowProvider`] + [`run_xr_windowed`]

pub mod mock;
pub mod provider;
pub mod types;

#[cfg(feature = "window")]
pub mod stereo_window;

pub use mock::MockProvider;
pub use provider::XrProvider;
pub use types::{
    XrAction, XrActionSet, XrBlendMode, XrConfig, XrError, XrFormFactor, XrHand, XrHaptics, XrPose,
    XrSessionState, XrViewConfiguration, XrViewState,
};

#[cfg(feature = "window")]
pub use stereo_window::{run_xr_windowed, StereoWindowProvider};
