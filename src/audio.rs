//! Spatial audio engine with HRTF, bus routing, and static-dispatch effects.
//!
//! Inspired by fyrox-sound's bus architecture and ping-pong buffer pattern,
//! but redesigned for ALICE's zero-allocation philosophy.
//!
//! ## Architecture
//!
//! `Source` → `AudioBus` (effects chain) → `PrimaryBus` → device output
//!
//! Effects use enum dispatch (no trait objects) for zero vtable overhead.

use crate::math::Vec3;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Sample buffer
// ---------------------------------------------------------------------------

/// Interleaved stereo sample buffer (left, right, left, right, ...).
#[derive(Debug, Clone)]
pub struct SampleBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl SampleBuffer {
    #[must_use]
    pub const fn new(sample_rate: u32, channels: u16) -> Self {
        Self {
            samples: Vec::new(),
            sample_rate,
            channels,
        }
    }

    /// Creates a zeroed buffer with given frame count.
    #[must_use]
    pub fn zeroed(sample_rate: u32, channels: u16, frames: usize) -> Self {
        Self {
            samples: vec![0.0; frames * channels as usize],
            sample_rate,
            channels,
        }
    }

    #[must_use]
    pub const fn frame_count(&self) -> usize {
        if self.channels == 0 {
            return 0;
        }
        self.samples.len() / self.channels as usize
    }

    #[must_use]
    pub fn duration_seconds(&self) -> f32 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.frame_count() as f32 / self.sample_rate as f32
    }

    /// Clears all samples to zero without deallocating.
    pub fn clear(&mut self) {
        self.samples.fill(0.0);
    }
}

// ---------------------------------------------------------------------------
// Effects (static dispatch via enum)
// ---------------------------------------------------------------------------

/// Audio effect — enum dispatch instead of trait objects.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Effect {
    Attenuate(Attenuate),
    LowPass(LowPassFilter),
    HighPass(HighPassFilter),
    Reverb(Reverb),
}

impl Effect {
    /// Process samples in-place.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        match self {
            Self::Attenuate(e) => e.process(input, output),
            Self::LowPass(e) => e.process(input, output),
            Self::HighPass(e) => e.process(input, output),
            Self::Reverb(e) => e.process(input, output),
        }
    }
}

/// Simple gain attenuation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attenuate {
    pub gain: f32,
}

impl Attenuate {
    fn process(&self, input: &[f32], output: &mut [f32]) {
        let len = input.len().min(output.len());
        for i in 0..len {
            output[i] = input[i] * self.gain;
        }
    }
}

/// First-order IIR low-pass filter.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LowPassFilter {
    pub cutoff: f32,
    #[serde(skip)]
    prev: f32,
}

impl LowPassFilter {
    #[must_use]
    pub const fn new(cutoff: f32) -> Self {
        Self { cutoff, prev: 0.0 }
    }

    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let len = input.len().min(output.len());
        let alpha = self.cutoff.clamp(0.0, 1.0);
        for i in 0..len {
            self.prev = alpha.mul_add(input[i], (1.0 - alpha) * self.prev);
            output[i] = self.prev;
        }
    }
}

/// First-order IIR high-pass filter.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HighPassFilter {
    pub cutoff: f32,
    #[serde(skip)]
    prev_input: f32,
    #[serde(skip)]
    prev_output: f32,
}

impl HighPassFilter {
    #[must_use]
    pub const fn new(cutoff: f32) -> Self {
        Self {
            cutoff,
            prev_input: 0.0,
            prev_output: 0.0,
        }
    }

    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let len = input.len().min(output.len());
        let alpha = (1.0 - self.cutoff).clamp(0.0, 1.0);
        for i in 0..len {
            self.prev_output = alpha * (self.prev_output + input[i] - self.prev_input);
            self.prev_input = input[i];
            output[i] = self.prev_output;
        }
    }
}

/// Simple feedback reverb (Schroeder-style comb filter).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reverb {
    pub decay: f32,
    pub delay_samples: usize,
    #[serde(skip)]
    buffer: Vec<f32>,
    #[serde(skip)]
    write_pos: usize,
}

