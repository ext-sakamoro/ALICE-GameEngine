//! VRM 1.0 look-at: the reader's `lookAt`, and gaze applied to a pose.
//!
//! oracle (the fixture's lookAt, expression type): horizontal maps take 60 degrees to an output
//! of 1.0, vertical maps take 40 degrees to 0.5. So 30 degrees to the left is lookLeft at
//! 30 / 60 * 1.0 = 0.5; 20 degrees up is lookUp at 20 / 40 * 0.5 = 0.25; past the input maximum
//! the input is the maximum. lookLeft binds target 1 at 1.0 and lookUp target 0 at 1.0.
//! For the bone type (a hand-built variant), the output is degrees of eye rotation: with the
//! specification's default maps (input maximum 90, output scale 10), 45 degrees to the left
//! turns the eye 5 degrees about +Y, so its forward (0, 0, 1) becomes (sin 5°, 0, cos 5°).

use alice_game_engine::vrm::{read_vrm, LookAtKind, RangeMap};
use alice_game_engine::vrm_pose::{PoseError, VrmPose};

const TINY: &[u8] = include_bytes!("data/vrm/tiny.vrm");

#[test]
fn the_look_at_is_read() {
    let doc = read_vrm(TINY).unwrap().document;
    let l = doc.look_at.expect("a lookAt");
    assert_eq!(l.kind, LookAtKind::Expression);
    assert_eq!(l.offset_from_head_bone, [0.0, 0.0625, 0.125]);
    assert_eq!(
        l.horizontal_inner,
        RangeMap {
            input_max_value: 60.0,
            output_scale: 1.0
        }
    );
    assert_eq!(
        l.horizontal_outer,
        RangeMap {
            input_max_value: 60.0,
            output_scale: 1.0
        }
    );
    assert_eq!(
        l.vertical_down,
        RangeMap {
            input_max_value: 40.0,
            output_scale: 0.5
        }
    );
    assert_eq!(
        l.vertical_up,
        RangeMap {
            input_max_value: 40.0,
            output_scale: 0.5
        }
    );
}

#[test]
fn an_expression_look_at_sets_the_look_expressions() {
    let model = read_vrm(TINY).unwrap();
    let mut pose = VrmPose::new(&model).unwrap();
    pose.set_look(30.0, 20.0).unwrap();
    // lookLeft 0.5 -> target 1 at 0.5; lookUp 0.25 -> target 0 at 0.25
    assert_eq!(pose.morph_weights(0), vec![0.25, 0.5]);
    // past the input maximum the output is the scale: 90 left -> 1.0, 100 up -> 0.5
    pose.set_look(90.0, 100.0).unwrap();
    assert_eq!(pose.morph_weights(0), vec![0.5, 1.0]);
    // looking straight ahead clears them
    pose.set_look(0.0, 0.0).unwrap();
    assert_eq!(pose.morph_weights(0), vec![0.0, 0.0]);
    // the model has no lookRight or lookDown: looking right or down is an error naming it
    assert!(
        matches!(pose.set_look(-10.0, 0.0), Err(PoseError::UnknownExpression(n)) if n == "lookRight")
    );
    assert!(
        matches!(pose.set_look(0.0, -10.0), Err(PoseError::UnknownExpression(n)) if n == "lookDown")
    );
    assert!(matches!(
        pose.set_look(f32::NAN, 0.0),
        Err(PoseError::NotFinite)
    ));
}

#[test]
fn a_bone_look_at_turns_the_eye_bone() {
    let mut model = read_vrm(TINY).unwrap();
    // a left eye on node 2, a bone-type look at with the specification's default maps
    model.document.bones.extend([("leftEye".to_owned(), 2)]);
    let la = model.document.look_at.as_mut().unwrap();
    la.kind = LookAtKind::Bone;
    let def = RangeMap {
        input_max_value: 90.0,
        output_scale: 10.0,
    };
    la.horizontal_inner = def;
    la.horizontal_outer = def;
    la.vertical_down = def;
    la.vertical_up = def;
    let mut pose = VrmPose::new(&model).unwrap();
    pose.set_look(45.0, 0.0).unwrap();
    let w = pose.world_matrices()[2];
    let f = w.0.transform_vector3(glam::Vec3::Z);
    let a = 5.0f32.to_radians();
    assert!(
        (f.x - a.sin()).abs() < 1e-6 && f.y.abs() < 1e-6 && (f.z - a.cos()).abs() < 1e-6,
        "{f:?}"
    );
    // 18 degrees up (out 2 degrees) turns forward towards +Y
    pose.set_look(0.0, 18.0).unwrap();
    let f = pose.world_matrices()[2].0.transform_vector3(glam::Vec3::Z);
    let b = 2.0f32.to_radians();
    assert!(
        f.x.abs() < 1e-6 && (f.y - b.sin()).abs() < 1e-6 && (f.z - b.cos()).abs() < 1e-6,
        "{f:?}"
    );
}

