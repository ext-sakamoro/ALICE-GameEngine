//! VRM 0.x files read into the 1.0 model: the hand-laid fixture of `scripts/vrm_fixture.py`
//! rewritten with a 0.x `VRM` extension in place of `VRMC_vrm`.
//!
//! oracle: the 1.0 reading of the same bytes, turned 180° about Y by hand, and the values
//! worked out by hand from the VRM 0.x specification and the reference migration's formulas
//! (sRGB to linear, the `MToon` shading range, the outline width units). Nothing here is
//! computed by the reader under test.
#![allow(clippy::float_cmp)]

use alice_game_engine::asset::parse_vrm_json;
use alice_game_engine::vrm::{
    read_vrm, read_vrm_json, split_glb, Licence, LookAtKind, OutlineWidthMode, VrmError, VrmModel,
};
use alice_game_engine::vrm0::Vrm0Meta;
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

/// The fixture's JSON and BIN, with a rotation on the hips, a translation off the axis on the
/// head, and inverse bind matrices that rotate and translate in x and z (appended to the BIN)
fn base() -> (Value, Vec<u8>) {
    let g = split_glb(TINY).unwrap();
    let mut j: Value = serde_json::from_str(g.json).unwrap();
    let mut bin = g.bin.unwrap().to_vec();
    // a quarter turn about z, then about y: quaternions with every component non-zero
    j["nodes"][1]["rotation"] = json!([0.1, 0.2, 0.3, 0.927_361_85]);
    j["nodes"][2]["translation"] = json!([0.25, 1.0, -0.5]);
    let ibm: [[f32; 16]; 2] = [
        [
            0., 0., -1., 0., 0., 1., 0., 0., 1., 0., 0., 0., 0.5, 0., 0.25, 1.,
        ],
        [
            1., 0., 0., 0., 0., 0., 1., 0., 0., -1., 0., 0., -0.25, -1., 0.75, 1.,
        ],
    ];
    let offset = bin.len();
    bin.extend(ibm.iter().flatten().flat_map(|f| f.to_le_bytes()));
    let views = j["bufferViews"].as_array_mut().unwrap();
    views.push(json!({"buffer": 0, "byteOffset": offset, "byteLength": 128}));
    let view = views.len() - 1;
    let accs = j["accessors"].as_array_mut().unwrap();
    accs.push(json!({"bufferView": view, "componentType": 5126, "count": 2, "type": "MAT4"}));
    j["skins"][0]["inverseBindMatrices"] = json!(accs.len() - 1);
    j["buffers"][0]["byteLength"] = json!(bin.len());
    // target 0's normal delta on vertex 2 off the y axis, so that turning it shows
    set_vec3(&j, &mut bin, 8, 2, [0.25, 0.5, -0.125]);
    (j, bin)
}

/// Overwrite element `i` of a float VEC3 accessor in the BIN chunk
fn set_vec3(j: &Value, bin: &mut [u8], acc: usize, i: usize, v: [f32; 3]) {
    let a = &j["accessors"][acc];
    assert_eq!(a["type"], "VEC3");
    let view = &j["bufferViews"][a["bufferView"].as_u64().unwrap() as usize];
    let at = view["byteOffset"].as_u64().unwrap_or(0) as usize
        + a["byteOffset"].as_u64().unwrap_or(0) as usize
        + i * 12;
    for (k, f) in v.iter().enumerate() {
        bin[at + 4 * k..at + 4 * k + 4].copy_from_slice(&f.to_le_bytes());
    }
}

