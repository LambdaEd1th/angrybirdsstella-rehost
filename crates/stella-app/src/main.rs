#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, anyhow};
#[cfg(any(debug_assertions, test))]
use clap::Parser;
use image::RgbaImage;
use stella_assets::ka3d::{
    BitmapFont, CompositePart, CompositeSpriteSet, Ka3dEnvelope, SpriteRegion, SpriteSheet,
};
use stella_script::{
    AudioOutputClock, AudioOutputState, AudioPlaybackTransitions, BoundCompositePart,
    CaptureRenderCommand, ColorMeshTopology, ColorProgram, DirtRenderCommand, GamerServicesView,
    MaskedTextureBinding, PlatformActionRequest, RectRenderCommand, RenderCommand, RenderQuad,
    RenderTriangle, ScreenshotShareRequest, SpriteCatalogRegion, SpriteCatalogSnapshot,
    SpriteGeometrySubmission, SpriteShader, StellaLua, SystemFontLayoutFace,
    SystemFontRenderBinding, SystemFontShapedLine, TextFontBinding, TextProjection3D,
    TextRenderCommand,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

mod account_ui;
mod app;
mod apprater_ui;
mod assets;
mod audio;
#[cfg(any(not(debug_assertions), test))]
mod bundle_format;
#[cfg(any(not(debug_assertions), test))]
mod bundled_runtime;
#[cfg(any(debug_assertions, test))]
mod cli;
mod gpu;
mod platform_ui_drawing;

use app::*;
use assets::*;
use audio::AudioDevice;
use gpu::GpuRenderer;

const GAME_WIDTH: u32 = 1024;
const GAME_HEIGHT: u32 = 768;
const MAX_GAME_DIMENSION: u32 = u16::MAX as u32;
// Purple's Configuration constructor (`sub_100401398`) writes framerate=60
// at +0x48. `-[AppController startUpdate]` then sets CADisplayLink's legacy
// frameInterval to integer `60 / framerate`, i.e. one display refresh. There
// is no adaptive/preferred-frame-rate branch in the Stella launch path.
const DISPLAY_LINK_STEP: Duration = Duration::from_nanos(16_666_667);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GameResolution {
    width: u32,
    height: u32,
}

impl GameResolution {
    fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(anyhow!("game resolution must be non-zero"));
        }
        if width > MAX_GAME_DIMENSION || height > MAX_GAME_DIMENSION {
            return Err(anyhow!(
                "game resolution {width}x{height} exceeds Purple's 16-bit sprite geometry"
            ));
        }
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

#[cfg(test)]
mod reference_renderer;

fn main() -> Result<()> {
    #[cfg(any(debug_assertions, test))]
    {
        cli::run()
    }
    #[cfg(all(not(debug_assertions), not(test)))]
    {
        let result = bundled_runtime::run();
        #[cfg(target_os = "windows")]
        if let Err(error) = &result {
            bundled_runtime::show_error(error);
        }
        result
    }
}
