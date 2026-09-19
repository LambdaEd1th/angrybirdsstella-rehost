//! Host-visible audio state emitted by Purple's AudioManager reimplementation.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

/// AudioClip storage retained at creation time. Composite clips own a frozen
/// sequence of child clips, matching Purple's intrusive-pointer vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioAssetSource {
    /// Convenience path source used by hosts/tests which construct a source
    /// directly. Purple-created clips use retained variants so their opened
    /// input stream does not change if the host path is later replaced.
    File(PathBuf),
    /// Encoded bytes and decoder state boundary retained by a streaming clip.
    EncodedFile {
        path: PathBuf,
        data: Arc<[u8]>,
        streaming: bool,
        channels: u16,
        bits_per_sample: u16,
        sample_rate: u32,
    },
    /// PCM vector owned by Purple's non-streaming `MemoryInputStream` clip.
    /// `origin` is diagnostic only; playback never reopens it.
    PcmData {
        origin: PathBuf,
        data: Arc<[u8]>,
        channels: u16,
        bits_per_sample: u16,
        sample_rate: u32,
    },
    /// File type zero in Purple's `AudioReader` is headerless PCM with the
    /// constructor's default configuration.  Keep the recovered format with
    /// the path because ordinary desktop decoders cannot infer it.
    RawPcmFile {
        path: PathBuf,
        data: Arc<[u8]>,
        streaming: bool,
        channels: u16,
        bits_per_sample: u16,
        sample_rate: u32,
    },
    Sequence(Vec<AudioAssetSource>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioPlaybackState {
    pub handle: i64,
    pub source: Option<AudioAssetSource>,
    /// Decoded source duration frozen with the clip pointer retained by the
    /// native AudioClipInstance. `None` denotes an unreadable source.
    pub duration: Option<Duration>,
    /// Decoded frames consumed by Purple's byte reader. The mixer submits
    /// these frames at the output rate and never resamples per clip.
    pub sample_frames: Option<u64>,
    pub source_channels: Option<u16>,
    pub source_bits_per_sample: Option<u16>,
    pub volume: f32,
    pub looping: bool,
    pub track: i32,
    /// Purple keeps an EOF/stopped AudioClipInstance in the manager until the
    /// next mixer block removes it, but it is no longer considered playing.
    pub finished: bool,
}

/// Instance-lifetime edges published by one output synchronization.
///
/// A short clip can cross both edges between two display ticks, so the two
/// vectors are intentionally independent rather than mutually exclusive.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioPlaybackTransitions {
    pub finished: Vec<i64>,
    pub removed: Vec<i64>,
}

impl AudioPlaybackTransitions {
    pub fn is_empty(&self) -> bool {
        self.finished.is_empty() && self.removed.is_empty()
    }

    pub fn merge(&mut self, mut other: Self) {
        self.finished.append(&mut other.finished);
        self.removed.append(&mut other.removed);
        self.normalize();
    }

