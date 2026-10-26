//! A glTF node may give its transform as a `matrix` (column-major 4x4) instead of translation,
//! rotation and scale; the reader turns it into the node's TRS.
//!
//! oracle: matrices composed by hand from a known translation, rotation and scale (M = T · R · S,
//! glTF 2.0 § 5.25), read back as those values, and the posed world matrix equal to the matrix
//! given. A matrix glTF forbids (with TRS beside it, not affine, sheared, a zero scale) is refused.
#![allow(clippy::float_cmp)]

use alice_game_engine::vrm::{read_vrm, split_glb, VrmError, VrmModel};
use alice_game_engine::vrm_pose::VrmPose;
use serde_json::{json, Value};

const TINY: &[u8] = include_bytes!("data/vrm/tiny.vrm");

/// A GLB from a JSON text and a BIN chunk (the container layout of glTF 2.0)
fn glb(json: &str, bin: &[u8]) -> Vec<u8> {
    let mut js = json.as_bytes().to_vec();
    js.resize(js.len().div_ceil(4) * 4, b' ');
    let total = 12 + 8 + js.len() + 8 + bin.len();
    let words: Vec<u32> = vec![0x4654_6C67, 2, total as u32, js.len() as u32, 0x4E4F_534A];
    let mut out: Vec<u8> = words.iter().flat_map(|v| v.to_le_bytes()).collect();
    out.extend_from_slice(&js);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    out.extend_from_slice(bin);
    out
}

/// The fixture with node 2 (the head, child of the hips at the origin) given by a matrix
fn with_matrix(m: Value, also: Option<(&str, Value)>) -> Result<VrmModel, VrmError> {
    let g = split_glb(TINY).unwrap();
    let mut j: Value = serde_json::from_str(g.json).unwrap();
    let head = j["nodes"][2].as_object_mut().unwrap();
    head.remove("translation");
    head.insert("matrix".into(), m);
    if let Some((k, v)) = also {
        head.insert(k.into(), v);
    }
    read_vrm(&glb(&j.to_string(), g.bin.unwrap()))
}

const H: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// cos 150° and sin 150°
const C: f32 = -0.866_025_4;
const S: f32 = 0.5;
/// sin 75° and cos 75° (the quaternion of a 150° turn)
const S75: f32 = 0.965_925_8;
const C75: f32 = 0.258_819_04;

/// T = (0.25, 1, -0.5), R = 90° about z (x → y, y → -x), S = (2, 3, 4): the columns are
/// R·(2, 0, 0) = (0, 2, 0), R·(0, 3, 0) = (-3, 0, 0), (0, 0, 4) and the translation
fn trs_matrix() -> Value {
    json!([0, 2, 0, 0, -3, 0, 0, 0, 0, 0, 4, 0, 0.25, 1, -0.5, 1])
}

#[test]
fn a_matrix_reads_as_its_translation_rotation_and_scale() {
    let m = with_matrix(trs_matrix(), None).unwrap();
    let n = &m.nodes[2];
    assert_eq!(n.translation, [0.25, 1.0, -0.5]);
    assert_eq!(n.scale, [2.0, 3.0, 4.0]);
    let want = [0.0, 0.0, H, H];
    assert!(
        n.rotation
            .iter()
            .zip(want)
            .all(|(a, b)| (a - b).abs() < 1e-6),
        "{:?}",
        n.rotation
    );
    // posed, the node's world matrix is the matrix given (its parent is at the origin)
    let w = VrmPose::new(&m).unwrap().world_matrices()[2]
        .0
        .to_cols_array();
    let given: Vec<f32> = trs_matrix()
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap() as f32)
        .collect();
    for (a, b) in w.iter().zip(&given) {
        assert!((a - b).abs() < 1e-5, "{w:?}");
    }
}

#[test]
fn a_mirroring_matrix_reads_as_a_negative_scale() {
    // det < 0: x mirrored, no rotation
    let m = with_matrix(
        json!([-1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 1, 0, 1]),
        None,
    )
    .unwrap();
    let n = &m.nodes[2];
    assert_eq!(
        (n.scale, n.translation),
        ([-1.0, 1.0, 1.0], [0.0, 1.0, 0.0])
    );
    assert!(
        n.rotation
            .iter()
            .zip([0.0, 0.0, 0.0, 1.0])
            .all(|(a, b)| (a - b).abs() < 1e-6),
        "{:?}",
        n.rotation
    );
}

#[test]
fn the_identity_matrix_is_the_identity() {
    let m = with_matrix(
        json!([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]),
        None,
    )
    .unwrap();
    let n = &m.nodes[2];
    assert_eq!(
        (n.translation, n.rotation, n.scale),
        ([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3])
    );
}

#[test]
fn a_matrix_gltf_forbids_is_refused() {
    let bad = |e: Result<VrmModel, VrmError>| {
        assert_eq!(e.unwrap_err(), VrmError::BadValue("nodes[2].matrix".into()))
    };
    // beside translation, rotation or scale
    for k in ["translation", "rotation", "scale"] {
        let v = if k == "rotation" {
            json!([0, 0, 0, 1])
        } else {
            json!([1, 1, 1])
        };
        bad(with_matrix(trs_matrix(), Some((k, v))));
    }
    // not affine (the last row is not 0 0 0 1)
    bad(with_matrix(
        json!([1, 0, 0, 0.5, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]),
        None,
    ));
    bad(with_matrix(
        json!([1, 0, 0, 0, 0, 1, 0, 0.5, 0, 0, 1, 0, 0, 0, 0, 1]),
        None,
    ));
    bad(with_matrix(
        json!([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0.5, 0, 0, 0, 1]),
        None,
    ));
    bad(with_matrix(
        json!([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 2]),
        None,
    ));
    // sheared: the second column leans on the first
    bad(with_matrix(
        json!([1, 0, 0, 0, 0.5, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]),
        None,
    ));
    // a zero scale has no rotation to read
    bad(with_matrix(
        json!([0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]),
        None,
    ));
    // not finite
    let mut m = trs_matrix();
    m[12] = json!(1e300);
    bad(with_matrix(m, None));
    // fifteen numbers
    bad(with_matrix(
        json!([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0]),
        None,
    ));
}

