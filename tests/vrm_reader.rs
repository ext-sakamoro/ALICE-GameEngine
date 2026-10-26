//! The VRM 1.0 reader against a file laid out by hand from the glTF 2.0 and VRMC_vrm 1.0
//! specifications (`scripts/vrm_fixture.py`, standard-library Python), and against broken
//! variants of it.
//!
//! oracle: `tests/data/vrm/expected.txt` holds the values the fixture script put into the
//! file (positions, joints, weights, indices, morph target deltas, the inverse bind matrix,
//! the full meta with its licence fields, the humanoid bones and every expression bind); the
//! reader must recover each of them. The values are not produced by this crate.

use alice_game_engine::vrm::{read_vrm, read_vrm_json, split_glb, VrmError};

const TINY: &[u8] = include_bytes!("data/vrm/tiny.vrm");
const EXPECTED: &str = include_str!("data/vrm/expected.txt");

fn rows(prefix: &str) -> Vec<Vec<String>> {
    EXPECTED
        .lines()
        .filter(|l| l.starts_with(prefix))
        .map(|l| l.split(' ').skip(1).map(str::to_owned).collect())
        .collect()
}

fn floats(v: &[String]) -> Vec<f32> {
    v.iter().map(|x| x.parse().expect("a float")).collect()
}

#[test]
fn the_glb_splits_into_its_json_and_bin_chunks() {
    let glb = split_glb(TINY).expect("splits");
    assert!(glb.json.starts_with('{'));
    let bin = glb.bin.expect("a BIN chunk");
    assert_eq!(bin.len() % 4, 0);
    assert!(!bin.is_empty());
}

