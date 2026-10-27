//! The alpha mask term: a separate mask image whose red channel sets or changes a part's alpha.
//!
//! oracle (worked by hand, the projection of `tests/vrm_terms.rs`): world (x, y, z) maps to clip
//! (x - 0.5, y - 0.5, 0.5 z + 0.5) on a 64 x 64 image, so pixel column c has its centre at world
//! x = (c + 0.5) / 32 - 0.5 and row 43 at world y = 1.5 - 43.5 / 32 = 0.140625. The triangle
//! (0.1, 0.1), (0.9, 0.1), (0.1, 0.9) carries UV = (x, y), so a fragment reads the mask at u = x.
//! The mask is 16 x 1 in 8 bands of 2 equal texels; a sample at u lies between texels
//! floor(16 u - 1/2) and the next, so where both are in one band the filtered value is that band's
//! value exactly. The part is white (base = shade = 1, no textures), blended over black: in the
//! image's encoding the pixel is `255 * alpha`, alpha computed below in f64 from the mask byte:
//! `k = clamp(r * scale + value, 0, 1)` with r the byte / 255, decoded from sRGB first when the
//! mask is sRGB-encoded; replace: k, multiply: a k, add: clamp(a + k), subtract: clamp(a - k),
//! a the base colour's alpha.

use alice_game_engine::vrm_render::{
    check_terms, AlphaMask, AlphaMaskMode, RenderError, ToonTerms, ToonTexture,
};

const BANDS: [u8; 8] = [0, 32, 64, 96, 128, 160, 192, 255];

fn to_linear(c: f64) -> f64 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn mask_bytes() -> Vec<u8> {
    BANDS
        .iter()
        .flat_map(|&b| [b, 0, 0, 255, b, 0, 0, 255])
        .collect()
}

fn alpha_of(base_alpha: f64, byte: u8, m: &AlphaMask<'_>) -> f64 {
    let r = f64::from(byte) / 255.0;
    let r = if m.srgb { to_linear(r) } else { r };
    let k = (r * f64::from(m.scale) + f64::from(m.value)).clamp(0.0, 1.0);
    match m.mode {
        AlphaMaskMode::Replace => k,
        AlphaMaskMode::Multiply => base_alpha * k,
        AlphaMaskMode::Add => (base_alpha + k).clamp(0.0, 1.0),
        AlphaMaskMode::Subtract => (base_alpha - k).clamp(0.0, 1.0),
    }
}

#[test]
fn bad_alpha_masks_are_refused() {
    let px: &'static [u8] = Box::leak(mask_bytes().into_boxed_slice());
    let good = AlphaMask {
        texture: ToonTexture {
            width: 16,
            height: 1,
            rgba: px,
            key: None,
        },
        mode: AlphaMaskMode::Replace,
        scale: 1.0,
        value: -0.01,
        srgb: true,
    };
    let t = |m: AlphaMask<'static>| ToonTerms {
        alpha_mask: Some(m),
        ..ToonTerms::default()
    };
    assert_eq!(check_terms(&t(good), true), Ok(()));
    // the mask is read at the surface's UVs
    assert!(matches!(
        check_terms(&t(good), false),
        Err(RenderError::BadTerms(_))
    ));
    for (what, m) in [
        (
            "scale nan",
            AlphaMask {
                scale: f32::NAN,
                ..good
            },
        ),
        (
            "value inf",
            AlphaMask {
                value: f32::INFINITY,
                ..good
            },
        ),
        (
            "bytes",
            AlphaMask {
                texture: ToonTexture {
                    width: 17,
                    ..good.texture
                },
                ..good
            },
        ),
        (
            "size 0",
            AlphaMask {
                texture: ToonTexture {
                    width: 0,
                    rgba: &[],
                    ..good.texture
                },
                ..good
            },
        ),
    ] {
        assert!(
            matches!(check_terms(&t(m), true), Err(RenderError::BadTerms(_))),
            "{what}"
        );
    }
}

#[cfg(feature = "gpu")]
mod gpu {
    use super::{alpha_of, mask_bytes, BANDS};
    use alice_game_engine::vrm_render::{
        AlphaMask, AlphaMaskMode, AlphaMode, Surface, ToonParams, ToonPart, ToonRenderer,
        ToonTerms, ToonTexture, UvTransform,
    };

