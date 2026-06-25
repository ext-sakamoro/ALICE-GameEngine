//! Tessendorf-style FFT ocean simulation (CPU).
//!
//! Implements the technique from Jerry Tessendorf's *Simulating Ocean
//! Water* paper: a stochastic frequency-domain spectrum (Phillips) is
//! evolved in time and converted to a real-valued height field with a
//! 2-D inverse FFT. The resulting grid is suitable for displacing a
//! tessellated plane on the GPU, generating per-vertex normals on the
//! CPU, and driving foam masks downstream.
//!
//! The module has **no external dependencies** beyond `crate::math`: a
//! tiny Cooley-Tukey radix-2 IFFT is included so the grid size must be
//! a power of two. 64-point grids run in well under a millisecond on
//! modern hardware; 256-point grids are still real-time-friendly.
//!
//! ## Quick example
//!
//! ```rust
//! use alice_game_engine::ocean::{OceanConfig, OceanSimulator};
//! use alice_game_engine::math::Vec2;
//!
//! let config = OceanConfig {
//!     grid_size: 32,
//!     ..OceanConfig::default()
//! };
//! let mut sim = OceanSimulator::new(config);
//! let frame = sim.simulate(0.0);
//! assert_eq!(frame.heights.len(), 32 * 32);
//! assert_eq!(frame.normals.len(), 32 * 32);
//! ```

use crate::math::{Vec2, Vec3};

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Tunable parameters for the Tessendorf ocean simulator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OceanConfig {
    /// Power-of-two grid resolution. Common values: 32, 64, 128, 256.
    pub grid_size: u32,
    /// Physical size of the simulated patch in metres. Heights tile
    /// seamlessly at this period.
    pub patch_size: f32,
    /// Prevailing wind direction. Stored as-is; the simulator normalises
    /// internally so callers do not need to.
    pub wind_direction: Vec2,
    /// Wind speed in metres per second.
    pub wind_speed: f32,
    /// Phillips spectrum amplitude scale.
    pub amplitude: f32,
    /// Gravity in m/s².
    pub gravity: f32,
}

impl Default for OceanConfig {
    fn default() -> Self {
        Self {
            grid_size: 64,
            patch_size: 100.0,
            wind_direction: Vec2::new(1.0, 0.0),
            wind_speed: 20.0,
            amplitude: 0.0005,
            gravity: 9.81,
        }
    }
}

// ---------------------------------------------------------------------------
// Simulator
// ---------------------------------------------------------------------------

/// Stateful Tessendorf ocean simulator. Owns the Phillips spectrum
/// coefficients and reusable scratch buffers; call [`simulate`] once
/// per frame.
///
/// [`simulate`]: OceanSimulator::simulate
pub struct OceanSimulator {
    pub config: OceanConfig,
    /// h0(k) coefficients (= time-invariant Phillips spectrum).
    h0_re: Vec<f32>,
    h0_im: Vec<f32>,
    /// Pre-computed angular frequencies ω(k) = sqrt(g · |k|).
    omega: Vec<f32>,
    /// Scratch buffers reused across `simulate` calls.
    work_re: Vec<f32>,
    work_im: Vec<f32>,
    /// Output heights (= row-major `grid_size × grid_size`).
    heights: Vec<f32>,
    /// Output normals (= same layout).
    normals: Vec<Vec3>,
}

/// Borrowed view of the most recent simulation result.
#[derive(Debug)]
pub struct OceanFrame<'a> {
    pub heights: &'a [f32],
    pub normals: &'a [Vec3],
    pub grid_size: u32,
    pub patch_size: f32,
}

