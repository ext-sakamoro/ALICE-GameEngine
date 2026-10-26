//! VRM MToon materials: the reader, and the MToon 1.0 shading of the toon renderer.
//!
//! oracle: the fixture's material (base (1, 0.5, 0.25, 1), shade (0.25, 0.125, 0.5), shading
//! shift -0.25, toony 0.75) and the MToon 1.0 shading written out by hand:
//! s = dot(n, l) + shift; t = clamp((s - (-1 + toony)) / (2 - 2 toony), 0, 1) (a step at s = 0
//! when toony is 1); colour = mix(shade, base, t). With the fixture's normal (0, 0, 1):
//! light (0, 0, 1) -> s = 0.75, t = (0.75 + 0.25) / 0.5 = 2 -> 1 -> base;
//! light (0, 1, 1) / sqrt 2 -> s = 0.7071 - 0.25 = 0.4571, t = 0.7071 / 0.5 -> 1 -> base;
//! light (0, 0.8, 0.6) -> s = 0.35, t = 0.6 / 0.5 -> 1;
//! light (0, 1, 0) -> s = -0.25, t = 0 / 0.5 = 0 -> shade;
//! light (0, 0.9539, 0.3) -> s = 0.05, t = 0.3 / 0.5 = 0.6 -> 0.4 shade + 0.6 base.

use alice_game_engine::vrm::read_vrm;
use alice_game_engine::vrm_render::ToonParams;

const TINY: &[u8] = include_bytes!("data/vrm/tiny.vrm");

#[test]
fn the_material_and_its_mtoon_parameters_are_read() {
    let model = read_vrm(TINY).unwrap();
    let m = &model.materials[0];
    assert_eq!(m.name.as_deref(), Some("skin"));
    assert_eq!(m.base_color, [1.0, 0.5, 0.25, 1.0]);
    let t = m.mtoon.expect("an MToon material");
    assert_eq!(t.shade_color, [0.25, 0.125, 0.5]);
    assert_eq!(t.shading_shift, -0.25);
    assert_eq!(t.shading_toony, 0.75);
    assert_eq!(model.meshes[0][0].material, Some(0));
    assert_eq!(model.meshes[0][1].material, None);
}

#[test]
fn toon_params_take_the_material() {
    let model = read_vrm(TINY).unwrap();
    let p = ToonParams::from_material(&model.materials[0], [0.0; 16], [0.0, 0.0, 1.0], [0.0; 4]);
    assert_eq!(p.base_color, [1.0, 0.5, 0.25, 1.0]);
    assert_eq!(p.shade_color, [0.25, 0.125, 0.5, 1.0]);
    assert_eq!((p.shading_shift, p.shading_toony), (-0.25, 0.75));
}

#[test]
fn absent_mtoon_values_take_the_specification_defaults() {
    use alice_game_engine::vrm::{read_vrm_json, split_glb};
    let g = split_glb(TINY).unwrap();
    let mut j: serde_json::Value = serde_json::from_str(g.json).unwrap();
    let mt = j["materials"][0]["extensions"]["VRMC_materials_mtoon"]
        .as_object_mut()
        .unwrap();
    for k in [
        "shadeColorFactor",
        "shadingShiftFactor",
        "shadingToonyFactor",
    ] {
        assert!(mt.remove(k).is_some(), "{k}");
    }
    j["materials"][0]["pbrMetallicRoughness"]
        .as_object_mut()
        .unwrap()
        .remove("baseColorFactor");
    let doc = read_vrm_json(&j.to_string()).unwrap();
    let _ = doc;
    let mut bin = g.bin.unwrap().to_vec();
    let js = j.to_string();
    let model = {
        let mut glb = Vec::new();
        let mut jb = js.into_bytes();
        jb.resize(jb.len().div_ceil(4) * 4, b' ');
        let total = 12 + 8 + jb.len() + 8 + bin.len();
        for v in [
            0x4654_6C67u32,
            2,
            total as u32,
            jb.len() as u32,
            0x4E4F_534A,
        ] {
            glb.extend_from_slice(&v.to_le_bytes());
        }
        glb.extend_from_slice(&jb);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(&0x004E_4942u32.to_le_bytes());
        glb.append(&mut bin);
        read_vrm(&glb).unwrap()
    };
    let m = &model.materials[0];
    // glTF: baseColorFactor defaults to 1; MToon 1.0: shade 0, shift 0, toony 0.9
    assert_eq!(m.base_color, [1.0; 4]);
    let t = m.mtoon.unwrap();
    assert_eq!(
        (t.shade_color, t.shading_shift, t.shading_toony),
        ([0.0; 3], 0.0, 0.9)
    );
}

