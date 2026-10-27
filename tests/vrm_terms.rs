//! The per-part terms (back-face culling, rim light), the mip chain and the sample count.
//!
//! oracle (worked by hand, as `tests/vrm_render_parts.rs`): the view-projection maps world
//! (x, y, z) to clip (x - 0.5, y - 0.5, 0.5 z + 0.5) on a 64 x 64 image, so pixel column c has its
//! centre at world x = (c + 0.5) / 32 - 0.5 and row r at world y = 1.5 - (r + 0.5) / 32. The
//! triangle TRI = (0.1, 0.1), (0.9, 0.1), (0.1, 0.9) at z = 0 runs counter-clockwise on screen, its
//! front face; the indices reversed give its back face. Pixel (25, 38) (world (0.297, 0.297)) is
//! inside it.
//!
//! The rim: the view ray of this projection is along z, so `|n . v| = |n.z|`. With every normal
//! `(sqrt(3) / 2, 0, 1 / 2)`, `f = (1 - 1/2)^1 = 1/2`; border 1/2 and blur 1/5 ramp from 2/5 to
//! 3/5, so the rim weight is `(1/2 - 2/5) / (1/5) = 1/2` (the normal is constant, `fwidth` is 0).
//! The light is that normal too, so with toony 1 the fragment is lit: base colour. The expected
//! value is the base colour in linear light plus half the rim colour, back to sRGB, computed
//! below in f64 independently of the shader.

use alice_game_engine::vrm_render::{
    check_terms, mip_chain, RenderError, Rim, ToonTerms, ToonTexture,
};

