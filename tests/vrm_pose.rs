//! Posing a VRM model on the CPU: expressions to morph target weights, morph target deltas,
//! node transforms and linear blend skinning, against values worked out by hand from the
//! fixture (`scripts/vrm_fixture.py`) and the glTF 2.0 rules.
//!
//! oracle (the fixture): vertex 0 is (0,0,0) with joints (0,1) weights (0.75, 0.25), vertex 1
//! is (1,0,0) with (0.5, 0.5), vertex 2 is (0,1,0) with joint 1 weight 1. Skin joints are nodes
//! [1 (hips), 2 (head)]; head is a child of hips at translation (0,1,0); the inverse bind
//! matrices are identity and a translation by (0,-1,0), so at rest every skinning matrix is the
//! identity. Morph target 0 moves vertex 2 by (0,-0.25,0); target 1 moves vertex 0 by
//! (-0.125, 0.0625, 0) and vertex 1 by (0.125, 0.0625, 0). Expression `aa` binds target 0 at
//! 1.0; `happy` binds target 1 at 1.0 and target 0 at 0.5; `smirk` (custom) binds target 1 at
//! 0.25. Every number here is a short binary fraction, so the expected values are exact.

use alice_game_engine::vrm::read_vrm;
use alice_game_engine::vrm_pose::{PoseError, VrmPose};

const TINY: &[u8] = include_bytes!("data/vrm/tiny.vrm");

fn posed(pose: &VrmPose<'_>) -> Vec<[f32; 3]> {
    pose.posed_positions(0, 0).expect("poses")
}

#[test]
fn at_rest_the_positions_are_the_file_positions() {
    let model = read_vrm(TINY).unwrap();
    let pose = VrmPose::new(&model).expect("a pose");
    assert_eq!(
        posed(&pose),
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
    );
}

#[test]
fn an_expression_moves_the_vertices_by_its_bound_targets() {
    let model = read_vrm(TINY).unwrap();
    let mut pose = VrmPose::new(&model).unwrap();
    pose.set_expression("aa", 1.0).unwrap();
    // target 0 at 1.0: vertex 2 = (0,1,0) + (0,-0.25,0)
    assert_eq!(
        posed(&pose),
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.75, 0.0]]
    );
    pose.set_expression("aa", 0.0).unwrap();
    pose.set_expression("happy", 1.0).unwrap();
    // target 1 at 1.0 and target 0 at 0.5:
    // v0 = (-0.125, 0.0625, 0); v1 = (1.125, 0.0625, 0); v2 = (0, 1 - 0.125, 0)
    assert_eq!(
        posed(&pose),
        vec![
            [-0.125, 0.0625, 0.0],
            [1.125, 0.0625, 0.0],
            [0.0, 0.875, 0.0]
        ]
    );
    // two expressions add on a target and the weight is clamped to 1:
    // happy (target 1 at 1.0) + smirk at 1.0 (target 1 at 0.25) -> target 1 at 1.0, not 1.25
    pose.set_expression("smirk", 1.0).unwrap();
    assert_eq!(pose.morph_weights(0), vec![0.5, 1.0]);
    // half an expression is half of each bind: happy 0.5 -> target 1 at 0.5, target 0 at 0.25
    let mut half = VrmPose::new(&model).unwrap();
    half.set_expression("happy", 0.5).unwrap();
    assert_eq!(half.morph_weights(0), vec![0.25, 0.5]);
}

