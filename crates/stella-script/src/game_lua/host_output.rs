//! Host-side queries and draining of the native render bridge.

use super::StellaLua;
use crate::*;

impl StellaLua {
    /// Export the active last-entry-wins sprite/composite catalog only when
    /// ResourceManager lifetime state changed since the host's last mirror.
    pub fn sprite_catalog_snapshot_since(&self, revision: u64) -> Option<SpriteCatalogSnapshot> {
        let resources = self
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        (resources.sprite_catalog_revision != revision)
            .then(|| resources.sprite_catalog_snapshot(&self.data_root))
    }

    /// Snapshot the native audio output and retained AudioClip graph for a
    /// host sound device. The gameplay VM remains authoritative for handles,
    /// track volumes and clip lifetime.
    pub fn audio_output_state(&self) -> AudioOutputState {
        let resources = self
            .resource_runtime
            .lock()
            .expect("resource runtime lock poisoned");
        let audio = self
            ._audio_runtime
            .lock()
            .expect("audio runtime lock poisoned");
        let configuration = resources.audio_output_configuration;
        AudioOutputState {
            generation: resources.audio_output_generation,
            started: resources.audio_output_created && resources.audio_output_started,
            master_volume: resources.master_volume,
            channels: configuration
                .and_then(|configuration| u16::try_from(configuration.channels).ok())
                .unwrap_or(0),
            bits_per_sample: configuration
                .and_then(|configuration| u16::try_from(configuration.bits_per_sample).ok())
                .unwrap_or(0),
            sample_rate: configuration
                .and_then(|configuration| u32::try_from(configuration.samples_per_second).ok())
                .unwrap_or(0),
            buffer_bytes: configuration
                .map(|configuration| configuration.buffer_bytes)
                .unwrap_or(0),
            track_volumes: audio.track_volumes,
            playbacks: audio
                .clips
                .iter()
                .map(|(handle, clip)| AudioPlaybackState {
                    handle: *handle,
                    source: clip.asset.as_ref().map(|asset| asset.source.clone()),
                    duration: clip.asset.as_ref().and_then(|asset| asset.duration),
                    sample_frames: clip.asset.as_ref().and_then(|asset| asset.sample_frames),
                    source_channels: clip.asset.as_ref().and_then(|asset| {
                        audio_source_format(&asset.source).map(|format| format.0)
                    }),
                    source_bits_per_sample: clip.asset.as_ref().and_then(|asset| {
                        audio_source_format(&asset.source).map(|format| format.1)
                    }),
                    volume: clip.volume,
                    looping: clip.looping,
                    track: clip.channel,
                })
                .collect(),
        }
    }

    /// Retire one-shot handles whose physical decoder reached end-of-stream.
    /// Purple's output callback drops completed AudioInstances asynchronously;
    /// the host reports that boundary at the next fixed update.
    pub fn finish_audio_playbacks(&self, handles: &[i64]) {
        let mut audio = self
            ._audio_runtime
            .lock()
            .expect("audio runtime lock poisoned");
        for handle in handles {
            audio.clips.remove(handle);
        }
    }

    pub fn missing_globals(&self) -> Vec<String> {
        self.missing_globals
            .lock()
            .expect("missing-global lock poisoned")
            .iter()
            .cloned()
            .collect()
    }

    /// Return executable global entries that still reached the generic
    /// compatibility adapter. Native resource/animation tables are closed
    /// inventories: unknown table fields are reported by
    /// [`Self::missing_globals`] and remain `nil`.
    pub fn fallback_calls(&self) -> Vec<String> {
        self.fallback_calls
            .lock()
            .expect("fallback-call lock poisoned")
            .iter()
            .cloned()
            .collect()
    }

    /// Return executable-registered names that still use the generic
    /// compatibility adapter after all typed subsystem installers ran.
    /// Unlike [`Self::fallback_calls`], this exposes dormant gaps before a
    /// particular game route happens to invoke them.
    pub fn compatibility_bindings(&self) -> Vec<String> {
        self.compatibility_bindings
            .lock()
            .expect("compatibility-binding lock poisoned")
            .iter()
            .cloned()
            .collect()
    }

    /// Drain draw commands emitted by original Lua UI code during the latest
    /// call to [`Self::draw`].
    pub fn take_render_commands(&self) -> Vec<RenderCommand> {
        std::mem::take(
            &mut self
                .render
                .lock()
                .expect("render bridge lock poisoned")
                .commands,
        )
    }

    /// Exchange all deferred frame queues with host-owned buffers.
    ///
    /// Purple submits each draw immediately, so it has no per-frame command
    /// allocation corresponding to the wgpu host's deferred queues. Swapping
    /// returns the preceding host allocations to `RenderBridge`; the next
    /// [`Self::draw`] clears their old elements and reuses their capacity while
    /// the host owns the newly completed frame.
    pub fn swap_frame_commands(
        &self,
        render_commands: &mut Vec<RenderCommand>,
        text_commands: &mut Vec<TextRenderCommand>,
        rect_commands: &mut Vec<RectRenderCommand>,
        capture_commands: &mut Vec<CaptureRenderCommand>,
    ) {
        let mut bridge = self.render.lock().expect("render bridge lock poisoned");
        std::mem::swap(render_commands, &mut bridge.commands);
        std::mem::swap(text_commands, &mut bridge.text_commands);
        std::mem::swap(rect_commands, &mut bridge.rect_commands);
        std::mem::swap(capture_commands, &mut bridge.capture_commands);
    }

