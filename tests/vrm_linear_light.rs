//! Shading in linear light: the colours and textures decoded from sRGB, the toon mix and the rim
//! in linear light, the result encoded once.
//!
//! oracle (worked by hand, the projection of `tests/vrm_terms.rs`): pixel (25, 38) is inside the
//! triangle. With the light along the normal, `dot(n, l) = 1`; a shift of -1 makes `s = 0`, and
//! toony 0 ramps over `s + 1` in 0..2, so the lit factor is exactly 1/2. In linear light the pixel
//! is `encode(mix(decode(shade) * decode(shade texel), decode(base) * decode(base texel), 1/2))`;
//! without the term it is `mix(shade * texel, base * texel, 1/2)` as before. A rim with the normal
//! tilted to `(sqrt(3)/2, 0, 1/2)` and the light along it is fully lit (t = 1) with rim weight 1/2
//! (see `tests/vrm_terms.rs`); with albedo mix 1 the rim is white times the base, so the pixel
//! is `encode(1.5 decode(base))` in both spaces. A base colour of 0.8 on the textured part
//! separates `decode(colour) * decode(texel)` from `decode(colour * texel)`. Expected values are
//! computed below in f64, independently of the shader.

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

/// The linear-light mix at lit factor 1/2 of colours given in sRGB (colour times texel), as bytes
fn linear_mix(base: f64, shade: f64, texel: f64) -> f64 {
    let b = to_linear(base) * to_linear(texel);
    let s = to_linear(shade) * to_linear(texel);
    to_srgb(0.5 * (b + s)) * 255.0
}

#[test]
fn the_oracle_separates_the_two_spaces() {
    // the gamma mix of 0.2 and 0.8 is 127.5; the linear one is far from it
    let lin = linear_mix(0.8, 0.2, 1.0);
    assert!((lin - 127.5).abs() > 20.0, "{lin}");
}

#[cfg(feature = "gpu")]
mod gpu {
    use super::{linear_mix, to_linear, to_srgb};
    use alice_game_engine::vrm_render::{
        AlphaMode, Rim, Surface, ToonParams, ToonPart, ToonRenderer, ToonTerms, ToonTexture,
        UvTransform,
    };

    const VIEW_PROJ: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
    ];
    const CLEAR: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
    const TRI: [[f32; 3]; 3] = [[0.1, 0.1, 0.0], [0.9, 0.1, 0.0], [0.1, 0.9, 0.0]];
    const UVS: [[f32; 2]; 3] = [[0.1, 0.1], [0.9, 0.1], [0.1, 0.9]];

    fn part<'a>(
        normal: [f32; 3],
        base: f32,
        shade: f32,
        shift: f32,
        toony: f32,
        tex: Option<ToonTexture<'a>>,
    ) -> ToonPart<'a> {
        ToonPart {
            positions: &TRI,
            normals: if normal[2] > 0.99 {
                &[[0.0, 0.0, 1.0]; 3]
            } else {
                &TILTED
            },
            indices: &[0, 1, 2],
            params: ToonParams {
                view_proj: VIEW_PROJ,
                base_color: [base, base, base, 1.0],
                shade_color: [shade, shade, shade, 1.0],
                light_dir: normal,
                shading_shift: shift,
                shading_toony: toony,
                clear: CLEAR,
            },
            paint: None,
            outline: None,
            surface: Some(Surface {
                uvs: &UVS,
                base: tex,
                shade: tex,
                transform: UvTransform::IDENTITY,
                alpha: AlphaMode::Opaque,
            }),
        }
    }

    const TILTED: [[f32; 3]; 3] = [[0.866_025_4, 0.0, 0.5]; 3];

    fn rgb(img: &[u8]) -> [f64; 3] {
        let i = (38 * 64 + 25) * 4;
        [
            f64::from(img[i]),
            f64::from(img[i + 1]),
            f64::from(img[i + 2]),
        ]
    }

    fn close(got: [f64; 3], want: [f64; 3], what: &str) {
        for c in 0..3 {
            assert!(
                (got[c] - want[c]).abs() <= 1.0,
                "{what}: got {got:?} want {want:?}"
            );
        }
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_toon_mix_is_in_linear_light_when_asked() {
        let r = ToonRenderer::new_headless(64, 64).expect("a GPU");
        let up = [0.0, 0.0, 1.0];
        let lin = ToonTerms {
            linear_light: true,
            ..ToonTerms::default()
        };
        // flat colours: the lit factor is 1/2
        let img = r
            .draw_parts_terms(&[part(up, 0.8, 0.2, -1.0, 0.0, None)], &[lin], CLEAR)
            .unwrap();
        let w = linear_mix(0.8, 0.2, 1.0);
        close(rgb(&img), [w; 3], "flat, linear");
        // without the term: the mix of the encoded values, as before
        let img = r
            .draw_parts_terms(
                &[part(up, 0.8, 0.2, -1.0, 0.0, None)],
                &[ToonTerms::default()],
                CLEAR,
            )
            .unwrap();
        close(rgb(&img), [127.5; 3], "flat, default");
        // textured: colour times texel, each decoded
        let px = [200u8, 100, 50, 255];
        let tex = ToonTexture {
            width: 1,
            height: 1,
            rgba: &px,
            key: None,
        };
        let img = r
            .draw_parts_terms(&[part(up, 0.8, 0.5, -1.0, 0.0, Some(tex))], &[lin], CLEAR)
            .unwrap();
        let want = [0, 1, 2].map(|c| linear_mix(0.8, 0.5, f64::from(px[c]) / 255.0));
        close(rgb(&img), want, "textured, linear");
        let img = r
            .draw_parts_terms(
                &[part(up, 0.8, 0.5, -1.0, 0.0, Some(tex))],
                &[ToonTerms::default()],
                CLEAR,
            )
            .unwrap();
        let want = [0, 1, 2].map(|c| 0.65 * f64::from(px[c]));
        close(rgb(&img), want, "textured, default");
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_rim_is_added_once_in_linear_light() {
        let r = ToonRenderer::new_headless(64, 64).expect("a GPU");
        let rim = Rim {
            color: [1.0, 1.0, 1.0, 1.0],
            fresnel_power: 1.0,
            border: 0.5,
            blur: 0.2,
            antialias: 0.0,
            shadow_mask: 0.0,
            albedo_mix: 1.0,
            skip_back_faces: true,
            mask: None,
        };
        let grey = 0.2f32;
        let want = (to_srgb(1.5 * to_linear(f64::from(grey))) * 255.0).round();
        for linear_light in [false, true] {
            let t = ToonTerms {
                rim: Some(rim),
                linear_light,
                ..ToonTerms::default()
            };
            let img = r
                .draw_parts_terms(&[part(TILTED[0], grey, 0.0, 0.0, 1.0, None)], &[t], CLEAR)
                .unwrap();
            close(rgb(&img), [want; 3], &format!("rim, linear {linear_light}"));
        }
    }
}
