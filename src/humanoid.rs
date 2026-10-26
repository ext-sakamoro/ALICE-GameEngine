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
//! 1.0 humanoid bone list (55 entries, fingers included), and
//! [`ExpressionChannel`] the 18 VRM 1.0 expression presets; both map to and
//! from the specification's names (`vrm_name` / `from_vrm_name`), and a
//! channel accepts arbitrary user keys.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Humanoid bone taxonomy (VRM 1.0)
// ---------------------------------------------------------------------------

/// Canonical humanoid bones. Names match the VRM 1.0 humanoid
/// specification (`snake_case`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum HumanoidBone {
    hips,
    spine,
    chest,
    upper_chest,
    neck,
    head,
    left_eye,
    right_eye,
    jaw,
    left_upper_leg,
    left_lower_leg,
    left_foot,
    left_toes,
    right_upper_leg,
    right_lower_leg,
    right_foot,
    right_toes,
    left_shoulder,
    left_upper_arm,
    left_lower_arm,
    left_hand,
    right_shoulder,
    right_upper_arm,
    right_lower_arm,
    right_hand,
    left_thumb_metacarpal,
    left_thumb_proximal,
    left_thumb_distal,
    left_index_proximal,
    left_index_intermediate,
    left_index_distal,
    left_middle_proximal,
    left_middle_intermediate,
    left_middle_distal,
    left_ring_proximal,
    left_ring_intermediate,
    left_ring_distal,
    left_little_proximal,
    left_little_intermediate,
    left_little_distal,
    right_thumb_metacarpal,
    right_thumb_proximal,
    right_thumb_distal,
    right_index_proximal,
    right_index_intermediate,
    right_index_distal,
    right_middle_proximal,
    right_middle_intermediate,
    right_middle_distal,
    right_ring_proximal,
    right_ring_intermediate,
    right_ring_distal,
    right_little_proximal,
    right_little_intermediate,
    right_little_distal,
}

/// Every [`HumanoidBone`] with its VRM 1.0 `humanBones` name, in the specification's order
const VRM_BONE_NAMES: [(HumanoidBone, &str); 55] = [
    (HumanoidBone::hips, "hips"),
    (HumanoidBone::spine, "spine"),
    (HumanoidBone::chest, "chest"),
    (HumanoidBone::upper_chest, "upperChest"),
    (HumanoidBone::neck, "neck"),
    (HumanoidBone::head, "head"),
    (HumanoidBone::left_eye, "leftEye"),
    (HumanoidBone::right_eye, "rightEye"),
    (HumanoidBone::jaw, "jaw"),
    (HumanoidBone::left_upper_leg, "leftUpperLeg"),
    (HumanoidBone::left_lower_leg, "leftLowerLeg"),
    (HumanoidBone::left_foot, "leftFoot"),
    (HumanoidBone::left_toes, "leftToes"),
    (HumanoidBone::right_upper_leg, "rightUpperLeg"),
    (HumanoidBone::right_lower_leg, "rightLowerLeg"),
    (HumanoidBone::right_foot, "rightFoot"),
    (HumanoidBone::right_toes, "rightToes"),
    (HumanoidBone::left_shoulder, "leftShoulder"),
    (HumanoidBone::left_upper_arm, "leftUpperArm"),
    (HumanoidBone::left_lower_arm, "leftLowerArm"),
    (HumanoidBone::left_hand, "leftHand"),
    (HumanoidBone::right_shoulder, "rightShoulder"),
    (HumanoidBone::right_upper_arm, "rightUpperArm"),
    (HumanoidBone::right_lower_arm, "rightLowerArm"),
    (HumanoidBone::right_hand, "rightHand"),
    (HumanoidBone::left_thumb_metacarpal, "leftThumbMetacarpal"),
    (HumanoidBone::left_thumb_proximal, "leftThumbProximal"),
    (HumanoidBone::left_thumb_distal, "leftThumbDistal"),
    (HumanoidBone::left_index_proximal, "leftIndexProximal"),
    (
        HumanoidBone::left_index_intermediate,
        "leftIndexIntermediate",
    ),
    (HumanoidBone::left_index_distal, "leftIndexDistal"),
    (HumanoidBone::left_middle_proximal, "leftMiddleProximal"),
    (
        HumanoidBone::left_middle_intermediate,
        "leftMiddleIntermediate",
    ),
    (HumanoidBone::left_middle_distal, "leftMiddleDistal"),
    (HumanoidBone::left_ring_proximal, "leftRingProximal"),
    (HumanoidBone::left_ring_intermediate, "leftRingIntermediate"),
    (HumanoidBone::left_ring_distal, "leftRingDistal"),
    (HumanoidBone::left_little_proximal, "leftLittleProximal"),
    (
        HumanoidBone::left_little_intermediate,
        "leftLittleIntermediate",
    ),
    (HumanoidBone::left_little_distal, "leftLittleDistal"),
    (HumanoidBone::right_thumb_metacarpal, "rightThumbMetacarpal"),
    (HumanoidBone::right_thumb_proximal, "rightThumbProximal"),
    (HumanoidBone::right_thumb_distal, "rightThumbDistal"),
    (HumanoidBone::right_index_proximal, "rightIndexProximal"),
    (
        HumanoidBone::right_index_intermediate,
        "rightIndexIntermediate",
    ),
    (HumanoidBone::right_index_distal, "rightIndexDistal"),
    (HumanoidBone::right_middle_proximal, "rightMiddleProximal"),
    (
        HumanoidBone::right_middle_intermediate,
        "rightMiddleIntermediate",
    ),
    (HumanoidBone::right_middle_distal, "rightMiddleDistal"),
    (HumanoidBone::right_ring_proximal, "rightRingProximal"),
    (
        HumanoidBone::right_ring_intermediate,
        "rightRingIntermediate",
    ),
    (HumanoidBone::right_ring_distal, "rightRingDistal"),
    (HumanoidBone::right_little_proximal, "rightLittleProximal"),
    (
        HumanoidBone::right_little_intermediate,
        "rightLittleIntermediate",
    ),
    (HumanoidBone::right_little_distal, "rightLittleDistal"),
];

