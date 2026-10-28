//! The toon terms at values where each one changes the picture: the rim's Fresnel power, its
//! weakening in shade, its mask texture and its alphas, the back faces under linear light, its
//! antialias, and the outline's base-colour multiply.
//!
//! oracle (worked by hand, the scene of `tests/vrm_terms.rs`): the view-projection maps world (x,
//! y, z) to clip (x - 0.5, y - 0.5, 0.5 z + 0.5) on a 64 x 64 image; pixel (25, 38) is inside the
//! triangle TRI. The view ray is along z, so `|n . v| = |n.z|`; with every normal
//! `(sqrt(3) / 2, 0, 1 / 2)` the Fresnel term is `f = (1 - 1/2)^p = 2^-p`. The rim weight is
//! `clamp((f - lo) / (hi - lo))` with `lo, hi = border -/+ blur / 2`; then
//! `r = mix(r, r · lit, shadow_mask)`, the rim colour is `rim_rgb · mask_rgb` (linear), times
//! `rim_alpha · mask_alpha · r`, added to the base colour in linear light and encoded once. The
//! expected values below are computed in f64 from these closed forms, not by the shader.
//!
//! The outline (the sphere of `tests/vrm_outline.rs`, world width 0.04, lighting mix 0): its colour
//! is the outline colour, or with `outline_times_base` the outline colour times the base colour.

#[cfg(feature = "gpu")]
mod gpu {
    use alice_game_engine::vrm_render::{
        AlphaMode, Outline, OutlineMode, Rim, Surface, ToonParams, ToonPart, ToonRenderer,
        ToonTerms, ToonTexture, UvTransform,
    };
    use std::f32::consts::PI;