fn to_linear(c: f64) -> f64 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn to_srgb(c: f64) -> f64 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn rim(mask: Option<ToonTexture<'_>>) -> Rim<'_> {
    Rim {
        color: [1.0, 1.0, 1.0, 1.0],
        fresnel_power: 1.0,
        border: 0.5,
        blur: 0.2,
        antialias: 0.0,
        shadow_mask: 0.0,
        albedo_mix: 0.0,
        skip_back_faces: true,
        mask,
    }
}

// oracle: each level averages the 2 x 2 texels above it in linear light (0 and 255 average to
// linear 1/2, sRGB 0.7354 -> 188), alpha linearly (0 and 255 -> 127.5 -> 128); an odd size repeats
// its last row / column; levels run to 1 x 1
#[test]
fn the_mip_chain_averages_in_linear_light_down_to_one_texel() {
    // 2 x 2: black, white, black, white (alpha 0, 255, 0, 255)
    let img = [
        0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0, 255, 255, 255, 255,
    ];
    let (bytes, levels) = mip_chain(&img, 2, 2);
    assert_eq!(levels, 2);
    assert_eq!(&bytes[..16], &img);
    let want = (to_srgb(0.5) * 255.0).round() as u8;
    assert_eq!(want, 188);
    assert_eq!(&bytes[16..], &[188, 188, 188, 128]);
    // 5 x 3 -> 2 x 1 -> 1 x 1
    let img = vec![200u8; 5 * 3 * 4];
    let (bytes, levels) = mip_chain(&img, 5, 3);
    assert_eq!(levels, 3);
    assert_eq!(bytes.len(), (15 + 2 + 1) * 4);
    // a flat image stays flat on every level
    assert!(bytes.iter().all(|&b| b == 200), "{bytes:?}");
    // 3 x 1: texel 2 is repeated for the second texel of level 1 (texels 2 and 2)
    let img = [0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255];
    let (bytes, levels) = mip_chain(&img, 3, 1);
    assert_eq!(levels, 2);
    assert_eq!(&bytes[12..], &[0, 0, 0, 255]);
    let (bytes, _) = mip_chain(
        &[
            0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        ],
        4,
        1,
    );
    assert_eq!(&bytes[16..24], &[0, 0, 0, 255, 255, 255, 255, 255]);
}

#[test]
fn bad_terms_are_refused() {
    let ok = ToonTerms {
        cull_back: true,
        rim: Some(rim(None)),
        outline_times_base: false,
    };
    assert_eq!(check_terms(&ok, false), Ok(()));
    assert_eq!(check_terms(&ToonTerms::default(), false), Ok(()));
    let bad = |f: &dyn Fn(&mut Rim<'static>)| {
        let mut r = rim(None);
        f(&mut r);
        check_terms(
            &ToonTerms {
                rim: Some(r),
                ..ToonTerms::default()
            },
            true,
        )
    };
    for (what, e) in [
        ("power 0", bad(&|r| r.fresnel_power = 0.0)),
        ("blur", bad(&|r| r.blur = -0.1)),
        ("antialias", bad(&|r| r.antialias = -1.0)),
        ("shadow mask", bad(&|r| r.shadow_mask = 1.5)),
        ("albedo mix", bad(&|r| r.albedo_mix = -0.5)),
        ("nan", bad(&|r| r.border = f32::NAN)),
        ("inf colour", bad(&|r| r.color[1] = f32::INFINITY)),
    ] {
        assert!(matches!(e, Err(RenderError::BadTerms(_))), "{what}: {e:?}");
    }
    let px = [255u8; 4];
    let mask = ToonTexture {
        width: 1,
        height: 1,
        rgba: &px,
        key: None,
    };
    // a mask needs a surface (its UVs)
    let t = ToonTerms {
        rim: Some(rim(Some(mask))),
        ..ToonTerms::default()
    };
    assert!(matches!(
        check_terms(&t, false),
        Err(RenderError::BadTerms(_))
    ));
    assert_eq!(check_terms(&t, true), Ok(()));
    let short = ToonTexture {
        width: 2,
        height: 1,
        rgba: &px,
        key: None,
    };
    let t = ToonTerms {
        rim: Some(rim(Some(short))),
        ..ToonTerms::default()
    };
    assert!(matches!(
        check_terms(&t, true),
        Err(RenderError::BadTerms(_))
    ));
}

#[cfg(feature = "gpu")]
mod gpu {
    use super::{rim, to_linear, to_srgb};
    use alice_game_engine::vrm_render::{
        RenderError, ToonParams, ToonPart, ToonRenderer, ToonTerms,
    };

    const VIEW_PROJ: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
    ];
    const CLEAR: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
    const TRI: [[f32; 3]; 3] = [[0.1, 0.1, 0.0], [0.9, 0.1, 0.0], [0.1, 0.9, 0.0]];
    const FRONT: [u32; 3] = [0, 1, 2];
    const BACK: [u32; 3] = [0, 2, 1];
    const GREY: f32 = 0.2;

    fn tilted() -> [f32; 3] {
        [3f32.sqrt() / 2.0, 0.0, 0.5]
    }

    fn part<'a>(normals: &'a [[f32; 3]], indices: &'a [u32]) -> ToonPart<'a> {
        let n = normals[0];
        ToonPart {
            positions: &TRI,
            normals,
            indices,
            params: ToonParams {
                view_proj: VIEW_PROJ,
                base_color: [GREY, GREY, GREY, 1.0],
                shade_color: [0.0, 0.0, 0.0, 1.0],
                light_dir: n,
                shading_shift: 0.0,
                shading_toony: 1.0,
                clear: CLEAR,
            },
            paint: None,
            outline: None,
            surface: None,
        }
    }

    fn pixel(img: &[u8], col: usize, row: usize) -> [u8; 4] {
        let i = (row * 64 + col) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    fn renderer() -> ToonRenderer {
        ToonRenderer::new_headless(64, 64).expect("a GPU")
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_back_face_is_drawn_unless_culled() {
        let r = renderer();
        let n = [[0.0, 0.0, 1.0]; 3];
        let grey = (GREY * 255.0).round() as u8;
        for (idx, cull, want) in [
            (&FRONT, false, grey),
            (&FRONT, true, grey),
            (&BACK, false, grey),
        ] {
            let t = ToonTerms {
                cull_back: cull,
                ..ToonTerms::default()
            };
            let img = r.draw_parts_terms(&[part(&n, idx)], &[t], CLEAR).unwrap();
            assert_eq!(
                pixel(&img, 25, 38),
                [want, want, want, 255],
                "{idx:?} cull {cull}"
            );
        }
        let t = ToonTerms {
            cull_back: true,
            ..ToonTerms::default()
        };
        let img = r.draw_parts_terms(&[part(&n, &BACK)], &[t], CLEAR).unwrap();
        assert_eq!(
            pixel(&img, 25, 38),
            [0, 255, 0, 255],
            "a culled back face shows the clear colour"
        );
        // no terms: as before (both faces)
        let img = r.draw_parts(&[part(&n, &BACK)], CLEAR).unwrap();
        assert_eq!(pixel(&img, 25, 38), [grey, grey, grey, 255]);
        // terms not one per part
        assert!(matches!(
            r.draw_parts_terms(&[part(&n, &FRONT)], &[t, t], CLEAR),
            Err(RenderError::BadTerms(_))
        ));
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_rim_adds_its_ramp_weight_in_linear_light() {
        let r = renderer();
        let n = [tilted(); 3];
        let want = (to_srgb(to_linear(f64::from(GREY)) + 0.5) * 255.0).round();
        let t = ToonTerms {
            rim: Some(rim(None)),
            ..ToonTerms::default()
        };
        let img = r
            .draw_parts_terms(&[part(&n, &FRONT)], &[t], CLEAR)
            .unwrap();
        let got = pixel(&img, 25, 38);
        for c in &got[..3] {
            assert!((f64::from(*c) - want).abs() <= 1.0, "{got:?} want {want}");
        }
        // without the rim: the base colour alone
        let img = r
            .draw_parts_terms(&[part(&n, &FRONT)], &[ToonTerms::default()], CLEAR)
            .unwrap();
        let grey = (GREY * 255.0).round() as u8;
        assert_eq!(pixel(&img, 25, 38), [grey, grey, grey, 255]);
        // on a back face the rim skips it when asked to, and adds it otherwise
        let img = r.draw_parts_terms(&[part(&n, &BACK)], &[t], CLEAR).unwrap();
        assert_eq!(pixel(&img, 25, 38), [grey, grey, grey, 255]);
        let mut keep = rim(None);
        keep.skip_back_faces = false;
        let t = ToonTerms {
            rim: Some(keep),
            ..ToonTerms::default()
        };
        let img = r.draw_parts_terms(&[part(&n, &BACK)], &[t], CLEAR).unwrap();
        assert!((f64::from(pixel(&img, 25, 38)[0]) - want).abs() <= 1.0);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn four_samples_blend_the_edges_and_leave_the_inside() {
        let mut r = renderer();
        let n = [[0.0, 0.0, 1.0]; 3];
        assert!(matches!(r.set_samples(3), Err(RenderError::BadSamples(_))));
        assert_eq!(r.samples(), 1);
        let one = r.draw_parts(&[part(&n, &FRONT)], CLEAR).unwrap();
        r.set_samples(4).unwrap();
        assert_eq!(r.samples(), 4);
        let four = r.draw_parts(&[part(&n, &FRONT)], CLEAR).unwrap();
        let grey = (GREY * 255.0).round() as u8;
        let is_pure = |p: [u8; 4]| p == [grey, grey, grey, 255] || p == [0, 255, 0, 255];
        // one sample: every pixel is the surface or the clear colour
        assert!(one
            .chunks_exact(4)
            .all(|p| is_pure([p[0], p[1], p[2], p[3]])));
        // four: some pixels along the edges are between them
        let mixed = four
            .chunks_exact(4)
            .filter(|p| !is_pure([p[0], p[1], p[2], p[3]]))
            .count();
        assert!(mixed > 20, "{mixed} edge pixels blended");
        // the inside is unchanged
        assert_eq!(pixel(&four, 25, 38), pixel(&one, 25, 38));
        r.set_samples(1).unwrap();
        assert_eq!(r.draw_parts(&[part(&n, &FRONT)], CLEAR).unwrap(), one);
    }
}