fn rebuilt(
    edit: impl FnOnce(&mut serde_json::Value),
) -> Result<alice_game_engine::vrm::VrmModel, alice_game_engine::vrm::VrmError> {
    let g = alice_game_engine::vrm::split_glb(TINY).unwrap();
    let mut j: serde_json::Value = serde_json::from_str(g.json).unwrap();
    edit(&mut j);
    let mut jb = j.to_string().into_bytes();
    jb.resize(jb.len().div_ceil(4) * 4, b' ');
    let bin = g.bin.unwrap();
    let total = 12 + 8 + jb.len() + 8 + bin.len();
    let words = [
        0x4654_6C67u32,
        2,
        total as u32,
        jb.len() as u32,
        0x4E4F_534A,
    ];
    let mut glb: Vec<u8> = words.iter().flat_map(|v| v.to_le_bytes()).collect();
    glb.extend_from_slice(&jb);
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    glb.extend_from_slice(bin);
    read_vrm(&glb)
}

#[test]
fn a_material_without_pbr_is_white_and_a_toony_outside_0_to_1_is_refused() {
    let m = rebuilt(|j| {
        assert!(j["materials"][0]
            .as_object_mut()
            .unwrap()
            .remove("pbrMetallicRoughness")
            .is_some());
    })
    .unwrap();
    assert_eq!(m.materials[0].base_color, [1.0; 4]);
    for bad in [1.5, -0.25] {
        let r = rebuilt(|j| {
            j["materials"][0]["extensions"]["VRMC_materials_mtoon"]["shadingToonyFactor"] =
                bad.into();
        });
        assert!(
            matches!(r, Err(alice_game_engine::vrm::VrmError::BadValue(ref p)) if p.ends_with("shadingToonyFactor")),
            "{bad}"
        );
    }
    // a primitive naming a material that does not exist
    let r = rebuilt(|j| j["meshes"][0]["primitives"][0]["material"] = 3.into());
    assert!(matches!(
        r,
        Err(alice_game_engine::vrm::VrmError::OutOfRange(_))
    ));
}

#[cfg(feature = "gpu")]
mod gpu {
    use super::TINY;
    use alice_game_engine::vrm::read_vrm;
    use alice_game_engine::vrm_pose::VrmPose;
    use alice_game_engine::vrm_render::{ToonParams, ToonRenderer};

    const VIEW_PROJ: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
    ];

    fn centroid_pixel(light: [f32; 3]) -> [u8; 4] {
        let model = read_vrm(TINY).unwrap();
        let pose = VrmPose::new(&model).unwrap();
        let prim = &model.meshes[0][0];
        let p =
            ToonParams::from_material(&model.materials[0], VIEW_PROJ, light, [0.0, 0.0, 0.0, 1.0]);
        let img = ToonRenderer::new_headless(64, 64)
            .expect("a GPU")
            .draw(
                &pose.posed_positions(0, 0).unwrap(),
                &pose.posed_normals(0, 0).unwrap(),
                prim.indices.as_ref().unwrap(),
                &p,
            )
            .unwrap();
        // pixel (26, 37) is inside the triangle (see tests/vrm_render.rs)
        let i = (37 * 64 + 26) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    fn near(got: [u8; 4], want: [f32; 3]) -> bool {
        got.iter()
            .zip(want)
            .all(|(&g, w)| (f32::from(g) - w * 255.0).abs() <= 1.0)
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn mtoon_shading_mixes_shade_and_base_by_the_shifted_toony_ramp() {
        // t = 1: base
        let p = centroid_pixel([0.0, 0.0, 1.0]);
        assert!(near(p, [1.0, 0.5, 0.25]), "{p:?}");
        // t = 0: shade
        let p = centroid_pixel([0.0, 1.0, 0.0]);
        assert!(near(p, [0.25, 0.125, 0.5]), "{p:?}");
        // t = 0.6: 0.4 shade + 0.6 base
        let (y, z) = (0.3f32.mul_add(-0.3, 1.0).sqrt(), 0.3f32);
        let p = centroid_pixel([0.0, y, z]);
        let want = [
            0.4 * 0.25 + 0.6,
            0.4 * 0.125 + 0.6 * 0.5,
            0.4 * 0.5 + 0.6 * 0.25,
        ];
        assert!(near(p, want), "{p:?} vs {want:?}");
    }
}
