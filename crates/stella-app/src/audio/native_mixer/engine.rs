//! Purple integer mixer and clip-instance lifecycle.
//!
//! This follows the executable boundary from AudioOutputImpl's queue/control
//! facade into `sub_100573424`/`sub_1005738C8`.

use std::collections::{BTreeMap, BTreeSet};

use stella_script::AudioPlaybackState;

use super::NativeMixerConfiguration;

mod reader;

#[cfg(test)]
pub(super) use reader::NativeReader;
pub(super) use reader::{NativeClip, decode_asset};

pub(super) struct MixerState {
    pub(super) configuration: NativeMixerConfiguration,
    pub(super) started: bool,
    pub(super) track_volumes: [f32; 8],
    pub(super) playbacks: BTreeMap<i64, NativePlayback>,
    pub(super) finished: BTreeSet<i64>,
    pub(super) completed: BTreeSet<i64>,
}

impl MixerState {
    pub(super) fn new(configuration: NativeMixerConfiguration) -> Self {
        Self {
            configuration,
            started: false,
            track_volumes: [1.0; 8],
            playbacks: BTreeMap::new(),
            finished: BTreeSet::new(),
            completed: BTreeSet::new(),
        }
    }

    pub(super) fn fill_block(&mut self) -> Vec<f32> {
        let finished = self
            .playbacks
            .iter()
            .filter_map(|(handle, playback)| playback.finished.then_some(*handle))
            .collect::<Vec<_>>();
        for handle in finished {
            self.playbacks.remove(&handle);
            self.completed.insert(handle);
        }

        let sample_bytes = usize::from(self.configuration.bits_per_sample / 8);
        let sample_count = self.configuration.buffer_bytes as usize / sample_bytes;
        if !self.started {
            return vec![0.0; sample_count];
        }
        let output = match self.configuration.bits_per_sample {
            8 => self.mix_u8(sample_count),
            16 => self.mix_i16(sample_count),
            _ => unreachable!("validated native bit depth"),
        };
        self.finished.extend(
            self.playbacks
                .iter()
                .filter_map(|(handle, playback)| playback.finished.then_some(*handle)),
        );
        output
    }

    fn mix_i16(&mut self, sample_count: usize) -> Vec<f32> {
        let mut accumulator = vec![0_i32; sample_count];
        let output_channels = self.configuration.channels;
        let output_bytes = self.configuration.buffer_bytes as usize;
        for playback in self.playbacks.values_mut() {
            if playback.finished || playback.clip.bits_per_sample != 16 {
                continue;
            }
            let Some(requested) =
                converted_read_size(output_bytes, output_channels, playback.clip.channels)
            else {
                continue;
            };
            let bytes = playback.read(requested);
            let track_volume = track_volume(&self.track_volumes, playback.track);
            let gain = native_gain(playback.volume, track_volume, 4_096.0);
            if gain < 1 {
                continue;
            }
            let source = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|bytes| i16::from_le_bytes(*bytes));
            match (output_channels, playback.clip.channels) {
                (2, 1) => {
                    for (output, sample) in
                        accumulator.as_chunks_mut::<2>().0.iter_mut().zip(source)
                    {
                        let scaled = scale(sample.into(), gain, 12);
                        output[0] = output[0].wrapping_add(scaled);
                        output[1] = output[1].wrapping_add(scaled);
                    }
                }
                (1, 2) => {
                    let samples = source.collect::<Vec<_>>();
                    for (output, input) in accumulator
                        .iter_mut()
                        .zip(samples.as_chunks::<2>().0.iter())
                    {
                        let left = scale(input[0].into(), gain, 13);
                        let right = scale(input[1].into(), gain, 13);
                        *output = output.wrapping_add(left).wrapping_add(right);
                    }
                }
                _ => {
                    for (output, sample) in accumulator.iter_mut().zip(source) {
                        *output = output.wrapping_add(scale(sample.into(), gain, 12));
                    }
                }
            }
        }
        accumulator
            .into_iter()
            .map(|sample| {
                f32::from(sample.clamp(i16::MIN.into(), i16::MAX.into()) as i16) / 32_768.0
            })
            .collect()
    }

    fn mix_u8(&mut self, sample_count: usize) -> Vec<f32> {
        let mut accumulator = vec![0_i32; sample_count];
        let output_channels = self.configuration.channels;
        let output_bytes = self.configuration.buffer_bytes as usize;
        for playback in self.playbacks.values_mut() {
            if playback.finished || playback.clip.bits_per_sample != 8 {
                continue;
            }
            let Some(requested) =
                converted_read_size(output_bytes, output_channels, playback.clip.channels)
            else {
                continue;
            };
            let bytes = playback.read(requested);
            let track_volume = track_volume(&self.track_volumes, playback.track);
            let gain = native_gain(playback.volume, track_volume, 256.0);
            if gain < 1 {
                continue;
            }
            match (output_channels, playback.clip.channels) {
                // These two conversion branches intentionally preserve the
                // target's missing unsigned-PCM centering subtraction.
                (2, 1) => {
                    for (output, sample) in accumulator
                        .as_chunks_mut::<2>()
                        .0
                        .iter_mut()
                        .zip(bytes.iter())
                    {
                        let scaled = scale(i32::from(*sample), gain, 8);
                        output[0] = output[0].wrapping_add(scaled);
                        output[1] = output[1].wrapping_add(scaled);
                    }
                }
                (1, 2) => {
                    for (output, input) in
                        accumulator.iter_mut().zip(bytes.as_chunks::<2>().0.iter())
                    {
                        let left = scale(i32::from(input[0]), gain, 9);
                        let right = scale(i32::from(input[1]), gain, 9);
                        *output = output.wrapping_add(left).wrapping_add(right);
                    }
                }
                _ => {
                    for (output, sample) in accumulator.iter_mut().zip(bytes.iter()) {
                        let centered = i32::from(*sample) - 128;
                        *output = output.wrapping_add(scale(centered, gain, 8));
                    }
                }
            }
        }
        accumulator
            .into_iter()
            .map(|sample| {
                let encoded = native_u8_saturate(sample);
                (f32::from(encoded) - 128.0) / 128.0
            })
            .collect()
    }
}