    pub fn normalize(&mut self) {
        self.finished.sort_unstable();
        self.finished.dedup();
        self.removed.sort_unstable();
        self.removed.dedup();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioOutputState {
    /// Identity of the current AudioOutputImpl. A replacement reconstructs
    /// its mixer and may immediately reuse the same numeric playback handles.
    pub generation: u64,
    pub started: bool,
    pub master_volume: f32,
    /// PCM format of the single native AudioOutputImpl mixing buffer.
    pub channels: u16,
    pub bits_per_sample: u16,
    pub sample_rate: u32,
    /// Size of one block filled by Purple's worker before it is queued to
    /// OpenAL. Mixing gains and completion edges are evaluated per block.
    pub buffer_bytes: u32,
    pub track_volumes: [f32; 8],
    pub playbacks: Vec<AudioPlaybackState>,
}

#[derive(Debug, Clone, Copy)]
struct ClockPlayback {
    remaining_frames: u64,
    looping: bool,
    finished: bool,
    advances: bool,
}

/// Device-independent clock for Purple's asynchronous audio mixer.
///
/// The original OpenAL worker advances AudioClipInstance decoders outside the
/// gameplay update. Hosts without a physical device still need that lifetime
/// boundary or one-shots permanently occupy the manager's finite channels.
#[derive(Debug, Default)]
pub struct AudioOutputClock {
    generation: Option<u64>,
    started: bool,
    worker_poll_nanos: u128,
    /// Output frames consumed since the last native unqueue, represented as
    /// frame-nanoseconds so fractional frames survive host update boundaries.
    output_frame_nanos: u128,
    playbacks: BTreeMap<i64, ClockPlayback>,
}

const WORKER_POLL_NANOS: u128 = 10_000_000;

impl AudioOutputClock {
    /// Reconcile a VM snapshot and advance the independent output clock.
    /// Returns the distinct native finished and next-block removal edges.
    pub fn synchronize(
        &mut self,
        state: &AudioOutputState,
        elapsed: Duration,
    ) -> AudioPlaybackTransitions {
        if self.generation != Some(state.generation) {
            self.generation = Some(state.generation);
            self.started = false;
            self.worker_poll_nanos = 0;
            self.output_frame_nanos = 0;
            self.playbacks.clear();
        }
        let live = state
            .playbacks
            .iter()
            .map(|playback| playback.handle)
            .collect::<BTreeSet<_>>();
        self.playbacks.retain(|handle, _| live.contains(handle));
        for playback in &state.playbacks {
            let active = self
                .playbacks
                .entry(playback.handle)
                .or_insert(ClockPlayback {
                    remaining_frames: playback
                        .sample_frames
                        .unwrap_or_else(|| duration_frames(playback.duration, state.sample_rate)),
                    looping: playback.looping,
                    finished: playback.finished,
                    advances: match (playback.source_bits_per_sample, playback.source_channels) {
                        (Some(bits), Some(channels)) => {
                            bits == state.bits_per_sample
                                && matches!((state.channels, channels), (1 | 2, 1 | 2))
                        }
                        // An absent reader reaches zero immediately; this is
                        // distinct from an explicit unsupported format, which
                        // the native mixer skips without advancing.
                        _ => true,
                    },
                });
            active.looping = playback.looping;
            active.finished |= playback.finished;
        }
        if !state.started {
            self.started = false;
            self.worker_poll_nanos = 0;
            self.output_frame_nanos = 0;
            return AudioPlaybackTransitions::default();
        }

        let mut transitions = AudioPlaybackTransitions::default();
        let Some(block_frames) = output_block_frames(state) else {
            return transitions;
        };
        if !self.started {
            self.started = true;
            self.worker_poll_nanos = 0;
            self.output_frame_nanos = 0;
            for _ in 0..6 {
                self.fill_block(block_frames, &mut transitions);
            }
        }
        self.advance_worker(state.sample_rate, block_frames, elapsed, &mut transitions);
        transitions.normalize();
        transitions
    }

    fn advance_worker(
        &mut self,
        sample_rate: u32,
        block_frames: u64,
        elapsed: Duration,
        transitions: &mut AudioPlaybackTransitions,
    ) {
        let block_frame_nanos = u128::from(block_frames) * 1_000_000_000;
        let queue_frame_nanos = block_frame_nanos * 6;
        let mut remaining_nanos = elapsed.as_nanos();
        while remaining_nanos > 0 {
            let until_poll = WORKER_POLL_NANOS - self.worker_poll_nanos;
            let slice = remaining_nanos.min(until_poll);
            self.output_frame_nanos = self
                .output_frame_nanos
                .saturating_add(slice.saturating_mul(u128::from(sample_rate)))
                .min(queue_frame_nanos);
            self.worker_poll_nanos += slice;
            remaining_nanos -= slice;

            if self.worker_poll_nanos == WORKER_POLL_NANOS {
                self.worker_poll_nanos = 0;
                let processed = self.output_frame_nanos / block_frame_nanos;
                // fillBuffer intentionally waits until OpenAL reports at
                // least two processed buffers, then unqueues and refills all
                // of them in one locked pass.
                if processed >= 2 {
                    self.output_frame_nanos %= block_frame_nanos;
                    for _ in 0..processed {
                        self.fill_block(block_frames, transitions);
                    }
                }
            }
        }
    }

    fn fill_block(&mut self, block_frames: u64, transitions: &mut AudioPlaybackTransitions) {
        let removed = self
            .playbacks
            .iter()
            .filter_map(|(handle, playback)| playback.finished.then_some(*handle))
            .collect::<Vec<_>>();
        for handle in removed {
            self.playbacks.remove(&handle);
            transitions.removed.push(handle);
        }
        for (handle, playback) in &mut self.playbacks {
            if playback.looping || !playback.advances {
                continue;
            }
            if playback.remaining_frames == 0 {
                playback.finished = true;
                transitions.finished.push(*handle);
            } else {
                playback.remaining_frames = playback.remaining_frames.saturating_sub(block_frames);
            }
        }
    }
}

fn duration_frames(duration: Option<Duration>, sample_rate: u32) -> u64 {
    duration
        .map(|duration| {
            duration
                .as_nanos()
                .saturating_mul(u128::from(sample_rate))
                .saturating_div(1_000_000_000)
                .min(u128::from(u64::MAX)) as u64
        })
        .unwrap_or(0)
}

fn output_block_frames(state: &AudioOutputState) -> Option<u64> {
    let bytes_per_frame =
        u64::from(state.bits_per_sample / 8).checked_mul(u64::from(state.channels))?;
    (bytes_per_frame > 0)
        .then(|| u64::from(state.buffer_bytes) / bytes_per_frame)
        .filter(|frames| *frames > 0 && state.sample_rate > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(started: bool, playbacks: Vec<AudioPlaybackState>) -> AudioOutputState {
        AudioOutputState {
            generation: 0,
            started,
            master_volume: 1.0,
            channels: 2,
            bits_per_sample: 16,
            sample_rate: 44_100,
            buffer_bytes: 8_192,
            track_volumes: [1.0; 8],
            playbacks,
        }
    }

    fn playback(handle: i64, duration: Option<Duration>, looping: bool) -> AudioPlaybackState {
        AudioPlaybackState {
            handle,
            source: None,
            duration,
            sample_frames: None,
            source_channels: None,
            source_bits_per_sample: None,
            volume: 1.0,
            looping,
            track: 0,
            finished: false,
        }
    }

    #[test]
    fn output_clock_uses_consumed_frames_at_output_rate_without_resampling() {
        let mut clock = AudioOutputClock::default();
        let mut clip = playback(5, Some(Duration::from_secs(1)), false);
        clip.sample_frames = Some(16_000);
        let snapshot = state(true, vec![clip]);
        // Six 2,048-frame blocks are consumed during initialization. Purple's
        // 10 ms worker waits for two processed OpenAL buffers, then refills
        // both: the first observes EOF and the second removes the handle.
        assert_eq!(
            clock.synchronize(&snapshot, Duration::ZERO),
            AudioPlaybackTransitions::default()
        );
        assert!(
            clock
                .synchronize(&snapshot, Duration::from_millis(189))
                .eq(&AudioPlaybackTransitions::default())
        );
        assert_eq!(
            clock.synchronize(&snapshot, Duration::from_millis(1)),
            AudioPlaybackTransitions {
                finished: vec![5],
                removed: vec![5],
            }
        );
    }

    #[test]
    fn one_shots_finish_while_loops_survive() {
        let mut clock = AudioOutputClock::default();
        let snapshot = state(
            true,
            vec![
                playback(7, Some(Duration::from_millis(25)), false),
                playback(8, Some(Duration::from_millis(5)), true),
            ],
        );
        assert_eq!(
            clock.synchronize(&snapshot, Duration::ZERO),
            AudioPlaybackTransitions {
                finished: vec![7],
                removed: vec![7],
            }
        );
        assert!(
            clock
                .synchronize(
                    &state(true, vec![playback(8, None, true)]),
                    Duration::from_secs(10),
                )
                .eq(&AudioPlaybackTransitions::default())
        );
    }

    #[test]
    fn stopped_output_freezes_existing_decoder_progress() {
        let mut clock = AudioOutputClock::default();
        let running = state(true, vec![playback(3, Some(Duration::from_secs(1)), false)]);
        assert_eq!(
            clock.synchronize(&running, Duration::ZERO),
            AudioPlaybackTransitions::default()
        );
        let stopped = state(
            false,
            vec![playback(3, Some(Duration::from_secs(1)), false)],
        );
        assert!(
            clock
                .synchronize(&stopped, Duration::from_secs(1))
                .eq(&AudioPlaybackTransitions::default())
        );
        // Restart executes a new six-block prefill from the retained cursor.
        assert_eq!(
            clock.synchronize(&running, Duration::ZERO),
            AudioPlaybackTransitions::default()
        );
    }

    #[test]
    fn unreadable_non_looping_source_retires_on_first_running_tick() {
        let mut clock = AudioOutputClock::default();
        assert_eq!(
            clock.synchronize(
                &state(true, vec![playback(12, None, false)]),
                Duration::ZERO,
            ),
            AudioPlaybackTransitions {
                finished: vec![12],
                removed: vec![12],
            }
        );
    }
}
