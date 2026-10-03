//! Single-threaded browser host. Emscripten supplies Lua's C ABI and MEMFS;
//! WebGL consumes the same renderer-neutral frame expansion as the desktop.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::{CStr, CString},
    fs,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::{Context as _, Result, anyhow};
use image::RgbaImage;
use serde_json::json;
use stella_assets::ka3d::{
    BitmapFont, CompositePart, CompositeSpriteSet, Ka3dEnvelope, SpriteRegion, SpriteSheet,
};
use stella_script::{
    AudioAssetSource, AudioOutputClock, BoundCompositePart, CaptureRenderCommand,
    ColorMeshTopology, ColorProgram, DirtRenderCommand, MaskedTextureBinding, RectRenderCommand,
    RenderCommand, RenderQuad, RenderTriangle, SpriteCatalogRegion, SpriteCatalogSnapshot,
    SpriteGeometrySubmission, SpriteShader, StellaLua, SystemFontLayoutFace,
    SystemFontRenderBinding, SystemFontShapedLine, TextFontBinding, TextProjection3D,
    TextRenderCommand,
};

// These modules contain no window/device dependencies. Sharing them preserves
// atlas pivots, bitmap text, Dirt, painter order and capture generations.
mod account;
#[allow(dead_code, unused_imports)]
#[path = "../../stella-app/src/account_ui.rs"]
mod account_ui;
#[path = "../../stella-app/src/apprater_ui.rs"]
mod apprater_ui;
#[allow(dead_code, unused_imports)]
#[path = "../../stella-app/src/assets/mod.rs"]
mod assets;
mod gpu;
mod input;
#[allow(dead_code, unused_imports)]
#[path = "../../stella-app/src/platform_ui_drawing.rs"]
mod platform_ui_drawing;
mod rating;
use assets::*;

const GAME_WIDTH: u32 = 1024;
const GAME_HEIGHT: u32 = 768;
// Same order as the shipped TEXTS_BASIC table and web/locales.js.
const LOCALES: [&str; 11] = [
    "en_EN", "fr_FR", "it_IT", "de_DE", "es_ES", "pt_BR", "zh_CN", "zh_TW", "ja_JP", "ko_KR",
    "ru_RU",
];

#[derive(Clone, Copy, PartialEq, Eq)]
struct GameResolution {
    width: u32,
    height: u32,
}

impl GameResolution {
    fn new(width: u32, height: u32) -> Result<Self> {
        anyhow::ensure!(
            width > 0
                && height > 0
                && width <= u32::from(u16::MAX)
                && height <= u32::from(u16::MAX),
            "Invalid drawable resolution"
        );
        Ok(Self { width, height })
    }
}

impl Default for GameResolution {
    fn default() -> Self {
        Self {
            width: GAME_WIDTH,
            height: GAME_HEIGHT,
        }
    }
}

struct BrowserGame {
    runtime: StellaLua,
    resolution: GameResolution,
    assets: AssetCatalog,
    revision: u64,
    audio_clock: AudioOutputClock,
    uploaded: HashSet<String>,
    before_clear: gpu::PreparedFrame,
    frame: gpu::PreparedFrame,
    packet: CString,
    audio_packet: CString,
    active: bool,
    touches: input::BrowserTouches,
    account: account::BrowserAccount,
    rating: rating::BrowserRating,
    platform_packet: CString,
}

thread_local! {
    static GAME: RefCell<Option<BrowserGame>> = const { RefCell::new(None) };
    static ERROR: RefCell<CString> = RefCell::new(CString::new("").unwrap());
    static LANGUAGE: RefCell<usize> = const { RefCell::new(0) };
}

trait ScriptResultExt<T> {
    fn browser(self) -> Result<T>;
}
impl<T> ScriptResultExt<T> for std::result::Result<T, stella_script::ScriptError> {
    fn browser(self) -> Result<T> {
        self.map_err(|error| anyhow!(error.to_string()))
    }
}

fn boundary(operation: impl FnOnce() -> Result<()>) -> i32 {
    match operation() {
        Ok(()) => 0,
        Err(error) => {
            let message = format!("{error:#}").replace('\0', " ");
            eprintln!("{message}");
            ERROR.with(|slot| *slot.borrow_mut() = CString::new(message).unwrap());
            -1
        }
    }
}

