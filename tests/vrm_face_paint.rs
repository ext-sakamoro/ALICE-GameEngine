//! Face paint: 2D shapes evaluated per fragment on a toon part.
//!
//! oracle (worked by hand): the view-projection maps world (x, y, z) to clip (x - 0.5, y - 0.5,
//! 0.5 z + 0.5), so on a 64 x 64 image pixel column c has its centre at world
//! x = (c + 0.5) / 32 - 0.5 and row r at world y = 1.5 - (r + 0.5) / 32. The part is the square
//! (0.1, 0.1)-(0.9, 0.9) at z = 0 facing +z, lit along its normal with toony 1, so an unpainted
//! fragment is exactly its grey base colour. With `to_face` the identity the face coordinates are
//! the world's.
//! - ellipse centre (0.5, 0.5), radii (0.2, 0.1), red;
//! - pixel (31, 31) = (0.484, 0.516): ((0.016/0.2)^2 + (0.016/0.1)^2 = 0.03 < 1) inside -> red;
//! - pixel (37, 31) = (0.672, 0.516): 0.74 + 0.02 < 1 inside -> red;
//! - pixel (37, 27) = (0.672, 0.641): 0.74 + (0.141/0.1)^2 = 2.72 > 1 outside (and 0.17 from the
//!   vertical stroke below) -> grey;
//! - a straight stroke (0.2, 0.3)-(0.8, 0.3), half width 0.03, blue: pixel (25, 38) =
//!   (0.297, 0.297) is 0.003 from it -> blue; pixel (20, 38) = (0.141, 0.297) is 0.059 from the
//!   end (0.2, 0.3) -> grey; on its last eighth (t 7/8..1, x 0.725..0.8) pixel (40, 38) =
//!   (0.766, 0.297) is 0.003 from it -> blue; pixel (43, 38) = (0.859, 0.297) is 0.059 past the
//!   end (0.8, 0.3), outside the round end of radius 0.03 -> grey;
//! - a vertical stroke through the ellipse, x = 0.5 from y 0.35 to 0.65, half width 0.02, green,
//!   after the ellipse: pixel (31, 31) is 0.016 from it -> green (drawn later); with the order
//!   reversed -> red;
//! - a curved stroke a (0.2, 0.7), control (0.5, 0.9), c (0.8, 0.7), half width 0.02: the curve is
//!   y = 0.7 + 0.4 t (1 - t) at x = 0.2 + 0.6 t, so at pixel (31, 22) = (0.484, 0.797), t = 0.474
//!   and the curve is at y = 0.800, 0.003 away -> painted; the control point's pixel (31, 19) =
//!   (0.484, 0.891) is 0.09 from the curve -> grey (the shape is the curve, not its control polygon);
//! - `to_face` = translate (-0.1, 0, 0) moves every shape 0.1 to the +x side of the world: pixel
//!   (31, 31) is then face (0.384, 0.516), inside the ellipse but 0.116 from the vertical stroke
//!   -> red; pixel (35, 31) = world (0.609, 0.516) is face (0.509, 0.516), 0.009 from the stroke -> green;
//! - `min_depth` 0.1 above the square's face depth 0 -> nothing painted;
//! - lit from behind (light (0, 0, -1)) the part is its black shade colour and the paint stays red
//!   (paint is flat).

use alice_game_engine::vrm_render::{
    check_paint, FacePaint, PaintShape, RenderError, MAX_PAINT_SHAPES,
};

const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];
const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const MAGENTA: [f32; 4] = [1.0, 0.0, 1.0, 1.0];

const ELLIPSE: PaintShape = PaintShape::Ellipse {
    center: [0.5, 0.5],
    radii: [0.2, 0.1],
    color: RED,
};
const FLAT: PaintShape = PaintShape::Stroke {
    a: [0.2, 0.3],
    b: [0.5, 0.3],
    c: [0.8, 0.3],
    half_width: 0.03,
    color: BLUE,
};
const VERTICAL: PaintShape = PaintShape::Stroke {
    a: [0.5, 0.35],
    b: [0.5, 0.5],
    c: [0.5, 0.65],
    half_width: 0.02,
    color: GREEN,
};
const CURVE: PaintShape = PaintShape::Stroke {
    a: [0.2, 0.7],
    b: [0.5, 0.9],
    c: [0.8, 0.7],
    half_width: 0.02,
    color: MAGENTA,
};

fn paint(shapes: Vec<PaintShape>) -> FacePaint {
    FacePaint {
        to_face: IDENTITY,
        min_depth: -0.1,
        shapes,
    }
}

#[test]
fn a_paint_is_checked_before_it_is_drawn() {
    assert_eq!(
        check_paint(&paint(vec![ELLIPSE, FLAT, VERTICAL, CURVE])),
        Ok(())
    );
    let too_many = paint(vec![ELLIPSE; MAX_PAINT_SHAPES + 1]);
    assert!(
        matches!(check_paint(&too_many), Err(RenderError::BadPaint(ref m)) if m.starts_with("33 shapes"))
    );
    assert_eq!(check_paint(&paint(vec![ELLIPSE; MAX_PAINT_SHAPES])), Ok(()));
    let flat_ellipse = PaintShape::Ellipse {
        center: [0.5, 0.5],
        radii: [0.2, 0.0],
        color: RED,
    };
    assert!(
        matches!(check_paint(&paint(vec![ELLIPSE, flat_ellipse])), Err(RenderError::BadPaint(ref m)) if m.starts_with("shape 1: "))
    );
    let thin = PaintShape::Stroke {
        a: [0.0; 2],
        b: [0.5; 2],
        c: [1.0; 2],
        half_width: -0.01,
        color: RED,
    };
    assert!(
        matches!(check_paint(&paint(vec![thin])), Err(RenderError::BadPaint(ref m)) if m.starts_with("shape 0: "))
    );
    let nan = PaintShape::Ellipse {
        center: [f32::NAN, 0.5],
        radii: [0.2, 0.1],
        color: RED,
    };
    assert!(matches!(
        check_paint(&paint(vec![nan])),
        Err(RenderError::BadPaint(_))
    ));
    let mut bad_matrix = paint(vec![ELLIPSE]);
    bad_matrix.to_face[12] = f32::INFINITY;
    assert!(matches!(
        check_paint(&bad_matrix),
        Err(RenderError::BadPaint(_))
    ));
}