impl OceanSimulator {
    /// Build a simulator and pre-compute the Phillips spectrum. The
    /// grid is filled with a deterministic Gaussian noise hash so two
    /// simulators built from the same `config` produce identical
    /// output — useful for tests and replays.
    ///
    /// # Panics
    ///
    /// Panics when `config.grid_size` is not a power of two or smaller
    /// than 2. Both restrictions come from the radix-2 IFFT path.
    #[must_use]
    pub fn new(config: OceanConfig) -> Self {
        assert!(
            config.grid_size >= 2 && config.grid_size.is_power_of_two(),
            "ocean grid_size must be a power of two ≥ 2 (got {})",
            config.grid_size,
        );
        let n = config.grid_size as usize;
        let total = n * n;

        let mut h0_re = vec![0.0_f32; total];
        let mut h0_im = vec![0.0_f32; total];
        let mut omega = vec![0.0_f32; total];

        // Normalise wind direction (zero falls back to +X).
        let wind_len = config.wind_direction.length();
        let wind = if wind_len > 1e-6 {
            config.wind_direction * wind_len.recip()
        } else {
            Vec2::new(1.0, 0.0)
        };

        let l_wind = config.wind_speed * config.wind_speed / config.gravity;
        let small = l_wind * 0.001;

        let two_pi_over_patch = std::f32::consts::TAU / config.patch_size;
        #[allow(clippy::cast_possible_wrap)]
        let half_n = (n as i32) / 2;

        for j in 0..n {
            #[allow(clippy::cast_possible_wrap)]
            let ky_i = (j as i32) - half_n;
            #[allow(clippy::cast_precision_loss)]
            let ky = (ky_i as f32) * two_pi_over_patch;
            for i in 0..n {
                #[allow(clippy::cast_possible_wrap)]
                let kx_i = (i as i32) - half_n;
                #[allow(clippy::cast_precision_loss)]
                let kx = (kx_i as f32) * two_pi_over_patch;
                let idx = j * n + i;

                let k_sq = kx.mul_add(kx, ky * ky);
                if k_sq < 1e-12 {
                    // DC component: leave zero.
                    omega[idx] = 0.0;
                    continue;
                }
                let k_len = k_sq.sqrt();
                omega[idx] = (config.gravity * k_len).sqrt();

                // Phillips: A · |k̂·ŵ|² / k⁴ · exp(-1/(kL)²) · exp(-k²·small²).
                let k_dot_w = (kx * wind.x() + ky * wind.y()) * k_len.recip();
                let factor = (-1.0 / (k_sq * l_wind * l_wind)).exp();
                let suppress = (-k_sq * small * small).exp();
                let phillips =
                    config.amplitude * factor * suppress * k_dot_w * k_dot_w / (k_sq * k_sq);
                let amp = phillips.max(0.0).sqrt() * std::f32::consts::FRAC_1_SQRT_2;

                // Deterministic Gaussian via Box-Muller on a hashed seed.
                let (g_re, g_im) = box_muller(hash_seed(i, j, config.grid_size));
                h0_re[idx] = g_re * amp;
                h0_im[idx] = g_im * amp;
            }
        }

        Self {
            config,
            h0_re,
            h0_im,
            omega,
            work_re: vec![0.0_f32; total],
            work_im: vec![0.0_f32; total],
            heights: vec![0.0_f32; total],
            normals: vec![Vec3::ZERO; total],
        }
    }

    /// Evolve the spectrum to time `time` (seconds) and produce a real
    /// height field plus matching normals.
    pub fn simulate(&mut self, time: f32) -> OceanFrame<'_> {
        let n = self.config.grid_size as usize;

        // 1. Build h(k, t) in the centred spectrum layout.
        for idx in 0..(n * n) {
            let omega_t = self.omega[idx] * time;
            let (sin_w, cos_w) = omega_t.sin_cos();
            let h0_re = self.h0_re[idx];
            let h0_im = self.h0_im[idx];
            // Conjugate of h0(-k): we look up the mirror index.
            let i = idx % n;
            let j = idx / n;
            let mi = (n - i) % n;
            let mj = (n - j) % n;
            let mirror = mj * n + mi;
            let h0c_re = self.h0_re[mirror];
            let h0c_im = -self.h0_im[mirror];

            // h(k,t) = h0(k)*(cos+i*sin) + conj(h0(-k))*(cos-i*sin)
            let term1_re = h0_re.mul_add(cos_w, -(h0_im * sin_w));
            let term1_im = h0_re.mul_add(sin_w, h0_im * cos_w);
            let term2_re = h0c_re.mul_add(cos_w, h0c_im * sin_w);
            let term2_im = (-h0c_re).mul_add(sin_w, h0c_im * cos_w);

            self.work_re[idx] = term1_re + term2_re;
            self.work_im[idx] = term1_im + term2_im;
        }