impl Reverb {
    #[must_use]
    pub fn new(decay: f32, delay_samples: usize) -> Self {
        Self {
            decay,
            delay_samples,
            buffer: vec![0.0; delay_samples.max(1)],
            write_pos: 0,
        }
    }

    fn process(&mut self, input: &[f32], output: &mut [f32]) {
        if self.buffer.is_empty() {
            self.buffer = vec![0.0; self.delay_samples.max(1)];
        }
        let len = input.len().min(output.len());
        let buf_len = self.buffer.len();
        for i in 0..len {
            let delayed = self.buffer[self.write_pos];
            let out = delayed.mul_add(self.decay, input[i]);
            self.buffer[self.write_pos] = out;
            self.write_pos = (self.write_pos + 1) % buf_len;
            output[i] = out;
        }
    }
}

// ---------------------------------------------------------------------------
// AudioBus
// ---------------------------------------------------------------------------

/// An audio bus processes samples through a chain of effects.
/// Uses ping-pong buffers to avoid allocation during rendering.
#[derive(Debug, Clone)]
pub struct AudioBus {
    pub name: String,
    pub effects: Vec<Effect>,
    pub volume: f32,
    buffer_a: Vec<f32>,
    buffer_b: Vec<f32>,
    use_a: bool,
}

impl AudioBus {
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            effects: Vec::new(),
            volume: 1.0,
            buffer_a: Vec::new(),
            buffer_b: Vec::new(),
            use_a: true,
        }
    }

    /// Process input samples through the effects chain.
    pub fn process(&mut self, input: &[f32]) -> &[f32] {
        let len = input.len();
        self.buffer_a.resize(len, 0.0);
        self.buffer_b.resize(len, 0.0);
        self.buffer_a[..len].copy_from_slice(input);
        self.use_a = true;

        for effect in &mut self.effects {
            if self.use_a {
                effect.process(&self.buffer_a, &mut self.buffer_b);
            } else {
                effect.process(&self.buffer_b, &mut self.buffer_a);
            }
            self.use_a = !self.use_a;
        }

        // Apply volume
        let output = if self.use_a {
            &mut self.buffer_a
        } else {
            &mut self.buffer_b
        };
        for sample in output.iter_mut() {
            *sample *= self.volume;
        }

        if self.use_a {
            &self.buffer_a
        } else {
            &self.buffer_b
        }
    }
}

// ---------------------------------------------------------------------------
// AudioSource
// ---------------------------------------------------------------------------

/// A sound source that can be 2D or 3D (spatial).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSource {
    pub name: String,
    pub bus_name: String,
    pub volume: f32,
    pub pitch: f32,
    pub looping: bool,
    pub spatial: bool,
    pub position: Vec3,
    pub max_distance: f32,
    pub playing: bool,
    /// Playback cursor in samples.
    pub cursor: usize,
    /// Optional PCM buffer (mono, f32). When `None`, the source produces
    /// a constant signal at its volume level (useful for test tones).
    #[serde(skip)]
    pub pcm_buffer: Option<Vec<f32>>,
}

impl AudioSource {
    #[must_use]
    pub fn new(name: &str, bus_name: &str) -> Self {
        Self {
            name: name.to_string(),
            bus_name: bus_name.to_string(),
            volume: 1.0,
            pitch: 1.0,
            looping: false,
            spatial: false,
            position: Vec3::ZERO,
            max_distance: 50.0,
            playing: false,
            cursor: 0,
            pcm_buffer: None,
        }
    }

    /// Sets the PCM buffer for this source.
    pub fn set_pcm(&mut self, data: Vec<f32>) {
        self.pcm_buffer = Some(data);
        self.cursor = 0;
    }

