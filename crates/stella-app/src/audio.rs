//! Cross-platform physical output for Purple's AudioManager state.

use anyhow::{Context, Result};
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player};

use super::AudioOutputState;

mod native_mixer;

use native_mixer::{NativeMixerConfiguration, NativeMixerControl};

pub(super) struct AudioDevice {
    sink: MixerDeviceSink,
    generation: Option<u64>,
    configuration: Option<NativeMixerConfiguration>,
    player: Option<Player>,
    mixer: Option<NativeMixerControl>,
    started: bool,
}

impl AudioDevice {
    pub(super) fn open() -> Result<Self> {
        let mut sink =
            DeviceSinkBuilder::open_default_sink().context("open default sound device")?;
        sink.log_on_drop(false);
        Ok(Self {
            sink,
            generation: None,
            configuration: None,
            player: None,
            mixer: None,
            started: false,
        })
    }

    pub(super) fn synchronize(&mut self, state: AudioOutputState) -> Vec<i64> {
        let configuration = NativeMixerConfiguration::from_state(&state);
        if self.generation != Some(state.generation) || self.configuration != configuration {
            self.generation = Some(state.generation);
            self.configuration = configuration;
            if let Some(player) = self.player.take() {
                player.stop();
            }
            self.mixer = None;
            self.started = false;
            if let Some(configuration) = configuration {
                self.mixer = Some(native_mixer::create(configuration));
            }
        }

        let mut finished = self
            .mixer
            .as_ref()
            .map(|mixer| mixer.synchronize(&state))
            .unwrap_or_default();
        if state.started && !self.started {
            if let Some(mixer) = &self.mixer {
                let player = Player::connect_new(self.sink.mixer());
                player.set_volume(state.master_volume);
                player.append(mixer.source(6));
                // source() performs Purple's synchronous six-buffer
                // initialization. Collect any short one-shots removed during
                // that prefill now so they are gone before the next VM tick,
                // just like the already-running native worker.
                finished.extend(mixer.synchronize(&state));
                player.play();
                self.player = Some(player);
                self.started = true;
            }
        } else if !state.started && self.started {
            if let Some(player) = self.player.take() {
                player.stop();
            }
            self.started = false;
        }
        if let Some(player) = &self.player {
            // Purple applies master gain to its one OpenAL source after the
            // integer mixer has saturated the block.
            player.set_volume(state.master_volume);
        }
        finished.sort_unstable();
        finished.dedup();
        finished
    }
}