        // 2. fftshift back to FFT-natural order (centred → corner) so
        //    the radix-2 IFFT produces a tileable real signal.
        fftshift_2d(&mut self.work_re, n);
        fftshift_2d(&mut self.work_im, n);

        // 3. 2-D inverse FFT: rows first, then columns.
        for j in 0..n {
            let row_start = j * n;
            ifft_inplace(
                &mut self.work_re[row_start..row_start + n],
                &mut self.work_im[row_start..row_start + n],
            );
        }
        transpose_in_place(&mut self.work_re, n);
        transpose_in_place(&mut self.work_im, n);
        for j in 0..n {
            let row_start = j * n;
            ifft_inplace(
                &mut self.work_re[row_start..row_start + n],
                &mut self.work_im[row_start..row_start + n],
            );
        }
        transpose_in_place(&mut self.work_re, n);
        transpose_in_place(&mut self.work_im, n);

        // 4. Real part is the displacement; sign flips with (i+j) parity
        //    to recentre the spectrum in spatial space (Tessendorf eq. 19).
        for j in 0..n {
            for i in 0..n {
                let idx = j * n + i;
                let sign = if (i + j) % 2 == 0 { 1.0 } else { -1.0 };
                self.heights[idx] = self.work_re[idx] * sign;
            }
        }

        // 5. Central-difference normals (tile across patch boundary).
        let inv_dx = (self.config.grid_size as f32) / self.config.patch_size;
        for j in 0..n {
            for i in 0..n {
                let idx = j * n + i;
                let left = self.heights[j * n + ((i + n - 1) % n)];
                let right = self.heights[j * n + ((i + 1) % n)];
                let down = self.heights[((j + n - 1) % n) * n + i];
                let up = self.heights[((j + 1) % n) * n + i];
                let dh_dx = (right - left) * 0.5 * inv_dx;
                let dh_dy = (up - down) * 0.5 * inv_dx;
                self.normals[idx] = Vec3::new(-dh_dx, 1.0, -dh_dy).normalize();
            }
        }

        OceanFrame {
            heights: &self.heights,
            normals: &self.normals,
            grid_size: self.config.grid_size,
            patch_size: self.config.patch_size,
        }
    }
}

// ---------------------------------------------------------------------------
// Internal: deterministic Gaussian noise
// ---------------------------------------------------------------------------