fn converted_read_size(
    output_bytes: usize,
    output_channels: u16,
    source_channels: u16,
) -> Option<usize> {
    match (output_channels, source_channels) {
        (output, source) if output == source => Some(output_bytes),
        (2, 1) => Some(output_bytes / 2),
        (1, 2) => output_bytes.checked_mul(2),
        _ => None,
    }
}

fn track_volume(volumes: &[f32; 8], track: i32) -> f32 {
    usize::try_from(track)
        .ok()
        .and_then(|track| volumes.get(track))
        .copied()
        .unwrap_or(0.0)
}

pub(super) fn native_gain(instance: f32, track: f32, scale: f32) -> i32 {
    let gain = (instance * track) * scale;
    // 0x1005735F8 / 0x100573A94 use FCVTZS W25,S0 after two FMULs.
    // ARM64 saturates signed overflow and converts NaN to zero.
    gain as i32
}

fn scale(sample: i32, gain: i32, shift: u32) -> i32 {
    sample.wrapping_mul(gain) >> shift
}

/// Scalar tail emitted by `sub_1005738C8`. This is deliberately the target's
/// branchless byte expression rather than a conventional clamp: at the upper
/// linear boundary it produces 254 and saturates to 255 only above it.
pub(super) fn native_u8_saturate(sample: i32) -> u8 {
    let low = sample.wrapping_add(128) as u32;
    let high = 127_i32.wrapping_sub(sample) as u32;
    ((low & !(low >> 7)) | (high >> 7)) as u8
}

pub(super) struct NativePlayback {
    pub(super) clip: NativeClip,
    pub(super) volume: f32,
    pub(super) looping: bool,
    pub(super) track: i32,
    pub(super) finished: bool,
}

impl NativePlayback {
    pub(super) fn new(playback: AudioPlaybackState, clip: NativeClip) -> Self {
        Self {
            clip,
            volume: playback.volume,
            looping: playback.looping,
            track: playback.track,
            finished: playback.finished,
        }
    }

    pub(super) fn read(&mut self, requested: usize) -> Vec<u8> {
        let mut output = Vec::with_capacity(requested);
        loop {
            let remaining = requested - output.len();
            let read = self.clip.reader.read_into(&mut output, remaining);
            if read == 0 {
                if self.looping {
                    // The native function resets its shared reader state and
                    // retries forever. Guard the pathological empty-reader
                    // case so a corrupt looping asset cannot wedge the host
                    // audio callback.
                    if !self.clip.reader.reset() {
                        break;
                    }
                } else {
                    self.finished = true;
                    break;
                }
            }
            // sub_100571E54 performs a second read after a short result only
            // for looping instances. Non-looping clips therefore expose a
            // silent block tail at every composite-child boundary.
            if !self.looping || output.len() >= requested {
                break;
            }
        }
        output
    }
}
