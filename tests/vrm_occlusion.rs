//! Light occlusion of a mesh by its own shape: a ray from each point toward the light, in fixed
//! point (`alice-physics`), shadowed when it meets any triangle of the scene.
//!
//! oracle (worked by hand): a receiver grid on the plane z = 0 (normals +z) under a square occluder
//! [0, 1] x [0, 1] at z = 1. The light is toward (-1, 0, 2) (normalised): a ray from (x, y, 0) meets
//! z = 1 at (x - 1/2, y), so the point is in shadow exactly when 0 < x - 1/2 < 1 and 0 < y < 1. The
//! grid steps by 1/8 from an offset of 1/16 + 1/64, so no point lands on an edge of that set
//! (x - 1/2 and y are never within 1/64 of 0 or 1). The receiver plane is part of the scene: the
//! bias along the normal keeps a point from shadowing itself.
#![cfg(feature = "physics")]

use alice_game_engine::vrm_occlusion::{OcclusionError, OcclusionScene};

const QUAD: [[f32; 3]; 4] = [
    [0.0, 0.0, 1.0],
    [1.0, 0.0, 1.0],
    [1.0, 1.0, 1.0],
    [0.0, 1.0, 1.0],
];
const QUAD_IDX: [u32; 6] = [0, 1, 2, 0, 2, 3];
const PLANE: [[f32; 3]; 4] = [
    [-3.0, -3.0, 0.0],
    [4.0, -3.0, 0.0],
    [4.0, 4.0, 0.0],
    [-3.0, 4.0, 0.0],
];
const LIGHT: [f32; 3] = [-1.0, 0.0, 2.0];

fn grid() -> Vec<[f32; 3]> {
    let off = 1.0 / 16.0 + 1.0 / 64.0;
    let mut v = Vec::new();
    for i in 0..32 {
        for j in 0..24 {
            v.push([
                -1.0 + off + i as f32 / 8.0,
                -1.0 + off + j as f32 / 8.0,
                0.0,
            ]);
        }
    }
    v
}

fn expected(p: [f32; 3]) -> bool {
    let (x, y) = (f64::from(p[0]) - 0.5, f64::from(p[1]));
    x > 0.0 && x < 1.0 && y > 0.0 && y < 1.0
}

fn scene() -> OcclusionScene {
    OcclusionScene::new(&[(&QUAD, &QUAD_IDX), (&PLANE, &QUAD_IDX)], 1e-4).unwrap()
}

#[test]
fn the_shadowed_points_are_exactly_those_under_the_occluder_along_the_light() {
    let pts = grid();
    let nrm = vec![[0.0, 0.0, 1.0]; pts.len()];
    let a = scene().attenuation(&pts, &nrm, LIGHT).unwrap();
    let (mut lit, mut dark) = (0, 0);
    for (p, &k) in pts.iter().zip(&a) {
        let want = if expected(*p) { 0.0 } else { 1.0 };
        assert_eq!(k, want, "point {p:?}");
        if want == 0.0 {
            dark += 1
        } else {
            lit += 1
        }
    }
    // both sides are populated: 8 x 8 grid steps fall inside the shifted square
    assert_eq!(dark, 64, "{dark} shadowed");
    assert!(lit > 300, "{lit} lit");
    // bit for bit the same on a second run (and from a scene built again)
    assert_eq!(a, scene().attenuation(&pts, &nrm, LIGHT).unwrap());
    let bits: Vec<u32> = a.iter().map(|x| x.to_bits()).collect();
    let again: Vec<u32> = scene()
        .attenuation(&pts, &nrm, LIGHT)
        .unwrap()
        .iter()
        .map(|x| x.to_bits())
        .collect();
    assert_eq!(bits, again);
    // the occluder given after the receiver: its indices are offset into the joined vertices
    let late = OcclusionScene::new(&[(&PLANE, &QUAD_IDX), (&QUAD, &QUAD_IDX)], 1e-4).unwrap();
    assert_eq!(late.attenuation(&pts, &nrm, LIGHT).unwrap(), a);
}

#[test]
fn a_point_is_not_shadowed_by_the_surface_it_lies_on() {
    // only the receiver plane in the scene: every point on it is lit
    let s = OcclusionScene::new(&[(&PLANE, &QUAD_IDX)], 1e-4).unwrap();
    let pts = grid();
    let nrm = vec![[0.0, 0.0, 1.0]; pts.len()];
    assert!(s
        .attenuation(&pts, &nrm, LIGHT)
        .unwrap()
        .iter()
        .all(|&k| k == 1.0));
    // the light below the plane: the ray leaves through the plane itself, so every point is shadowed
    assert!(s
        .attenuation(&pts, &nrm, [0.0, 0.0, -1.0])
        .unwrap()
        .iter()
        .all(|&k| k == 0.0));
}

#[test]
fn an_empty_scene_shadows_nothing_and_bad_input_is_refused() {
    let empty = OcclusionScene::new(&[], 1e-4).unwrap();
    let pts = grid();
    let nrm = vec![[0.0, 0.0, 1.0]; pts.len()];
    assert!(empty
        .attenuation(&pts, &nrm, LIGHT)
        .unwrap()
        .iter()
        .all(|&k| k == 1.0));
    assert!(empty.attenuation(&[], &[], LIGHT).unwrap().is_empty());
    let s = scene();
    for (what, r) in [
        ("lengths", s.attenuation(&pts, &nrm[1..], LIGHT)),
        ("zero light", s.attenuation(&pts, &nrm, [0.0; 3])),
        ("nan light", s.attenuation(&pts, &nrm, [f32::NAN, 0.0, 1.0])),
        (
            "nan point",
            s.attenuation(&[[f32::NAN, 0.0, 0.0]], &[[0.0, 0.0, 1.0]], LIGHT),
        ),
    ] {
        assert!(
            matches!(r, Err(OcclusionError::BadInput(_))),
            "{what}: {r:?}"
        );
    }
    for (what, r) in [
        ("index", OcclusionScene::new(&[(&QUAD, &[0, 1, 9])], 1e-4)),
        (
            "not triangles",
            OcclusionScene::new(&[(&QUAD, &[0, 1])], 1e-4),
        ),
        (
            "negative bias",
            OcclusionScene::new(&[(&QUAD, &QUAD_IDX)], -1.0),
        ),
        (
            "nan vertex",
            OcclusionScene::new(&[(&[[f32::NAN; 3]; 3], &[0, 1, 2])], 1e-4),
        ),
    ] {
        assert!(matches!(r, Err(OcclusionError::BadInput(_))), "{what}");
    }
}