    /// Reads `count` mono samples from the PCM buffer (or generates a
    /// constant signal if no buffer is attached). Advances the cursor.
    #[must_use]
    pub fn read_samples(&mut self, count: usize, gain: f32) -> Vec<f32> {
        let Some(ref buf) = self.pcm_buffer else {
            return vec![gain; count];
        };
        if buf.is_empty() {
            return vec![0.0; count];
        }
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            if self.cursor >= buf.len() {
                if self.looping {
                    self.cursor = 0;
                } else {
                    self.playing = false;
                    out.resize(count, 0.0);
                    return out;
                }
            }
            out.push(buf[self.cursor] * gain);
            self.cursor += 1;
        }
        out
    }

    /// Calculates distance-based attenuation (inverse distance).
    #[inline]
    #[must_use]
    pub fn distance_attenuation(&self, listener_pos: Vec3) -> f32 {
        if !self.spatial {
            return 1.0;
        }
        let dist = self.position.distance(listener_pos);
        if dist >= self.max_distance {
            return 0.0;
        }
        if dist < 1e-6 {
            return 1.0;
        }
        (1.0 - dist / self.max_distance).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// AudioEngine
// ---------------------------------------------------------------------------

/// The audio engine manages buses and sources.
pub struct AudioEngine {
    pub buses: Vec<AudioBus>,
    pub sources: Vec<AudioSource>,
    pub listener_position: Vec3,
    pub listener_forward: Vec3,
    pub master_volume: f32,
}

impl AudioEngine {
    #[must_use]
    pub fn new() -> Self {
        let mut engine = Self {
            buses: Vec::new(),
            sources: Vec::new(),
            listener_position: Vec3::ZERO,
            listener_forward: Vec3::new(0.0, 0.0, -1.0),
            master_volume: 1.0,
        };
        engine.buses.push(AudioBus::new("master"));
        engine
    }

    /// Adds a bus.
    pub fn add_bus(&mut self, bus: AudioBus) {
        self.buses.push(bus);
    }

    /// Finds a bus by name.
    #[must_use]
    pub fn find_bus(&self, name: &str) -> Option<usize> {
        self.buses.iter().position(|b| b.name == name)
    }

    /// Adds a source.
    pub fn add_source(&mut self, source: AudioSource) -> usize {
        self.sources.push(source);
        self.sources.len() - 1
    }

    /// Returns the number of playing sources.
    #[must_use]
    pub fn playing_count(&self) -> usize {
        self.sources.iter().filter(|s| s.playing).count()
    }

    /// Returns all bus names.
    #[must_use]
    pub fn bus_names(&self) -> Vec<&str> {
        self.buses.iter().map(|b| b.name.as_str()).collect()
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// BandPassFilter
// ---------------------------------------------------------------------------

/// Band-pass filter combining low-pass and high-pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandPassFilter {
    pub low_cutoff: f32,
    pub high_cutoff: f32,
    #[serde(skip)]
    lp: LowPassFilter,
    #[serde(skip)]
    hp: HighPassFilter,
}

impl BandPassFilter {
    #[must_use]
    pub const fn new(low_cutoff: f32, high_cutoff: f32) -> Self {
        Self {
            low_cutoff,
            high_cutoff,
            lp: LowPassFilter::new(high_cutoff),
            hp: HighPassFilter::new(low_cutoff),
        }
    }

    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        let mut temp = vec![0.0_f32; input.len()];
        self.lp.process(input, &mut temp);
        self.hp.process(&temp, output);
    }
}

// ---------------------------------------------------------------------------
// HRTF — interaural time difference
// ---------------------------------------------------------------------------

/// HRTF processor: applies interaural time delay (ITD) and
/// level difference (ILD) based on source angle relative to listener.
#[derive(Debug, Clone)]
pub struct HrtfProcessor {
    pub max_delay_samples: usize,
    delay_buffer_l: Vec<f32>,
    delay_buffer_r: Vec<f32>,
    write_pos: usize,
}

impl HrtfProcessor {
    #[must_use]
    pub fn new(max_delay_samples: usize) -> Self {
        let size = max_delay_samples.max(1);
        Self {
            max_delay_samples: size,
            delay_buffer_l: vec![0.0; size],
            delay_buffer_r: vec![0.0; size],
            write_pos: 0,
        }
    }

    /// Computes left/right gains and delay from a source direction.
    /// `angle` is in radians: 0 = front, PI/2 = right, -PI/2 = left.
    #[must_use]
    pub fn compute_params(&self, angle: f32) -> HrtfParams {
        let sin_a = angle.sin();
        let left_gain = (0.5 * (1.0 - sin_a)).clamp(0.0, 1.0);
        let right_gain = (0.5 * (1.0 + sin_a)).clamp(0.0, 1.0);
        let delay_frac = (sin_a.abs() * 0.5).clamp(0.0, 0.5);
        #[allow(clippy::cast_sign_loss)]
        let delay_samples = (delay_frac * self.max_delay_samples as f32) as usize;
        HrtfParams {
            left_gain,
            right_gain,
            left_delay: if sin_a > 0.0 { delay_samples } else { 0 },
            right_delay: if sin_a < 0.0 { delay_samples } else { 0 },
        }
    }

    /// Process mono input into stereo output using HRTF params.
    pub fn process(&mut self, mono_input: &[f32], params: &HrtfParams, stereo_output: &mut [f32]) {
        let buf_len = self.delay_buffer_l.len();
        for (i, &sample) in mono_input.iter().enumerate() {
            self.delay_buffer_l[self.write_pos] = sample;
            self.delay_buffer_r[self.write_pos] = sample;

            let l_idx = (self.write_pos + buf_len - params.left_delay) % buf_len;
            let r_idx = (self.write_pos + buf_len - params.right_delay) % buf_len;

            let out_idx = i * 2;
            if out_idx + 1 < stereo_output.len() {
                stereo_output[out_idx] = self.delay_buffer_l[l_idx] * params.left_gain;
                stereo_output[out_idx + 1] = self.delay_buffer_r[r_idx] * params.right_gain;
            }
            self.write_pos = (self.write_pos + 1) % buf_len;
        }
    }
}

/// HRTF processing parameters.
#[derive(Debug, Clone, Copy)]
pub struct HrtfParams {
    pub left_gain: f32,
    pub right_gain: f32,
    pub left_delay: usize,
    pub right_delay: usize,
}

// ---------------------------------------------------------------------------
// AudioEngine.render — full pipeline
// ---------------------------------------------------------------------------

impl AudioEngine {
    /// Renders all playing sources through their assigned buses.
    /// Sources with a `pcm_buffer` read from it; sources without one
    /// produce a constant signal at their volume level.
    pub fn render(&mut self, frames: usize, sample_rate: u32) -> SampleBuffer {
        let mut output = SampleBuffer::zeroed(sample_rate, 2, frames);

        let bus_map: Vec<Option<usize>> = self
            .sources
            .iter()
            .map(|s| self.buses.iter().position(|b| b.name == s.bus_name))
            .collect();

        let listener = self.listener_position;
        let listener_fwd = self.listener_forward;
        let master = self.master_volume;

        for (si, source) in self.sources.iter_mut().enumerate() {
            if !source.playing {
                continue;
            }
            let atten = source.distance_attenuation(listener) * source.volume;
            if atten < 1e-6 {
                continue;
            }
            let mono = source.read_samples(frames, atten);

            // Process through bus effects chain.
            let processed = if let Some(bus_idx) = bus_map[si] {
                self.buses[bus_idx].process(&mono).to_vec()
            } else {
                mono
            };

            // Spatial panning: compute left/right gain from angle.
            let (gain_l, gain_r) = if source.spatial {
                let to_source = source.position - listener;
                let dist = to_source.length();
                if dist > 1e-6 {
                    let dir = to_source * (1.0 / dist);
                    let right = listener_fwd.cross(Vec3::Y).normalize();
                    let pan = dir.dot(right).clamp(-1.0, 1.0);
                    // Equal-power panning
                    let angle = (pan + 1.0) * 0.5 * std::f32::consts::FRAC_PI_2;
                    (angle.cos(), angle.sin())
                } else {
                    (0.707, 0.707)
                }
            } else {
                (0.707, 0.707)
            };

            for (i, &s) in processed.iter().enumerate() {
                let out_idx = i * 2;
                if out_idx + 1 < output.samples.len() {
                    let scaled = s * master;
                    output.samples[out_idx] += scaled * gain_l;
                    output.samples[out_idx + 1] += scaled * gain_r;
                }
            }
        }
        output
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_buffer_zeroed() {
        let buf = SampleBuffer::zeroed(44100, 2, 100);
        assert_eq!(buf.frame_count(), 100);
        assert_eq!(buf.samples.len(), 200);
    }

    #[test]
    fn sample_buffer_duration() {
        let buf = SampleBuffer::zeroed(44100, 2, 44100);
        assert!((buf.duration_seconds() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn sample_buffer_clear() {
        let mut buf = SampleBuffer::zeroed(44100, 1, 10);
        buf.samples[0] = 1.0;
        buf.clear();
        assert_eq!(buf.samples[0], 0.0);
    }

    #[test]
    fn sample_buffer_zero_channels() {
        let buf = SampleBuffer::new(44100, 0);
        assert_eq!(buf.frame_count(), 0);
    }

    #[test]
    fn attenuate_effect() {
        let input = [1.0, 0.5, -0.5];
        let mut output = [0.0; 3];
        let att = Attenuate { gain: 0.5 };
        att.process(&input, &mut output);
        assert_eq!(output, [0.5, 0.25, -0.25]);
    }

    #[test]
    fn lowpass_filter() {
        let mut lp = LowPassFilter::new(0.1);
        let input = [1.0; 10];
        let mut output = [0.0; 10];
        lp.process(&input, &mut output);
        // Output should converge towards 1.0
        assert!(output[9] > output[0]);
        assert!(output[9] < 1.0);
    }

    #[test]
    fn highpass_filter() {
        let mut hp = HighPassFilter::new(0.1);
        let input = [1.0; 10];
        let mut output = [0.0; 10];
        hp.process(&input, &mut output);
        // DC signal should be attenuated
        assert!(output[9].abs() < 1.0);
    }

    #[test]
    fn reverb_effect() {
        let mut rev = Reverb::new(0.5, 4);
        let input = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let mut output = [0.0; 8];
        rev.process(&input, &mut output);
        assert_eq!(output[0], 1.0);
        // Delayed feedback should appear
        assert!(output[4] > 0.0);
    }

    #[test]
    fn audio_bus_passthrough() {
        let mut bus = AudioBus::new("test");
        let input = [0.5, -0.5, 0.25];
        let output = bus.process(&input);
        assert_eq!(output.len(), 3);
        assert_eq!(output[0], 0.5);
    }

    #[test]
    fn audio_bus_with_effects() {
        let mut bus = AudioBus::new("fx");
        bus.effects.push(Effect::Attenuate(Attenuate { gain: 0.5 }));
        let input = [1.0, 1.0, 1.0];
        let output = bus.process(&input);
        assert!((output[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn audio_bus_volume() {
        let mut bus = AudioBus::new("quiet");
        bus.volume = 0.25;
        let input = [1.0, 1.0];
        let output = bus.process(&input);
        assert!((output[0] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn audio_source_2d_attenuation() {
        let src = AudioSource::new("click", "master");
        let atten = src.distance_attenuation(Vec3::new(100.0, 0.0, 0.0));
        assert_eq!(atten, 1.0);
    }

    #[test]
    fn audio_source_3d_attenuation() {
        let mut src = AudioSource::new("step", "master");
        src.spatial = true;
        src.max_distance = 100.0;
        src.position = Vec3::ZERO;
        let atten = src.distance_attenuation(Vec3::new(50.0, 0.0, 0.0));
        assert!((atten - 0.5).abs() < 1e-5);
    }

    #[test]
    fn audio_source_beyond_max_distance() {
        let mut src = AudioSource::new("far", "master");
        src.spatial = true;
        src.max_distance = 10.0;
        let atten = src.distance_attenuation(Vec3::new(20.0, 0.0, 0.0));
        assert_eq!(atten, 0.0);
    }

    #[test]
    fn audio_engine_default_has_master() {
        let engine = AudioEngine::new();
        assert_eq!(engine.buses.len(), 1);
        assert_eq!(engine.buses[0].name, "master");
    }

    #[test]
    fn audio_engine_add_source() {
        let mut engine = AudioEngine::new();
        let idx = engine.add_source(AudioSource::new("test", "master"));
        assert_eq!(idx, 0);
    }

    #[test]
    fn audio_engine_playing_count() {
        let mut engine = AudioEngine::new();
        let mut src = AudioSource::new("a", "master");
        src.playing = true;
        engine.add_source(src);
        engine.add_source(AudioSource::new("b", "master"));
        assert_eq!(engine.playing_count(), 1);
    }

    #[test]
    fn audio_engine_find_bus() {
        let mut engine = AudioEngine::new();
        engine.add_bus(AudioBus::new("sfx"));
        assert_eq!(engine.find_bus("sfx"), Some(1));
        assert_eq!(engine.find_bus("nonexistent"), None);
    }

    #[test]
    fn audio_engine_bus_names() {
        let mut engine = AudioEngine::new();
        engine.add_bus(AudioBus::new("music"));
        let names = engine.bus_names();
        assert!(names.contains(&"master"));
        assert!(names.contains(&"music"));
    }

    #[test]
    fn effect_enum_dispatch() {
        let mut eff = Effect::Attenuate(Attenuate { gain: 2.0 });
        let input = [0.5];
        let mut output = [0.0];
        eff.process(&input, &mut output);
        assert_eq!(output[0], 1.0);
    }

    #[test]
    fn reverb_zero_delay() {
        let mut rev = Reverb::new(0.5, 0);
        let input = [1.0, 0.5];
        let mut output = [0.0; 2];
        rev.process(&input, &mut output);
        // Should not panic
        assert!(output[0].is_finite());
    }

    #[test]
    fn ping_pong_multiple_effects() {
        let mut bus = AudioBus::new("chain");
        bus.effects.push(Effect::Attenuate(Attenuate { gain: 0.5 }));
        bus.effects.push(Effect::Attenuate(Attenuate { gain: 0.5 }));
        let input = [4.0, 4.0];
        let output = bus.process(&input);
        assert!((output[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn bandpass_filter() {
        let mut bp = BandPassFilter::new(0.1, 0.9);
        let input = [1.0; 20];
        let mut output = [0.0; 20];
        bp.process(&input, &mut output);
        // Should produce some filtered output
        assert!(output.iter().any(|&s| s != 0.0));
    }

    #[test]
    fn hrtf_front_balanced() {
        let hrtf = HrtfProcessor::new(16);
        let params = hrtf.compute_params(0.0); // Front
        assert!((params.left_gain - 0.5).abs() < 1e-3);
        assert!((params.right_gain - 0.5).abs() < 1e-3);
    }

    #[test]
    fn hrtf_right_louder() {
        let hrtf = HrtfProcessor::new(16);
        let params = hrtf.compute_params(std::f32::consts::FRAC_PI_2); // Right
        assert!(params.right_gain > params.left_gain);
    }

    #[test]
    fn hrtf_left_louder() {
        let hrtf = HrtfProcessor::new(16);
        let params = hrtf.compute_params(-std::f32::consts::FRAC_PI_2); // Left
        assert!(params.left_gain > params.right_gain);
    }

    #[test]
    fn hrtf_process_stereo() {
        let mut hrtf = HrtfProcessor::new(16);
        let mono = [1.0_f32; 8];
        let params = hrtf.compute_params(0.3);
        let mut stereo = [0.0_f32; 16];
        hrtf.process(&mono, &params, &mut stereo);
        // Should produce non-zero stereo output
        assert!(stereo.iter().any(|&s| s != 0.0));
    }

    #[test]
    fn hrtf_delay_right() {
        let hrtf = HrtfProcessor::new(32);
        let params = hrtf.compute_params(std::f32::consts::FRAC_PI_2);
        assert!(params.left_delay > 0);
        assert_eq!(params.right_delay, 0);
    }

    #[test]
    fn engine_render_silent() {
        let mut engine = AudioEngine::new();
        let output = engine.render(128, 44100);
        assert_eq!(output.samples.len(), 256);
        assert!(output.samples.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn engine_render_with_source() {
        let mut engine = AudioEngine::new();
        let mut src = AudioSource::new("test", "master");
        src.playing = true;
        src.volume = 0.5;
        engine.add_source(src);
        let output = engine.render(64, 44100);
        assert!(output.samples.iter().any(|&s| s > 0.0));
    }

    #[test]
    fn engine_render_respects_master_volume() {
        let mut engine = AudioEngine::new();
        engine.master_volume = 0.0;
        let mut src = AudioSource::new("test", "master");
        src.playing = true;
        engine.add_source(src);
        let output = engine.render(64, 44100);
        assert!(output.samples.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn engine_render_spatial_attenuation() {
        let mut engine = AudioEngine::new();
        let mut src = AudioSource::new("far", "master");
        src.playing = true;
        src.spatial = true;
        src.max_distance = 10.0;
        src.position = Vec3::new(100.0, 0.0, 0.0);
        engine.add_source(src);
        let output = engine.render(64, 44100);
        assert!(output.samples.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn source_pcm_playback() {
        let mut src = AudioSource::new("test", "master");
        src.set_pcm(vec![0.5, 1.0, -0.5, 0.0]);
        src.playing = true;
        let samples = src.read_samples(4, 1.0);
        assert_eq!(samples.len(), 4);
        assert!((samples[0] - 0.5).abs() < 1e-6);
        assert!((samples[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn source_pcm_looping() {
        let mut src = AudioSource::new("test", "master");
        src.set_pcm(vec![1.0, 2.0]);
        src.playing = true;
        src.looping = true;
        let samples = src.read_samples(5, 1.0);
        assert_eq!(samples.len(), 5);
        assert!((samples[0] - 1.0).abs() < 1e-6);
        assert!((samples[2] - 1.0).abs() < 1e-6); // looped back
    }

    #[test]
    fn source_pcm_stops_when_done() {
        let mut src = AudioSource::new("test", "master");
        src.set_pcm(vec![1.0, 2.0]);
        src.playing = true;
        src.looping = false;
        let samples = src.read_samples(5, 1.0);
        assert!(!src.playing);
        assert_eq!(samples[4], 0.0); // padded with silence
    }

    #[test]
    fn source_no_pcm_constant_signal() {
        let mut src = AudioSource::new("test", "master");
        src.playing = true;
        let samples = src.read_samples(10, 0.75);
        assert!(samples.iter().all(|&s| (s - 0.75).abs() < 1e-6));
    }

    #[test]
    fn engine_render_with_pcm() {
        let mut engine = AudioEngine::new();
        let mut src = AudioSource::new("pcm", "master");
        src.set_pcm(vec![0.5; 64]);
        src.playing = true;
        engine.add_source(src);
        let output = engine.render(64, 44100);
        assert!(output.samples.iter().any(|&s| s > 0.0));
    }

    #[test]
    fn engine_spatial_panning() {
        let mut engine = AudioEngine::new();
        engine.listener_forward = Vec3::new(0.0, 0.0, -1.0);
        let mut src = AudioSource::new("right", "master");
        src.playing = true;
        src.spatial = true;
        src.position = Vec3::new(10.0, 0.0, 0.0);
        src.max_distance = 50.0;
        engine.add_source(src);
        let output = engine.render(16, 44100);
        // Source is to the right → right channel should be louder
        let left_sum: f32 = output.samples.iter().step_by(2).sum();
        let right_sum: f32 = output.samples.iter().skip(1).step_by(2).sum();
        assert!(right_sum > left_sum);
    }

    #[test]
    fn engine_spatial_left_panning() {
        let mut engine = AudioEngine::new();
        engine.listener_forward = Vec3::new(0.0, 0.0, -1.0);
        let mut src = AudioSource::new("left", "master");
        src.playing = true;
        src.spatial = true;
        src.position = Vec3::new(-10.0, 0.0, 0.0);
        src.max_distance = 50.0;
        engine.add_source(src);
        let output = engine.render(16, 44100);
        let left_sum: f32 = output.samples.iter().step_by(2).sum();
        let right_sum: f32 = output.samples.iter().skip(1).step_by(2).sum();
        assert!(left_sum > right_sum);
    }
}
