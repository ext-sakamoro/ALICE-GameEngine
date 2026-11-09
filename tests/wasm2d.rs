//! The browser entry in a real browser: generated art drawn with WebGL2 and read back,
//! compared cell by cell with what the art says each pixel must be.
#![cfg(all(target_arch = "wasm32", feature = "wasm"))]

use alice_game_engine::wasm2d::{Renderer2D, WasmSpriteAnimator as SpriteAnimator};
use wasm_bindgen::{Clamped, JsCast};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{
    CanvasRenderingContext2d, HtmlCanvasElement, HtmlImageElement, ImageData,
    WebGl2RenderingContext as Gl,
};

wasm_bindgen_test_configure!(run_in_browser);

const ART_W: u32 = 7;
const ART_H: u32 = 5;
const CANVAS_W: u32 = 40;
const CANVAS_H: u32 = 30;
const SCALE: u32 = 3;
const AT_X: u32 = 5;
const AT_Y: u32 = 4;
const CLEAR: [u8; 3] = [0, 0, 255];
const UNDER: [u8; 3] = [0, 255, 0];

// Every opaque cell has its own colour, so a shifted or mirrored read cannot match by
// accident. Blue stays 0: the clear colour is pure blue. (6, 4) and (0, 2) are transparent.
fn art(x: u32, y: u32) -> [u8; 4] {
    if (x, y) == (6, 4) || (x, y) == (0, 2) {
        return [0, 0, 0, 0];
    }
    [(30 + x * 30) as u8, (40 + y * 40) as u8, 0, 255]
}

fn canvas(w: u32, h: u32) -> HtmlCanvasElement {
    let c: HtmlCanvasElement = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    c.set_width(w);
    c.set_height(h);
    c
}

async fn art_image() -> HtmlImageElement {
    let c = canvas(ART_W, ART_H);
    let g: CanvasRenderingContext2d = c.get_context("2d").unwrap().unwrap().dyn_into().unwrap();
    let mut bytes = Vec::new();
    for y in 0..ART_H {
        for x in 0..ART_W {
            bytes.extend_from_slice(&art(x, y));
        }
    }
    let data = ImageData::new_with_u8_clamped_array_and_sh(Clamped(&bytes), ART_W, ART_H).unwrap();
    g.put_image_data(&data, 0.0, 0.0).unwrap();
    let image = HtmlImageElement::new().unwrap();
    image.set_src(&c.to_data_url().unwrap());
    JsFuture::from(image.decode()).await.unwrap();
    image
}

// RGB from the top-left, reading bottom-up rows the way WebGL returns them.
fn read(c: &HtmlCanvasElement) -> Vec<[u8; 3]> {
    let g: Gl = c
        .get_context("webgl2")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    let (w, h) = (c.width() as usize, c.height() as usize);
    let mut px = vec![0u8; w * h * 4];
    g.read_pixels_with_opt_u8_array(
        0,
        0,
        w as i32,
        h as i32,
        Gl::RGBA,
        Gl::UNSIGNED_BYTE,
        Some(&mut px),
    )
    .unwrap();
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let i = (h - 1 - y) * w * 4 + x * 4;
            out.push([px[i], px[i + 1], px[i + 2]]);
        }
    }
    out
}

// What each pixel must be: `art_at` gives the art cell under a pixel of the drawn rectangle.
fn check(
    got: &[[u8; 3]],
    background: impl Fn(u32, u32) -> [u8; 3],
    art_at: impl Fn(u32, u32) -> (u32, u32),
) {
    assert_eq!(got.len(), (CANVAS_W * CANVAS_H) as usize);
    let (mut drawn, mut showing_through) = (0, 0);
    for y in 0..CANVAS_H {
        for x in 0..CANVAS_W {
            let inside = (AT_X..AT_X + ART_W * SCALE).contains(&x)
                && (AT_Y..AT_Y + ART_H * SCALE).contains(&y);
            let want = if inside {
                let (ax, ay) = art_at((x - AT_X) / SCALE, (y - AT_Y) / SCALE);
                let cell = art(ax, ay);
                if cell[3] == 255 {
                    drawn += 1;
                    [cell[0], cell[1], cell[2]]
                } else {
                    showing_through += 1;
                    background(x, y)
                }
            } else {
                background(x, y)
            };
            assert_eq!(got[(y * CANVAS_W + x) as usize], want, "pixel {x},{y}");
        }
    }
    assert_eq!(drawn, ((ART_W * ART_H - 2) * SCALE * SCALE) as i32);
    assert_eq!(showing_through, (2 * SCALE * SCALE) as i32);
}