fn hash_seed(i: usize, j: usize, grid_size: u32) -> u32 {
    // SplitMix64-style avalanche keyed on (i, j, grid).
    #[allow(clippy::cast_possible_truncation)]
    let mut x = ((i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        ^ ((j as u64)
            .wrapping_mul(0xBF58_476D_1CE4_E5B9)
            .wrapping_add(u64::from(grid_size).wrapping_mul(0x94D0_49BB_1331_11EB)));
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    x as u32
}

/// Box-Muller: returns two independent standard Gaussian samples from a
/// 32-bit seed.
fn box_muller(seed: u32) -> (f32, f32) {
    let u1 = ((seed >> 8) as f32 + 1.0) / 16_777_217.0; // (0, 1]
    let u2 = ((seed.rotate_left(13) >> 8) as f32) / 16_777_216.0;
    let r = (-2.0 * u1.ln()).sqrt();
    let (sin_t, cos_t) = (std::f32::consts::TAU * u2).sin_cos();
    (r * cos_t, r * sin_t)
}

// ---------------------------------------------------------------------------
// Internal: tiny Cooley-Tukey radix-2 IFFT
// ---------------------------------------------------------------------------

/// In-place 1-D inverse FFT (radix-2 Cooley-Tukey). `re.len()` must be
/// a power of two and equal to `im.len()`.
fn ifft_inplace(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert_eq!(n, im.len());
    debug_assert!(n.is_power_of_two());

    // Conjugate.
    for x in im.iter_mut() {
        *x = -*x;
    }
    fft_inplace(re, im);
    // Conjugate + scale.
    let inv_n = (n as f32).recip();
    for (r, i) in re.iter_mut().zip(im.iter_mut()) {
        *r *= inv_n;
        *i = -*i * inv_n;
    }
}

/// In-place 1-D forward FFT (radix-2 Cooley-Tukey).
fn fft_inplace(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    debug_assert_eq!(n, im.len());
    debug_assert!(n.is_power_of_two());

    // Bit-reversal permutation.
    let mut j = 0_usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Butterflies.
    let mut len = 2_usize;
    while len <= n {
        #[allow(clippy::cast_precision_loss)]
        let theta = -2.0 * std::f32::consts::PI / (len as f32);
        let (wp_im, wp_re) = theta.sin_cos();
        let half = len / 2;
        let mut k = 0_usize;
        while k < n {
            let mut w_re = 1.0_f32;
            let mut w_im = 0.0_f32;
            for m in 0..half {
                let t_re = w_re.mul_add(re[k + m + half], -(w_im * im[k + m + half]));
                let t_im = w_re.mul_add(im[k + m + half], w_im * re[k + m + half]);
                let u_re = re[k + m];
                let u_im = im[k + m];
                re[k + m] = u_re + t_re;
                im[k + m] = u_im + t_im;
                re[k + m + half] = u_re - t_re;
                im[k + m + half] = u_im - t_im;
                // Advance twiddle.
                let new_w_re = w_re.mul_add(wp_re, -(w_im * wp_im));
                w_im = w_re.mul_add(wp_im, w_im * wp_re);
                w_re = new_w_re;
            }
            k += len;
        }
        len <<= 1;
    }
}

/// Swap quadrants so the DC component sits at the corner. Equivalent to
/// `numpy.fft.fftshift` for a square 2-D grid.
fn fftshift_2d(data: &mut [f32], n: usize) {
    let half = n / 2;
    for j in 0..half {
        for i in 0..half {
            data.swap(j * n + i, (j + half) * n + (i + half));
            data.swap(j * n + (i + half), (j + half) * n + i);
        }
    }
}

/// In-place transpose of a square `n × n` row-major matrix.
fn transpose_in_place(data: &mut [f32], n: usize) {
    for j in 0..n {
        for i in (j + 1)..n {
            data.swap(j * n + i, i * n + j);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_default_grid_64_patch_100() {
        let c = OceanConfig::default();
        assert_eq!(c.grid_size, 64);
        assert!((c.patch_size - 100.0).abs() < 1e-6);
        assert!((c.gravity - 9.81).abs() < 1e-6);
    }

    #[test]
    fn ifft_round_trip_reconstructs_input() {
        // 8-sample signal → FFT → IFFT must return the original.
        let original_re: Vec<f32> = (0..8).map(|i| i as f32 - 3.5).collect();
        let original_im = vec![0.0_f32; 8];
        let mut re = original_re.clone();
        let mut im = original_im.clone();
        fft_inplace(&mut re, &mut im);
        ifft_inplace(&mut re, &mut im);
        for i in 0..8 {
            assert!(
                (re[i] - original_re[i]).abs() < 1e-3,
                "re[{i}]: {} vs {}",
                re[i],
                original_re[i],
            );
            assert!(im[i].abs() < 1e-3);
        }
    }

    #[test]
    fn simulator_new_h0_has_nonzero_entries() {
        let sim = OceanSimulator::new(OceanConfig {
            grid_size: 16,
            ..OceanConfig::default()
        });
        let any_nonzero =
            sim.h0_re.iter().any(|x| x.abs() > 1e-9) || sim.h0_im.iter().any(|x| x.abs() > 1e-9);
        assert!(any_nonzero, "Phillips coefficients should be nonzero");
    }

    #[test]
    fn simulate_returns_grid_size_squared_buffers() {
        let mut sim = OceanSimulator::new(OceanConfig {
            grid_size: 32,
            ..OceanConfig::default()
        });
        let frame = sim.simulate(0.0);
        assert_eq!(frame.heights.len(), 32 * 32);
        assert_eq!(frame.normals.len(), 32 * 32);
        assert_eq!(frame.grid_size, 32);
    }

    #[test]
    fn simulate_normals_are_unit_length() {
        let mut sim = OceanSimulator::new(OceanConfig {
            grid_size: 32,
            ..OceanConfig::default()
        });
        let frame = sim.simulate(1.0);
        for (i, n) in frame.normals.iter().enumerate() {
            let len = n.length();
            assert!(
                (len - 1.0).abs() < 1e-3,
                "normal {i} has length {len}, expected ~1.0",
            );
        }
    }

    #[test]
    fn simulate_is_deterministic_for_same_time() {
        let mut a = OceanSimulator::new(OceanConfig {
            grid_size: 16,
            ..OceanConfig::default()
        });
        let mut b = OceanSimulator::new(OceanConfig {
            grid_size: 16,
            ..OceanConfig::default()
        });
        let h_a = a.simulate(2.5).heights.to_vec();
        let h_b = b.simulate(2.5).heights.to_vec();
        for (x, y) in h_a.iter().zip(h_b.iter()) {
            assert!((x - y).abs() < 1e-5, "{x} vs {y}");
        }
    }

    #[test]
    fn simulate_changes_over_time() {
        let mut sim = OceanSimulator::new(OceanConfig {
            grid_size: 16,
            ..OceanConfig::default()
        });
        let h0 = sim.simulate(0.0).heights.to_vec();
        let h1 = sim.simulate(1.5).heights.to_vec();
        let diff: f32 = h0.iter().zip(h1.iter()).map(|(a, b)| (a - b).abs()).sum();
        assert!(
            diff > 1e-3,
            "heights should evolve over time, sum |Δ| = {diff}"
        );
    }

    #[test]
    fn heights_average_close_to_zero() {
        let mut sim = OceanSimulator::new(OceanConfig {
            grid_size: 32,
            ..OceanConfig::default()
        });
        let frame = sim.simulate(0.5);
        let total: f32 = frame.heights.iter().sum();
        let avg = total / (frame.heights.len() as f32);
        assert!(avg.abs() < 1e-2, "avg height = {avg}, expected near 0");
    }

    #[test]
    fn wind_direction_zero_falls_back_to_x() {
        // Should not panic — internal normalisation falls back to +X.
        let mut sim = OceanSimulator::new(OceanConfig {
            grid_size: 16,
            wind_direction: Vec2::new(0.0, 0.0),
            ..OceanConfig::default()
        });
        let _ = sim.simulate(0.0);
    }

    #[test]
    fn higher_amplitude_produces_taller_waves() {
        let small_cfg = OceanConfig {
            grid_size: 16,
            amplitude: 0.0001,
            ..OceanConfig::default()
        };
        let big_cfg = OceanConfig {
            grid_size: 16,
            amplitude: 0.01,
            ..OceanConfig::default()
        };
        let mut small_sim = OceanSimulator::new(small_cfg);
        let mut big_sim = OceanSimulator::new(big_cfg);
        let small_rms = rms(small_sim.simulate(1.0).heights);
        let big_rms = rms(big_sim.simulate(1.0).heights);
        assert!(
            big_rms > small_rms,
            "big amplitude RMS {big_rms} should exceed small {small_rms}",
        );
    }

    fn rms(xs: &[f32]) -> f32 {
        let sum_sq: f32 = xs.iter().map(|x| x * x).sum();
        (sum_sq / (xs.len() as f32)).sqrt()
    }

    #[test]
    fn fftshift_then_fftshift_is_identity() {
        let mut data: Vec<f32> = (0..16).map(|i| i as f32).collect();
        let original = data.clone();
        fftshift_2d(&mut data, 4);
        fftshift_2d(&mut data, 4);
        assert_eq!(data, original);
    }
}