#[test]
fn every_kind_of_rotation_reads_back_to_its_matrix() {
    // (columns of the rotation, the quaternion): half turns about x, y and z (trace -1) and a
    // third of a turn about (1, 1, 1) that cycles the axes x → y → z → x (trace 0)
    let cases: [([f32; 9], [f32; 4]); 12] = [
        ([1., 0., 0., 0., -1., 0., 0., 0., -1.], [1., 0., 0., 0.]),
        ([-1., 0., 0., 0., 1., 0., 0., 0., -1.], [0., 1., 0., 0.]),
        ([-1., 0., 0., 0., -1., 0., 0., 0., 1.], [0., 0., 1., 0.]),
        ([0., 1., 0., 0., 0., 1., 1., 0., 0.], [0.5, 0.5, 0.5, 0.5]),
        // a quarter turn about x (y → z, z → -y) and about y (z → x, x → -z): trace 1
        ([1., 0., 0., 0., 0., 1., 0., -1., 0.], [H, 0., 0., H]),
        ([0., 0., -1., 0., 1., 0., 1., 0., 0.], [0., H, 0., H]),
        // 150° about x, y and z: trace 1 + 2 cos 150° < 0, so each reaches its own axis's branch
        // with w = cos 75° (not 0, so the sign of the w term shows)
        ([1., 0., 0., 0., C, S, 0., -S, C], [S75, 0., 0., C75]),
        ([C, 0., -S, 0., 1., 0., S, 0., C], [0., S75, 0., C75]),
        ([C, S, 0., -S, C, 0., 0., 0., 1.], [0., 0., S75, C75]),
        // 150° about the axes (2, 1, 0.5), (0.5, 2, 1) and (1, 0.5, 2), normalized (Rodrigues;
        // q = (n sin 75°, cos 75°)): the same branches with every term of the quaternion non-zero
        (
            [
                0.555_708_24,
                0.819_975_8,
                0.137_215_52,
                0.601_757_9,
                -0.510_592,
                0.614_152_5,
                0.573_651_3,
                -0.258_719_08,
                -0.777_167_05,
            ],
            [0.843_129_2, 0.421_564_6, 0.210_782_3, C75],
        ),
        (
            [
                -0.777_167_05,
                0.573_651_3,
                -0.258_719_08,
                0.137_215_52,
                0.555_708_24,
                0.819_975_8,
                0.614_152_5,
                0.601_757_9,
                -0.510_592,
            ],
            [0.210_782_3, 0.843_129_2, 0.421_564_6, C75],
        ),
        (
            [
                -0.510_592,
                0.614_152_5,
                0.601_757_9,
                -0.258_719_08,
                -0.777_167_05,
                0.573_651_3,
                0.819_975_8,
                0.137_215_52,
                0.555_708_24,
            ],
            [0.421_564_6, 0.210_782_3, 0.843_129_2, C75],
        ),
    ];
    for (r, q) in cases {
        let given = [
            r[0], r[1], r[2], 0., r[3], r[4], r[5], 0., r[6], r[7], r[8], 0., 0.5, 0.25, 0.125, 1.,
        ];
        let m = with_matrix(json!(given), None).unwrap();
        let n = &m.nodes[2];
        assert!(
            n.rotation.iter().zip(q).all(|(a, b)| (a - b).abs() < 1e-6),
            "{r:?}: {:?}",
            n.rotation
        );
        let w = VrmPose::new(&m).unwrap().world_matrices()[2]
            .0
            .to_cols_array();
        assert!(
            w.iter().zip(given).all(|(a, b)| (a - b).abs() < 1e-5),
            "{r:?}: {w:?}"
        );
    }
}

#[test]
fn a_0x_matrix_node_is_turned_with_the_model() {
    let g = split_glb(TINY).unwrap();
    let mut j: Value = serde_json::from_str(g.json).unwrap();
    j["extensions"] = json!({"VRM": {"meta": {}, "humanoid": {"humanBones": []}}});
    j["materials"][0]
        .as_object_mut()
        .unwrap()
        .remove("extensions");
    let head = j["nodes"][2].as_object_mut().unwrap();
    head.remove("translation");
    head.insert("matrix".into(), trs_matrix());
    let m = read_vrm(&glb(&j.to_string(), g.bin.unwrap())).unwrap();
    let n = &m.nodes[2];
    // turned 180° about y: the translation's x and z, the quaternion's x and z change sign
    assert_eq!(
        (n.translation, n.scale),
        ([-0.25, 1.0, 0.5], [2.0, 3.0, 4.0])
    );
    assert!(
        n.rotation
            .iter()
            .zip([0.0, 0.0, -H, H])
            .all(|(a, b)| (a - b).abs() < 1e-6),
        "{:?}",
        n.rotation
    );
}