fn clear_colour(_: u32, _: u32) -> [u8; 3] {
    CLEAR
}

fn begin() -> (HtmlCanvasElement, Renderer2D) {
    let c = canvas(CANVAS_W, CANVAS_H);
    let mut r = Renderer2D::new(&c)
        .map_err(|_| "WebGL2 is required")
        .unwrap();
    r.clear(0.0, 0.0, 1.0);
    (c, r)
}

#[wasm_bindgen_test]
async fn sprite_draws_every_cell_at_a_whole_scale() {
    let image = art_image().await;
    let (c, mut r) = begin();
    let t = r.texture(&image).map_err(|_| "texture").unwrap();
    r.sprite(
        t,
        AT_X as f32,
        AT_Y as f32,
        (ART_W * SCALE) as f32,
        (ART_H * SCALE) as f32,
        0.0,
        0.0,
        1.0,
        1.0,
        1.0,
    )
    .map_err(|_| "sprite")
    .unwrap();
    r.flush();
    check(&read(&c), clear_colour, |x, y| (x, y));
}

// An id that no texture() returned draws nothing (it used to draw silently with the texture bound last).
#[wasm_bindgen_test]
async fn sprite_with_an_unknown_texture_id_draws_nothing() {
    let image = art_image().await;
    let (c, mut r) = begin();
    let t = r.texture(&image).map_err(|_| "texture").unwrap();
    r.sprite(
        t,
        AT_X as f32,
        AT_Y as f32,
        (ART_W * SCALE) as f32,
        (ART_H * SCALE) as f32,
        0.0,
        0.0,
        1.0,
        1.0,
        1.0,
    )
    .map_err(|_| "sprite")
    .unwrap();
    // outside the art's rectangle: with the old behaviour these 36 pixels showed the art
    assert!(r
        .sprite(t + 1, 30.0, 22.0, 6.0, 6.0, 0.0, 0.0, 1.0, 1.0, 1.0)
        .is_err());
    r.flush();
    check(&read(&c), clear_colour, |x, y| (x, y));
}

#[wasm_bindgen_test]
async fn swapped_u_mirrors() {
    let image = art_image().await;
    let (c, mut r) = begin();
    let t = r.texture(&image).map_err(|_| "texture").unwrap();
    r.sprite(
        t,
        AT_X as f32,
        AT_Y as f32,
        (ART_W * SCALE) as f32,
        (ART_H * SCALE) as f32,
        1.0,
        0.0,
        0.0,
        1.0,
        1.0,
    )
    .map_err(|_| "sprite")
    .unwrap();
    r.flush();
    check(&read(&c), clear_colour, |x, y| (ART_W - 1 - x, y));
}

#[wasm_bindgen_test]
async fn part_of_the_atlas_is_drawn_alone() {
    let image = art_image().await;
    let (c, mut r) = begin();
    let t = r.texture(&image).map_err(|_| "texture").unwrap();
    // Cells 1..3 of row 1, each as 2 pixels.
    let (u0, u1) = (1.0 / ART_W as f32, 3.0 / ART_W as f32);
    let (v0, v1) = (1.0 / ART_H as f32, 2.0 / ART_H as f32);
    r.sprite(t, 10.0, 10.0, 4.0, 2.0, u0, v0, u1, v1, 1.0)
        .map_err(|_| "sprite")
        .unwrap();
    r.flush();
    let got = read(&c);
    for y in 0..CANVAS_H {
        for x in 0..CANVAS_W {
            let want = if (10..14).contains(&x) && (10..12).contains(&y) {
                let cell = art(1 + (x - 10) / 2, 1);
                [cell[0], cell[1], cell[2]]
            } else {
                CLEAR
            };
            assert_eq!(got[(y * CANVAS_W + x) as usize], want, "pixel {x},{y}");
        }
    }
}