fn with_game(operation: impl FnOnce(&mut BrowserGame) -> Result<()>) -> i32 {
    boundary(|| {
        GAME.with(|slot| {
            let mut slot = slot.borrow_mut();
            operation(slot.as_mut().context("Game has not started")?)
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_error() -> *const std::ffi::c_char {
    ERROR.with(|slot| slot.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_set_locale(index: i32) -> i32 {
    boundary(|| {
        let index = usize::try_from(index).context("Invalid language index")?;
        let locale = LOCALES.get(index).context("Unsupported language index")?;
        GAME.with(|slot| -> Result<()> {
            if let Some(game) = slot.borrow_mut().as_mut() {
                game.runtime.set_preferred_language(locale).browser()?;
                game.account.set_language(locale);
            }
            Ok(())
        })?;
        LANGUAGE.with(|slot| *slot.borrow_mut() = index);
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_init(width: u32, height: u32) -> i32 {
    boundary(|| {
        let resolution = if width == 0 && height == 0 {
            GameResolution::default()
        } else {
            GameResolution::new(width, height)?
        };
        let root = PathBuf::from("/runtime/data");
        let runtime =
            StellaLua::new_with_resolution(&root, resolution.width, resolution.height).browser()?;
        runtime.enable_local_services().browser()?;
        runtime
            .set_preferred_language(LANGUAGE.with(|slot| LOCALES[*slot.borrow()]))
            .browser()?;
        runtime.boot("scripts/game.lua").browser()?;
        runtime.set_application_active(true).browser()?;
        runtime.post_application_resumed();
        runtime.set_application_audio_active(true).browser()?;
        let assets = AssetCatalog::load(root.join("images/1024x768"), root.join("fonts/1024x768"))?;
        let account =
            account::BrowserAccount::new(&runtime, LANGUAGE.with(|slot| LOCALES[*slot.borrow()]));
        GAME.with(|slot| {
            *slot.borrow_mut() = Some(BrowserGame {
                runtime,
                resolution,
                assets,
                revision: 0,
                audio_clock: AudioOutputClock::default(),
                uploaded: HashSet::new(),
                before_clear: gpu::PreparedFrame::default(),
                frame: gpu::PreparedFrame::default(),
                packet: CString::new("{}").unwrap(),
                audio_packet: CString::new("{}").unwrap(),
                active: true,
                touches: input::BrowserTouches::default(),
                account,
                rating: rating::BrowserRating::default(),
                platform_packet: CString::new("[]").unwrap(),
            })
        });
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_resize(width: u32, height: u32) -> i32 {
    with_game(|game| {
        let resolution = GameResolution::new(width, height)?;
        if game.resolution != resolution {
            anyhow::ensure!(
                !game.runtime.has_frame_commands(),
                "Flush pending render commands before resizing the drawable"
            );
            // JavaScript installs the new WebGL drawable first, after the
            // previous frame (including captures) has finished rendering.
            // Use the same GameApp resolution notification as the desktop.
            game.resolution = resolution;
            game.runtime
                .set_screen_resolution(width, height)
                .browser()?;
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_frame(delta: f64) -> i32 {
    with_game(|game| {
        let elapsed = Duration::from_secs_f64(if delta.is_finite() {
            delta.clamp(0.0, 0.1)
        } else {
            0.0
        });
        game.synchronize_audio_clock(elapsed);
        if game.active {
            game.runtime.update(elapsed.as_secs_f64()).browser()?;
        }
        game.synchronize_account()?;
        game.synchronize_rating()?;
        // GameLua::draw clears its queues. Consume the complete immediate
        // stream first, including scheduler/update captures and the resources
        // they retain before draw can release or recreate those resources.
        game.before_clear = game.prepare_render_stream()?;
        let before_clear = game
            .before_clear
            .packet(&mut game.assets, &mut game.uploaded)?;
        let background = game.runtime.background_color();
        let clear_clip = game.runtime.framebuffer_clip_rect();
        game.runtime.draw().browser()?;
        game.synchronize_audio_clock(Duration::ZERO);
        game.frame = game.prepare_render_stream()?;
        let mut packet = game.frame.packet(&mut game.assets, &mut game.uploaded)?;
        packet["beforeClear"] = before_clear;
        packet["background"] = json!(background);
        packet["resolution"] = json!([game.resolution.width, game.resolution.height]);
        packet["clearClip"] = json!(clear_clip);
        packet["clearScissor"] = json!(gpu::clear_scissor(clear_clip, game.resolution));
        packet["audio"] = audio_packet(&game.runtime.audio_output_state());
        packet["exit"] = json!(game.runtime.exit_requested());
        // The private account presentation is transferred separately, so it
        // cannot enter the game stream or capture textures.
        packet["account"] = json!(game.runtime.account_ui().map(|view| view.id));
        packet["locale"] = json!(game.runtime.current_locale().browser()?);
        packet["rating"] = json!(game.runtime.app_rating_prompt().map(|p| p.id));
        // External actions are drained separately, including calls made from
        // UI callbacks while the display link is paused. Rating never supplies
        // an answer until a user invokes one of the retained alert's buttons.
        game.runtime.take_screenshot_share_requests();
        game.runtime.take_analytics_events();
        game.packet = CString::new(serde_json::to_vec(&packet)?)?;
        Ok(())
    })
}

fn audio_packet(state: &stella_script::AudioOutputState) -> serde_json::Value {
    let playbacks = state.playbacks.iter().filter(|playback| !playback.finished).map(|playback| {
        fn source(value: &AudioAssetSource) -> serde_json::Value {
            match value {
                AudioAssetSource::EncodedFile { data, .. } => json!({"encoded": true, "pointer": data.as_ptr() as usize, "length": data.len()}),
                AudioAssetSource::PcmData { data, channels, bits_per_sample, sample_rate, .. }
                | AudioAssetSource::RawPcmFile { data, channels, bits_per_sample, sample_rate, .. } => json!({"encoded": false, "pointer": data.as_ptr() as usize, "length": data.len(), "channels": channels, "bits": bits_per_sample, "rate": sample_rate}),
                AudioAssetSource::Sequence(values) => json!({"sequence": values.iter().map(source).collect::<Vec<_>>()}),
                AudioAssetSource::File(_) => serde_json::Value::Null,
            }
        }
        json!({"handle": playback.handle, "source": playback.source.as_ref().map(source), "volume": playback.volume * state.master_volume * state.track_volumes.get(playback.track as usize).copied().unwrap_or(1.0), "loop": playback.looping})
    }).collect::<Vec<_>>();
    json!({"generation": state.generation, "started": state.started, "playbacks": playbacks})
}

impl BrowserGame {
    fn synchronize_rating(&mut self) -> Result<()> {
        if self.rating.sync(&self.runtime, self.resolution) {
            self.runtime.clear_platform_input_for_modal().browser()?;
            self.touches.clear();
            self.account.cancel_pointer();
        }
        Ok(())
    }

    fn game_input_available(&self) -> bool {
        self.runtime.account_ui().is_none() && self.runtime.app_rating_prompt().is_none()
    }

    fn synchronize_account(&mut self) -> Result<()> {
        if self.account.sync(&self.runtime, self.resolution)? {
            self.touches.clear();
        }
        Ok(())
    }
    fn prepare_render_stream(&mut self) -> Result<gpu::PreparedFrame> {
        if let Some(snapshot) = self.runtime.sprite_catalog_snapshot_since(self.revision) {
            self.revision = snapshot.revision;
            self.assets.apply_sprite_catalog_snapshot(snapshot)?;
        }
        self.assets
            .apply_composite_updates(self.runtime.take_composite_updates());
        self.assets.prepare_gpu_frame_at_resolution(
            self.resolution,
            &self.runtime.take_render_commands(),
            &self.runtime.take_text_commands(),
            &self.runtime.take_rect_commands(),
            &self.runtime.take_capture_commands(),
        )
    }

    fn synchronize_audio_clock(&mut self, elapsed: Duration) {
        let state = self.runtime.audio_output_state();
        let transitions = self.audio_clock.synchronize(&state, elapsed);
        self.runtime.apply_audio_playback_transitions(&transitions);
    }
}

/// Complete calls made outside the display callback without clearing the
/// target. The host must render this packet before replacing the drawable.
#[unsafe(no_mangle)]
pub extern "C" fn stella_flush() -> i32 {
    with_game(|game| {
        game.before_clear = gpu::PreparedFrame::default();
        game.frame = game.prepare_render_stream()?;
        let packet = game.frame.packet(&mut game.assets, &mut game.uploaded)?;
        game.packet = CString::new(serde_json::to_vec(&packet)?)?;
        Ok(())
    })
}

/// Lifecycle events must reconcile physical players even when no frame runs.
#[unsafe(no_mangle)]
pub extern "C" fn stella_audio_packet() -> *const std::ffi::c_char {
    let mut pointer = std::ptr::null();
    with_game(|game| {
        game.audio_packet = CString::new(serde_json::to_vec(&audio_packet(
            &game.runtime.audio_output_state(),
        ))?)?;
        pointer = game.audio_packet.as_ptr();
        Ok(())
    });
    pointer
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_packet() -> *const std::ffi::c_char {
    GAME.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |game| game.packet.as_ptr())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_pointer(x: f64, y: f64, down: i32) -> i32 {
    with_game(|game| {
        if game.game_input_available() {
            game.runtime.set_cursor(x, y, down != 0).browser()?;
        }
        Ok(())
    })
}

/// Preserve the full native touch vector. Lua publishes its first two entries,
/// but GameApp's pinch state tests the uncapped vector for exactly two touches.
#[unsafe(no_mangle)]
pub extern "C" fn stella_touch(phase: i32, id: u32, x: f64, y: f64) -> i32 {
    with_game(|game| {
        if game.game_input_available() {
            game.touches.event(&game.runtime, phase, id, x, y)?;
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_touches(
    count: i32,
    id1: u32,
    x1: i32,
    y1: i32,
    id2: u32,
    x2: i32,
    y2: i32,
) -> i32 {
    with_game(|game| {
        if !game.game_input_available() {
            return Ok(());
        }
        let touches = [(u64::from(id1), x1, y1), (u64::from(id2), x2, y2)];
        game.touches
            .replace(&game.runtime, &touches[..count.clamp(0, 2) as usize])
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_key(code: i32, down: i32) -> i32 {
    with_game(|game| {
        if !game.game_input_available() {
            return Ok(());
        }
        let name = match code {
            0 => "KEY_BACK",
            1 => "KEY_MENU",
            2 => "VOLUME_UP",
            3 => "VOLUME_DOWN",
            _ => return Ok(()),
        };
        game.runtime.set_key(name, down != 0).browser()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_wheel(delta: i32, shift: i32, control: i32) -> i32 {
    with_game(|game| {
        if !game.game_input_available() {
            return Ok(());
        }
        game.runtime
            .mouse_wheel(delta, shift != 0, control != 0)
            .browser()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_active(active: i32) -> i32 {
    with_game(|game| {
        if game.active == (active != 0) {
            return Ok(());
        }
        game.runtime.set_application_active(active != 0).browser()?;
        game.touches.clear();
        game.rating.cancel_pointer();
        game.account.cancel_pointer();
        if active != 0 {
            game.runtime.post_application_resumed();
        }
        game.runtime
            .set_application_audio_active(active != 0)
            .browser()?;
        game.synchronize_audio_clock(Duration::ZERO);
        game.active = active != 0;
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_save() -> i32 {
    with_game(|game| {
        // The original pause callback serializes settings/highscores/BI.
        game.runtime.call_global("gamePaused").browser()?;
        if game.active {
            game.runtime.call_global("gameResumed").browser()?;
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_shutdown() {
    GAME.with(|slot| *slot.borrow_mut() = None);
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_cancel_account() -> i32 {
    with_game(|game| {
        if let Some(view) = game.runtime.account_ui() {
            game.runtime
                .account_ui_action(view.id, stella_script::AccountUiAction::Cancel)
                .browser()?;
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_account_frame(now: f64) -> i32 {
    with_game(|game| {
        if game
            .account
            .frame(&game.runtime, game.resolution, now, game.active)?
        {
            game.touches.clear();
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_account_packet() -> *const std::ffi::c_char {
    GAME.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |game| game.account.packet())
    })
}

fn with_account(token: u32, operation: impl FnOnce(&mut BrowserGame) -> Result<()>) -> i32 {
    with_game(|game| {
        game.synchronize_account()?;
        game.synchronize_rating()?;
        if game.active && game.runtime.app_rating_prompt().is_none() && game.account.accepts(token)
        {
            operation(game)?;
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_account_pointer(
    token: u32,
    phase: i32,
    x: f32,
    y: f32,
    extend: i32,
) -> i32 {
    with_account(token, |game| {
        game.account
            .pointer(&game.runtime, phase, x, y, extend != 0)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_account_key(token: u32, code: i32, shift: i32) -> i32 {
    let names = [
        "Escape",
        "Enter",
        "Tab",
        "ArrowUp",
        "ArrowDown",
        "Home",
        "End",
    ];
    with_account(token, |game| {
        if let Some(name) = usize::try_from(code).ok().and_then(|code| names.get(code)) {
            game.account.key(&game.runtime, name, shift != 0)?;
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_account_wheel(token: u32, rows: i32) -> i32 {
    with_account(token, |game| {
        game.account.wheel(rows);
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_account_focus(token: u32, field: i32) -> i32 {
    with_account(token, |game| {
        game.account.focus(field);
        Ok(())
    })
}

/// # Safety
/// `name` must point to a valid, NUL-terminated UTF-8 string for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stella_account_control(token: u32, name: *const std::ffi::c_char) -> i32 {
    with_account(token, |game| {
        let name = unsafe { CStr::from_ptr(name) }
            .to_str()
            .context("Invalid account control")?;
        game.account.control(&game.runtime, name)
    })
}

/// # Safety
/// `value` must point to a valid, NUL-terminated UTF-8 JSON string for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn stella_account_edit(
    token: u32,
    field: i32,
    value: *const std::ffi::c_char,
) -> i32 {
    with_account(token, |game| {
        let value = unsafe { CStr::from_ptr(value) }
            .to_str()
            .map_err(|_| anyhow!("Invalid account editor input"))?;
        let value =
            serde_json::from_str(value).map_err(|_| anyhow!("Invalid account editor input"))?;
        game.account.edit(field, &value);
        Ok(())
    })
}

/// Private editor transfer, consumed only by live input elements. It is never
/// exposed in public frame packets, diagnostics, Lua globals or save data.
#[unsafe(no_mangle)]
pub extern "C" fn stella_account_editor(token: u32, field: i32) -> *const std::ffi::c_char {
    let mut pointer = std::ptr::null();
    with_account(token, |game| {
        pointer = game.account.editor(field)?;
        Ok(())
    });
    pointer
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_rating_frame() -> i32 {
    with_game(|game| {
        game.synchronize_rating()?;
        game.rating.frame(&game.runtime)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_rating_packet() -> *const std::ffi::c_char {
    GAME.with(|slot| {
        slot.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |game| game.rating.packet())
    })
}

fn with_rating(token: u32, operation: impl FnOnce(&mut BrowserGame) -> Result<()>) -> i32 {
    with_game(|game| {
        game.synchronize_rating()?;
        if game.active && game.rating.accepts(token) {
            operation(game)?;
            game.synchronize_rating()?;
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_rating_pointer(token: u32, phase: i32, x: f32, y: f32) -> i32 {
    with_rating(token, |game| {
        game.rating.pointer(&game.runtime, phase, x, y)
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_rating_choose(token: u32, code: i32) -> i32 {
    with_rating(token, |game| {
        if let Some(choice) = rating::choice(code) {
            game.rating.choose(&game.runtime, choice)?;
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_rating_focus(token: u32, code: i32) -> i32 {
    with_rating(token, |game| {
        if let Some(choice) = rating::choice(code) {
            game.rating.focus(choice);
        }
        Ok(())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_rating_key(token: u32, code: i32, shift: i32) -> i32 {
    with_rating(token, |game| {
        game.rating.key(&game.runtime, code, shift != 0)
    })
}

/// One-shot egress also works inside a trusted UI callback, so a Rate action
/// can attempt its native external URL without waiting for another game frame.
#[unsafe(no_mangle)]
pub extern "C" fn stella_platform_packet() -> *const std::ffi::c_char {
    let mut pointer = std::ptr::null();
    with_game(|game| {
        use stella_script::{GamerServicesView, PlatformActionRequest};
        let actions: Vec<_> = game.runtime.take_platform_action_requests().into_iter().map(|action| {
            match action {
                PlatformActionRequest::OpenUrl { url } => json!({"kind":"openUrl","url":url}),
                PlatformActionRequest::OpenAppStoreProduct { product_id, product_type } => json!({"kind":"appStoreProduct","productId":product_id,"productType":product_type}),
                PlatformActionRequest::PlayVideo { path } => json!({"kind":"video","path":path}),
                PlatformActionRequest::ShowGamerServices { view, entries } => json!({"kind":"gamerServices","view":match view { GamerServicesView::Achievements=>"achievements", GamerServicesView::Leaderboards=>"leaderboards" },"entries":entries}),
            }
        }).collect();
        game.platform_packet = CString::new(serde_json::to_vec(&actions)?)?;
        pointer = game.platform_packet.as_ptr();
        Ok(())
    });
    pointer
}

fn main() {}