#[test]
fn a_model_without_look_at_refuses_a_gaze() {
    let mut model = read_vrm(TINY).unwrap();
    model.document.look_at = None;
    let mut pose = VrmPose::new(&model).unwrap();
    assert!(matches!(pose.set_look(10.0, 0.0), Err(PoseError::NoLookAt)));
}

#[test]
fn an_eye_turning_inward_uses_the_inner_map() {
    // left eye on node 2; inner map 90 -> 20 degrees, outer map 90 -> 10 degrees.
    // Looking 45 to the right turns the left eye inward: 45 / 90 * 20 = 10 degrees to -X
    let mut model = read_vrm(TINY).unwrap();
    model.document.bones.extend([("leftEye".to_owned(), 2)]);
    let la = model.document.look_at.as_mut().unwrap();
    la.kind = LookAtKind::Bone;
    la.horizontal_inner = RangeMap {
        input_max_value: 90.0,
        output_scale: 20.0,
    };
    la.horizontal_outer = RangeMap {
        input_max_value: 90.0,
        output_scale: 10.0,
    };
    let mut pose = VrmPose::new(&model).unwrap();
    pose.set_look(-45.0, 0.0).unwrap();
    let f = pose.world_matrices()[2].0.transform_vector3(glam::Vec3::Z);
    let a = 10.0f32.to_radians();
    assert!(
        (f.x + a.sin()).abs() < 1e-6 && (f.z - a.cos()).abs() < 1e-6,
        "{f:?}"
    );
    // and outward (to the left) the outer map: 45 / 90 * 10 = 5 degrees to +X
    pose.set_look(45.0, 0.0).unwrap();
    let f = pose.world_matrices()[2].0.transform_vector3(glam::Vec3::Z);
    let b = 5.0f32.to_radians();
    assert!(
        (f.x - b.sin()).abs() < 1e-6 && (f.z - b.cos()).abs() < 1e-6,
        "{f:?}"
    );
}

#[test]
fn absent_maps_take_the_defaults_and_an_unknown_type_is_refused() {
    use alice_game_engine::vrm::{read_vrm_json, split_glb, VrmError};
    let glb = split_glb(TINY).unwrap();
    let mut j: serde_json::Value = serde_json::from_str(glb.json).unwrap();
    let la = j["extensions"]["VRMC_vrm"]["lookAt"]
        .as_object_mut()
        .unwrap();
    for k in ["rangeMapVerticalUp", "type"] {
        assert!(la.remove(k).is_some(), "{k} is in the fixture");
    }
    let doc = read_vrm_json(&j.to_string()).unwrap();
    let l = doc.look_at.unwrap();
    // the specification's defaults: type bone, maps 90 -> 10
    assert_eq!(l.kind, LookAtKind::Bone);
    assert_eq!(
        l.vertical_up,
        RangeMap {
            input_max_value: 90.0,
            output_scale: 10.0
        }
    );
    j["extensions"]["VRMC_vrm"]["lookAt"]["type"] = "eyes".into();
    assert!(
        matches!(read_vrm_json(&j.to_string()), Err(VrmError::BadValue(p)) if p == "lookAt.type")
    );
}

#[test]
fn an_expression_look_at_without_maps_takes_90_to_10_and_saturates() {
    // VRMC_vrm 1.0 gives no default for an absent range map; the reference implementation keeps
    // 90 -> 10 for both types and clamps an expression weight to 0..=1:
    // 4.5 degrees left -> 4.5 / 90 * 10 = 0.5 on lookLeft (target 1); 30 degrees -> 3.33 -> 1
    let mut model = read_vrm(TINY).unwrap();
    let la = model.document.look_at.as_mut().unwrap();
    la.horizontal_inner = RangeMap::DEFAULT;
    la.horizontal_outer = RangeMap::DEFAULT;
    la.vertical_down = RangeMap::DEFAULT;
    la.vertical_up = RangeMap::DEFAULT;
    let mut pose = VrmPose::new(&model).unwrap();
    pose.set_look(4.5, 0.0).unwrap();
    assert_eq!(pose.morph_weights(0), vec![0.0, 0.5]);
    pose.set_look(30.0, 0.0).unwrap();
    assert_eq!(pose.morph_weights(0), vec![0.0, 1.0]);
    // the clamp is on the expression weight, before the binds: with lookLeft's bind at 0.25,
    // 30 degrees gives lookLeft 1 (not 3.33) and target 1 at 0.25 (not 0.83)
    let i = model
        .document
        .expressions
        .iter()
        .position(|e| e.name == "lookLeft")
        .unwrap();
    model.document.expressions[i].binds[0].weight = 0.25;
    let mut pose = VrmPose::new(&model).unwrap();
    pose.set_look(30.0, 0.0).unwrap();
    assert_eq!(pose.morph_weights(0), vec![0.0, 0.25]);
}

#[test]
fn an_input_maximum_of_zero_gives_the_scale_for_any_non_zero_input() {
    let m = RangeMap {
        input_max_value: 0.0,
        output_scale: 0.75,
    };
    assert_eq!(m.map(0.0), 0.0);
    assert_eq!(m.map(0.01), 0.75);
    assert_eq!(m.map(-30.0), 0.75);
}