/// The 0.x extension the tests start from
fn vrm0_ext() -> Value {
    json!({
        "specVersion": "0.0",
        "meta": {
            "title": "Tiny", "version": "", "author": "Someone",
            "allowedUserName": "ExplicitlyLicensedPerson",
            "violentUssageName": "Disallow", "sexualUssageName": "Allow",
            "commercialUssageName": "Allow", "licenseName": "Redistribution_Prohibited",
            "otherLicenseUrl": ""
        },
        "humanoid": {"humanBones": [
            {"bone": "hips", "node": 1},
            {"bone": "head", "node": 2},
            {"bone": "leftThumbProximal", "node": 2},
            {"bone": "rightThumbIntermediate", "node": 1},
            {"bone": "leftThumbDistal", "node": 2}
        ]},
        "blendShapeMaster": {"blendShapeGroups": [
            {"name": "Joy", "presetName": "Joy", "isBinary": true,
             "binds": [{"mesh": 0, "index": 1, "weight": 50}, {"mesh": 0, "index": 0, "weight": 100}]},
            {"name": "A", "presetName": "a", "binds": [{"mesh": 0, "index": 0, "weight": 100}]},
            {"name": "Wink", "presetName": "unknown", "binds": [{"mesh": 0, "index": 1, "weight": 25}]},
            {"name": "BL", "presetName": "blink_l", "binds": []}
        ]},
        "firstPerson": {
            "firstPersonBone": 2,
            "firstPersonBoneOffset": {"x": 0.1, "y": 0.05, "z": 0.125},
            "lookAtTypeName": "BlendShape",
            "lookAtHorizontalInner": {"xRange": 30, "yRange": 1},
            "lookAtVerticalUp": {"xRange": 20, "yRange": 0.5}
        },
        "materialProperties": [{
            "name": "skin", "shader": "VRM/MToon",
            "floatProperties": {
                "_ShadeShift": 0.2, "_ShadeToony": 0.5,
                "_OutlineWidthMode": 1, "_OutlineWidth": 0.625,
                "_OutlineColorMode": 1, "_OutlineLightingMix": 0.25
            },
            "vectorProperties": {
                "_Color": [1.0, 0.5, 0.0, 0.75],
                "_ShadeColor": [0.0, 0.02, 1.0, 1.0],
                "_OutlineColor": [0.5, 0.0, 1.0, 1.0]
            }
        }]
    })
}

/// The fixture as a 0.x file: the `VRMC_*` extensions out, the 0.x one in
fn file0(ext: Value) -> (Value, Vec<u8>) {
    let (mut j, bin) = base();
    j["extensions"] = json!({ "VRM": ext });
    j["extensionsUsed"] = json!(["VRM"]);
    j["materials"][0]
        .as_object_mut()
        .unwrap()
        .remove("extensions");
    (j, bin)
}

fn read0(ext: Value) -> Result<VrmModel, VrmError> {
    let (j, bin) = file0(ext);
    read_vrm(&glb(&j.to_string(), &bin))
}

/// The same bytes read as 1.0 (the reference orientation)
fn read1() -> VrmModel {
    let (j, bin) = base();
    read_vrm(&glb(&j.to_string(), &bin)).unwrap()
}

fn turn(p: [f32; 3]) -> [f32; 3] {
    [-p[0], p[1], -p[2]]
}

fn srgb(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[test]
fn the_meta_is_kept_as_the_file_states_it() {
    let m = read0(vrm0_ext()).unwrap();
    assert_eq!(m.document.spec_version, "0.0");
    assert_eq!(
        m.document.meta,
        Licence::Vrm0(Vrm0Meta {
            title: Some("Tiny".into()),
            version: None,
            author: Some("Someone".into()),
            contact_information: None,
            reference: None,
            allowed_user_name: Some("ExplicitlyLicensedPerson".into()),
            violent_usage_name: Some("Disallow".into()),
            sexual_usage_name: Some("Allow".into()),
            commercial_usage_name: Some("Allow".into()),
            other_permission_url: None,
            license_name: Some("Redistribution_Prohibited".into()),
            other_license_url: None,
        })
    );
    assert_eq!(
        (m.document.meta.title(), m.document.meta.author()),
        ("Tiny", "Someone")
    );
    // the asset header reads a 0.x file too
    let (j, _) = file0(vrm0_ext());
    let h = parse_vrm_json(&j.to_string()).unwrap();
    assert_eq!(
        (h.spec_version.as_str(), h.title.as_str(), h.author.as_str()),
        ("0.0", "Tiny", "Someone")
    );
    // a value outside the 0.x set is refused, not mapped
    for (key, value) in [
        ("licenseName", "CC_BY_ANY"),
        ("allowedUserName", "Anyone"),
        ("commercialUssageName", "Yes"),
    ] {
        let mut e = vrm0_ext();
        e["meta"][key] = json!(value);
        assert_eq!(
            read0(e).unwrap_err(),
            VrmError::BadValue(format!("meta.{key}")),
            "{key}"
        );
    }
    let mut e = vrm0_ext();
    e.as_object_mut().unwrap().remove("meta");
    assert_eq!(read0(e).unwrap_err(), VrmError::Missing("meta".into()));
}

#[test]
fn the_bones_take_their_1_0_names() {
    let m = read0(vrm0_ext()).unwrap();
    assert_eq!(
        m.document.bones,
        vec![
            ("head".to_owned(), 2),
            ("hips".to_owned(), 1),
            ("leftThumbDistal".to_owned(), 2),
            ("leftThumbMetacarpal".to_owned(), 2),
            ("rightThumbProximal".to_owned(), 1),
        ]
    );
}

/// An expression as (name, preset, binary, binds as (node, index, weight))
type Expr<'a> = (&'a str, bool, bool, Vec<(u32, u32, f32)>);