impl HumanoidBone {
    /// Every variant, in the order of the VRM 1.0 specification
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::hips,
            Self::spine,
            Self::chest,
            Self::upper_chest,
            Self::neck,
            Self::head,
            Self::left_eye,
            Self::right_eye,
            Self::jaw,
            Self::left_upper_leg,
            Self::left_lower_leg,
            Self::left_foot,
            Self::left_toes,
            Self::right_upper_leg,
            Self::right_lower_leg,
            Self::right_foot,
            Self::right_toes,
            Self::left_shoulder,
            Self::left_upper_arm,
            Self::left_lower_arm,
            Self::left_hand,
            Self::right_shoulder,
            Self::right_upper_arm,
            Self::right_lower_arm,
            Self::right_hand,
            Self::left_thumb_metacarpal,
            Self::left_thumb_proximal,
            Self::left_thumb_distal,
            Self::left_index_proximal,
            Self::left_index_intermediate,
            Self::left_index_distal,
            Self::left_middle_proximal,
            Self::left_middle_intermediate,
            Self::left_middle_distal,
            Self::left_ring_proximal,
            Self::left_ring_intermediate,
            Self::left_ring_distal,
            Self::left_little_proximal,
            Self::left_little_intermediate,
            Self::left_little_distal,
            Self::right_thumb_metacarpal,
            Self::right_thumb_proximal,
            Self::right_thumb_distal,
            Self::right_index_proximal,
            Self::right_index_intermediate,
            Self::right_index_distal,
            Self::right_middle_proximal,
            Self::right_middle_intermediate,
            Self::right_middle_distal,
            Self::right_ring_proximal,
            Self::right_ring_intermediate,
            Self::right_ring_distal,
            Self::right_little_proximal,
            Self::right_little_intermediate,
            Self::right_little_distal,
        ]
    }

    /// The bone's VRM 1.0 `humanBones` name (camelCase, e.g. `leftUpperArm`)
    #[must_use]
    pub fn vrm_name(self) -> &'static str {
        VRM_BONE_NAMES
            .iter()
            .find(|(b, _)| *b == self)
            .map_or("", |(_, n)| n)
    }

    /// The bone of a VRM 1.0 `humanBones` name; `None` for a name the specification does not list
    #[must_use]
    pub fn from_vrm_name(name: &str) -> Option<Self> {
        VRM_BONE_NAMES
            .iter()
            .find(|(_, n)| *n == name)
            .map(|(b, _)| *b)
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

    /// Bulk-bind from a VRM 1.x extract (= the output of
    /// [`asset::parse_vrm_full`](crate::asset::parse_vrm_full)).
    /// VRM `humanBones` keys are camelCase (e.g. `"leftUpperArm"`)
    /// while [`HumanoidBone`] uses snake_case (e.g.
    /// `left_upper_arm`); this helper bridges the naming gap. Returns
    /// the number of bones it bound.
    pub fn bind_from_vrm(&mut self, extract: &crate::asset::VrmExtract) -> usize {
        let mut count = 0;
        for entry in &extract.bones {
            if let Some(bone) = HumanoidBone::from_vrm_name(&entry.bone_name) {
                self.bind(bone, entry.node_index);
                count += 1;
            }
        }
        count
    }

    /// Binds every bone of a VRM document ([`crate::vrm::read_vrm_json`]) to its node index and
    /// returns the bone names the VRM 1.0 specification does not list (empty when all bound)
    pub fn bind_from_vrm_document(&mut self, doc: &crate::vrm::VrmDocument) -> Vec<String> {
        let mut unknown = Vec::new();
        for (name, node) in &doc.bones {
            match HumanoidBone::from_vrm_name(name) {
                Some(bone) => self.bind(bone, *node),
                None => unknown.push(name.clone()),
            }
        }
        unknown
    }
}