#[wasm_bindgen_test]
async fn later_calls_draw_over_earlier_ones_across_textures() {
    let image = art_image().await;
    let (c, mut r) = begin();
    let t = r.texture(&image).map_err(|_| "texture").unwrap();
    // A rectangle under the left half of the sprite, then over its first row.
    r.rect(0.0, 0.0, 20.0, CANVAS_H as f32, 0.0, 1.0, 0.0, 1.0);
    r.sprite(
        t,
        AT_X as f32,
        AT_Y as f32,
        (ART_W * SCALE) as f32,
        (ART_H * SCALE) as f32,
        0.0,
        0.0,
        1.0,
        1.0,
        1.0,
    )
    .map_err(|_| "sprite")
    .unwrap();
    r.rect(
        AT_X as f32,
        AT_Y as f32,
        (ART_W * SCALE) as f32,
        SCALE as f32,
        0.0,
        0.0,
        1.0,
        1.0,
    );
    r.flush();
    let got = read(&c);
    let background = |x: u32, _: u32| if x < 20 { UNDER } else { CLEAR };
    for y in 0..CANVAS_H {
        for x in 0..CANVAS_W {
            let inside = (AT_X..AT_X + ART_W * SCALE).contains(&x)
                && (AT_Y..AT_Y + ART_H * SCALE).contains(&y);
            let want = if inside && y < AT_Y + SCALE {
                CLEAR
            } else if inside {
                let cell = art((x - AT_X) / SCALE, (y - AT_Y) / SCALE);
                if cell[3] == 255 {
                    [cell[0], cell[1], cell[2]]
                } else {
                    background(x, y)
                }
            } else {
                background(x, y)
            };
            assert_eq!(got[(y * CANVAS_W + x) as usize], want, "pixel {x},{y}");
        }
    }
}

fn walk() -> SpriteAnimator {
    let mut a = SpriteAnimator::new();
    a.add_row("walk", vec![3, 4, 5, 6], vec![50, 50, 50, 50], true)
        .map_err(|_| "add_row")
        .unwrap();
    a.play("walk").map_err(|_| "play").unwrap();
    a
}

#[wasm_bindgen_test]
fn animator_runs_on_time_and_on_distance() {
    let mut a = walk();
    assert_eq!(a.frame(), Some(3));
    a.advance(120);
    assert_eq!((a.position(), a.frame()), (2, Some(5)));
    for (x, snapped, position) in [
        (0, 0.0, 0),
        (5, 0.0, 0),
        (6, 6.0, 1),
        (23, 18.0, 3),
        (24, 24.0, 0),
    ] {
        assert_eq!(
            a.seek_by_distance(x, 6, false).map_err(|_| "seek").unwrap(),
            snapped,
            "x {x}"
        );
        assert_eq!(a.position(), position, "x {x}");
    }
    assert_eq!(
        a.seek_by_distance(-1, 6, false)
            .map_err(|_| "seek")
            .unwrap(),
        -6.0
    );
    assert_eq!(a.position(), 3);
    assert_eq!(
        a.seek_by_distance(-1, 6, true).map_err(|_| "seek").unwrap(),
        -6.0
    );
    assert_eq!(a.position(), 1);
}

#[wasm_bindgen_test]
fn animator_refusals_reach_javascript() {
    let mut a = walk();
    assert!(a.seek_by_distance(0, 0, false).is_err());
    assert!(a.play("missing").is_err());
    assert!(a.add_row("bad", vec![1, 2], vec![50], true).is_err());
    assert!(SpriteAnimator::new().seek_by_distance(0, 6, false).is_err());
}
