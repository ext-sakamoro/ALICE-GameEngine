//! A point far from the scene is still shadowed by it: the ray reaches the scene from the
//! farthest point queried.
//!
//! oracle (worked by hand): a square occluder [0, 1] x [0, 1] at z = 1 (a scene whose bounds are
//! about 1.4 across). A point at (0.5, 0.5, -1000) with the light toward +z meets the square at
//! t = 1001, far past the scene's own extent; it is shadowed (0). A point at (5.5, 0.5, -1000)
//! misses the square (x = 5.5 is outside it) and is lit (1).
#![cfg(feature = "physics")]

use alice_game_engine::vrm_occlusion::OcclusionScene;

const QUAD: [[f32; 3]; 4] = [
    [0.0, 0.0, 1.0],
    [1.0, 0.0, 1.0],
    [1.0, 1.0, 1.0],
    [0.0, 1.0, 1.0],
];
const QUAD_IDX: [u32; 6] = [0, 1, 2, 0, 2, 3];

#[test]
fn a_far_point_is_shadowed_by_the_scene_along_the_light() {
    let scene = OcclusionScene::new(&[(&QUAD, &QUAD_IDX)], 0.001).unwrap();
    let points = [[0.5f32, 0.5, -1000.0], [5.5, 0.5, -1000.0]];
    let normals = [[0.0f32, 0.0, 1.0]; 2];
    let a = scene
        .attenuation(&points, &normals, [0.0, 0.0, 1.0])
        .unwrap();
    assert_eq!(
        a,
        vec![0.0, 1.0],
        "under the square 1000 away: shadowed; beside it: lit"
    );
}
