//! Cross-platform wgpu desktop host facade.

mod account;
mod apprater;
mod input;
mod lifecycle;
mod platform_actions;
mod runtime;
mod screenshot;
mod sharing;
mod window;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlatformOverlay {
    Account,
    AppRating,
}

pub(super) struct StellaApp {
    pub(super) runtime: StellaLua,
    assets: AssetCatalog,
    render_commands: Vec<RenderCommand>,
    text_commands: Vec<TextRenderCommand>,
    rect_commands: Vec<RectRenderCommand>,
    capture_commands: Vec<CaptureRenderCommand>,
    /// The current command stream has already reached the game target.
    /// Presenting it again must not resolve captures against newer pixels.
    rendered_frame_ready: bool,
    screenshot_share_requests: Vec<ScreenshotShareRequest>,
    background_color: [u8; 3],
    frame_clear_clip: Option<[i32; 4]>,
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
    account_ui: crate::account_ui::AccountUi,
    app_rating_ui: crate::apprater_ui::AppRatingUi,
    account_painter: crate::account_ui::AccountPainter,
    account_ime_enabled: bool,
    account_owner: Option<u64>,
    platform_overlay: Option<PlatformOverlay>,
    account_clipboard: Option<arboard::Clipboard>,
    account_started: Instant,
    active: bool,
    close_request: window::CloseRequest,
    fatal_error: Option<String>,
}

#[derive(Clone, Copy, Default)]
pub(super) struct PlatformServiceOptions<'a> {
    pub(super) telepod_code: Option<&'a str>,
    pub(super) installed_url_schemes: &'a [String],
    pub(super) local_services: bool,
    pub(super) game_server_url: Option<&'a str>,
    pub(super) server_time_url: Option<&'a str>,
    pub(super) assets_url: Option<&'a str>,
    pub(super) identity_url: Option<&'a str>,
    pub(super) identity_client_id: Option<&'a str>,
    pub(super) identity_client_signature: Option<&'a str>,
    pub(super) identity_client_salt: Option<&'a str>,
    pub(super) identity_signing_key: Option<&'a [u8]>,
    pub(super) storage_url: Option<&'a str>,
    pub(super) storage_access_token: Option<&'a str>,
    pub(super) storage_signature: Option<&'a str>,
    pub(super) social_url: Option<&'a str>,
}

impl StellaApp {
    pub(super) fn new_with_missing_global_diagnostics(
        data_root: PathBuf,
        resolution: GameResolution,
        track_missing_globals: bool,
        services: PlatformServiceOptions<'_>,
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
        if services.local_services {
            runtime
                .enable_local_services()
                .map_err(|error| anyhow!("enable local account/cloud services: {error}"))?;
        }
        if !services.installed_url_schemes.is_empty() {
            runtime
                .set_installed_url_schemes(
                    services.installed_url_schemes.iter().map(String::as_str),
                )
                .map_err(|error| anyhow!("configure installed URL schemes: {error}"))?;
        }
        if let Some(url) = services.game_server_url {
            runtime
                .set_game_server_url(url)
                .map_err(|error| anyhow!("configure compatible game server: {error}"))?;
        }
        if let Some(url) = services.server_time_url {
            runtime
                .set_server_time_url(url)
                .map_err(|error| anyhow!("configure compatible server time: {error}"))?;
        }
        if let Some(url) = services.assets_url {
            runtime
                .set_assets_url(url)
                .map_err(|error| anyhow!("configure compatible assets service: {error}"))?;
        }
        if let Some(url) = services.identity_url {
            runtime
                .set_identity_url(url)
                .map_err(|error| anyhow!("configure compatible identity service: {error}"))?;
        }
        if services.identity_client_id.is_some()
            || services.identity_client_signature.is_some()
            || services.identity_client_salt.is_some()
        {
            runtime
                .set_identity_client(
                    services.identity_client_id,
                    services.identity_client_signature,
                    services.identity_client_salt,
                )
                .map_err(|error| anyhow!("configure compatible identity client: {error}"))?;
        }
        if let Some(url) = services.storage_url {
            runtime
                .set_storage_url(url)
                .map_err(|error| anyhow!("configure compatible storage service: {error}"))?;
        }
        if let Some(key) = services.identity_signing_key {
            runtime
                .set_identity_signing_key(key)
                .map_err(|error| anyhow!("configure compatible identity signing: {error}"))?;
        }
        if services.storage_access_token.is_some() || services.storage_signature.is_some() {
            runtime
                .set_storage_credentials(services.storage_access_token, services.storage_signature)
                .map_err(|error| anyhow!("configure compatible storage credentials: {error}"))?;
        }
        if let Some(url) = services.social_url {
            runtime
                .set_social_url(url)
                .map_err(|error| anyhow!("configure compatible social service: {error}"))?;
        }
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
        if let Some(code) = services.telepod_code {
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
        let account_painter = crate::account_ui::AccountPainter::new(data_root, &runtime);
        Ok(Self {
            runtime,
            assets,
            render_commands: Vec::new(),
            text_commands: Vec::new(),
            rect_commands: Vec::new(),
            capture_commands: Vec::new(),
            rendered_frame_ready: false,
            screenshot_share_requests: Vec::new(),
            background_color,
            frame_clear_clip: None,
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
            account_ui: crate::account_ui::AccountUi::default(),
            app_rating_ui: crate::apprater_ui::AppRatingUi::default(),
            account_painter,
            account_ime_enabled: false,
            account_owner: None,
            platform_overlay: None,
            account_clipboard: None,
            account_started: Instant::now(),
            active: false,
            close_request: window::CloseRequest::default(),
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
