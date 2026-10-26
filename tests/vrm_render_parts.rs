//! Multi-part toon drawing: several primitives, each with its own material, in one image with one
//! depth buffer.
//!
//! oracle (worked by hand): the view-projection maps world (x, y, z) to clip (x - 0.5, y - 0.5,
//! 0.5 z + 0.5), so on a 64 x 64 image pixel column c has its centre at world
//! x = (c + 0.5) / 32 - 0.5 and row r at world y = 1.5 - (r + 0.5) / 32, and a smaller z is nearer
//! (depth compare `Less`). Part FAR (red) is the triangle (0.1, 0.1), (0.9, 0.1), (0.1, 0.9) at
//! z = 0.5; part NEAR (blue) is (0.2, 0.2), (0.6, 0.2), (0.2, 0.6) at z = -0.5. Both face the light
//! (normal and light (0, 0, 1)) with toony 1 and shift 0, so a lit fragment is exactly its base
//! colour.
//! - pixel (25, 38): world (0.297, 0.297), inside both (x + y <= 0.8) -> NEAR's blue, in either
//!   order of the parts;
//! - pixel (38, 43): world (0.703, 0.141), inside FAR (x + y <= 1, y >= 0.1) but not NEAR
//!   (y < 0.2) -> red;
//! - pixel (60, 4): world (1.39, 1.36), inside neither -> the clear colour.

#[cfg(feature = "gpu")]
mod gpu {
    use alice_game_engine::vrm_render::{RenderError, ToonParams, ToonPart, ToonRenderer};

    const VIEW_PROJ: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
    ];
    const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
    const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
    const CLEAR: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
    const UP: [[f32; 3]; 3] = [[0.0, 0.0, 1.0]; 3];
    const FAR: [[f32; 3]; 3] = [[0.1, 0.1, 0.5], [0.9, 0.1, 0.5], [0.1, 0.9, 0.5]];
    const NEAR: [[f32; 3]; 3] = [[0.2, 0.2, -0.5], [0.6, 0.2, -0.5], [0.2, 0.6, -0.5]];
    const TRI: [u32; 3] = [0, 1, 2];

    fn params(color: [f32; 4]) -> ToonParams {
        ToonParams {
            view_proj: VIEW_PROJ,
            base_color: color,
            shade_color: color,
            light_dir: [0.0, 0.0, 1.0],
            shading_shift: 0.0,
            shading_toony: 1.0,
            clear: [0.5; 4],
        }
    }

    fn part<'a>(positions: &'a [[f32; 3]], color: [f32; 4]) -> ToonPart<'a> {
        ToonPart {
            positions,
            normals: &UP,
            indices: &TRI,
            params: params(color),
            paint: None,
        }
    }

    fn renderer() -> ToonRenderer {
        ToonRenderer::new_headless(64, 64).expect("a GPU")
    }

    fn pixel(img: &[u8], col: usize, row: usize) -> [u8; 4] {
        let i = (row * 64 + col) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_nearer_part_hides_the_farther_whatever_their_order() {
        let r = renderer();
        let far_first = r
            .draw_parts(&[part(&FAR, RED), part(&NEAR, BLUE)], CLEAR)
            .unwrap();
        let near_first = r
            .draw_parts(&[part(&NEAR, BLUE), part(&FAR, RED)], CLEAR)
            .unwrap();
        for img in [&far_first, &near_first] {
            assert_eq!(pixel(img, 25, 38), [0, 0, 255, 255]);
            assert_eq!(pixel(img, 38, 43), [255, 0, 0, 255]);
            assert_eq!(pixel(img, 60, 4), [0, 255, 0, 255]);
        }
        assert_eq!(far_first, near_first);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn one_part_draws_the_same_bytes_as_draw() {
        let r = renderer();
        let p = ToonParams {
            clear: CLEAR,
            ..params(RED)
        };
        let single = r.draw(&FAR, &UP, &TRI, &p).unwrap();
        let parts = r.draw_parts(&[part(&FAR, RED)], CLEAR).unwrap();
        assert_eq!(single, parts);
        assert_eq!(pixel(&parts, 38, 43), [255, 0, 0, 255]);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn no_part_or_a_bad_part_is_refused_and_named() {
        let r = renderer();
        assert!(matches!(
            r.draw_parts(&[], CLEAR),
            Err(RenderError::BadMesh(ref m)) if m == "no part to draw"
        ));
        let bad = ToonPart {
            indices: &[0, 1, 7],
            ..part(&NEAR, BLUE)
        };
        assert!(matches!(
            r.draw_parts(&[part(&FAR, RED), bad], CLEAR),
            Err(RenderError::BadMesh(ref m)) if m.starts_with("part 1: ")
        ));
    }
}
