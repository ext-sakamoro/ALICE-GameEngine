//! Textured toon parts: base and shade textures, the UV transform, mask and blend alpha, and the
//! checks on a surface.
//!
//! oracle (worked by hand): the view-projection and triangles are those of `vrm_render_parts`
//! (FAR = (0.1, 0.1), (0.9, 0.1), (0.1, 0.9) at z = 0.5; NEAR = (0.2, 0.2), (0.6, 0.2), (0.2, 0.6)
//! at z = -0.5, pixel (25, 38) inside both, pixel (38, 43) inside FAR only, pixel (60, 4) inside
//! neither). The texture is 2 x 2, rows top first: (0, 0) red, (1, 0) green, (0, 1) blue,
//! (1, 1) white. Every vertex of a triangle carries the same UV, a texel centre, so every fragment
//! samples exactly one texel (linear filtering at a texel centre returns that texel):
//! - (0.25, 0.25) -> red, (0.75, 0.25) -> green, (0.25, 0.75) -> blue, (0.75, 0.75) -> white;
//! - transform `offset + rotation(scale * uv)` with rotation r taking (x, y) to
//!   (cos r x + sin r y, -sin r x + cos r y) (the `KHR_texture_transform` matrix, as three.js
//!   applies it): offset (0.5, 0) moves (0.25, 0.25) to green; scale (3, 1) moves it to
//!   (0.75, 0.25), green; rotation pi/2 takes (0.25, 0.75) to (0.75, -0.25), which repeats to
//!   (0.75, 0.75), white;
//! - with the light along the normal (toony 1, shift 0) a fragment is base colour x base texel;
//!   with the light opposite it is shade colour x shade texel;
//! - filtering is linear with repeat: (0.5, 0.25) is half red half green, (1.25, 1.25) repeats
//!   to red, and UVs running over many textures per pixel mix texels;
//! - mask: a texel below the cutoff is not drawn (the clear colour, or a farther part, shows),
//!   one at or above it is drawn solid;
//! - blend: a red texel of alpha 128 over green gives (128 or 127, 127 or 128, 0), as
//!   src x 128/255 + dst x 127/255, whatever the parts' order, because blended parts are drawn
//!   after the solid ones.

use alice_game_engine::vrm_render::{
    check_surface, AlphaMode, RenderError, Surface, ToonTexture, UvTransform,
};

const RGBA: [u8; 16] = [
    255, 0, 0, 255, 0, 255, 0, 255, // top row: red, green
    0, 0, 255, 255, 255, 255, 255, 255, // bottom row: blue, white
];

fn tex(rgba: &[u8]) -> ToonTexture<'_> {
    ToonTexture {
        width: 2,
        height: 2,
        rgba,
    }
}

fn surface<'a>(uvs: &'a [[f32; 2]], alpha: AlphaMode) -> Surface<'a> {
    Surface {
        uvs,
        base: Some(tex(&RGBA)),
        shade: None,
        transform: UvTransform::IDENTITY,
        alpha,
    }
}

fn refused(s: &Surface<'_>, vertices: usize) -> String {
    match check_surface(s, vertices) {
        Err(RenderError::BadSurface(m)) => m,
        other => panic!("expected BadSurface, got {other:?}"),
    }
}

#[test]
fn a_good_surface_passes() {
    let uvs = [[0.25, 0.25]; 3];
    check_surface(&surface(&uvs, AlphaMode::Opaque), 3).unwrap();
    check_surface(&surface(&uvs, AlphaMode::Mask(0.0)), 3).unwrap();
    check_surface(&surface(&uvs, AlphaMode::Mask(1.0)), 3).unwrap();
    check_surface(&surface(&uvs, AlphaMode::Blend), 3).unwrap();
    let mut s = surface(&uvs, AlphaMode::Opaque);
    s.base = None;
    check_surface(&s, 3).unwrap();
}

#[test]
fn the_uvs_must_be_one_finite_pair_per_vertex() {
    let uvs = [[0.25, 0.25]; 3];
    assert!(refused(&surface(&uvs, AlphaMode::Opaque), 4).contains("3 uvs for 4 vertices"));
    let bad = [[0.25, 0.25], [f32::NAN, 0.0], [0.0, 0.0]];
    assert!(refused(&surface(&bad, AlphaMode::Opaque), 3).contains("not finite"));
    let inf = [[0.25, 0.25], [0.0, f32::INFINITY], [0.0, 0.0]];
    assert!(refused(&surface(&inf, AlphaMode::Opaque), 3).contains("not finite"));
}