// ---------------------------------------------------------------------------
// Expression channels (VRM-style)
// ---------------------------------------------------------------------------

/// VRM 1.0 expression presets. Use [`ExpressionChannel::Custom`] for
/// project-specific channels (e.g. "smirk", "exhausted").
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ExpressionChannel {
    Happy,
    Angry,
    Sad,
    Relaxed,
    Surprised,
    Aa,
    Ih,
    Ou,
    Ee,
    Oh,
    Blink,
    BlinkLeft,
    BlinkRight,
    LookUp,
    LookDown,
    LookLeft,
    LookRight,
    Neutral,
    Custom(String),
}

const VRM_PRESET_NAMES: [(ExpressionChannel, &str); 18] = [
    (ExpressionChannel::Happy, "happy"),
    (ExpressionChannel::Angry, "angry"),
    (ExpressionChannel::Sad, "sad"),
    (ExpressionChannel::Relaxed, "relaxed"),
    (ExpressionChannel::Surprised, "surprised"),
    (ExpressionChannel::Aa, "aa"),
    (ExpressionChannel::Ih, "ih"),
    (ExpressionChannel::Ou, "ou"),
    (ExpressionChannel::Ee, "ee"),
    (ExpressionChannel::Oh, "oh"),
    (ExpressionChannel::Blink, "blink"),
    (ExpressionChannel::BlinkLeft, "blinkLeft"),
    (ExpressionChannel::BlinkRight, "blinkRight"),
    (ExpressionChannel::LookUp, "lookUp"),
    (ExpressionChannel::LookDown, "lookDown"),
    (ExpressionChannel::LookLeft, "lookLeft"),
    (ExpressionChannel::LookRight, "lookRight"),
    (ExpressionChannel::Neutral, "neutral"),
];

impl ExpressionChannel {
    /// The 18 presets of VRM 1.0, in the specification's order
    #[must_use]
    pub fn presets() -> Vec<Self> {
        VRM_PRESET_NAMES.iter().map(|(c, _)| c.clone()).collect()
    }

    /// The VRM 1.0 expression name (a preset name, or the custom key)
    #[must_use]
    pub fn vrm_name(&self) -> &str {
        match self {
            Self::Custom(name) => name,
            preset => VRM_PRESET_NAMES
                .iter()
                .find(|(c, _)| c == preset)
                .map_or("", |(_, n)| n),
        }
    }

    /// The channel of a VRM 1.0 expression name: a preset, or [`Self::Custom`] for any other
    #[must_use]
    pub fn from_vrm_name(name: &str) -> Self {
        VRM_PRESET_NAMES
            .iter()
            .find(|(_, n)| *n == name)
            .map_or_else(|| Self::Custom(name.to_owned()), |(c, _)| c.clone())
    }
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
    weights: BTreeMap<ExpressionChannel, f32>,
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

    /// Iterate over `(channel, weight)` pairs in the order of the channels (the presets in the
    /// order of [`ExpressionChannel`], then the custom ones by name), whatever the order of `set`
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
        assert_eq!(v.len(), 55, "VRM 1.0 humanoid bones, fingers included");
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
    fn bind_from_vrm_extracts_camelcase_to_snakecase() {
        use crate::asset::{VrmBoneBinding, VrmExtract, VrmHeader};
        let extract = VrmExtract {
            header: VrmHeader {
                spec_version: "1.0".into(),
                title: "T".into(),
                author: "A".into(),
            },
            bones: vec![
                VrmBoneBinding {
                    bone_name: "head".into(),
                    node_index: 12,
                },
                VrmBoneBinding {
                    bone_name: "leftUpperArm".into(),
                    node_index: 7,
                },
                VrmBoneBinding {
                    bone_name: "unknownBone".into(),
                    node_index: 99,
                },
            ],
            expressions: vec![],
        };
        let mut h = Humanoid::new();
        let bound = h.bind_from_vrm(&extract);
        assert_eq!(bound, 2);
        assert_eq!(h.get(HumanoidBone::head), Some(12));
        assert_eq!(h.get(HumanoidBone::left_upper_arm), Some(7));
    }

    #[test]
    fn expression_channel_serde_round_trip() {
        let ch = ExpressionChannel::Custom("grin".into());
        let j = serde_json::to_string(&ch).unwrap();
        let back: ExpressionChannel = serde_json::from_str(&j).unwrap();
        assert_eq!(ch, back);
    }
}