/// The preset names are read without regard to case, as the reference implementation reads them
#[test]
fn the_expressions_take_their_1_0_names_nodes_and_weights() {
    let m = read0(vrm0_ext()).unwrap();
    let got: Vec<Expr<'_>> = m
        .document
        .expressions
        .iter()
        .map(|e| {
            (
                e.name.as_str(),
                e.preset,
                e.is_binary,
                e.binds
                    .iter()
                    .map(|b| (b.node, b.index, b.weight))
                    .collect(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("aa", true, false, vec![(0, 0, 1.0)]),
            ("blinkLeft", true, false, vec![]),
            ("happy", true, true, vec![(0, 1, 0.5), (0, 0, 1.0)]),
            ("Wink", false, false, vec![(0, 1, 0.25)]),
        ]
    );
    // a bind goes to the first node that holds the mesh
    let (mut j, bin) = file0(vrm0_ext());
    let nodes = j["nodes"].as_array_mut().unwrap();
    nodes.insert(0, json!({"name": "before", "children": []}));
    nodes.push(json!({"name": "copy", "mesh": 0}));
    // the indices that pointed at nodes move by one
    j["nodes"][2]["children"] = json!([3]);
    j["skins"][0]["joints"] = json!([2, 3]);
    j["scenes"][0]["nodes"] = json!([1, 2, 0]);
    let mut e = vrm0_ext();
    e["humanoid"]["humanBones"] = json!([]);
    e["firstPerson"]["firstPersonBone"] = json!(3);
    j["extensions"]["VRM"] = e;
    let m = read_vrm(&glb(&j.to_string(), &bin)).unwrap();
    assert!(m
        .document
        .expressions
        .iter()
        .flat_map(|e| &e.binds)
        .all(|b| b.node == 1));
    // a mesh that no node holds is refused
    let mut e = vrm0_ext();
    e["blendShapeMaster"]["blendShapeGroups"][1]["binds"][0]["mesh"] = json!(1);
    assert_eq!(
        read0(e).unwrap_err(),
        VrmError::OutOfRange("blendShapeMaster.blendShapeGroups[1].binds.mesh".into())
    );
}

#[test]
fn the_look_at_offset_and_ranges() {
    let m = read0(vrm0_ext()).unwrap();
    let l = m.document.look_at.unwrap();
    assert_eq!(l.kind, LookAtKind::Expression);
    // stored with the model facing +Z in a left-handed frame: only x changes sign
    assert_eq!(l.offset_from_head_bone, [-0.1, 0.05, 0.125]);
    assert_eq!(
        (
            l.horizontal_inner.input_max_value,
            l.horizontal_inner.output_scale
        ),
        (30.0, 1.0)
    );
    assert_eq!(
        (l.vertical_up.input_max_value, l.vertical_up.output_scale),
        (20.0, 0.5)
    );
    // an absent map takes the default
    assert_eq!(
        (
            l.horizontal_outer.input_max_value,
            l.horizontal_outer.output_scale
        ),
        (90.0, 10.0)
    );
    let mut e = vrm0_ext();
    e["firstPerson"]["lookAtTypeName"] = json!("Eyes");
    assert_eq!(
        read0(e).unwrap_err(),
        VrmError::BadValue("firstPerson.lookAtTypeName".into())
    );
}

#[test]
fn mtoon_0x_is_converted_as_the_migration_does() {
    let m = read0(vrm0_ext()).unwrap();
    let mat = &m.materials[0];
    assert_eq!(mat.base_color, [1.0, srgb(0.5), 0.0, 0.75]);
    let t = mat.mtoon.unwrap();
    assert_eq!(t.shade_color, [0.0, 0.02 / 12.92, 1.0]);
    // the 0.x range runs from 0.2 to 1 + (0.2 - 1) * 0.5 = 0.6: centre -0.4, margin 0.8
    assert!((t.shading_shift - -0.4).abs() < 1e-6, "{}", t.shading_shift);
    assert!((t.shading_toony - 0.8).abs() < 1e-6, "{}", t.shading_toony);
    // world: centimetres to metres
    assert_eq!(t.outline_width_mode, OutlineWidthMode::World);
    assert!((t.outline_width - 0.006_25).abs() < 1e-9);
    assert_eq!(t.outline_color, [srgb(0.5), 0.0, 1.0]);
    assert_eq!(t.outline_lighting_mix, 0.25);
    // screen: percent of half the screen height to a fraction of the height; fixed colour: no mix
    let mut e = vrm0_ext();
    e["materialProperties"][0]["floatProperties"]["_OutlineWidthMode"] = json!(2);
    e["materialProperties"][0]["floatProperties"]["_OutlineColorMode"] = json!(0);
    let t = read0(e).unwrap().materials[0].mtoon.unwrap();
    assert_eq!(t.outline_width_mode, OutlineWidthMode::Screen);
    assert!((t.outline_width - 0.003_125).abs() < 1e-9);
    assert_eq!(t.outline_lighting_mix, 0.0);
    // absent properties take the 0.x shader's defaults: shift 0 and toony 0.9 give -0.05 and
    // 0.95, no outline, a white lit colour
    let mut e = vrm0_ext();
    e["materialProperties"][0]["floatProperties"] = json!({});
    e["materialProperties"][0]["vectorProperties"] = json!({});
    let mat = &read0(e).unwrap().materials[0];
    let t = mat.mtoon.unwrap();
    assert!((t.shading_shift - -0.05).abs() < 1e-6 && (t.shading_toony - 0.95).abs() < 1e-6);
    assert_eq!(
        (t.outline_width_mode, t.outline_width),
        (OutlineWidthMode::None, 0.0)
    );
    assert_eq!(mat.base_color, [1.0; 4]);
    assert_eq!(t.shade_color, [srgb(0.97), srgb(0.81), srgb(0.86)]);
    // another shader keeps the glTF material
    let mut e = vrm0_ext();
    e["materialProperties"][0]["shader"] = json!("VRM/UnlitTexture");
    let mat = &read0(e).unwrap().materials[0];
    assert_eq!((mat.base_color, mat.mtoon), ([1.0, 0.5, 0.25, 1.0], None));
    // an unknown mode is refused
    let mut e = vrm0_ext();
    e["materialProperties"][0]["floatProperties"]["_OutlineWidthMode"] = json!(3);
    assert_eq!(
        read0(e).unwrap_err(),
        VrmError::BadValue("materialProperties[0].floatProperties._OutlineWidthMode".into())
    );
    // a property list longer than the materials is refused
    let mut e = vrm0_ext();
    let p = e["materialProperties"][0].clone();
    e["materialProperties"].as_array_mut().unwrap().push(p);
    assert_eq!(
        read0(e).unwrap_err(),
        VrmError::OutOfRange("materialProperties[1]".into())
    );
}

/// Column-major 4x4 product
fn mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    std::array::from_fn(|k| (0..4).map(|i| a[i * 4 + k % 4] * b[k / 4 * 4 + i]).sum())
}

/// The column-major matrix of a node's translation, rotation and scale
fn trs(t: [f32; 3], q: [f32; 4], s: [f32; 3]) -> [f32; 16] {
    let [x, y, z, w] = q;
    let r = [
        1. - 2. * (y * y + z * z),
        2. * (x * y + z * w),
        2. * (x * z - y * w),
        2. * (x * y - z * w),
        1. - 2. * (x * x + z * z),
        2. * (y * z + x * w),
        2. * (x * z + y * w),
        2. * (y * z - x * w),
        1. - 2. * (x * x + y * y),
    ];
    [
        r[0] * s[0],
        r[1] * s[0],
        r[2] * s[0],
        0.,
        r[3] * s[1],
        r[4] * s[1],
        r[5] * s[1],
        0.,
        r[6] * s[2],
        r[7] * s[2],
        r[8] * s[2],
        0.,
        t[0],
        t[1],
        t[2],
        1.,
    ]
}

/// Every vertex of the first primitive skinned in its bind pose with the nodes as they stand
fn skinned(m: &VrmModel) -> Vec<[f32; 3]> {
    let world = |j: u32| {
        // the joints are hips (root) and head (its child)
        let n = |i: usize| {
            trs(
                m.nodes[i].translation,
                m.nodes[i].rotation,
                m.nodes[i].scale,
            )
        };
        if j == 1 {
            n(1)
        } else {
            mul(&n(1), &n(2))
        }
    };
    let skin = &m.skins[0];
    let p = &m.meshes[0][0];
    p.positions
        .iter()
        .zip(p.joints.as_ref().unwrap())
        .zip(p.weights.as_ref().unwrap())
        .map(|((v, js), ws)| {
            let mut out = [0.0_f32; 3];
            for (j, w) in js.iter().zip(ws) {
                let k = *j as usize;
                let mm = mul(&world(skin.joints[k]), &skin.inverse_bind_matrices[k]);
                for (r, o) in out.iter_mut().enumerate() {
                    *o += w * (mm[r] * v[0] + mm[4 + r] * v[1] + mm[8 + r] * v[2] + mm[12 + r]);
                }
            }
            out
        })
        .collect()
}

#[test]
fn the_model_is_turned_to_face_plus_z() {
    let (m0, m1) = (read0(vrm0_ext()).unwrap(), read1());
    for (a, b) in m0.meshes.iter().flatten().zip(m1.meshes.iter().flatten()) {
        assert_eq!(
            a.positions,
            b.positions.iter().map(|p| turn(*p)).collect::<Vec<_>>()
        );
        assert_eq!(
            a.normals,
            b.normals
                .as_ref()
                .map(|n| n.iter().map(|p| turn(*p)).collect())
        );
        for (ta, tb) in a.targets.iter().zip(&b.targets) {
            assert_eq!(*ta, tb.iter().map(|p| turn(*p)).collect::<Vec<_>>());
        }
        for (ta, tb) in a.target_normals.iter().zip(&b.target_normals) {
            assert_eq!(
                *ta,
                tb.as_ref().map(|n| n.iter().map(|p| turn(*p)).collect())
            );
        }
        // the triangles keep their order: a turn is not a mirror
        assert_eq!(a.indices, b.indices);
    }
    for (a, b) in m0.nodes.iter().zip(&m1.nodes) {
        assert_eq!(a.translation, turn(b.translation));
        let [x, y, z, w] = b.rotation;
        assert_eq!(a.rotation, [-x, y, -z, w]);
        assert_eq!(a.scale, b.scale);
    }
    // the skinned mesh is the turned skinned mesh: the inverse bind matrices turned with it
    for (a, b) in skinned(&m0).iter().zip(skinned(&m1)) {
        let want = turn(b);
        assert!(
            a.iter().zip(want).all(|(x, y)| (x - y).abs() < 1e-5),
            "{a:?} {want:?}"
        );
    }
    // and it is not the unturned one
    assert_ne!(skinned(&m0), skinned(&m1));
}

#[test]
fn a_1_0_file_is_not_turned() {
    let m = read1();
    assert!(matches!(m.document.meta, Licence::Vrm1(_)));
    let g = split_glb(TINY).unwrap();
    assert!(matches!(
        read_vrm_json(g.json).unwrap().meta,
        Licence::Vrm1(ref meta) if !meta.name.is_empty()
    ));
}

#[test]
fn malformed_0x_parts_are_refused() {
    let cases: [(&str, Value, VrmError); 6] = [
        (
            "/humanoid/humanBones",
            json!({"hips": 1}),
            VrmError::BadValue("humanoid.humanBones".into()),
        ),
        (
            "/humanoid/humanBones/0",
            json!({"node": 1}),
            VrmError::Missing("humanoid.humanBones[0].bone".into()),
        ),
        (
            "/blendShapeMaster/blendShapeGroups/1/binds/0",
            json!({"mesh": 0, "index": 0}),
            VrmError::Missing("blendShapeMaster.blendShapeGroups[1].binds.weight".into()),
        ),
        (
            "/blendShapeMaster/blendShapeGroups/0/isBinary",
            json!("yes"),
            VrmError::BadValue("blendShapeMaster.blendShapeGroups[0].isBinary".into()),
        ),
        (
            "/firstPerson/firstPersonBoneOffset/x",
            json!("left"),
            VrmError::BadValue("firstPerson.firstPersonBoneOffset.x".into()),
        ),
        (
            "/firstPerson/lookAtHorizontalInner/xRange",
            json!(-1),
            VrmError::BadValue("firstPerson.lookAtHorizontalInner.xRange".into()),
        ),
    ];
    for (at, value, want) in cases {
        let mut e = vrm0_ext();
        *e.pointer_mut(at).unwrap() = value;
        assert_eq!(read0(e).unwrap_err(), want, "{at}");
    }
    // every byte of a broken 0.x file is an error or a model, never a panic
    let (j, bin) = file0(vrm0_ext());
    let data = glb(&j.to_string(), &bin);
    for i in (0..data.len()).step_by(7) {
        let mut d = data.clone();
        d[i] ^= 0x5A;
        let _ = read_vrm(&d);
    }
}