    const VIEW_PROJ: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
    ];
    const CLEAR: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
    const TRI: [[f32; 3]; 3] = [[0.1, 0.1, 0.0], [0.9, 0.1, 0.0], [0.1, 0.9, 0.0]];
    const FRONT: [u32; 3] = [0, 1, 2];
    const BACK: [u32; 3] = [0, 2, 1];
    const GREY: f32 = 0.2;

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

    fn tilted() -> [f32; 3] {
        [3f32.sqrt() / 2.0, 0.0, 0.5]
    }

    /// The triangle with these normals, lit along `light`
    fn part<'a>(normals: &'a [[f32; 3]], indices: &'a [u32], light: [f32; 3]) -> ToonPart<'a> {
        ToonPart {
            positions: &TRI,
            normals,
            indices,
            params: ToonParams {
                view_proj: VIEW_PROJ,
                base_color: [GREY, GREY, GREY, 1.0],
                shade_color: [0.0, 0.0, 0.0, 1.0],
                light_dir: light,
                shading_shift: 0.0,
                shading_toony: 1.0,
                clear: CLEAR,
            },
            paint: None,
            outline: None,
            surface: None,
        }
    }

    const UVS: [[f32; 2]; 3] = [[0.5, 0.5]; 3];

    /// The triangle with a surface (UVs, no texture): a rim mask is read at the surface's UVs
    fn with_surface(p: ToonPart<'_>) -> ToonPart<'_> {
        ToonPart {
            surface: Some(Surface {
                uvs: &UVS,
                base: None,
                shade: None,
                transform: UvTransform::IDENTITY,
                alpha: AlphaMode::Opaque,
            }),
            ..p
        }
    }

    fn rim<'a>(power: f32, border: f32, blur: f32) -> Rim<'a> {
        Rim {
            color: [1.0, 1.0, 1.0, 1.0],
            fresnel_power: power,
            border,
            blur,
            antialias: 0.0,
            shadow_mask: 0.0,
            albedo_mix: 0.0,
            skip_back_faces: true,
            mask: None,
        }
    }

    fn red_at(img: &[u8], col: usize, row: usize) -> f64 {
        f64::from(img[(row * 64 + col) * 4])
    }

    fn draw(p: ToonPart<'_>, t: ToonTerms<'_>) -> Vec<u8> {
        ToonRenderer::new_headless(64, 64)
            .expect("a GPU")
            .draw_parts_terms(&[p], &[t], CLEAR)
            .unwrap()
    }

    /// The base colour `base` (encoded) plus `add` in linear light, encoded, as a byte
    fn expect(base: f64, add: f64) -> f64 {
        (to_srgb(to_linear(base) + add) * 255.0).round()
    }

    fn close(got: f64, want: f64, what: &str) {
        assert!((got - want).abs() <= 1.0, "{what}: got {got}, want {want}");
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_fresnel_power_shapes_the_rim() {
        // p = 2: f = 1/4; border 1/4, blur 1/5: lo 0.15, hi 0.35, weight 1/2 (at p = 1 it would be 1)
        let n = [tilted(); 3];
        let t = ToonTerms {
            rim: Some(rim(2.0, 0.25, 0.2)),
            ..ToonTerms::default()
        };
        let img = draw(part(&n, &FRONT, tilted()), t);
        close(
            red_at(&img, 25, 38),
            expect(f64::from(GREY), 0.5),
            "power 2",
        );
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_shadow_mask_weakens_the_rim_in_shade() {
        // the light opposite the normal: lit 0, the colour is the shade colour (black); weight 1/2
        // (power 1, border 1/2, blur 1/5) times (1 - shadow_mask) = 1/2 x 1/2
        let n = [tilted(); 3];
        let away = tilted().map(|x| -x);
        let mut r = rim(1.0, 0.5, 0.2);
        r.shadow_mask = 0.5;
        let t = ToonTerms {
            rim: Some(r),
            ..ToonTerms::default()
        };
        let img = draw(part(&n, &FRONT, away), t);
        close(
            red_at(&img, 25, 38),
            expect(0.0, 0.25),
            "in shade, shadow mask 1/2",
        );
        // shadow mask 0: the full weight even in shade
        let t0 = ToonTerms {
            rim: Some(rim(1.0, 0.5, 0.2)),
            ..ToonTerms::default()
        };
        let img = draw(part(&n, &FRONT, away), t0);
        close(
            red_at(&img, 25, 38),
            expect(0.0, 0.5),
            "in shade, shadow mask 0",
        );
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_mask_texture_and_the_alphas_scale_the_rim() {
        let n = [tilted(); 3];
        // a grey mask: the rim colour times the mask's colour in linear light
        let grey_mask = [128u8, 128, 128, 255];
        let mut r = rim(1.0, 0.5, 0.2);
        r.mask = Some(ToonTexture {
            width: 1,
            height: 1,
            rgba: &grey_mask,
            key: None,
        });
        let img = draw(
            with_surface(part(&n, &FRONT, tilted())),
            ToonTerms {
                rim: Some(r),
                ..ToonTerms::default()
            },
        );
        close(
            red_at(&img, 25, 38),
            expect(f64::from(GREY), 0.5 * to_linear(128.0 / 255.0)),
            "grey mask",
        );
        // the rim colour's alpha 1/2 and a mask alpha of 128: weight 1/2 x 1/2 x 128/255
        let half_alpha = [255u8, 255, 255, 128];
        let mut r = rim(1.0, 0.5, 0.2);
        r.color = [1.0, 1.0, 1.0, 0.5];
        r.mask = Some(ToonTexture {
            width: 1,
            height: 1,
            rgba: &half_alpha,
            key: None,
        });
        let img = draw(
            with_surface(part(&n, &FRONT, tilted())),
            ToonTerms {
                rim: Some(r),
                ..ToonTerms::default()
            },
        );
        close(
            red_at(&img, 25, 38),
            expect(f64::from(GREY), 0.5 * 0.5 * 128.0 / 255.0),
            "half alphas",
        );
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn under_linear_light_a_back_face_skips_the_rim_when_asked() {
        let n = [tilted(); 3];
        let linear = |skip: bool| {
            let mut r = rim(1.0, 0.5, 0.2);
            r.skip_back_faces = skip;
            ToonTerms {
                rim: Some(r),
                linear_light: true,
                ..ToonTerms::default()
            }
        };
        let img = draw(part(&n, &BACK, tilted()), linear(true));
        close(
            red_at(&img, 25, 38),
            expect(f64::from(GREY), 0.0),
            "back face, skipped",
        );
        let img = draw(part(&n, &BACK, tilted()), linear(false));
        close(
            red_at(&img, 25, 38),
            expect(f64::from(GREY), 0.5),
            "back face, kept",
        );
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_antialias_softens_a_hard_rim_edge() {
        // normals varying across the triangle, so the Fresnel term (and its fwidth) varies; blur
        // 0 makes the rim a hard step: without antialias every pixel is either the base colour or
        // base plus the full rim, with antialias the step spreads over fwidth and pixels between
        // the two appear
        let n = [tilted(), [0.0, 0.0, 1.0], tilted()];
        let levels = |aa: f32| {
            let mut r = rim(1.0, 0.25, 0.0);
            r.antialias = aa;
            let img = draw(
                part(&n, &FRONT, [0.0, 0.0, 1.0]),
                ToonTerms {
                    rim: Some(r),
                    ..ToonTerms::default()
                },
            );
            // the full rim saturates the byte at 255
            let (lo, hi) = (
                expect(f64::from(GREY), 0.0),
                expect(f64::from(GREY), 1.0).min(255.0),
            );
            // the pixels inside the triangle (x + y < 0.9 in world, with a margin)
            let mut between = 0;
            for row in 0..64usize {
                for col in 0..64usize {
                    let (x, y) = (
                        (col as f64 + 0.5) / 32.0 - 0.5,
                        1.5 - (row as f64 + 0.5) / 32.0,
                    );
                    if x > 0.15 && y > 0.15 && x + y < 0.95 {
                        let v = red_at(&img, col, row);
                        if v > lo + 2.0 && v < hi - 2.0 {
                            between += 1;
                        }
                    }
                }
            }
            between
        };
        assert_eq!(
            levels(0.0),
            0,
            "a hard step has no pixel between the two levels"
        );
        assert!(levels(1.0) > 0, "antialias spreads the step");
    }

    /// A UV sphere, every triangle wound counter-clockwise seen from outside (as `vrm_outline.rs`)
    fn sphere() -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<u32>) {
        let (r, c) = (0.25f32, [0.5f32, 0.5, 0.0]);
        let (seg, rings) = (64u32, 32u32);
        let mut pos = Vec::new();
        let mut nor = Vec::new();
        for i in 0..=rings {
            let th = PI * i as f32 / rings as f32;
            for j in 0..=seg {
                let ph = 2.0 * PI * j as f32 / seg as f32;
                let n = [th.sin() * ph.cos(), th.cos(), th.sin() * ph.sin()];
                nor.push(n);
                pos.push([c[0] + r * n[0], c[1] + r * n[1], c[2] + r * n[2]]);
            }
        }
        let mut idx = Vec::new();
        let w = seg + 1;
        for i in 0..rings {
            for j in 0..seg {
                let (a, b, cc, d) = (
                    i * w + j,
                    i * w + j + 1,
                    (i + 1) * w + j,
                    (i + 1) * w + j + 1,
                );
                for t in [[a, cc, b], [b, cc, d]] {
                    let [p0, p1, p2] = t.map(|k| pos[k as usize]);
                    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
                    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
                    let cr = [
                        e1[1] * e2[2] - e1[2] * e2[1],
                        e1[2] * e2[0] - e1[0] * e2[2],
                        e1[0] * e2[1] - e1[1] * e2[0],
                    ];
                    let out = [p0[0] - c[0], p0[1] - c[1], p0[2] - c[2]];
                    let dot = cr[0] * out[0] + cr[1] * out[1] + cr[2] * out[2];
                    if dot.abs() < 1e-12 {
                        continue;
                    }
                    if dot > 0.0 {
                        idx.extend(t)
                    } else {
                        idx.extend([t[0], t[2], t[1]])
                    }
                }
            }
        }
        (pos, nor, idx)
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_outline_can_take_the_base_colour() {
        const SPHERE_VP: [f32; 16] = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
        ];
        let base = 128.0f32 / 255.0;
        let (pos, nor, idx) = sphere();
        let draw_ring = |times_base: bool| {
            let part = ToonPart {
                positions: &pos,
                normals: &nor,
                indices: &idx,
                params: ToonParams {
                    view_proj: SPHERE_VP,
                    base_color: [base, base, base, 1.0],
                    shade_color: [0.25, 0.25, 0.25, 1.0],
                    light_dir: [0.0, 0.0, 1.0],
                    shading_shift: 0.0,
                    shading_toony: 1.0,
                    clear: [0.0; 4],
                },
                paint: None,
                outline: Some(Outline {
                    mode: OutlineMode::World,
                    width: 0.04,
                    color: [1.0, 0.0, 0.0],
                    lighting_mix: 0.0,
                }),
                surface: None,
            };
            let t = ToonTerms {
                outline_times_base: times_base,
                ..ToonTerms::default()
            };
            let img = ToonRenderer::new_headless(256, 256)
                .expect("a GPU")
                .draw_parts_terms(&[part], &[t], [0.0, 0.0, 1.0, 1.0])
                .unwrap();
            // the ring (as vrm_outline.rs): pixel (162, 128)
            let i = (128 * 256 + 162) * 4;
            [img[i], img[i + 1], img[i + 2]]
        };
        assert_eq!(draw_ring(false), [255, 0, 0], "the outline colour");
        assert_eq!(
            draw_ring(true),
            [128, 0, 0],
            "the outline colour times the base colour"
        );
    }
}
