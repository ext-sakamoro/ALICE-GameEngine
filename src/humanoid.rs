//! Humanoid skeleton + expression (VRM-style).
//!
//! Provides a strongly-typed mapping from a standard humanoid bone
//! taxonomy (VRM 1.0 plus a handful of common extras) to engine bone
//! indices, plus an `Expression` channel system that drives blendshape
//! weights for things like `aa`, `blink_left`, or arbitrary user-named
//! expressions.
//!
//! The module is intentionally **renderer-agnostic**: it only resolves
//! *which* bone index represents a given humanoid joint and *what*
//! weight an expression channel currently holds. Skinning and
//! blendshape application live in the renderer / animation pipeline.
//!
//! VRM compatibility note: the [`HumanoidBone`] enum follows the VRM
//! 1.0 humanoid bone list (52 entries). [`ExpressionChannel`] follows
//! the standard VRM expression presets (`aa` / `ih` / `ou` / `ee` /
//! `oh` / `blink_left` / `blink_right` / `happy` / `angry` / `sad` /
//! `surprised` / `neutral`) and accepts arbitrary user keys.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Humanoid bone taxonomy (VRM 1.0)
// ---------------------------------------------------------------------------

/// Canonical humanoid bones. Names match the VRM 1.0 humanoid
/// specification (`snake_case`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum HumanoidBone {
    // Trunk
    hips,
    spine,
    chest,
    upper_chest,
    neck,
    head,
    // Arms — left
    left_shoulder,
    left_upper_arm,
    left_lower_arm,
    left_hand,
    // Arms — right
    right_shoulder,
    right_upper_arm,
    right_lower_arm,
    right_hand,
    // Legs — left
    left_upper_leg,
    left_lower_leg,
    left_foot,
    left_toes,
    // Legs — right
    right_upper_leg,
    right_lower_leg,
    right_foot,
    right_toes,
    // Eyes / jaw
    left_eye,
    right_eye,
    jaw,
}

impl HumanoidBone {
    /// Every variant — useful for iteration and validation.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::hips,
            Self::spine,
            Self::chest,
            Self::upper_chest,
            Self::neck,
            Self::head,
            Self::left_shoulder,
            Self::left_upper_arm,
            Self::left_lower_arm,
            Self::left_hand,
            Self::right_shoulder,
            Self::right_upper_arm,
            Self::right_lower_arm,
            Self::right_hand,
            Self::left_upper_leg,
            Self::left_lower_leg,
            Self::left_foot,
            Self::left_toes,
            Self::right_upper_leg,
            Self::right_lower_leg,
            Self::right_foot,
            Self::right_toes,
            Self::left_eye,
            Self::right_eye,
            Self::jaw,
        ]
    }

    /// Required bones for a minimum-viable VRM humanoid.
    /// (Spec: hips / spine / head / both shoulders / arms / hands /
    /// both legs / feet — eyes/jaw are optional.)
    #[must_use]
    pub const fn required() -> &'static [Self] {
        &[
            Self::hips,
            Self::spine,
            Self::head,
            Self::left_upper_arm,
            Self::left_lower_arm,
            Self::left_hand,
            Self::right_upper_arm,
            Self::right_lower_arm,
            Self::right_hand,
            Self::left_upper_leg,
            Self::left_lower_leg,
            Self::left_foot,
            Self::right_upper_leg,
            Self::right_lower_leg,
            Self::right_foot,
        ]
    }
}

// ---------------------------------------------------------------------------
// Humanoid skeleton mapping
// ---------------------------------------------------------------------------

/// Maps every [`HumanoidBone`] to a renderer-side bone index (= an
/// index into `Skeleton::bones`). Unmapped bones evaluate to `None`.
#[derive(Debug, Default, Clone)]
pub struct Humanoid {
    map: HashMap<HumanoidBone, u32>,
}

impl Humanoid {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a humanoid bone to a renderer bone index.
    pub fn bind(&mut self, bone: HumanoidBone, index: u32) {
        self.map.insert(bone, index);
    }

    /// Look up the bone index, if bound.
    #[must_use]
    pub fn get(&self, bone: HumanoidBone) -> Option<u32> {
        self.map.get(&bone).copied()
    }

    /// Number of bones currently bound.
    #[must_use]
    pub fn bound_count(&self) -> usize {
        self.map.len()
    }

    /// True if every bone returned by [`HumanoidBone::required`] is
    /// bound — the engine treats this as the minimum bar for skeletal
    /// playback.
    #[must_use]
    pub fn meets_required(&self) -> bool {
        HumanoidBone::required()
            .iter()
            .all(|b| self.map.contains_key(b))
    }

