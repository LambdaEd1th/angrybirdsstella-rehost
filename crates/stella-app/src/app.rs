//! Cross-platform wgpu desktop host facade.

mod input;
mod lifecycle;
mod platform_actions;
mod runtime;
mod screenshot;
mod sharing;
mod window;

use super::*;

pub(super) struct StellaApp {
    pub(super) runtime: StellaLua,
    assets: AssetCatalog,
    render_commands: Vec<RenderCommand>,
    text_commands: Vec<TextRenderCommand>,
    rect_commands: Vec<RectRenderCommand>,
    capture_commands: Vec<CaptureRenderCommand>,
    screenshot_share_requests: Vec<ScreenshotShareRequest>,
    background_color: [u8; 3],
    resource_revision: u64,
    resolution: GameResolution,
    window: Option<Arc<Window>>,
    renderer: Option<GpuRenderer>,
    audio: Option<AudioDevice>,
    audio_clock: AudioOutputClock,
    last_tick: Instant,
    accumulator: Duration,
    cursor: (f64, f64),
    cursor_down: bool,
    touches: Vec<(u64, i32, i32)>,
    primary_touch: Option<u64>,
    modifiers: ModifiersState,
    active: bool,
    fatal_error: Option<String>,
}

impl StellaApp {
    pub(super) fn new_with_missing_global_diagnostics(
        data_root: PathBuf,
        resolution: GameResolution,
        track_missing_globals: bool,
        telepod_code: Option<&str>,
    ) -> Result<Self> {
        let runtime = if track_missing_globals {
            StellaLua::new_with_resolution_and_missing_global_diagnostics(
                &data_root,
                resolution.width,
                resolution.height,
            )
        } else {
            StellaLua::new_with_resolution(&data_root, resolution.width, resolution.height)
        }
        .map_err(|error| anyhow!("create Lua runtime: {error}"))?;
        // Purple constructs and publishes QrScanner while it builds the
        // native service graph, before game.lua loads characters.lua.  That
        // script snapshots Telepods.areSupported() into
        // g_showTelepodButtons while parsing telepod_configuration.json.
        //
        // The desktop app always has the virtual scanner ingress available;
        // a code is optional and remains queued until the shipped scan page
        // starts the scanner and installs its recognition callback.
        runtime
            .set_qr_scanner_available(true)
            .map_err(|error| anyhow!("publish virtual Telepods scanner: {error}"))?;
        if let Some(code) = telepod_code {
            runtime
                .submit_qr_code(code)
                .map_err(|error| anyhow!("queue Telepods code: {error}"))?;
        }
        runtime
            .boot("scripts/game.lua")
            .map_err(|error| anyhow!("boot original scripts: {error}"))?;
        let mut assets = AssetCatalog::load(
            data_root.join("images/1024x768"),
            data_root.join("fonts/1024x768"),
        )?;
        let mut resource_revision = 0;
        if let Some(snapshot) = runtime.sprite_catalog_snapshot_since(resource_revision) {
            resource_revision = snapshot.revision;
            assets.apply_sprite_catalog_snapshot(snapshot)?;
        }
        let background_color = runtime.background_color();
        Ok(Self {
            runtime,
            assets,
            render_commands: Vec::new(),
            text_commands: Vec::new(),
            rect_commands: Vec::new(),
            capture_commands: Vec::new(),
            screenshot_share_requests: Vec::new(),
            background_color,
            resource_revision,
            resolution,
            window: None,
            renderer: None,
            audio: None,
            audio_clock: AudioOutputClock::default(),
            last_tick: Instant::now(),
            accumulator: Duration::ZERO,
            cursor: (0.0, 0.0),
            cursor_down: false,
            touches: Vec::new(),
            primary_touch: None,
            modifiers: ModifiersState::empty(),
            active: false,
            fatal_error: None,
        })
    }

    pub(super) fn synchronize_sprite_catalog(&mut self) -> Result<()> {
        if let Some(snapshot) = self
            .runtime
            .sprite_catalog_snapshot_since(self.resource_revision)
        {
            self.resource_revision = snapshot.revision;
            self.assets.apply_sprite_catalog_snapshot(snapshot)?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) use input::map_window_to_game;