    /// Report whether the current immediate-order stream contains a framebuffer
    /// capture. The deterministic host can leave ordinary draw queues in place
    /// until the final frame, retaining Purple's no-intermediate-consumer path.
    pub fn has_capture_commands(&self) -> bool {
        !self
            .render
            .lock()
            .expect("render bridge lock poisoned")
            .capture_commands
            .is_empty()
    }

    pub fn take_text_commands(&self) -> Vec<TextRenderCommand> {
        std::mem::take(
            &mut self
                .render
                .lock()
                .expect("render bridge lock poisoned")
                .text_commands,
        )
    }

    pub fn take_rect_commands(&self) -> Vec<RectRenderCommand> {
        std::mem::take(
            &mut self
                .render
                .lock()
                .expect("render bridge lock poisoned")
                .rect_commands,
        )
    }

    pub fn take_capture_commands(&self) -> Vec<CaptureRenderCommand> {
        std::mem::take(
            &mut self
                .render
                .lock()
                .expect("render bridge lock poisoned")
                .capture_commands,
        )
    }

    /// Drain `GameLua::shareScreenShot` requests in native call order. The
    /// platform host owns framebuffer readback and temporary-file lifetime.
    pub fn take_screenshot_share_requests(&self) -> Vec<ScreenshotShareRequest> {
        std::mem::take(
            &mut self
                .render
                .lock()
                .expect("render bridge lock poisoned")
                .screenshot_share_requests,
        )
    }

    /// Drain platform-owned URL and store actions in the same order in which
    /// the original native members accepted them from Lua.
    pub fn take_platform_action_requests(&self) -> Vec<PlatformActionRequest> {
        std::mem::take(
            &mut self
                .render
                .lock()
                .expect("render bridge lock poisoned")
                .platform_action_requests,
        )
    }

    /// Drain composite definitions changed through `setCompoSpriteEntry` so
    /// the host renderer can update its asset-side expansion cache.
    pub fn take_composite_updates(&self) -> BTreeMap<String, Vec<CompositePart>> {
        std::mem::take(
            &mut self
                .render
                .lock()
                .expect("render bridge lock poisoned")
                .composite_updates,
        )
    }

    pub fn background_color(&self) -> [u8; 3] {
        self.render
            .lock()
            .expect("render bridge lock poisoned")
            .background_color
    }

    pub fn exit_requested(&self) -> bool {
        self.render
            .lock()
            .expect("render bridge lock poisoned")
            .exit_requested
    }

    /// Return the frame-latched value exposed by GameApp's safe-to-quit
    /// virtual member (`sub_100028FB0` -> `sub_100062518`).
    pub fn safe_to_quit(&self) -> bool {
        self.render
            .lock()
            .expect("render bridge lock poisoned")
            .safe_to_quit
    }

    /// Return GameLua's script-load completion byte at `+0x513`.
    pub fn gamelogic_loaded(&self) -> bool {
        self.gamelogic_loaded.get()
    }

    /// Return the string keys currently installed in Lua's global table.
    pub fn global_names(&self) -> Result<Vec<String>, ScriptError> {
        let mut names = self
            .lua
            .globals()
            .pairs::<Value, Value>()
            .filter_map(|pair| match pair {
                Ok((Value::String(name), _)) => Some(Ok(name.to_string_lossy())),
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<LuaResult<Vec<_>>>()?;
        if let Ok(environment) = game_environment(&self.lua) {
            for pair in environment.pairs::<Value, Value>() {
                if let (Value::String(name), _) = pair? {
                    names.push(name.to_string_lossy());
                }
            }
        }
        names.sort();
        names.dedup();
        Ok(names)
    }

    /// Invoke a no-argument global callback used by the original native loop.
    pub fn call_global(&self, name: &str) -> Result<bool, ScriptError> {
        let environment = game_environment(&self.lua)?;
        match environment.get::<Value>(name)? {
            Value::Function(function) => {
                function.call::<()>(())?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

fn audio_source_format(source: &AudioAssetSource) -> Option<(u16, u16)> {
    match source {
        AudioAssetSource::EncodedFile {
            channels,
            bits_per_sample,
            ..
        }
        | AudioAssetSource::PcmData {
            channels,
            bits_per_sample,
            ..
        }
        | AudioAssetSource::RawPcmFile {
            channels,
            bits_per_sample,
            ..
        } => Some((*channels, *bits_per_sample)),
        AudioAssetSource::Sequence(parts) => parts.first().and_then(audio_source_format),
        AudioAssetSource::File(_) => None,
    }
}