#[test]
fn the_meta_has_every_licence_field() {
    let doc = read_vrm_json(split_glb(TINY).unwrap().json).expect("reads");
    let m = &doc.meta;
    let got: Vec<(String, String)> = vec![
        (
            "allowAntisocialOrHateUsage".into(),
            m.allow_antisocial_or_hate_usage.to_string(),
        ),
        (
            "allowExcessivelySexualUsage".into(),
            m.allow_excessively_sexual_usage.to_string(),
        ),
        (
            "allowExcessivelyViolentUsage".into(),
            m.allow_excessively_violent_usage.to_string(),
        ),
        (
            "allowPoliticalOrReligiousUsage".into(),
            m.allow_political_or_religious_usage.to_string(),
        ),
        (
            "allowRedistribution".into(),
            m.allow_redistribution.to_string(),
        ),
        (
            "authors".into(),
            format!(
                "[{}]",
                m.authors
                    .iter()
                    .map(|a| format!("\"{a}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ),
        (
            "avatarPermission".into(),
            format!("\"{}\"", m.avatar_permission),
        ),
        (
            "commercialUsage".into(),
            format!("\"{}\"", m.commercial_usage),
        ),
        (
            "copyrightInformation".into(),
            format!("\"{}\"", m.copyright_information.as_deref().unwrap_or("")),
        ),
        (
            "creditNotation".into(),
            format!("\"{}\"", m.credit_notation),
        ),
        ("licenseUrl".into(), format!("\"{}\"", m.license_url)),
        ("modification".into(), format!("\"{}\"", m.modification)),
        ("name".into(), format!("\"{}\"", m.name)),
        (
            "version".into(),
            format!("\"{}\"", m.version.as_deref().unwrap_or("")),
        ),
    ];
    let want: Vec<(String, String)> = EXPECTED
        .lines()
        .filter_map(|l| l.strip_prefix("meta."))
        .map(|l| {
            let (k, v) = l.split_once(' ').unwrap();
            (k.to_owned(), v.to_owned())
        })
        .collect();
    assert_eq!(
        want.len(),
        14,
        "expected.txt meta rows: nothing or too little compared"
    );
    assert_eq!(got, want);
    assert_eq!(doc.spec_version, "1.0");
}

#[test]
fn the_humanoid_bones_and_every_expression_bind() {
    let doc = read_vrm_json(split_glb(TINY).unwrap().json).expect("reads");
    let bones: Vec<String> = doc.bones.iter().map(|(b, n)| format!("{b} {n}")).collect();
    let want: Vec<String> = rows("bone ").iter().map(|r| r.join(" ")).collect();
    assert_eq!(bones, want);
    let exprs: Vec<String> = doc
        .expressions
        .iter()
        .map(|e| {
            let kind = if e.preset { "preset" } else { "custom" };
            let tail = if e.is_binary && e.binds.is_empty() {
                "binary".to_owned()
            } else {
                e.binds
                    .iter()
                    .map(|b| format!("{}:{}:{:?}", b.node, b.index, b.weight))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            format!("{kind} {} {tail}", e.name)
        })
        .collect();
    let want: Vec<String> = rows("expression ").iter().map(|r| r.join(" ")).collect();
    assert_eq!(want.len(), 4);
    assert_eq!(exprs, want);
}

#[test]
fn the_mesh_skin_and_morph_targets_are_the_written_values() {
    let model = read_vrm(TINY).expect("reads");
    let prim = &model.meshes[0][0];
    for r in rows("position ") {
        let i: usize = r[0].parse().unwrap();
        assert_eq!(prim.positions[i].to_vec(), floats(&r[1..]), "position {i}");
    }
    let joints = prim.joints.as_ref().expect("JOINTS_0");
    for r in rows("joints ") {
        let i: usize = r[0].parse().unwrap();
        let want: Vec<u16> = r[1..].iter().map(|x| x.parse().unwrap()).collect();
        assert_eq!(joints[i].to_vec(), want, "joints {i}");
    }
    let weights = prim.weights.as_ref().expect("WEIGHTS_0");
    for r in rows("weights ") {
        let i: usize = r[0].parse().unwrap();
        assert_eq!(weights[i].to_vec(), floats(&r[1..]), "weights {i}");
    }
    let idx: Vec<u32> = rows("indices ")[0]
        .iter()
        .map(|x| x.parse().unwrap())
        .collect();
    assert_eq!(prim.indices.as_deref(), Some(idx.as_slice()));
    let targets = rows("target ");
    assert_eq!(targets.len(), 6);
    for r in targets {
        let (t, i): (usize, usize) = (r[0].parse().unwrap(), r[1].parse().unwrap());
        assert_eq!(
            prim.targets[t][i].to_vec(),
            floats(&r[2..]),
            "target {t} vertex {i}"
        );
    }
    let skin = &model.skins[0];
    assert_eq!(skin.joints, vec![1, 2]);
    let ibm = rows("ibm ");
    assert_eq!(skin.inverse_bind_matrices[1].to_vec(), floats(&ibm[0][1..]));
    assert_eq!(model.nodes[0].mesh, Some(0));
    assert_eq!(model.nodes[0].skin, Some(0));
    assert_eq!(model.nodes[1].children, vec![2]);
    assert_eq!(model.nodes[2].translation, [0.0, 1.0, 0.0]);
}

fn with_u32(data: &[u8], at: usize, v: u32) -> Vec<u8> {
    let mut d = data.to_vec();
    d[at..at + 4].copy_from_slice(&v.to_le_bytes());
    d
}

/// Each broken file is refused with an error, never a panic, a silent default or an
/// allocation sized by the file
#[test]
fn broken_files_are_refused() {
    assert!(matches!(split_glb(&TINY[..11]), Err(VrmError::Truncated)));
    assert!(matches!(
        split_glb(&with_u32(TINY, 0, 0)),
        Err(VrmError::NotGlb)
    ));
    assert!(matches!(
        split_glb(&with_u32(TINY, 4, 1)),
        Err(VrmError::UnsupportedVersion(1))
    ));
    // a total length past the bytes
    assert!(matches!(
        split_glb(&with_u32(TINY, 8, u32::MAX)),
        Err(VrmError::Truncated)
    ));
    // a JSON chunk length past the bytes
    assert!(matches!(
        split_glb(&with_u32(TINY, 12, u32::MAX - 3)),
        Err(VrmError::Truncated)
    ));
    // the first chunk must be JSON
    assert!(matches!(
        split_glb(&with_u32(TINY, 16, 0x004E_4942)),
        Err(VrmError::BadChunk)
    ));
    // every prefix of the file is refused or read, never a panic
    for n in 0..TINY.len() {
        let _ = read_vrm(&TINY[..n]);
    }
    // every single byte flipped: refused or read, never a panic
    for i in 0..TINY.len() {
        let mut d = TINY.to_vec();
        d[i] ^= 0xFF;
        let _ = read_vrm(&d);
    }
}

#[test]
fn json_that_breaks_the_rules_is_refused() {
    let glb = split_glb(TINY).unwrap();
    let base = glb.json.trim_end();
    // VRM 0.x is named, not silently unread
    assert!(matches!(
        read_vrm_json(r#"{"asset":{"version":"2.0"},"extensions":{"VRM":{"meta":{}}}}"#),
        Err(VrmError::Vrm0)
    ));
    assert!(matches!(
        read_vrm_json(r#"{"asset":{"version":"2.0"}}"#),
        Err(VrmError::NotVrm)
    ));
    // licenseUrl is required by VRMC_vrm 1.0 meta
    assert!(base.contains(r#""licenseUrl":"https://vrm.dev/licenses/1.0/","#));
    let no_url = base.replace(r#""licenseUrl":"https://vrm.dev/licenses/1.0/","#, "");
    assert!(matches!(read_vrm_json(&no_url), Err(VrmError::Missing(f)) if f == "meta.licenseUrl"));
    // a licence value outside the specification's set is refused, not mapped to a default
    assert!(
        base.contains(r#""commercialUsage":"corporation""#),
        "the fixture text was found"
    );
    let odd = base.replace(
        r#""commercialUsage":"corporation""#,
        r#""commercialUsage":"anything""#,
    );
    assert!(
        matches!(read_vrm_json(&odd), Err(VrmError::BadValue(f)) if f == "meta.commercialUsage")
    );
    // absent optional licence fields take the specification's defaults: remove every licence
    // key the fixture sets to a non-default value
    let mut lean: serde_json::Value = serde_json::from_str(base).unwrap();
    let meta = lean["extensions"]["VRMC_vrm"]["meta"]
        .as_object_mut()
        .unwrap();
    for k in [
        "avatarPermission",
        "allowExcessivelyViolentUsage",
        "allowExcessivelySexualUsage",
        "commercialUsage",
        "allowPoliticalOrReligiousUsage",
        "allowAntisocialOrHateUsage",
        "creditNotation",
        "allowRedistribution",
        "modification",
    ] {
        assert!(meta.remove(k).is_some(), "{k} is in the fixture");
    }
    let m = read_vrm_json(&lean.to_string()).expect("reads").meta;
    assert_eq!(
        (
            m.avatar_permission.as_str(),
            m.commercial_usage.as_str(),
            m.credit_notation.as_str(),
            m.modification.as_str()
        ),
        ("onlyAuthor", "personalNonProfit", "required", "prohibited")
    );
    assert_eq!(
        [
            m.allow_excessively_violent_usage,
            m.allow_excessively_sexual_usage,
            m.allow_political_or_religious_usage,
            m.allow_antisocial_or_hate_usage,
            m.allow_redistribution
        ],
        [false; 5]
    );
    // an expression bind naming a morph target index that is not a number is refused
    let bad_bind = base.replace(
        r#"{"index":0,"node":0,"weight":1.0}"#,
        r#"{"index":"x","node":0,"weight":1.0}"#,
    );
    assert_ne!(bad_bind, base, "the fixture text was found");
    assert!(read_vrm_json(&bad_bind).is_err());
}

#[test]
fn a_strided_view_and_normalized_byte_weights_and_byte_indices() {
    let model = read_vrm(TINY).expect("reads");
    let p = &model.meshes[0][1];
    for r in rows("p2position ") {
        let i: usize = r[0].parse().unwrap();
        assert_eq!(p.positions[i].to_vec(), floats(&r[1..]), "p2 position {i}");
    }
    let w = p.weights.as_ref().expect("WEIGHTS_0");
    for r in rows("p2weights ") {
        let i: usize = r[0].parse().unwrap();
        assert_eq!(w[i].to_vec(), floats(&r[1..]), "p2 weights {i}");
    }
    let idx: Vec<u32> = rows("p2indices ")[0]
        .iter()
        .map(|x| x.parse().unwrap())
        .collect();
    assert_eq!(p.indices.as_deref(), Some(idx.as_slice()));
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

fn parts() -> (serde_json::Value, Vec<u8>) {
    let g = split_glb(TINY).unwrap();
    (
        serde_json::from_str(g.json).unwrap(),
        g.bin.unwrap().to_vec(),
    )
}

#[test]
fn data_that_points_outside_is_refused() {
    // a buffer view one component shorter than its accessor needs
    let (mut j, bin) = parts();
    let len = j["bufferViews"][0]["byteLength"].as_u64().unwrap();
    j["bufferViews"][0]["byteLength"] = (len - 4).into();
    assert!(matches!(
        read_vrm(&glb(&j.to_string(), &bin)),
        Err(VrmError::OutOfRange(_))
    ));
    // an index naming a vertex that does not exist
    let (j, mut bin) = parts();
    let off = j["bufferViews"][4]["byteOffset"].as_u64().unwrap() as usize;
    bin[off..off + 2].copy_from_slice(&3u16.to_le_bytes());
    assert!(matches!(
        read_vrm(&glb(&j.to_string(), &bin)),
        Err(VrmError::OutOfRange(_))
    ));
    // an expression bind naming a morph target the mesh does not have
    let (mut j, bin) = parts();
    j["extensions"]["VRMC_vrm"]["expressions"]["preset"]["aa"]["morphTargetBinds"][0]["index"] =
        2.into();
    assert!(matches!(
        read_vrm(&glb(&j.to_string(), &bin)),
        Err(VrmError::OutOfRange(_))
    ));
    // the untouched parts read (the helper builds a valid file)
    let (j, bin) = parts();
    assert!(read_vrm(&glb(&j.to_string(), &bin)).is_ok());
}

#[test]
fn a_skin_without_inverse_bind_matrices_takes_identities() {
    let (mut j, bin) = parts();
    j["skins"][0]
        .as_object_mut()
        .unwrap()
        .remove("inverseBindMatrices");
    let model = read_vrm(&glb(&j.to_string(), &bin)).expect("reads");
    let id: [f32; 16] = [
        1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
    ];
    assert_eq!(model.skins[0].inverse_bind_matrices, vec![id, id]);
}

#[test]
fn morph_target_normal_deltas_are_read_and_absent_ones_are_none() {
    let model = read_vrm(TINY).expect("reads");
    let prim = &model.meshes[0][0];
    let t0 = prim.target_normals[0]
        .as_ref()
        .expect("target 0 has normals");
    let rows = rows("target_normal ");
    assert_eq!(rows.len(), 3);
    for r in rows {
        let i: usize = r[1].parse().unwrap();
        assert_eq!(t0[i].to_vec(), floats(&r[2..]), "target 0 normal {i}");
    }
    assert!(prim.target_normals[1].is_none());
}
