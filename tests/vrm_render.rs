//! The toon renderer of posed VRM primitives.
//!
//! oracle: coverage written out by hand. The camera maps world x and y in [-0.5, 1.5] to the
//! whole 64 x 64 image (an orthographic projection), so pixel (col, row) has its centre at
//! x = -0.5 + (col + 0.5) / 32 and y = 1.5 - (row + 0.5) / 32. The fixture's first triangle is
//! (0,0), (1,0), (0,1): a pixel centre is inside it when x >= 0, y >= 0 and x + y <= 1.
//! With the expression `aa` its third vertex is (0, 0.75), so inside means x + y / 0.75 <= 1.
//! The pixels chosen are at least 2 pixel widths from every edge.

use alice_game_engine::vrm_render::{check_mesh, toon_wgsl, RenderError};

#[test]
fn the_toon_shader_validates() {
    let module = naga::front::wgsl::parse_str(toon_wgsl()).expect("parses");
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .expect("validates");
    for entry in ["vs_main", "fs_main"] {
        assert!(
            module.entry_points.iter().any(|e| e.name == entry),
            "{entry}"
        );
    }
}

#[test]
fn a_mesh_that_does_not_describe_triangles_is_refused() {
    let p = [[0.0f32; 3]; 3];
    let n = [[0.0, 0.0, 1.0f32]; 3];
    assert!(check_mesh(&p, &n, &[0, 1, 2]).is_ok());
    for (pos, nor, idx) in [
        (&p[..], &n[..2], &[0u32, 1, 2][..]),
        (&p[..], &n[..], &[0, 1][..]),
        (&p[..], &n[..], &[][..]),
        (&p[..], &n[..], &[0, 1, 3][..]),
    ] {
        assert!(matches!(
            check_mesh(pos, nor, idx),
            Err(RenderError::BadMesh(_))
        ));
    }
    let mut bad = p;
    bad[1][0] = f32::NAN;
    assert!(check_mesh(&bad, &n, &[0, 1, 2]).is_err());
}

#[cfg(feature = "gpu")]
mod gpu {
    use alice_game_engine::vrm::read_vrm;
    use alice_game_engine::vrm_pose::VrmPose;
    use alice_game_engine::vrm_render::{ToonParams, ToonRenderer};

    const TINY: &[u8] = include_bytes!("data/vrm/tiny.vrm");
    const RED: [u8; 4] = [255, 0, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const BLACK: [u8; 4] = [0, 0, 0, 255];

    /// world x, y in [-0.5, 1.5] to clip [-1, 1]: x' = x - 0.5, y' = y - 0.5 (column-major)
    const VIEW_PROJ: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
    ];

    fn params(light: [f32; 3]) -> ToonParams {
        ToonParams {
            view_proj: VIEW_PROJ,
            base_color: [1.0, 0.0, 0.0, 1.0],
            shade_color: [0.0, 0.0, 1.0, 1.0],
            light_dir: light,
            shading_shift: 0.0,
            shading_toony: 1.0,
            clear: [0.0, 0.0, 0.0, 1.0],
        }
    }

    fn px(img: &[u8], col: usize, row: usize) -> [u8; 4] {
        let i = (row * 64 + col) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    fn draw(pose: &VrmPose<'_>, light: [f32; 3]) -> Vec<u8> {
        let model = read_vrm(TINY).unwrap();
        let prim = &model.meshes[0][0];
        let pos = pose.posed_positions(0, 0).unwrap();
        let r = ToonRenderer::new_headless(64, 64).expect("a GPU");
        r.draw(
            &pos,
            prim.normals.as_ref().unwrap(),
            prim.indices.as_ref().unwrap(),
            &params(light),
        )
        .expect("draws")
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_triangle_covers_the_pixels_inside_it_and_lights_by_the_normal() {
        let model = read_vrm(TINY).unwrap();
        let pose = VrmPose::new(&model).unwrap();
        let img = draw(&pose, [0.0, 0.0, 1.0]);
        // centre of (26, 37) is (0.328, 0.328): inside, normal (0,0,1) faces the light -> lit
        assert_eq!(px(&img, 26, 37), RED);
        // centre of (40, 23) is (0.766, 0.766): x + y > 1, outside
        assert_eq!(px(&img, 40, 23), BLACK);
        // centre of (17, 20) is (0.047, 0.859): inside at rest
        assert_eq!(px(&img, 17, 20), RED);
        // the light behind the triangle: the same pixel is in shade
        assert_eq!(px(&draw(&pose, [0.0, 0.0, -1.0]), 26, 37), BLUE);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn an_expression_changes_what_the_image_covers() {
        let model = read_vrm(TINY).unwrap();
        let mut pose = VrmPose::new(&model).unwrap();
        pose.set_expression("aa", 1.0).unwrap();
        let img = draw(&pose, [0.0, 0.0, 1.0]);
        // (0.047, 0.859): 0.047 + 0.859 / 0.75 > 1, now outside
        assert_eq!(px(&img, 17, 20), BLACK);
        // the centroid of the lowered triangle (0.333, 0.25) is still inside: pixel (26, 39)
        // has centre (0.328, 0.266)
        assert_eq!(px(&img, 26, 39), RED);
    }
}
