//! Single-threaded browser host. Emscripten supplies Lua's C ABI and MEMFS;
//! WebGL consumes the same renderer-neutral frame expansion as the desktop.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::CString,
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
#[allow(dead_code, unused_imports)]
#[path = "../../stella-app/src/assets/mod.rs"]
mod assets;
mod gpu;
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
    frame: gpu::PreparedFrame,
    packet: CString,
    active: bool,
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
            if let Some(game) = slot.borrow().as_ref() {
                game.runtime.set_preferred_language(locale).browser()?;
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
        GAME.with(|slot| {
            *slot.borrow_mut() = Some(BrowserGame {
                runtime,
                resolution,
                assets,
                revision: 0,
                audio_clock: AudioOutputClock::default(),
                uploaded: HashSet::new(),
                frame: gpu::PreparedFrame::default(),
                packet: CString::new("{}").unwrap(),
                active: true,
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
        let state = game.runtime.audio_output_state();
        let transitions = game.audio_clock.synchronize(&state, elapsed);
        game.runtime.apply_audio_playback_transitions(&transitions);
        if game.active {
            game.runtime.update(elapsed.as_secs_f64()).browser()?;
        }
        let background = game.runtime.background_color();
        let clear_clip = game.runtime.framebuffer_clip_rect();
        game.runtime.draw().browser()?;
        if let Some(snapshot) = game.runtime.sprite_catalog_snapshot_since(game.revision) {
            game.revision = snapshot.revision;
            game.assets.apply_sprite_catalog_snapshot(snapshot)?;
        }
        game.assets
            .apply_composite_updates(game.runtime.take_composite_updates());
        game.frame = game.assets.prepare_gpu_frame_at_resolution(
            game.resolution,
            &game.runtime.take_render_commands(),
            &game.runtime.take_text_commands(),
            &game.runtime.take_rect_commands(),
            &game.runtime.take_capture_commands(),
        )?;
        let mut packet = game.frame.packet(&mut game.assets, &mut game.uploaded)?;
        packet["background"] = json!(background);
        packet["resolution"] = json!([game.resolution.width, game.resolution.height]);
        packet["clearClip"] = json!(clear_clip);
        packet["audio"] = audio_packet(&game.runtime.audio_output_state());
        packet["exit"] = json!(game.runtime.exit_requested());
        // Browser account modals have a cancel ingress instead of leaving the
        // game trapped behind an unavailable native UIKit view.
        packet["account"] = json!(game.runtime.account_ui().map(|view| view.id));
        packet["locale"] = json!(game.runtime.current_locale().browser()?);
        if let Some(prompt) = game.runtime.app_rating_prompt() {
            game.runtime
                .answer_app_rating(prompt.id, stella_script::AppRatingChoice::Later)
                .browser()?;
        }
        // Offline host actions are deliberately handled here, so requests do
        // not accumulate indefinitely when the browser has no native provider.
        game.runtime.take_platform_action_requests();
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
    with_game(|game| game.runtime.set_cursor(x, y, down != 0).browser())
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
        let touches = [(u64::from(id1), x1, y1), (u64::from(id2), x2, y2)];
        game.runtime
            .set_touches(&touches[..count.clamp(0, 2) as usize])
            .browser()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn stella_key(code: i32, down: i32) -> i32 {
    with_game(|game| {
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
        game.runtime
            .set_application_audio_active(active != 0)
            .browser()?;
        if active != 0 {
            game.runtime.post_application_resumed();
        }
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

fn main() {}
