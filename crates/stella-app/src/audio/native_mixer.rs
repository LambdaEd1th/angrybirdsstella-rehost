//! Integer PCM mixer reconstructed from Purple 1.1.6.
//!
//! `sub_100573424` and `sub_1005738C8` mix every native output block into a
//! signed 32-bit accumulator. Clip and track gain are quantized before the
//! multiply; saturation happens once after all instances have contributed.

use std::{
    collections::{BTreeSet, VecDeque},
    num::NonZero,
    sync::{Arc, Mutex},
    time::Duration,
};

use rodio::{ChannelCount, SampleRate, Source};

use super::{AudioOutputState, AudioPlaybackTransitions};

mod engine;

use engine::{MixerState, NativeClip, NativePlayback, decode_asset};
#[cfg(test)]
use engine::{NativeReader, native_gain, native_u8_saturate};
#[cfg(test)]
use stella_script::{AudioAssetSource, AudioPlaybackState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeMixerConfiguration {
    channels: u16,
    bits_per_sample: u16,
    sample_rate: u32,
    buffer_bytes: u32,
}

impl NativeMixerConfiguration {
    pub(super) fn from_state(state: &AudioOutputState) -> Option<Self> {
        let configuration = Self {
            channels: state.channels,
            bits_per_sample: state.bits_per_sample,
            sample_rate: state.sample_rate,
            buffer_bytes: state.buffer_bytes,
        };
        let bytes_per_frame = u32::from(configuration.channels)
            .checked_mul(u32::from(configuration.bits_per_sample / 8));
        (matches!(configuration.channels, 1 | 2)
            && matches!(configuration.bits_per_sample, 8 | 16)
            && configuration.sample_rate > 0
            && bytes_per_frame.is_some_and(|bytes| {
                bytes > 0
                    && configuration.buffer_bytes >= bytes
                    && configuration.buffer_bytes.is_multiple_of(bytes)
            }))
        .then_some(configuration)
    }
}

#[derive(Clone)]
pub(super) struct NativeMixerControl {
    shared: Arc<Mutex<MixerState>>,
}

pub(super) struct NativeMixerSource {
    shared: Arc<Mutex<MixerState>>,
    configuration: NativeMixerConfiguration,
    current_block: Vec<f32>,
    queued_blocks: VecDeque<Vec<f32>>,
    cursor: usize,
    processed_blocks: usize,
    channel_cursor: u16,
    worker_poll_frame_nanos: u128,
}

pub(super) fn create(configuration: NativeMixerConfiguration) -> NativeMixerControl {
    let shared = Arc::new(Mutex::new(MixerState::new(configuration)));
    NativeMixerControl { shared }
}

impl NativeMixerControl {
    /// Drain worker-produced edges without reconciling the VM snapshot again.
    /// This is used after synchronous startup prefill: the caller has not yet
    /// applied the first reconcile's removals, so a second reconcile against
    /// that stale snapshot could otherwise recreate the removed handle.
    pub(super) fn take_transitions(&self) -> AudioPlaybackTransitions {
        let mut mixer = self.shared.lock().expect("native mixer lock poisoned");
        AudioPlaybackTransitions {
            finished: std::mem::take(&mut mixer.finished).into_iter().collect(),
            removed: std::mem::take(&mut mixer.completed).into_iter().collect(),
        }
    }

    /// Purple fills all six OpenAL buffers synchronously before starting its
    /// one source. Reader cursors and completed-instance removal therefore run
    /// six blocks ahead of audible playback at every output start.
    pub(super) fn source(&self, prefill_blocks: usize) -> NativeMixerSource {
        let mut mixer = self.shared.lock().expect("native mixer lock poisoned");
        let configuration = mixer.configuration;
        let mut queued_blocks = VecDeque::with_capacity(prefill_blocks.max(1));
        for _ in 0..prefill_blocks {
            queued_blocks.push_back(mixer.fill_block());
        }
        let current_block = queued_blocks
            .pop_front()
            .unwrap_or_else(|| mixer.fill_block());
        NativeMixerSource {
            shared: Arc::clone(&self.shared),
            configuration,
            current_block,
            queued_blocks,
            cursor: 0,
            processed_blocks: 0,
            channel_cursor: 0,
            worker_poll_frame_nanos: 0,
        }
    }

    /// Reconcile the VM-owned instance vector with the asynchronous mixer.
    /// EOF and next-block removal are returned as distinct edges, matching
    /// AudioClipInstance `+0x1E` and `sub_100573250` respectively.
    pub(super) fn synchronize(&self, state: &AudioOutputState) -> AudioPlaybackTransitions {
        let live = state
            .playbacks
            .iter()
            .map(|playback| playback.handle)
            .collect::<BTreeSet<_>>();

        let (mut finished, mut completed, missing) = {
            let mut mixer = self.shared.lock().expect("native mixer lock poisoned");
            mixer.started = state.started;
            mixer.track_volumes = state.track_volumes;
            let finished = std::mem::take(&mut mixer.finished);
            let completed = std::mem::take(&mut mixer.completed);
            mixer.playbacks.retain(|handle, _| live.contains(handle));
            for playback in &state.playbacks {
                if let Some(active) = mixer.playbacks.get_mut(&playback.handle) {
                    active.volume = playback.volume;
                    active.looping = playback.looping;
                    active.track = playback.track;
                    active.finished |= playback.finished;
                }
            }
            let missing = state
                .playbacks
                .iter()
                .filter(|playback| {
                    !completed.contains(&playback.handle)
                        && !mixer.playbacks.contains_key(&playback.handle)
                })
                .cloned()
                .collect::<Vec<_>>();
            (finished, completed, missing)
        };

        // Decoding may touch a compressed stream. Keep it outside the audio
        // callback lock so a newly requested music track cannot starve a block.
        let decoded = missing
            .into_iter()
            .map(|playback| {
                let clip = playback.source.as_ref().map(decode_asset).transpose();
                (playback, clip)
            })
            .collect::<Vec<_>>();

        let mut mixer = self.shared.lock().expect("native mixer lock poisoned");
        for (playback, clip) in decoded {
            if !live.contains(&playback.handle) || mixer.playbacks.contains_key(&playback.handle) {
                continue;
            }
            match clip {
                Ok(Some(clip)) => {
                    mixer
                        .playbacks
                        .insert(playback.handle, NativePlayback::new(playback, clip));
                }
                Ok(None) | Err(_) if playback.looping => {
                    // Native `sub_100571E54` never marks an empty looping
                    // reader finished. Represent that frozen edge without
                    // reproducing its unbounded reset/read loop on this host.
                    mixer.playbacks.insert(
                        playback.handle,
                        NativePlayback::new(playback, NativeClip::empty()),
                    );
                }
                Ok(None) | Err(_) => {
                    finished.insert(playback.handle);
                    completed.insert(playback.handle);
                }
            }
        }
        AudioPlaybackTransitions {
            finished: finished.into_iter().collect(),
            removed: completed.into_iter().collect(),
        }
    }
}

impl Iterator for NativeMixerSource {
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor == self.current_block.len() {
            self.current_block = self.queued_blocks.pop_front().unwrap_or_else(|| {
                // A valid Purple output block is longer than the 10 ms worker
                // sleep and cannot drain all six queued buffers between
                // polls. Keep malformed host snapshots non-panicking.
                vec![0.0; self.configuration.channels.into()]
            });
            self.cursor = 0;
        }
        let sample = self.current_block[self.cursor];
        self.cursor += 1;
        self.channel_cursor += 1;
        if self.channel_cursor == self.configuration.channels {
            self.channel_cursor = 0;
            if self.cursor == self.current_block.len() {
                self.processed_blocks += 1;
            }
            self.worker_poll_frame_nanos += 1_000_000_000;
            let poll_frame_nanos = u128::from(self.configuration.sample_rate) * WORKER_POLL_NANOS;
            while self.worker_poll_frame_nanos >= poll_frame_nanos {
                self.worker_poll_frame_nanos -= poll_frame_nanos;
                self.refill_processed_blocks();
            }
        }
        Some(sample)
    }
}

const WORKER_POLL_NANOS: u128 = 10_000_000;

impl NativeMixerSource {
    fn refill_processed_blocks(&mut self) {
        if self.processed_blocks < 2 {
            return;
        }
        let mut mixer = self.shared.lock().expect("native mixer lock poisoned");
        for _ in 0..self.processed_blocks {
            self.queued_blocks.push_back(mixer.fill_block());
        }
        self.processed_blocks = 0;
    }
}

impl Source for NativeMixerSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        NonZero::new(self.configuration.channels).expect("validated native channel count")
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(self.configuration.sample_rate).expect("validated native sample rate")
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests;
