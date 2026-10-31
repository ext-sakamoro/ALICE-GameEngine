//! Humanoid bones and expression presets against the names of the VRM 1.0 specification.
//!
//! oracle: the two lists below are the VRMC_vrm 1.0 humanoid bone names (55, fingers included)
//! and expression preset names (18), copied from the specification, not from this crate.

use alice_game_engine::humanoid::{ExpressionChannel, ExpressionSet, Humanoid, HumanoidBone};
use alice_game_engine::vrm::read_vrm;
use alice_game_engine::vrm_pose::VrmPose;

const VRM_BONES: [&str; 55] = [
    "hips",
    "spine",
    "chest",
    "upperChest",
    "neck",
    "head",
    "leftEye",
    "rightEye",
    "jaw",
    "leftUpperLeg",
    "leftLowerLeg",
    "leftFoot",
    "leftToes",
    "rightUpperLeg",
    "rightLowerLeg",
    "rightFoot",
    "rightToes",
    "leftShoulder",
    "leftUpperArm",
    "leftLowerArm",
    "leftHand",
    "rightShoulder",
    "rightUpperArm",
    "rightLowerArm",
    "rightHand",
    "leftThumbMetacarpal",
    "leftThumbProximal",
    "leftThumbDistal",
    "leftIndexProximal",
    "leftIndexIntermediate",
    "leftIndexDistal",
    "leftMiddleProximal",
    "leftMiddleIntermediate",
    "leftMiddleDistal",
    "leftRingProximal",
    "leftRingIntermediate",
    "leftRingDistal",
    "leftLittleProximal",
    "leftLittleIntermediate",
    "leftLittleDistal",
    "rightThumbMetacarpal",
    "rightThumbProximal",
    "rightThumbDistal",
    "rightIndexProximal",
    "rightIndexIntermediate",
    "rightIndexDistal",
    "rightMiddleProximal",
    "rightMiddleIntermediate",
    "rightMiddleDistal",
    "rightRingProximal",
    "rightRingIntermediate",
    "rightRingDistal",
    "rightLittleProximal",
    "rightLittleIntermediate",
    "rightLittleDistal",
];

const VRM_PRESETS: [&str; 18] = [
    "happy",
    "angry",
    "sad",
    "relaxed",
    "surprised",
    "aa",
    "ih",
    "ou",
    "ee",
    "oh",
    "blink",
    "blinkLeft",
    "blinkRight",
    "lookUp",
    "lookDown",
    "lookLeft",
    "lookRight",
    "neutral",
];

#[test]
fn every_vrm_bone_name_maps_to_a_bone_and_back() {
    for name in VRM_BONES {
        let bone = HumanoidBone::from_vrm_name(name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(bone.vrm_name(), name);
    }
    assert_eq!(HumanoidBone::all().len(), VRM_BONES.len());
    for bone in HumanoidBone::all() {
        assert!(VRM_BONES.contains(&bone.vrm_name()), "{bone:?}");
    }
    assert_eq!(HumanoidBone::from_vrm_name("leftWing"), None);
}

#[test]
fn every_preset_name_maps_to_a_channel_and_back_and_others_are_custom() {
    for name in VRM_PRESETS {
        let c = ExpressionChannel::from_vrm_name(name);
        assert!(
            !matches!(c, ExpressionChannel::Custom(_)),
            "{name} is a preset"
        );
        assert_eq!(c.vrm_name(), name);
    }
    let all = ExpressionChannel::presets();
    let presets: Vec<&str> = all.iter().map(ExpressionChannel::vrm_name).collect();
    assert_eq!(presets.len(), VRM_PRESETS.len());
    assert_eq!(
        ExpressionChannel::from_vrm_name("smirk"),
        ExpressionChannel::Custom("smirk".into())
    );
    assert_eq!(
        ExpressionChannel::Custom("smirk".into()).vrm_name(),
        "smirk"
    );
}

#[test]
fn binding_a_vrm_document_reports_the_names_it_could_not_map() {
    let model = read_vrm(include_bytes!("data/vrm/tiny.vrm")).unwrap();
    let mut doc = model.document.clone();
    doc.bones.push(("leftWing".into(), 1));
    let mut h = alice_game_engine::humanoid::Humanoid::new();
    let unknown = h.bind_from_vrm_document(&doc);
    assert_eq!(unknown, vec!["leftWing".to_owned()]);
    assert_eq!(h.get(HumanoidBone::head), Some(2));
    assert_eq!(h.get(HumanoidBone::hips), Some(1));
}

#[test]
fn an_expression_set_drives_a_pose_and_names_what_the_model_lacks() {
    let model = read_vrm(include_bytes!("data/vrm/tiny.vrm")).unwrap();
    let mut pose = VrmPose::new(&model).unwrap();
    let mut set = ExpressionSet::new();
    set.set(ExpressionChannel::Aa, 1.0);
    set.set(ExpressionChannel::Custom("smirk".into()), 0.5);
    set.set(ExpressionChannel::LookDown, 1.0);
    let missing = pose.apply_expression_set(&set);
    // the fixture has aa, happy, blink, lookLeft, lookUp and the custom smirk; lookDown is not in it
    assert_eq!(missing, vec!["lookDown".to_owned()]);
    // aa at 1.0 binds target 0 at 1.0; smirk at 0.5 binds target 1 at 0.5 x 0.25 = 0.125
    assert_eq!(pose.morph_weights(0), vec![1.0, 0.125]);
}

#[test]
fn an_expression_set_iterates_in_a_fixed_order() {
    let mut a = ExpressionSet::new();
    let mut b = ExpressionSet::new();
    for c in [
        ExpressionChannel::Sad,
        ExpressionChannel::Aa,
        ExpressionChannel::Custom("x".into()),
    ] {
        a.set(c, 0.5);
    }
    for c in [
        ExpressionChannel::Custom("x".into()),
        ExpressionChannel::Aa,
        ExpressionChannel::Sad,
    ] {
        b.set(c, 0.5);
    }
    let ka: Vec<_> = a.iter().map(|(c, _)| c.clone()).collect();
    let kb: Vec<_> = b.iter().map(|(c, _)| c.clone()).collect();
    assert_eq!(ka, kb);
}

/// The bone map is ordered by the bone: two maps with the same bindings, bound in opposite
/// orders, print the same, and the print lists the bones in the enum's (specification) order.
/// A hashed map would differ between two instances (each draws its own hash keys)
#[test]
fn the_bone_map_is_ordered_by_the_bone() {
    let all = HumanoidBone::all();
    let mut forward = Humanoid::new();
    let mut reverse = Humanoid::new();
    for (i, b) in all.iter().enumerate() {
        forward.bind(*b, u32::try_from(i).unwrap());
    }
    for (i, b) in all.iter().enumerate().rev() {
        reverse.bind(*b, u32::try_from(i).unwrap());
    }
    let (f, r) = (format!("{forward:?}"), format!("{reverse:?}"));
    assert_eq!(f, r);
    let at = |b: HumanoidBone| {
        f.find(&format!("{b:?}:"))
            .unwrap_or_else(|| panic!("{b:?} missing in {f}"))
    };
    let positions: Vec<usize> = all.iter().map(|b| at(*b)).collect();
    assert_eq!(all.len(), 55, "compared all 55 bones");
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "not in the bone order: {f}"
    );
}