#[test]
fn a_texture_must_have_a_size_in_range_and_exactly_its_bytes() {
    let uvs = [[0.25, 0.25]; 3];
    let empty: [u8; 0] = [];
    for (w, h, bytes, base) in [
        (0, 2, &empty[..], true),
        (2, 0, &empty[..], false),
        (8193, 1, &empty[..], true),
        (1, 8193, &empty[..], false),
        (2, 2, &RGBA[..15], true),
        (2, 2, &RGBA[..15], false),
    ] {
        let t = ToonTexture {
            width: w,
            height: h,
            rgba: bytes,
        };
        let mut s = surface(&uvs, AlphaMode::Opaque);
        if base {
            s.base = Some(t);
        } else {
            s.shade = Some(t);
        }
        let m = refused(&s, 3);
        assert!(
            m.starts_with(if base {
                "base texture"
            } else {
                "shade texture"
            }),
            "{m}"
        );
    }
    // the largest size with its bytes passes
    let big = vec![0u8; 8192 * 4];
    let mut s = surface(&uvs, AlphaMode::Opaque);
    s.shade = Some(ToonTexture {
        width: 8192,
        height: 1,
        rgba: &big,
    });
    check_surface(&s, 3).unwrap();
}

#[test]
fn the_transform_must_be_finite_and_the_cutoff_in_range() {
    let uvs = [[0.25, 0.25]; 3];
    for t in [
        UvTransform {
            offset: [f32::NAN, 0.0],
            ..UvTransform::IDENTITY
        },
        UvTransform {
            offset: [0.0, f32::INFINITY],
            ..UvTransform::IDENTITY
        },
        UvTransform {
            scale: [f32::NAN, 1.0],
            ..UvTransform::IDENTITY
        },
        UvTransform {
            scale: [1.0, f32::NEG_INFINITY],
            ..UvTransform::IDENTITY
        },
        UvTransform {
            rotation: f32::NAN,
            ..UvTransform::IDENTITY
        },
    ] {
        let mut s = surface(&uvs, AlphaMode::Opaque);
        s.transform = t;
        assert!(refused(&s, 3).contains("transform"));
    }
    for c in [-0.01, 1.01, f32::NAN] {
        assert!(refused(&surface(&uvs, AlphaMode::Mask(c)), 3).contains("cutoff"));
    }
}

#[cfg(feature = "gpu")]
mod gpu {
    use super::{surface, RGBA};
    use alice_game_engine::vrm_render::{
        AlphaMode, RenderError, Surface, ToonParams, ToonPart, ToonRenderer, ToonTexture,
        UvTransform,
    };

