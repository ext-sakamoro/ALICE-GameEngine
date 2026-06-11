//! cpal-backed audio output device. Streams samples produced by
//! [`crate::audio::AudioEngine::render`] (or any compatible producer) to
//! the host's default output device — typically the speakers / headphones.
//!
//! Only compiled when the `audio_output` Cargo feature is enabled (which
//! pulls in `cpal` as an optional dep). On a machine without an audio
//! device the constructor returns an error rather than panicking, so this
//! module is safe to ship into CI environments.

use std::sync::mpsc::{Receiver, SyncSender};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

/// Errors returned by [`AudioOutput::start`].
#[derive(Debug)]
pub enum AudioOutputError {
    NoOutputDevice,
    UnsupportedConfig(String),
    BuildStream(String),
    Play(String),
}

impl std::fmt::Display for AudioOutputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoOutputDevice => f.write_str("no default output device"),
            Self::UnsupportedConfig(m) => write!(f, "unsupported audio config: {m}"),
            Self::BuildStream(m) => write!(f, "build stream: {m}"),
            Self::Play(m) => write!(f, "play: {m}"),
        }
    }
}

impl std::error::Error for AudioOutputError {}

/// Active cpal output stream + a producer-side sender to push interleaved
/// stereo samples for playback. Dropping the [`AudioOutput`] tears down
/// the stream and stops playback.
pub struct AudioOutput {
    /// Producer side: send interleaved stereo `(l, r, l, r, ...)` frames.
    /// Backpressure is bounded — full sends block. For a real-time loop
    /// that should never stall, wrap this in a `try_send` ring buffer.
    pub producer: SyncSender<Vec<f32>>,
    /// Final sample rate negotiated with the device.
    pub sample_rate: u32,
    /// `2` for stereo, `1` for mono fallback.
    pub channels: u16,
    _stream: cpal::Stream,
}

impl AudioOutput {
    /// Open the default output device and start playback. `buffer_frames`
    /// is the cpal-side hint for chunk size (lower → less latency, more
    /// CPU; common range: 256..1024).
    ///
    /// # Errors
    /// Returns [`AudioOutputError`] if no output device is available, the
    /// host won't accept the requested configuration, or the stream fails
    /// to build / play.
    pub fn start(buffer_frames: u32) -> Result<Self, AudioOutputError> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or(AudioOutputError::NoOutputDevice)?;
        let supported = device
            .default_output_config()
            .map_err(|e| AudioOutputError::UnsupportedConfig(format!("{e:?}")))?;
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels();
        let mut config: cpal::StreamConfig = supported.config();
        config.buffer_size = cpal::BufferSize::Fixed(buffer_frames);

        // 4-frame backpressure window — drops if producer is too slow.
        let (sender, receiver): (SyncSender<Vec<f32>>, Receiver<Vec<f32>>) =
            std::sync::mpsc::sync_channel(4);
        let mut pending: Vec<f32> = Vec::new();
        let mut pending_cursor = 0usize;

        let err_fn = |err| eprintln!("audio output stream error: {err:?}");
        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
                    let mut i = 0;
                    while i < data.len() {
                        if pending_cursor >= pending.len() {
                            // Pull next chunk; if none is ready, fill rest
                            // with silence and bail.
                            match receiver.try_recv() {
                                Ok(next) => {
                                    pending = next;
                                    pending_cursor = 0;
                                }
                                Err(_) => {
                                    for s in &mut data[i..] {
                                        *s = 0.0;
                                    }
                                    return;
                                }
                            }
                        }
                        let take = (data.len() - i).min(pending.len() - pending_cursor);
                        data[i..i + take]
                            .copy_from_slice(&pending[pending_cursor..pending_cursor + take]);
                        pending_cursor += take;
                        i += take;
                    }
                },
                err_fn,
                None,
            )
            .map_err(|e| AudioOutputError::BuildStream(format!("{e:?}")))?;
        stream
            .play()
            .map_err(|e| AudioOutputError::Play(format!("{e:?}")))?;
        Ok(Self {
            producer: sender,
            sample_rate,
            channels,
            _stream: stream,
        })
    }

    /// Push a sample frame for playback. The frame must be in interleaved
    /// channel order — `[l, r, l, r, ...]` for stereo. Returns
    /// `Err(())` if the audio thread has stopped accepting frames
    /// (i.e. the stream was dropped or the channel is full enough that
    /// the receiver hasn't been polled).
    ///
    /// # Errors
    /// Returns `Err(())` when the bounded channel is full or the audio
    /// thread has been dropped.
    pub fn push_frames(&self, frames: Vec<f32>) -> Result<(), ()> {
        self.producer.try_send(frames).map_err(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cpal` won't necessarily find an output device on every CI
    /// runner, so this test is `#[ignore]`-by-default.
    #[test]
    #[ignore = "needs a working audio output device"]
    fn open_default_output_succeeds() {
        let out = AudioOutput::start(512).expect("open");
        assert!(out.sample_rate > 0);
        assert!(out.channels >= 1);
        // Push one tiny frame and immediately drop the output so we don't
        // actually play anything that would pollute a quiet CI run.
        let _ = out.push_frames(vec![0.0; 64]);
    }
}
