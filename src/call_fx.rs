//! A degraded video-call look on an RGBA8 image, in integer arithmetic only, so the same image
//! and parameters give the same bytes on every target.
//!
//! Applied in this order (alpha is kept as it is):
//! 1. `block`: each `block` x `block` tile (the last ones may be smaller) takes, per channel, the
//!    floor of the mean of its pixels
//! 2. `chroma_shift` s: the red of pixel x is the red of x - s, the blue the blue of x + s
//!    (clamped to the row); green stays
//! 3. `color_bits` b: each channel keeps its top b bits
//! 4. `scanline_every` k (k > 0): rows with `y % k == k - 1` are multiplied by `scanline_dim / 256`
//! 5. `noise` a (a > 0): a xorshift64 generator seeded with `seed` (0 is replaced by a fixed odd
//!    constant) gives one value per pixel in row-major order; channel c adds
//!    `((value >> 16c) & 0xFFFF) % (2a + 1) - a`, clamped to 0..=255
//!
//! `scripts/call_fx_reference.py` is an independent implementation of the same definition.

/// The parameters of the look
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallFx {
    /// Tile size of the blocks, at least 1 (1 is no blocking)
    pub block: u32,
    /// Bits kept per channel, 1..=8 (8 is no quantisation)
    pub color_bits: u8,
    /// Every this many rows the last one is dimmed (0 is no scanlines)
    pub scanline_every: u32,
    /// The scanline factor in 256ths, 0..=256
    pub scanline_dim: u16,
    /// Pixels the red and blue channels are moved apart by (0 is none)
    pub chroma_shift: i32,
    /// The noise amplitude per channel (0 is none)
    pub noise: u8,
}

impl CallFx {
    /// No effect at all
    pub const NONE: Self = Self {
        block: 1,
        color_bits: 8,
        scanline_every: 0,
        scanline_dim: 256,
        chroma_shift: 0,
        noise: 0,
    };
}

/// Why the look was not applied
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CallFxError {
    /// The byte length is not `width * height * 4`, or the image is empty
    Size,
    /// A parameter outside its range
    Params,
}

impl std::fmt::Display for CallFxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Size => f.write_str("the image is empty or its length is not width x height x 4"),
            Self::Params => f.write_str("a call effect parameter is outside its range"),
        }
    }
}

impl std::error::Error for CallFxError {}

const SEED_ZERO: u64 = 0x9E37_79B9_7F4A_7C15;

const fn xorshift64(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

/// Applies the look to an RGBA8 image of `width` x `height`, row-major
///
/// # Errors
///
/// [`CallFxError::Size`] or [`CallFxError::Params`]
pub fn apply(
    img: &[u8],
    width: u32,
    height: u32,
    fx: &CallFx,
    seed: u64,
) -> Result<Vec<u8>, CallFxError> {
    let (w, h) = (width as usize, height as usize);
    let len = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(4))
        .ok_or(CallFxError::Size)?;
    if len == 0 || img.len() != len {
        return Err(CallFxError::Size);
    }
    if fx.block == 0 || !(1..=8).contains(&fx.color_bits) || fx.scanline_dim > 256 {
        return Err(CallFxError::Params);
    }
    let mut px = img.to_vec();
    let at = |x: usize, y: usize| (y * w + x) * 4;
    let block = fx.block as usize;
    if block > 1 {
        for by in (0..h).step_by(block) {
            for bx in (0..w).step_by(block) {
                let (ex, ey) = ((bx + block).min(w), (by + block).min(h));
                let n = (ex - bx) * (ey - by);
                for c in 0..3 {
                    let sum: usize = (by..ey)
                        .flat_map(|y| (bx..ex).map(move |x| (x, y)))
                        .map(|(x, y)| usize::from(px[at(x, y) + c]))
                        .sum();
                    #[allow(clippy::cast_possible_truncation)]
                    let mean = (sum / n) as u8;
                    for y in by..ey {
                        for x in bx..ex {
                            px[at(x, y) + c] = mean;
                        }
                    }
                }
            }
        }
    }
    if fx.chroma_shift != 0 {
        let src = px.clone();
        let s = i64::from(fx.chroma_shift);
        #[allow(
            clippy::cast_possible_wrap,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let clamp = |x: i64| x.clamp(0, w as i64 - 1) as usize;
        for y in 0..h {
            for x in 0..w {
                #[allow(clippy::cast_possible_wrap)]
                let xi = x as i64;
                px[at(x, y)] = src[at(clamp(xi - s), y)];
                px[at(x, y) + 2] = src[at(clamp(xi + s), y) + 2];
            }
        }
    }
    let mask = 0xFFu8 << (8 - fx.color_bits);
    if fx.scanline_every > 0 || fx.color_bits < 8 {
        let k = fx.scanline_every as usize;
        for y in 0..h {
            let dim = k > 0 && y % k == k - 1;
            for x in 0..w {
                for c in 0..3 {
                    let i = at(x, y) + c;
                    let v = px[i] & mask;
                    #[allow(clippy::cast_possible_truncation)]
                    let v = if dim {
                        (u32::from(v) * u32::from(fx.scanline_dim) / 256) as u8
                    } else {
                        v
                    };
                    px[i] = v;
                }
            }
        }
    }
    if fx.noise > 0 {
        let a = i32::from(fx.noise);
        let modulus = (2 * a + 1) as u64;
        let mut state = if seed == 0 { SEED_ZERO } else { seed };
        for p in px.chunks_exact_mut(4) {
            state = xorshift64(state);
            for (c, v) in p.iter_mut().take(3).enumerate() {
                #[allow(clippy::cast_possible_truncation)]
                let d = (((state >> (16 * c)) & 0xFFFF) % modulus) as i32 - a;
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                let nv = (i32::from(*v) + d).clamp(0, 255) as u8;
                *v = nv;
            }
        }
    }
    Ok(px)
}

/// Which frames of a stuttering call are new: every `every`-th frame (from frame 0), the frames
/// between repeat the last new one (0 or 1: every frame is new)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHold {
    every: u64,
}

impl FrameHold {
    /// A hold that shows every `every`-th frame
    #[must_use]
    pub const fn new(every: u64) -> Self {
        Self { every }
    }

    /// Whether frame `index` is a new one (otherwise the last new frame is shown again)
    #[must_use]
    pub const fn is_new(&self, index: u64) -> bool {
        self.every <= 1 || index.is_multiple_of(self.every)
    }
}