    const VIEW_PROJ: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
    ];
    const WHITE: [f32; 4] = [1.0; 4];
    const CLEAR: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
    const UP: [[f32; 3]; 3] = [[0.0, 0.0, 1.0]; 3];
    const FAR: [[f32; 3]; 3] = [[0.1, 0.1, 0.5], [0.9, 0.1, 0.5], [0.1, 0.9, 0.5]];
    const NEAR: [[f32; 3]; 3] = [[0.2, 0.2, -0.5], [0.6, 0.2, -0.5], [0.2, 0.6, -0.5]];
    const TRI: [u32; 3] = [0, 1, 2];

    fn params(base: [f32; 4], shade: [f32; 4], light: [f32; 3]) -> ToonParams {
        ToonParams {
            view_proj: VIEW_PROJ,
            base_color: base,
            shade_color: shade,
            light_dir: light,
            shading_shift: 0.0,
            shading_toony: 1.0,
            clear: [0.5; 4],
        }
    }

    fn part<'a>(positions: &'a [[f32; 3]], s: Option<Surface<'a>>) -> ToonPart<'a> {
        ToonPart {
            positions,
            normals: &UP,
            indices: &TRI,
            params: params(WHITE, WHITE, [0.0, 0.0, 1.0]),
            paint: None,
            outline: None,
            surface: s,
        }
    }

    fn renderer() -> ToonRenderer {
        ToonRenderer::new_headless(64, 64).expect("a GPU")
    }

    fn pixel(img: &[u8], col: usize, row: usize) -> [u8; 4] {
        let i = (row * 64 + col) * 4;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    fn rgb(img: &[u8], col: usize, row: usize) -> [u8; 3] {
        let p = pixel(img, col, row);
        [p[0], p[1], p[2]]
    }

    /// FAR drawn with one constant UV and the given transform; the colour at (38, 43)
    fn far_texel(r: &ToonRenderer, uv: [f32; 2], transform: UvTransform) -> [u8; 3] {
        let uvs = [uv; 3];
        let mut s = surface(&uvs, AlphaMode::Opaque);
        s.transform = transform;
        let img = r.draw_parts(&[part(&FAR, Some(s))], CLEAR).unwrap();
        rgb(&img, 38, 43)
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_base_texture_is_sampled_at_the_uv() {
        let r = renderer();
        let id = UvTransform::IDENTITY;
        assert_eq!(far_texel(&r, [0.25, 0.25], id), [255, 0, 0]);
        assert_eq!(far_texel(&r, [0.75, 0.25], id), [0, 255, 0]);
        assert_eq!(far_texel(&r, [0.25, 0.75], id), [0, 0, 255]);
        assert_eq!(far_texel(&r, [0.75, 0.75], id), [255, 255, 255]);
        // repeat: one whole texture further along is the same texel
        assert_eq!(far_texel(&r, [1.75, -0.75], id), [0, 255, 0]);
        // repeat in both directions: (1.25, 1.25) is (0.25, 0.25), red (clamped it would be the
        // last texel, white)
        assert_eq!(far_texel(&r, [1.25, 1.25], id), [255, 0, 0]);
        assert_eq!(far_texel(&r, [-0.75, 0.25], id), [255, 0, 0]);
        assert_eq!(far_texel(&r, [0.25, -0.75], id), [255, 0, 0]);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn sampling_is_linear_when_magnified_and_minified() {
        let r = renderer();
        let id = UvTransform::IDENTITY;
        // halfway between the red and the green texel: half of each (nearest would give one)
        let got = far_texel(&r, [0.5, 0.25], id);
        assert!(
            got[0].abs_diff(128) <= 1 && got[1].abs_diff(128) <= 1 && got[2] == 0,
            "{got:?}"
        );
        // minified: the UVs run over 40 textures across the triangle, several texels per pixel;
        // linear filtering mixes neighbouring texels, so some pixels are not a pure texel colour
        // (nearest gives only the four pure colours)
        let uvs = [[0.0, 0.0], [40.0, 0.0], [0.0, 40.0]];
        let s = surface(&uvs, AlphaMode::Opaque);
        let img = r.draw_parts(&[part(&FAR, Some(s))], CLEAR).unwrap();
        let mixed = (8..30)
            .flat_map(|row| (8..30).map(move |col| (col, row + 20)))
            .filter(|&(col, row)| rgb(&img, col, row).iter().any(|&c| c != 0 && c != 255))
            .count();
        assert!(mixed > 0, "no mixed pixel");
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_texture_larger_than_the_downlevel_limit_is_drawn() {
        // 4096 texels across: past the downlevel 2048, within what the adapters in use allow
        let r = renderer();
        let red: Vec<u8> = [255u8, 0, 0, 255].repeat(4096);
        let uvs = [[0.5, 0.5]; 3];
        let s = Surface {
            base: Some(ToonTexture {
                width: 4096,
                height: 1,
                rgba: &red,
            }),
            ..surface(&uvs, AlphaMode::Opaque)
        };
        let img = r.draw_parts(&[part(&FAR, Some(s))], CLEAR).unwrap();
        assert_eq!(rgb(&img, 38, 43), [255, 0, 0]);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_texture_multiplies_the_base_colour() {
        let r = renderer();
        let uvs = [[0.75, 0.75]; 3];
        let mut p = part(&FAR, Some(surface(&uvs, AlphaMode::Opaque)));
        p.params.base_color = [1.0, 0.0, 1.0, 1.0];
        let img = r.draw_parts(&[p], CLEAR).unwrap();
        // white texel x magenta
        assert_eq!(rgb(&img, 38, 43), [255, 0, 255]);
        let uvs = [[0.25, 0.25]; 3];
        let mut p = part(&FAR, Some(surface(&uvs, AlphaMode::Opaque)));
        p.params.base_color = [0.0, 0.0, 1.0, 1.0];
        let img = r.draw_parts(&[p], CLEAR).unwrap();
        // red texel x blue = black
        assert_eq!(rgb(&img, 38, 43), [0, 0, 0]);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_uv_transform_offsets_scales_and_rotates() {
        let r = renderer();
        let off = UvTransform {
            offset: [0.5, 0.0],
            ..UvTransform::IDENTITY
        };
        assert_eq!(far_texel(&r, [0.25, 0.25], off), [0, 255, 0]);
        let scale = UvTransform {
            scale: [3.0, 1.0],
            ..UvTransform::IDENTITY
        };
        assert_eq!(far_texel(&r, [0.25, 0.25], scale), [0, 255, 0]);
        let rot = UvTransform {
            rotation: std::f32::consts::FRAC_PI_2,
            ..UvTransform::IDENTITY
        };
        // (0.25, 0.75) -> (0.75, -0.25) -> white; unrotated it is blue
        assert_eq!(far_texel(&r, [0.25, 0.75], rot), [255, 255, 255]);
        let rot_back = UvTransform {
            rotation: -std::f32::consts::FRAC_PI_2,
            ..UvTransform::IDENTITY
        };
        // (0.25, 0.75) -> (-0.75, 0.25) -> repeats to (0.25, 0.25), red
        assert_eq!(far_texel(&r, [0.25, 0.75], rot_back), [255, 0, 0]);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_shade_texture_colours_the_unlit_side() {
        let r = renderer();
        let uvs = [[0.75, 0.25]; 3];
        let shade_rgba: [u8; 16] = [
            0, 0, 255, 255, 0, 0, 255, 255, // blue everywhere
            0, 0, 255, 255, 0, 0, 255, 255,
        ];
        let s = Surface {
            shade: Some(ToonTexture {
                width: 2,
                height: 2,
                rgba: &shade_rgba,
            }),
            ..surface(&uvs, AlphaMode::Opaque)
        };
        let mut lit = part(&FAR, Some(s));
        lit.params = params(WHITE, WHITE, [0.0, 0.0, 1.0]);
        let mut unlit = part(&FAR, Some(s));
        unlit.params = params(WHITE, WHITE, [0.0, 0.0, -1.0]);
        // lit: the base texel (green); unlit: the shade texel (blue)
        assert_eq!(
            rgb(&r.draw_parts(&[lit], CLEAR).unwrap(), 38, 43),
            [0, 255, 0]
        );
        assert_eq!(
            rgb(&r.draw_parts(&[unlit], CLEAR).unwrap(), 38, 43),
            [0, 0, 255]
        );
        // no shade texture: the unlit side is the shade colour alone
        let mut plain = part(&FAR, Some(surface(&uvs, AlphaMode::Opaque)));
        plain.params = params(WHITE, [1.0, 0.0, 0.0, 1.0], [0.0, 0.0, -1.0]);
        assert_eq!(
            rgb(&r.draw_parts(&[plain], CLEAR).unwrap(), 38, 43),
            [255, 0, 0]
        );
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_mask_drops_texels_below_the_cutoff() {
        let r = renderer();
        let mut holes = RGBA;
        holes[3] = 0; // red texel transparent
        holes[15] = 200; // white texel: alpha 0.78
        let draw = |uv: [f32; 2], alpha: AlphaMode| {
            let uvs = [uv; 3];
            let s = Surface {
                base: Some(ToonTexture {
                    width: 2,
                    height: 2,
                    rgba: &holes,
                }),
                ..surface(&uvs, alpha)
            };
            pixel(
                &r.draw_parts(&[part(&FAR, Some(s))], CLEAR).unwrap(),
                38,
                43,
            )
        };
        // alpha 0 dropped: the clear colour (green) shows
        assert_eq!(draw([0.25, 0.25], AlphaMode::Mask(0.5)), [0, 255, 0, 255]);
        assert_eq!(draw([0.25, 0.75], AlphaMode::Mask(0.5)), [0, 0, 255, 255]);
        // alpha 0.78 kept at cutoff 0.5, drawn solid
        assert_eq!(
            draw([0.75, 0.75], AlphaMode::Mask(0.5)),
            [255, 255, 255, 255]
        );
        // and dropped at cutoff 0.9
        assert_eq!(draw([0.75, 0.75], AlphaMode::Mask(0.9)), [0, 255, 0, 255]);
        // opaque draws the transparent texel solid
        assert_eq!(draw([0.25, 0.25], AlphaMode::Opaque), [255, 0, 0, 255]);
        let near_far = |cut: f32| {
            let uvs = [[0.25, 0.25]; 3];
            let s = Surface {
                base: Some(ToonTexture {
                    width: 2,
                    height: 2,
                    rgba: &holes,
                }),
                ..surface(&uvs, AlphaMode::Mask(cut))
            };
            let far_uvs = [[0.25, 0.75]; 3];
            let img = r
                .draw_parts(
                    &[
                        part(&NEAR, Some(s)),
                        part(&FAR, Some(surface(&far_uvs, AlphaMode::Opaque))),
                    ],
                    CLEAR,
                )
                .unwrap();
            rgb(&img, 25, 38)
        };
        // the dropped texel writes no depth, so the farther part shows through
        assert_eq!(near_far(0.5), [0, 0, 255]);
        // cutoff 0 keeps it
        assert_eq!(near_far(0.0), [255, 0, 0]);
    }

    fn close(got: [u8; 3], want: [u8; 3]) -> bool {
        got.iter().zip(want).all(|(&g, w)| g.abs_diff(w) <= 1)
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_blended_part_mixes_over_what_is_behind_after_the_solid_parts() {
        let r = renderer();
        let mut half = RGBA;
        half[3] = 128;
        let uvs = [[0.25, 0.25]; 3];
        let s = Surface {
            base: Some(ToonTexture {
                width: 2,
                height: 2,
                rgba: &half,
            }),
            ..surface(&uvs, AlphaMode::Blend)
        };
        // over the clear colour
        let img = r.draw_parts(&[part(&NEAR, Some(s))], CLEAR).unwrap();
        let got = rgb(&img, 25, 38);
        assert!(close(got, [128, 127, 0]), "{got:?}");
        // over a solid farther part (blue texel), listed first or last
        let far_uvs = [[0.25, 0.75]; 3];
        let far = part(&FAR, Some(surface(&far_uvs, AlphaMode::Opaque)));
        let first = r.draw_parts(&[part(&NEAR, Some(s)), far], CLEAR).unwrap();
        let last = r.draw_parts(&[far, part(&NEAR, Some(s))], CLEAR).unwrap();
        for img in [&first, &last] {
            let got = rgb(img, 25, 38);
            assert!(close(got, [128, 0, 127]), "{got:?}");
            // FAR alone outside NEAR
            assert_eq!(rgb(img, 38, 43), [0, 0, 255]);
        }
        assert_eq!(first, last);
        // blended parts write no depth: a farther blended part listed after a nearer one is
        // mixed over it (blue texel at half alpha over (128, 127, 0) = (64, 63, 128))
        let mut half_blue = RGBA;
        half_blue[11] = 128;
        let far_uvs = [[0.25, 0.75]; 3];
        let far_s = Surface {
            base: Some(ToonTexture {
                width: 2,
                height: 2,
                rgba: &half_blue,
            }),
            ..surface(&far_uvs, AlphaMode::Blend)
        };
        let img = r
            .draw_parts(&[part(&NEAR, Some(s)), part(&FAR, Some(far_s))], CLEAR)
            .unwrap();
        let got = rgb(&img, 25, 38);
        assert!(close(got, [64, 63, 128]), "{got:?}");
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_bad_surface_or_a_texture_past_the_device_is_refused_by_part() {
        let r = renderer();
        let uvs = [[0.25, 0.25]; 2];
        match r.draw_parts(
            &[
                part(&NEAR, None),
                part(&FAR, Some(surface(&uvs, AlphaMode::Opaque))),
            ],
            CLEAR,
        ) {
            Err(RenderError::BadSurface(m)) => assert!(m.starts_with("part 1:"), "{m}"),
            other => panic!("{other:?}"),
        }
        // a texture wider than any device allows, refused before any upload
        let wide = vec![0u8; 8192 * 4 * 2];
        let uvs = [[0.25, 0.25]; 3];
        let s = Surface {
            base: Some(ToonTexture {
                width: 16384,
                height: 1,
                rgba: &wide,
            }),
            ..surface(&uvs, AlphaMode::Opaque)
        };
        match r.draw_parts(&[part(&FAR, Some(s))], CLEAR) {
            Err(RenderError::BadSurface(m)) => assert!(m.starts_with("part 0:"), "{m}"),
            other => panic!("{other:?}"),
        }
    }
}
