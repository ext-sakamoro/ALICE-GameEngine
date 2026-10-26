//! A VRM model as a format-neutral rig (`vrm_measure::rig`), on the hand-laid fixture of
//! `scripts/vrm_fixture.py`
//!
//! oracle: the fixture's own values (three nodes: a mesh node, the hips at the origin and the head
//! one unit above; one skin of the hips and the head; the vertices, joints and weights the script
//! wrote). In the rest pose every joint's world matrix times its inverse bind matrix is the
//! identity, so the skinned vertices are the written ones.
#![cfg(feature = "lol-measure")]
#![allow(clippy::float_cmp)]

use alice_game_engine::vrm::read_vrm;
use alice_game_engine::vrm_measure::rig;
use alice_lol_humanoid::measure::MeasureError;
use alice_lol_humanoid::rig::body_marks;

const TINY: &[u8] = include_bytes!("data/vrm/tiny.vrm");

/// A rig bone as (name, parent, bind position)
type BoneRow<'a> = (&'a str, Option<usize>, Option<[f64; 3]>);

#[test]
fn the_humanoid_names_and_places_the_bones() {
    let r = rig(&read_vrm(TINY).unwrap()).unwrap();
    let got: Vec<BoneRow<'_>> = r
        .bones
        .iter()
        .map(|b| (b.name.as_str(), b.parent, b.bind))
        .collect();
    assert_eq!(
        got,
        vec![
            // the mesh node is not in the humanoid: unnamed, unbound
            ("", None, None),
            ("hips", None, Some([0.0, 0.0, 0.0])),
            ("head", Some(1), Some([0.0, 1.0, 0.0])),
        ]
    );
}

#[test]
fn the_skinned_mesh_follows_its_heaviest_joint() {
    let r = rig(&read_vrm(TINY).unwrap()).unwrap();
    assert_eq!(r.meshes.len(), 1);
    let m = &r.meshes[0];
    // both primitives, in order: the written positions
    let want = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [2.0, 0.0, 0.0],
        [3.0, 0.0, 0.0],
        [2.0, 1.0, 0.0],
    ];
    assert_eq!(m.positions.len(), want.len());
    for (p, w) in m.positions.iter().zip(want) {
        assert!(
            p.iter().zip(w).all(|(a, b)| (a - b).abs() < 1e-6),
            "{p:?} {w:?}"
        );
    }
    // weights (0.75, 0.25) / (0.5, 0.5) / (1, 0) on joints (0, 1) / (0, 1) / (1, 0), then
    // (255, 0) / (191, 64) / (128, 127) of 255 on the same joints: a tie goes to the first
    assert_eq!(
        m.leads,
        vec![Some(1), Some(1), Some(2), Some(1), Some(1), Some(2)]
    );
}

#[test]
fn a_rig_without_eyes_is_not_measured() {
    let r = rig(&read_vrm(TINY).unwrap()).unwrap();
    assert_eq!(body_marks(&r).unwrap_err(), MeasureError::Missing("eyes"));
}

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

#[test]
fn binds_are_world_positions_and_an_unweighted_vertex_follows_none() {
    let g = alice_game_engine::vrm::split_glb(TINY).unwrap();
    let mut j: serde_json::Value = serde_json::from_str(g.json).unwrap();
    let mut bin = g.bin.unwrap().to_vec();
    // the hips off the origin: the head's world position is the sum
    j["nodes"][1]["translation"] = serde_json::json!([0.5, 0.25, -0.125]);
    // the first primitive's weights all on the first joint slot (joints (0, 1) / (0, 1) / (1, 0)),
    // except vertex 1, which has none at all
    let zeros: [[f32; 4]; 3] = [[1.0, 0.0, 0.0, 0.0], [0.0; 4], [1.0, 0.0, 0.0, 0.0]];
    let offset = bin.len();
    bin.extend(zeros.iter().flatten().flat_map(|f| f.to_le_bytes()));
    let views = j["bufferViews"].as_array_mut().unwrap();
    views.extend([serde_json::json!({"buffer": 0, "byteOffset": offset, "byteLength": 48})]);
    let view = views.len() - 1;
    let accs = j["accessors"].as_array_mut().unwrap();
    accs.extend([
        serde_json::json!({"bufferView": view, "componentType": 5126, "count": 3, "type": "VEC4"}),
    ]);
    j["meshes"][0]["primitives"][0]["attributes"]["WEIGHTS_0"] = serde_json::json!(accs.len() - 1);
    j["buffers"][0]["byteLength"] = serde_json::json!(bin.len());
    let r = rig(&read_vrm(&glb(&j.to_string(), &bin)).unwrap()).unwrap();
    assert_eq!(r.bones[1].bind, Some([0.5, 0.25, -0.125]));
    assert_eq!(r.bones[2].bind, Some([0.5, 1.25, -0.125]));
    assert_eq!(&r.meshes[0].leads[..3], &[Some(1), None, Some(2)]);
}
