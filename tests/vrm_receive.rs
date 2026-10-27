//! The received light: a per-vertex attenuation (shadow from the scene's shape, see
//! `vrm_occlusion`) scales the half-Lambert term before the toon ramp.
//!
//! oracle (worked by hand, the projection of `tests/vrm_terms.rs`, pixel (25, 38) inside the
//! triangle): the part is white where lit and black in shade, so the pixel is `255 t`. With the
//! normal +z and the light tilted so `dot(n, l) = d`, the half-Lambert is `h = (d + 1) / 2`; the
//! attenuation a gives `h a`, back to `d' = 2 h a - 1`, and the ramp of shift 0 and toony 0.9 is
//! `t = clamp((d' + 0.1) / 0.2, 0, 1)` (a half-Lambert border of 1/2, blur 1/10). The attenuation
//! is the same on the three vertices, so it is constant over the triangle. With a = 1 the image is
//! the image without the term, bit for bit.

use alice_game_engine::vrm_render::{check_terms, RenderError, ToonTerms};

fn lit(d: f64, a: f64) -> f64 {
    let h = ((d + 1.0) / 2.0).clamp(0.0, 1.0) * a;
    ((2.0 * h - 1.0 + 0.1) / 0.2).clamp(0.0, 1.0)
}

#[test]
fn the_oracle_spans_lit_ramp_and_shade() {
    assert!((lit(0.5, 0.8) - 1.0).abs() < 1e-12);
    assert!((lit(0.5, 0.68) - 0.6).abs() < 1e-9);
    assert!(lit(0.5, 0.6).abs() < 1e-12);
}

#[test]
fn bad_receive_is_refused() {
    let ok: &'static [f32] = &[1.0, 0.5, 0.0];
    let t = |r: &'static [f32]| ToonTerms {
        receive: Some(r),
        ..ToonTerms::default()
    };
    assert_eq!(check_terms(&t(ok), false), Ok(()));
    for (what, r) in [
        ("above 1", &[1.5f32, 1.0, 1.0][..]),
        ("negative", &[0.5, -0.1, 1.0][..]),
        ("nan", &[f32::NAN, 1.0, 1.0][..]),
    ] {
        assert!(
            matches!(check_terms(&t(r), false), Err(RenderError::BadTerms(_))),
            "{what}"
        );
    }
}

#[cfg(feature = "gpu")]
mod gpu {
    use super::lit;
    use alice_game_engine::vrm_render::{
        RenderError, ToonParams, ToonPart, ToonRenderer, ToonTerms,
    };

    const VIEW_PROJ: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
    ];
    const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    const TRI: [[f32; 3]; 3] = [[0.1, 0.1, 0.0], [0.9, 0.1, 0.0], [0.1, 0.9, 0.0]];

    fn part(d: f32) -> ToonPart<'static> {
        let s = (1.0 - d * d).sqrt();
        ToonPart {
            positions: &TRI,
            normals: &[[0.0, 0.0, 1.0]; 3],
            indices: &[0, 1, 2],
            params: ToonParams {
                view_proj: VIEW_PROJ,
                base_color: [1.0; 4],
                shade_color: [0.0, 0.0, 0.0, 1.0],
                light_dir: [s, 0.0, d],
                shading_shift: 0.0,
                shading_toony: 0.9,
                clear: BLACK,
            },
            paint: None,
            outline: None,
            surface: None,
        }
    }

    fn px(img: &[u8]) -> f64 {
        f64::from(img[(38 * 64 + 25) * 4])
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_attenuation_scales_the_half_lambert_before_the_ramp() {
        let r = ToonRenderer::new_headless(64, 64).expect("a GPU");
        let mut n = 0;
        for (d, a) in [
            (0.5f32, 0.8f32),
            (0.5, 0.68),
            (0.5, 0.6),
            (0.9, 0.55),
            (0.2, 0.0),
            (-0.05, 1.0),
        ] {
            let rec = [a; 3];
            let t = ToonTerms {
                receive: Some(&rec),
                ..ToonTerms::default()
            };
            let img = r.draw_parts_terms(&[part(d)], &[t], BLACK).unwrap();
            let want = lit(f64::from(d), f64::from(a)) * 255.0;
            assert!(
                (px(&img) - want).abs() <= 1.5,
                "d {d} a {a}: got {} want {want:.2}",
                px(&img)
            );
            n += 1;
        }
        assert_eq!(n, 6);
        // a = 1 everywhere is the image without the term, bit for bit
        for d in [0.5f32, 0.02, -0.3] {
            let ones = [1.0f32; 3];
            let with = r
                .draw_parts_terms(
                    &[part(d)],
                    &[ToonTerms {
                        receive: Some(&ones),
                        ..ToonTerms::default()
                    }],
                    BLACK,
                )
                .unwrap();
            let without = r
                .draw_parts_terms(&[part(d)], &[ToonTerms::default()], BLACK)
                .unwrap();
            assert_eq!(with, without, "d {d}");
        }
        // one value per vertex
        let short = [1.0f32; 2];
        assert!(matches!(
            r.draw_parts_terms(
                &[part(0.5)],
                &[ToonTerms {
                    receive: Some(&short),
                    ..ToonTerms::default()
                }],
                BLACK
            ),
            Err(RenderError::BadTerms(_))
        ));
    }
}
