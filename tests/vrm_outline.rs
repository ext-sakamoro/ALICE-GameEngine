//! Inverted-hull outlines (`MToon` 1.0): the reader's outline parameters and the outline drawn
//! around a sphere.
//!
//! oracle (worked by hand): a sphere of radius 0.25 at (0.5, 0.5, 0), 64 x 32 segments, wound
//! counter-clockwise seen from outside; the view-projection maps (x, y, z) to clip
//! (x - 0.5, y - 0.5, 0.5 - 0.5 z), an orthographic view from +z. On a 256 x 256 image one unit
//! is 128 pixels, so the sphere's outline runs 32 pixels from the centre (128, 128); pixel
//! column c on row 128 is (c + 0.5 - 128) pixels from it.
//! - world mode, width 0.04: the hull's radius is 0.29, 37.1 pixels; column 162 (34.5 px) is
//!   outline, column 166 (38.5 px) is clear, column 158 (30.5 px, more than the 0.03 % the
//!   segments cut off the radius) is the sphere, and the centre is the sphere (front faces of
//!   the hull are culled);
//! - screen mode, width 0.04 of the image height: 2 x 0.04 in NDC = 10.24 pixels, so the ring
//!   reaches 42.2 pixels and column 166 is outline;
//! - with the whole view-projection times 2 the image is the same but w is 2, and a screen-mode
//!   width is still 10.24 pixels (column 166 outline, 172 clear);
//! - on a 512 x 256 image (aspect 2) one unit is 256 pixels across: the sphere reaches 64
//!   pixels to the right and a screen-mode width of 0.04 still adds 10.24 pixels (it is a
//!   fraction of the height), so column 256 + 69 is outline and 256 + 79 is clear;
//! - the outline colour (1, 0, 0) with lighting mix 0 is exactly red; with mix 1 it is red times
//!   the lit colour of the hull's normal there, which points away from the light (0, 0, 1)
//!   (the visible hull is its far half), so the shade colour 0.25: (64, 0, 0);
//! - with a texture of alpha 0 under a 0.5 mask, neither the sphere nor its hull is drawn: the
//!   centre, the ring column 162 and the ring above are the clear colour (alpha 1 draws both)

use alice_game_engine::vrm::{read_vrm, split_glb, OutlineWidthMode, VrmError};
use alice_game_engine::vrm_render::{Outline, OutlineMode};

const TINY: &[u8] = include_bytes!("data/vrm/tiny.vrm");

/// The fixture with its first material's `MToon` extension edited, read whole
fn with_mtoon(
    edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>),
) -> Result<alice_game_engine::vrm::VrmModel, VrmError> {
    let g = split_glb(TINY).unwrap();
    let mut j: serde_json::Value = serde_json::from_str(g.json).unwrap();
    edit(
        j["materials"][0]["extensions"]["VRMC_materials_mtoon"]
            .as_object_mut()
            .unwrap(),
    );
    let mut jb = j.to_string().into_bytes();
    jb.resize(jb.len().div_ceil(4) * 4, b' ');
    let bin = g.bin.unwrap();
    let total = 12 + 8 + jb.len() + 8 + bin.len();
    let words = [
        0x4654_6C67u32,
        2,
        total as u32,
        jb.len() as u32,
        0x4E4F_534A,
    ];
    let mut glb: Vec<u8> = words.iter().flat_map(|v| v.to_le_bytes()).collect();
    glb.extend_from_slice(&jb);
    glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    glb.extend_from_slice(&0x004E_4942u32.to_le_bytes());
    glb.extend_from_slice(bin);
    read_vrm(&glb)
}

#[test]
fn the_reader_takes_the_outline_parameters_and_their_defaults() {
    let m = &read_vrm(TINY).unwrap().materials[0];
    let t = m.mtoon.unwrap();
    // the fixture's material has no outline: mode none, width 0, black, lighting mix 1
    assert_eq!(t.outline_width_mode, OutlineWidthMode::None);
    assert_eq!(
        (t.outline_width, t.outline_color, t.outline_lighting_mix),
        (0.0, [0.0; 3], 1.0)
    );
    assert_eq!(Outline::from_material(m), None);
}