    const VIEW_PROJ: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
    ];
    const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    const TRI: [[f32; 3]; 3] = [[0.1, 0.1, 0.0], [0.9, 0.1, 0.0], [0.1, 0.9, 0.0]];
    const UVS: [[f32; 2]; 3] = [[0.1, 0.1], [0.9, 0.1], [0.1, 0.9]];
    const ROW: usize = 43;

    fn part(base_alpha: f32, alpha: AlphaMode) -> ToonPart<'static> {
        ToonPart {
            positions: &TRI,
            normals: &[[0.0, 0.0, 1.0]; 3],
            indices: &[0, 1, 2],
            params: ToonParams {
                view_proj: VIEW_PROJ,
                base_color: [1.0, 1.0, 1.0, base_alpha],
                shade_color: [1.0, 1.0, 1.0, base_alpha],
                light_dir: [0.0, 0.0, 1.0],
                shading_shift: 0.0,
                shading_toony: 1.0,
                clear: BLACK,
            },
            paint: None,
            outline: None,
            surface: Some(Surface {
                uvs: &UVS,
                base: None,
                shade: None,
                transform: UvTransform::IDENTITY,
                alpha,
            }),
        }
    }

    /// The columns of row 43 inside the triangle whose sample falls between two texels of one
    /// band, with that band's mask byte
    fn columns() -> Vec<(usize, u8)> {
        (0..64)
            .filter_map(|c| {
                let x = (c as f64 + 0.5) / 32.0 - 0.5;
                let y = 0.140_625;
                if x <= 0.1 + 1e-9 || x + y >= 1.0 - 1e-9 {
                    return None;
                }
                let lo = (16.0 * x - 0.5).floor();
                if lo < 0.0 || lo + 1.0 > 15.0 {
                    return None;
                }
                let (a, b) = (lo as usize / 2, (lo as usize + 1) / 2);
                (a == b).then_some((c, BANDS[a]))
            })
            .collect()
    }

    fn grey(img: &[u8], col: usize) -> u8 {
        let i = (ROW * 64 + col) * 4;
        assert!(
            img[i] == img[i + 1] && img[i + 1] == img[i + 2],
            "column {col}: {:?}",
            &img[i..i + 4]
        );
        img[i]
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_mask_sets_the_alpha_by_its_closed_form_in_every_mode() {
        let r = ToonRenderer::new_headless(64, 64).expect("a GPU");
        let px = mask_bytes();
        let cols = columns();
        // every band but the edge ones is hit, and more than one column per band
        let bands: std::collections::BTreeSet<u8> = cols.iter().map(|c| c.1).collect();
        assert!(bands.len() >= 6, "{cols:?}");
        let mask = |mode, scale, value, srgb| AlphaMask {
            texture: ToonTexture {
                width: 16,
                height: 1,
                rgba: &px,
                key: None,
            },
            mode,
            scale,
            value,
            srgb,
        };
        let mut checked = 0;
        for (base_alpha, m) in [
            // the case a character's face overlay uses: replace, sRGB-encoded, a small negative value
            (1.0, mask(AlphaMaskMode::Replace, 1.0, -0.01, true)),
            (1.0, mask(AlphaMaskMode::Replace, 1.0, 0.0, false)),
            (0.5, mask(AlphaMaskMode::Multiply, 1.0, 0.0, true)),
            (0.25, mask(AlphaMaskMode::Add, 0.5, 0.0, false)),
            (0.75, mask(AlphaMaskMode::Subtract, 2.0, -0.25, false)),
        ] {
            let t = ToonTerms {
                alpha_mask: Some(m),
                base_level_only: true,
                ..ToonTerms::default()
            };
            let img = r
                .draw_parts_terms(&[part(base_alpha, AlphaMode::Blend)], &[t], BLACK)
                .unwrap();
            for &(c, byte) in &cols {
                let want = alpha_of(f64::from(base_alpha), byte, &m) * 255.0;
                let got = f64::from(grey(&img, c));
                assert!(
                    (got - want).abs() <= 1.0,
                    "{:?} base alpha {base_alpha} column {c} byte {byte}: got {got} want {want:.2}",
                    m.mode
                );
                checked += 1;
            }
        }
        assert!(checked >= 30, "{checked} pixels compared");
    }

    // oracle: a mask cutout uses the alpha after the mask: drawn white where k >= 1/2, the clear
    // colour (black) elsewhere
    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_cutout_discards_by_the_masked_alpha() {
        let r = ToonRenderer::new_headless(64, 64).expect("a GPU");
        let px = mask_bytes();
        let m = AlphaMask {
            texture: ToonTexture {
                width: 16,
                height: 1,
                rgba: &px,
                key: None,
            },
            mode: AlphaMaskMode::Replace,
            scale: 1.0,
            value: 0.0,
            srgb: false,
        };
        let t = ToonTerms {
            alpha_mask: Some(m),
            base_level_only: true,
            ..ToonTerms::default()
        };
        let img = r
            .draw_parts_terms(&[part(1.0, AlphaMode::Mask(0.5))], &[t], BLACK)
            .unwrap();
        let cols = columns();
        let (mut on, mut off) = (0, 0);
        for &(c, byte) in &cols {
            let k = alpha_of(1.0, byte, &m);
            let want = if k >= 0.5 { 255 } else { 0 };
            assert_eq!(grey(&img, c), want, "column {c} byte {byte}");
            if want == 255 {
                on += 1
            } else {
                off += 1
            }
        }
        assert!(
            on > 0 && off > 0,
            "both sides of the cutoff: {on} on {off} off"
        );
        // without the mask the part is solid white everywhere it is sampled
        let img = r
            .draw_parts_terms(
                &[part(1.0, AlphaMode::Mask(0.5))],
                &[ToonTerms::default()],
                BLACK,
            )
            .unwrap();
        assert!(cols.iter().all(|&(c, _)| grey(&img, c) == 255));
    }
}