    /// List the required bones that are still unbound.
    #[must_use]
    pub fn missing_required(&self) -> Vec<HumanoidBone> {
        HumanoidBone::required()
            .iter()
            .filter(|b| !self.map.contains_key(b))
            .copied()
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Expression channels (VRM-style)
// ---------------------------------------------------------------------------

/// VRM expression presets. Use [`ExpressionChannel::Custom`] for
/// project-specific channels (e.g. "smirk", "exhausted").
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ExpressionChannel {
    Aa,
    Ih,
    Ou,
    Ee,
    Oh,
    BlinkLeft,
    BlinkRight,
    Happy,
    Angry,
    Sad,
    Surprised,
    Neutral,
    Custom(String),
}

/// Container for current expression weights. Each channel is a value
/// in `[0, 1]` (clamped). Set weights with [`set`], read them with
/// [`weight`], and bulk-clear with [`reset`].
///
/// [`set`]: ExpressionSet::set
/// [`weight`]: ExpressionSet::weight
/// [`reset`]: ExpressionSet::reset
#[derive(Debug, Default, Clone)]
pub struct ExpressionSet {
    weights: HashMap<ExpressionChannel, f32>,
}

impl ExpressionSet {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a channel weight, clamped to `[0, 1]`. Setting to 0 keeps
    /// the entry; call [`remove`] to drop it entirely.
    ///
    /// [`remove`]: ExpressionSet::remove
    pub fn set(&mut self, channel: ExpressionChannel, weight: f32) {
        self.weights.insert(channel, weight.clamp(0.0, 1.0));
    }

    /// Returns the current weight, or 0 when the channel was never set.
    #[must_use]
    pub fn weight(&self, channel: &ExpressionChannel) -> f32 {
        self.weights.get(channel).copied().unwrap_or(0.0)
    }

    /// Drop a channel from the set (and from any downstream blendshape
    /// evaluation).
    pub fn remove(&mut self, channel: &ExpressionChannel) {
        self.weights.remove(channel);
    }

    /// Zero every channel without dropping entries — useful when an
    /// animation graph wants to keep the channel allocated.
    pub fn reset(&mut self) {
        for w in self.weights.values_mut() {
            *w = 0.0;
        }
    }

    /// Iterate over `(channel, weight)` pairs in insertion order. Order
    /// is not specified but stable across reads with no mutation.
    pub fn iter(&self) -> impl Iterator<Item = (&ExpressionChannel, &f32)> {
        self.weights.iter()
    }

    /// Number of channels currently stored.
    #[must_use]
    pub fn channel_count(&self) -> usize {
        self.weights.len()
    }

    /// Convenience: blend two viseme channels for lip sync, e.g. "aa"
    /// at 0.7 and "ih" at 0.3.
    pub fn set_visemes(
        &mut self,
        primary: ExpressionChannel,
        primary_w: f32,
        secondary: ExpressionChannel,
        secondary_w: f32,
    ) {
        self.set(primary, primary_w);
        self.set(secondary, secondary_w);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bones_unique() {
        let v = HumanoidBone::all();
        let set: std::collections::HashSet<_> = v.iter().collect();
        assert_eq!(v.len(), set.len());
        assert_eq!(v.len(), 25, "VRM canonical 25 bones (without fingers)");
    }

    #[test]
    fn required_subset_of_all() {
        let all: std::collections::HashSet<_> = HumanoidBone::all().iter().collect();
        for b in HumanoidBone::required() {
            assert!(all.contains(b));
        }
    }

    #[test]
    fn humanoid_bind_and_get() {
        let mut h = Humanoid::new();
        h.bind(HumanoidBone::head, 7);
        assert_eq!(h.get(HumanoidBone::head), Some(7));
        assert_eq!(h.get(HumanoidBone::jaw), None);
        assert_eq!(h.bound_count(), 1);
    }

    #[test]
    fn meets_required_only_when_all_required_bound() {
        let mut h = Humanoid::new();
        for b in HumanoidBone::required() {
            h.bind(*b, 0);
        }
        assert!(h.meets_required());
        assert!(h.missing_required().is_empty());
    }

    #[test]
    fn missing_required_lists_unbound_bones() {
        let mut h = Humanoid::new();
        h.bind(HumanoidBone::head, 0);
        let missing = h.missing_required();
        assert!(!missing.is_empty());
        assert!(!missing.contains(&HumanoidBone::head));
    }

    #[test]
    fn expression_set_default_weight_is_zero() {
        let e = ExpressionSet::new();
        assert!((e.weight(&ExpressionChannel::Happy) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn expression_set_clamps_weight_to_unit_range() {
        let mut e = ExpressionSet::new();
        e.set(ExpressionChannel::Aa, 1.5);
        e.set(ExpressionChannel::Ih, -0.4);
        assert!((e.weight(&ExpressionChannel::Aa) - 1.0).abs() < 1e-6);
        assert!((e.weight(&ExpressionChannel::Ih)).abs() < 1e-6);
    }

    #[test]
    fn expression_set_remove_drops_channel() {
        let mut e = ExpressionSet::new();
        e.set(ExpressionChannel::Happy, 0.8);
        e.remove(&ExpressionChannel::Happy);
        assert_eq!(e.channel_count(), 0);
    }

    #[test]
    fn expression_set_reset_zeros_channels_but_keeps_entries() {
        let mut e = ExpressionSet::new();
        e.set(ExpressionChannel::Aa, 0.5);
        e.set(ExpressionChannel::Ih, 0.3);
        e.reset();
        assert_eq!(e.channel_count(), 2);
        assert!((e.weight(&ExpressionChannel::Aa)).abs() < 1e-6);
        assert!((e.weight(&ExpressionChannel::Ih)).abs() < 1e-6);
    }

    #[test]
    fn custom_expression_channel_supported() {
        let mut e = ExpressionSet::new();
        let smirk = ExpressionChannel::Custom("smirk".into());
        e.set(smirk.clone(), 0.6);
        assert!((e.weight(&smirk) - 0.6).abs() < 1e-6);
    }

    #[test]
    fn set_visemes_blends_primary_and_secondary() {
        let mut e = ExpressionSet::new();
        e.set_visemes(ExpressionChannel::Aa, 0.7, ExpressionChannel::Ih, 0.3);
        assert!((e.weight(&ExpressionChannel::Aa) - 0.7).abs() < 1e-6);
        assert!((e.weight(&ExpressionChannel::Ih) - 0.3).abs() < 1e-6);
    }

    #[test]
    fn expression_channel_serde_round_trip() {
        let ch = ExpressionChannel::Custom("grin".into());
        let j = serde_json::to_string(&ch).unwrap();
        let back: ExpressionChannel = serde_json::from_str(&j).unwrap();
        assert_eq!(ch, back);
    }
}