#[test]
fn outline_values_are_checked() {
    let m = with_mtoon(|t| {
        t.insert("outlineWidthMode".into(), "worldCoordinates".into());
        t.insert("outlineWidthFactor".into(), 0.002.into());
        t.insert(
            "outlineColorFactor".into(),
            serde_json::json!([0.5, 0.25, 0.125]),
        );
        t.insert("outlineLightingMixFactor".into(), 0.5.into());
    })
    .unwrap();
    let t = m.materials[0].mtoon.unwrap();
    assert_eq!(
        (
            t.outline_width_mode,
            t.outline_width,
            t.outline_color,
            t.outline_lighting_mix
        ),
        (OutlineWidthMode::World, 0.002, [0.5, 0.25, 0.125], 0.5)
    );
    for (key, bad) in [
        ("outlineWidthMode", serde_json::json!("pixels")),
        ("outlineWidthFactor", serde_json::json!(-0.001)),
        ("outlineLightingMixFactor", serde_json::json!(1.5)),
        ("outlineColorFactor", serde_json::json!([0.5, 0.25])),
    ] {
        let r = with_mtoon(|t| {
            t.insert(key.into(), bad.clone());
        });
        assert!(
            matches!(r, Err(VrmError::BadValue(ref p)) if p.ends_with(key)),
            "{key}: {r:?}"
        );
    }
}

#[test]
fn a_material_outline_becomes_a_part_outline() {
    let mut model = read_vrm(TINY).unwrap();
    let t = model.materials[0].mtoon.as_mut().unwrap();
    t.outline_width_mode = OutlineWidthMode::Screen;
    t.outline_width = 0.01;
    t.outline_color = [0.1, 0.2, 0.3];
    t.outline_lighting_mix = 0.25;
    assert_eq!(
        Outline::from_material(&model.materials[0]),
        Some(Outline {
            mode: OutlineMode::Screen,
            width: 0.01,
            color: [0.1, 0.2, 0.3],
            lighting_mix: 0.25
        })
    );
    // a width of 0 draws nothing
    model.materials[0].mtoon.as_mut().unwrap().outline_width = 0.0;
    assert_eq!(Outline::from_material(&model.materials[0]), None);
}

#[cfg(feature = "gpu")]
mod gpu {
    use alice_game_engine::vrm_render::{
        AlphaMode, Outline, OutlineMode, RenderError, Surface, ToonParams, ToonPart, ToonRenderer,
        ToonTexture, UvTransform,
    };
    use std::f32::consts::PI;

