//! The degraded video-call look in integer arithmetic.
//!
//! oracle: two kinds. Small cases worked out by hand from the definition in
//! `scripts/call_fx_reference.py`, and 9 cases whose result digests that script (an independent
//! implementation, standard-library Python) wrote to `tests/data/call_fx/expected.txt`.

use alice_game_engine::call_fx::{apply, CallFx, CallFxError, FrameHold};
use sha2::{Digest, Sha256};

fn image(w: u32, h: u32) -> Vec<u8> {
    let mut v = Vec::new();
    for y in 0..h {
        for x in 0..w {
            v.extend_from_slice(&[
                ((x * 37 + y * 11) % 256) as u8,
                ((x * 5 + y * 53) % 256) as u8,
                ((x * y * 7 + 13) % 256) as u8,
                255,
            ]);
        }
    }
    v
}

#[test]
fn every_reference_case_has_the_reference_digest() {
    let text = include_str!("data/call_fx/expected.txt");
    let mut compared = 0;
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split(' ').collect();
        let n = |i: usize| f[i].parse::<i64>().unwrap();
        let (w, h) = (n(1) as u32, n(2) as u32);
        let fx = CallFx {
            block: n(3) as u32,
            color_bits: n(4) as u8,
            scanline_every: n(5) as u32,
            scanline_dim: n(6) as u16,
            chroma_shift: n(7) as i32,
            noise: n(8) as u8,
        };
        let out = apply(&image(w, h), w, h, &fx, n(9) as u64).expect(f[0]);
        let digest: String = Sha256::digest(&out)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(digest, f[10], "case {}", f[0]);
        compared += 1;
    }
    assert_eq!(compared, 9, "expected.txt: nothing or too little compared");
}

#[test]
fn small_cases_by_hand() {
    let none = CallFx::NONE;
    // two pixels, block 2: each channel is floor((10 + 21) / 2) = 15, alpha kept
    let img = [10, 100, 7, 255, 21, 0, 8, 128];
    let out = apply(&img, 2, 1, &CallFx { block: 2, ..none }, 0).unwrap();
    assert_eq!(out, [15, 50, 7, 255, 15, 50, 7, 128]);
    // color bits 2: 0b1011_0111 -> 0b1000_0000
    let out = apply(
        &[0xB7, 0x40, 0xFF, 255],
        1,
        1,
        &CallFx {
            color_bits: 2,
            ..none
        },
        0,
    )
    .unwrap();
    assert_eq!(out, [0x80, 0x40, 0xC0, 255]);
    // chroma shift 1 on three pixels: red from the left neighbour (clamped), blue from the right
    let img = [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255];
    let out = apply(
        &img,
        3,
        1,
        &CallFx {
            chroma_shift: 1,
            ..none
        },
        0,
    )
    .unwrap();
    assert_eq!(out, [1, 2, 6, 255, 1, 5, 9, 255, 4, 8, 9, 255]);
    // scanlines every 2 at 128/256: the second row halves (floor)
    let img = [200, 101, 0, 255, 200, 101, 0, 255];
    let out = apply(
        &img,
        1,
        2,
        &CallFx {
            scanline_every: 2,
            scanline_dim: 128,
            ..none
        },
        0,
    )
    .unwrap();
    assert_eq!(out, [200, 101, 0, 255, 100, 50, 0, 255]);
    // no effect: the image comes back
    assert_eq!(apply(&img, 1, 2, &none, 99).unwrap(), img);
}

#[test]
fn noise_depends_on_the_seed_and_on_nothing_else() {
    let img = image(8, 6);
    let fx = CallFx {
        noise: 9,
        ..CallFx::NONE
    };
    let a = apply(&img, 8, 6, &fx, 1).unwrap();
    assert_eq!(a, apply(&img, 8, 6, &fx, 1).unwrap());
    assert_ne!(a, apply(&img, 8, 6, &fx, 2).unwrap());
    // within the amplitude, alpha untouched
    for (o, i) in a.chunks(4).zip(img.chunks(4)) {
        for c in 0..3 {
            assert!((i32::from(o[c]) - i32::from(i[c])).abs() <= 9);
        }
        assert_eq!(o[3], i[3]);
    }
}

#[test]
fn what_it_refuses() {
    let none = CallFx::NONE;
    assert!(matches!(
        apply(&[0; 7], 2, 1, &none, 0),
        Err(CallFxError::Size)
    ));
    assert!(matches!(apply(&[], 0, 0, &none, 0), Err(CallFxError::Size)));
    assert!(matches!(
        apply(&[0; 8], u32::MAX, 2, &none, 0),
        Err(CallFxError::Size)
    ));
    for bad in [
        CallFx { block: 0, ..none },
        CallFx {
            color_bits: 0,
            ..none
        },
        CallFx {
            color_bits: 9,
            ..none
        },
        CallFx {
            scanline_dim: 257,
            ..none
        },
    ] {
        assert!(
            matches!(apply(&[0; 4], 1, 1, &bad, 0), Err(CallFxError::Params)),
            "{bad:?}"
        );
    }
}

#[test]
fn a_held_frame_repeats_the_last_one() {
    let hold = FrameHold::new(3);
    let shown: Vec<bool> = (0..7).map(|i| hold.is_new(i)).collect();
    // every third frame is new, the two after it repeat it
    assert_eq!(shown, [true, false, false, true, false, false, true]);
    assert!((0..5).all(|i| FrameHold::new(0).is_new(i)));
    assert!((0..5).all(|i| FrameHold::new(1).is_new(i)));
}