#[cfg(feature = "gpu")]
mod gpu {
    use super::*;
    use alice_game_engine::vrm_render::{ToonParams, ToonPart, ToonRenderer};

    const VIEW_PROJ: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
    ];
    const GREY: [u8; 4] = [128, 128, 128, 255];
    const SQUARE: [[f32; 3]; 4] = [
        [0.1, 0.1, 0.0],
        [0.9, 0.1, 0.0],
        [0.9, 0.9, 0.0],
        [0.1, 0.9, 0.0],
    ];
    const UP: [[f32; 3]; 4] = [[0.0, 0.0, 1.0]; 4];
    const QUAD: [u32; 6] = [0, 1, 2, 0, 2, 3];

    fn draw(p: Option<&FacePaint>) -> Vec<u8> {
        draw_lit(p, [0.0, 0.0, 1.0])
    }

    fn draw_lit(p: Option<&FacePaint>, light: [f32; 3]) -> Vec<u8> {
        let grey = 128.0 / 255.0;
        let part = ToonPart {
            positions: &SQUARE,
            normals: &UP,
            indices: &QUAD,
            params: ToonParams {
                view_proj: VIEW_PROJ,
                base_color: [grey, grey, grey, 1.0],
                shade_color: [0.0, 0.0, 0.0, 1.0],
                light_dir: light,
                shading_shift: 0.0,
                shading_toony: 1.0,
                clear: [0.0; 4],
            },
            paint: p,
            outline: None,
            surface: None,
        };
        ToonRenderer::new_headless(64, 64)
            .expect("a GPU")
            .draw_parts(&[part], [0.0, 0.0, 0.0, 1.0])
            .unwrap()
    }

    fn px(img: &[u8], col: usize, row: usize) -> [u8; 4] {
        let i = (row * 64 + col) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    const R: [u8; 4] = [255, 0, 0, 255];
    const G: [u8; 4] = [0, 255, 0, 255];
    const B: [u8; 4] = [0, 0, 255, 255];
    const M: [u8; 4] = [255, 0, 255, 255];

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn shapes_paint_their_insides_in_order_and_flat() {
        let img = draw(Some(&paint(vec![ELLIPSE, FLAT, VERTICAL, CURVE])));
        assert_eq!(
            px(&img, 31, 31),
            G,
            "the vertical stroke is drawn over the ellipse"
        );
        assert_eq!(px(&img, 37, 31), R);
        assert_eq!(px(&img, 37, 27), GREY);
        assert_eq!(px(&img, 25, 38), B);
        assert_eq!(px(&img, 20, 38), GREY);
        assert_eq!(px(&img, 40, 38), B, "on the stroke's last eighth");
        assert_eq!(px(&img, 43, 38), GREY, "past the stroke's end");
        assert_eq!(px(&img, 31, 22), M, "on the curve");
        assert_eq!(
            px(&img, 31, 19),
            GREY,
            "at the control point, off the curve"
        );
        let reversed = draw(Some(&paint(vec![VERTICAL, ELLIPSE])));
        assert_eq!(px(&reversed, 31, 31), R);
        // lit from behind the part is in shade (black), the paint keeps its colour
        let behind = draw_lit(Some(&paint(vec![ELLIPSE])), [0.0, 0.0, -1.0]);
        assert_eq!(px(&behind, 37, 31), R);
        assert_eq!(px(&behind, 37, 27), [0, 0, 0, 255]);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_face_matrix_and_depth_place_and_limit_the_paint() {
        let mut moved = paint(vec![ELLIPSE, VERTICAL]);
        moved.to_face[12] = -0.1;
        let img = draw(Some(&moved));
        assert_eq!(px(&img, 31, 31), R);
        assert_eq!(px(&img, 35, 31), G);
        let mut behind = paint(vec![ELLIPSE, VERTICAL]);
        behind.min_depth = 0.1;
        assert_eq!(draw(Some(&behind)), draw(None));
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn no_paint_and_an_empty_paint_draw_the_toon_part_alone() {
        let plain = draw(None);
        assert_eq!(px(&plain, 31, 31), GREY);
        assert_eq!(draw(Some(&paint(Vec::new()))), plain);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_bad_paint_is_refused_with_its_part() {
        let bad = paint(vec![ELLIPSE; MAX_PAINT_SHAPES + 1]);
        let part = |p| ToonPart {
            positions: &SQUARE,
            normals: &UP,
            indices: &QUAD,
            params: ToonParams {
                view_proj: VIEW_PROJ,
                base_color: [1.0; 4],
                shade_color: [1.0; 4],
                light_dir: [0.0, 0.0, 1.0],
                shading_shift: 0.0,
                shading_toony: 1.0,
                clear: [0.0; 4],
            },
            paint: p,
            outline: None,
            surface: None,
        };
        let r = ToonRenderer::new_headless(64, 64)
            .expect("a GPU")
            .draw_parts(&[part(None), part(Some(&bad))], [0.0; 4]);
        assert!(
            matches!(r, Err(RenderError::BadPaint(ref m)) if m.starts_with("part 1: 33 shapes"))
        );
    }
}