    const VIEW_PROJ: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -0.5, 0.0, -0.5, -0.5, 0.5, 1.0,
    ];
    const GREY: f32 = 128.0 / 255.0;

    /// A UV sphere, every triangle wound counter-clockwise seen from outside
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
                        continue; // the poles' degenerate triangles
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

    fn draw(w: u32, h: u32, outline: Option<Outline>) -> Vec<u8> {
        draw_with(w, h, VIEW_PROJ, outline)
    }

    fn draw_with(w: u32, h: u32, view_proj: [f32; 16], outline: Option<Outline>) -> Vec<u8> {
        let (pos, nor, idx) = sphere();
        let part = ToonPart {
            positions: &pos,
            normals: &nor,
            indices: &idx,
            params: ToonParams {
                view_proj,
                base_color: [GREY, GREY, GREY, 1.0],
                shade_color: [0.25, 0.25, 0.25, 1.0],
                light_dir: [0.0, 0.0, 1.0],
                shading_shift: 0.0,
                shading_toony: 1.0,
                clear: [0.0; 4],
            },
            paint: None,
            outline,
            surface: None,
        };
        ToonRenderer::new_headless(w, h)
            .expect("a GPU")
            .draw_parts(&[part], [0.0, 0.0, 1.0, 1.0])
            .unwrap()
    }

    /// The world-outlined sphere with a 1 x 1 texture of alpha `alpha` at every UV, mask 0.5
    fn draw_masked(alpha: u8) -> Vec<u8> {
        let (pos, nor, idx) = sphere();
        let uvs = vec![[0.5f32, 0.5]; pos.len()];
        let texel = [255, 255, 255, alpha];
        let part = ToonPart {
            positions: &pos,
            normals: &nor,
            indices: &idx,
            params: ToonParams {
                view_proj: VIEW_PROJ,
                base_color: [GREY, GREY, GREY, 1.0],
                shade_color: [0.25, 0.25, 0.25, 1.0],
                light_dir: [0.0, 0.0, 1.0],
                shading_shift: 0.0,
                shading_toony: 1.0,
                clear: [0.0; 4],
            },
            paint: None,
            outline: red(OutlineMode::World, 0.04, 0.0),
            surface: Some(Surface {
                uvs: &uvs,
                base: Some(ToonTexture {
                    width: 1,
                    height: 1,
                    rgba: &texel,
                    key: None,
                }),
                shade: None,
                transform: UvTransform::IDENTITY,
                alpha: AlphaMode::Mask(0.5),
            }),
        };
        ToonRenderer::new_headless(256, 256)
            .expect("a GPU")
            .draw_parts(&[part], [0.0, 0.0, 1.0, 1.0])
            .unwrap()
    }

    fn px(img: &[u8], w: u32, col: u32, row: u32) -> [u8; 4] {
        let i = ((row * w + col) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    const RED: [u8; 4] = [255, 0, 0, 255];
    const CLEAR: [u8; 4] = [0, 0, 255, 255];
    const SURFACE: [u8; 4] = [128, 128, 128, 255];

    fn red(mode: OutlineMode, width: f32, mix: f32) -> Option<Outline> {
        Some(Outline {
            mode,
            width,
            color: [1.0, 0.0, 0.0],
            lighting_mix: mix,
        })
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_world_outline_is_a_ring_of_its_width() {
        let img = draw(256, 256, red(OutlineMode::World, 0.04, 0.0));
        assert_eq!(px(&img, 256, 128, 128), SURFACE, "the centre is the sphere");
        assert_eq!(px(&img, 256, 158, 128), SURFACE);
        assert_eq!(px(&img, 256, 162, 128), RED);
        assert_eq!(px(&img, 256, 166, 128), CLEAR);
        // the same ring above the sphere
        assert_eq!(px(&img, 256, 128, 128 - 35), RED);
        let none = draw(256, 256, None);
        assert_eq!(px(&none, 256, 162, 128), CLEAR);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_screen_outline_is_a_fraction_of_the_image_height() {
        let img = draw(256, 256, red(OutlineMode::Screen, 0.04, 0.0));
        assert_eq!(px(&img, 256, 166, 128), RED);
        assert_eq!(px(&img, 256, 172, 128), CLEAR, "44.5 px is past 42.2");
        // aspect 2: across, the sphere is 64 px and the ring adds 10.24 px
        let wide = draw(512, 256, red(OutlineMode::Screen, 0.04, 0.0));
        assert_eq!(px(&wide, 512, 256 + 60, 128), SURFACE);
        assert_eq!(px(&wide, 512, 256 + 69, 128), RED);
        assert_eq!(px(&wide, 512, 256 + 79, 128), CLEAR);
        // the whole matrix times 2: the same image with w = 2; a screen width holds its pixels
        let deep = draw_with(
            256,
            256,
            VIEW_PROJ.map(|x| 2.0 * x),
            red(OutlineMode::Screen, 0.04, 0.0),
        );
        assert_eq!(px(&deep, 256, 166, 128), RED);
        assert_eq!(px(&deep, 256, 172, 128), CLEAR);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_masked_texel_drops_its_outline_too() {
        // alpha 1: the sphere and its ring as without a texture
        let solid = draw_masked(255);
        assert_eq!(px(&solid, 256, 128, 128), SURFACE);
        assert_eq!(px(&solid, 256, 162, 128), RED);
        // alpha 0 below the cutoff: neither the surface nor its outline is drawn
        let gone = draw_masked(0);
        assert_eq!(px(&gone, 256, 128, 128), CLEAR);
        assert_eq!(px(&gone, 256, 162, 128), CLEAR);
        assert_eq!(px(&gone, 256, 128, 128 - 35), CLEAR);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn the_lighting_mix_tints_the_outline_by_the_lit_colour() {
        let img = draw(256, 256, red(OutlineMode::World, 0.04, 1.0));
        assert_eq!(px(&img, 256, 162, 128), [64, 0, 0, 255]);
    }

    #[test]
    #[ignore = "needs a working GPU adapter"]
    fn a_bad_outline_is_refused_with_its_part() {
        let (pos, nor, idx) = sphere();
        let part = |o| ToonPart {
            positions: &pos,
            normals: &nor,
            indices: &idx,
            params: ToonParams {
                view_proj: VIEW_PROJ,
                base_color: [1.0; 4],
                shade_color: [1.0; 4],
                light_dir: [0.0, 0.0, 1.0],
                shading_shift: 0.0,
                shading_toony: 1.0,
                clear: [0.0; 4],
            },
            paint: None,
            outline: o,
            surface: None,
        };
        let r = ToonRenderer::new_headless(64, 64)
            .expect("a GPU")
            .draw_parts(
                &[part(None), part(red(OutlineMode::World, -0.1, 0.0))],
                [0.0; 4],
            );
        assert!(matches!(r, Err(RenderError::BadOutline(ref m)) if m.starts_with("part 1: width")));
        let r = ToonRenderer::new_headless(64, 64)
            .expect("a GPU")
            .draw_parts(&[part(red(OutlineMode::World, 0.1, 2.0))], [0.0; 4]);
        assert!(matches!(r, Err(RenderError::BadOutline(ref m)) if m.contains("lighting mix")));
    }
}