#[test]
fn a_joint_transform_skins_the_vertices_by_their_weights() {
    let model = read_vrm(TINY).unwrap();
    let mut pose = VrmPose::new(&model).unwrap();
    // head (node 2) moved to (0.5, 1, 0): its skinning matrix is a translation by (0.5, 0, 0)
    pose.set_translation(2, [0.5, 1.0, 0.0]).unwrap();
    // v0 moves 0.25 * 0.5 = 0.125; v1 moves 0.5 * 0.5 = 0.25; v2 moves 0.5
    assert_eq!(
        posed(&pose),
        vec![[0.125, 0.0, 0.0], [1.25, 0.0, 0.0], [0.5, 1.0, 0.0]]
    );
    // the parent moves the child: hips (node 1) to (0, 0, 2) moves every vertex by (0, 0, 2)
    let mut p2 = VrmPose::new(&model).unwrap();
    p2.set_translation(1, [0.0, 0.0, 2.0]).unwrap();
    assert_eq!(
        posed(&p2),
        vec![[0.0, 0.0, 2.0], [1.0, 0.0, 2.0], [0.0, 1.0, 2.0]]
    );
    // a quarter turn of the head about z (quaternion (0, 0, sin 45, cos 45)) turns vertex 2,
    // which sits at the head's origin (0,1,0) in model space, onto itself
    let mut p3 = VrmPose::new(&model).unwrap();
    let s = std::f32::consts::FRAC_1_SQRT_2;
    p3.set_rotation(2, [0.0, 0.0, s, s]).unwrap();
    let v2 = posed(&p3)[2];
    assert!((v2[0]).abs() < 1e-6 && (v2[1] - 1.0).abs() < 1e-6, "{v2:?}");
    // the head's skinning matrix turns about (0, 1, 0): vertex 1 (1, 0, 0) is (1, -1) from the
    // pivot, turned to (1, 1), so at (1, 2, 0); weighted half with the hips (identity):
    // 0.5 (1, 0, 0) + 0.5 (1, 2, 0) = (1, 1, 0). Vertex 0 (0, 0, 0) is (0, -1) from the pivot,
    // turned to (1, 0), so at (1, 1, 0); weighted a quarter: (0.25, 0.25, 0)
    let posed3 = posed(&p3);
    for (got, want) in [(posed3[1], [1.0, 1.0, 0.0]), (posed3[0], [0.25, 0.25, 0.0])] {
        assert!(
            got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-6),
            "{got:?} vs {want:?}"
        );
    }
}

#[test]
fn morph_then_skin() {
    // glTF applies morph targets before skinning: aa lowers vertex 2 to (0, 0.75, 0), then the
    // head translation (0.5, 0, 0) moves it to (0.5, 0.75, 0)
    let model = read_vrm(TINY).unwrap();
    let mut pose = VrmPose::new(&model).unwrap();
    pose.set_expression("aa", 1.0).unwrap();
    pose.set_translation(2, [0.5, 1.0, 0.0]).unwrap();
    assert_eq!(posed(&pose)[2], [0.5, 0.75, 0.0]);
}

#[test]
fn what_a_pose_refuses() {
    let model = read_vrm(TINY).unwrap();
    let mut pose = VrmPose::new(&model).unwrap();
    assert!(matches!(
        pose.set_expression("nonexistent", 1.0),
        Err(PoseError::UnknownExpression(_))
    ));
    assert!(matches!(
        pose.set_expression("aa", f32::NAN),
        Err(PoseError::BadWeight)
    ));
    assert!(matches!(
        pose.set_expression("aa", 1.5),
        Err(PoseError::BadWeight)
    ));
    assert!(matches!(
        pose.set_translation(9, [0.0; 3]),
        Err(PoseError::NoSuchNode(9))
    ));
    assert!(matches!(
        pose.set_translation(1, [f32::INFINITY, 0.0, 0.0]),
        Err(PoseError::NotFinite)
    ));
    assert!(matches!(
        pose.posed_positions(5, 0),
        Err(PoseError::NoSuchMesh(5))
    ));
    assert!(matches!(
        pose.posed_positions(0, 7),
        Err(PoseError::NoSuchMesh(0))
    ));
}

#[test]
fn a_node_cycle_is_refused() {
    let mut model = read_vrm(TINY).unwrap();
    // head's child is hips, and hips' child is head
    model.nodes[2].children = vec![1];
    assert!(matches!(VrmPose::new(&model), Err(PoseError::NodeCycle)));
}

#[test]
fn a_hand_built_model_with_a_child_past_the_nodes_is_refused() {
    let mut model = read_vrm(TINY).unwrap();
    model.nodes[1].children = vec![9];
    assert!(matches!(
        VrmPose::new(&model),
        Err(PoseError::NoSuchNode(9))
    ));
}

#[test]
fn a_rotation_must_be_a_unit_quaternion() {
    let model = read_vrm(TINY).unwrap();
    let mut pose = VrmPose::new(&model).unwrap();
    assert!(matches!(
        pose.set_rotation(2, [0.0, 0.0, 0.0, 0.0]),
        Err(PoseError::NotUnitRotation)
    ));
    assert!(matches!(
        pose.set_rotation(2, [0.0, 0.0, 0.0, 2.0]),
        Err(PoseError::NotUnitRotation)
    ));
    // within the tolerance of a unit length (|q|^2 within 1e-4 of 1) it is accepted
    let s = std::f32::consts::FRAC_1_SQRT_2;
    assert!(pose.set_rotation(2, [0.0, 0.0, s, s]).is_ok());
    assert!(pose.set_rotation(2, [0.0, 0.0, 0.0, 1.00004]).is_ok());
}

#[test]
fn normals_follow_the_joints_and_stay_unit_length() {
    // the fixture's normals are all (0, 0, 1). A quarter turn of the head about x
    // (quaternion (sin 45, 0, 0, cos 45)) turns (0, 0, 1) to (0, -1, 0):
    // vertex 2 (head only) -> (0, -1, 0); vertex 1 (half hips, half head) ->
    // normalize((0, 0, 1) + (0, -1, 0)) = (0, -1/sqrt 2, 1/sqrt 2); vertex 0 (a quarter head) ->
    // normalize(0.75 (0, 0, 1) + 0.25 (0, -1, 0)) = (0, -0.25, 0.75) / sqrt(0.625)
    let model = read_vrm(TINY).unwrap();
    let mut pose = VrmPose::new(&model).unwrap();
    assert_eq!(
        pose.posed_normals(0, 0).unwrap(),
        vec![[0.0, 0.0, 1.0]; 3],
        "at rest the file normals"
    );
    let s = std::f32::consts::FRAC_1_SQRT_2;
    pose.set_rotation(2, [s, 0.0, 0.0, s]).unwrap();
    let n = pose.posed_normals(0, 0).unwrap();
    let l = 0.625f32.sqrt();
    let want = [[0.0, -0.25 / l, 0.75 / l], [0.0, -s, s], [0.0, -1.0, 0.0]];
    for (got, want) in n.iter().zip(want) {
        assert!(
            got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-6),
            "{got:?} vs {want:?}"
        );
    }
    // a translation moves no normal
    let mut t = VrmPose::new(&model).unwrap();
    t.set_translation(2, [5.0, 1.0, 0.0]).unwrap();
    assert_eq!(t.posed_normals(0, 0).unwrap(), vec![[0.0, 0.0, 1.0]; 3]);
    // a primitive without normals is an error, not an empty list
    assert!(matches!(
        t.posed_normals(0, 1),
        Err(PoseError::NoNormals(0))
    ));
}

#[test]
fn an_expression_moves_the_normals_by_its_targets_normal_deltas() {
    // aa: target 0 at 1.0 adds (0, 0.5, 0) to vertex 2's normal: (0, 0.5, 1) / sqrt(1.25)
    let model = read_vrm(TINY).unwrap();
    let mut pose = VrmPose::new(&model).unwrap();
    pose.set_expression("aa", 1.0).unwrap();
    let n = pose.posed_normals(0, 0).unwrap();
    let l = 1.25f32.sqrt();
    assert_eq!(n[0], [0.0, 0.0, 1.0]);
    assert!(
        (n[2][1] - 0.5 / l).abs() < 1e-6 && (n[2][2] - 1.0 / l).abs() < 1e-6,
        "{:?}",
        n[2]
    );
}

#[test]
fn normals_use_the_inverse_transpose_under_non_uniform_scale() {
    // head scaled (2, 1, 1): its skinning matrix's 3x3 part is diag(2, 1, 1); a normal
    // (1, 0, 1) on vertex 2 (head only) turns by the inverse transpose diag(0.5, 1, 1) to
    // (0.5, 0, 1) / sqrt(1.25), not by the matrix itself to (2, 0, 1) / sqrt(5)
    let mut model = read_vrm(TINY).unwrap();
    model.nodes[2].scale = [2.0, 1.0, 1.0];
    model.meshes[0][0].normals = Some(vec![[1.0, 0.0, 1.0]; 3]);
    let pose = VrmPose::new(&model).unwrap();
    let n = pose.posed_normals(0, 0).unwrap()[2];
    let l = 1.25f32.sqrt();
    let want = [0.5 / l, 0.0, 1.0 / l];
    assert!(
        n.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-6),
        "{n:?}"
    );
}
