//! Text draw commands submitted through Purple's IFont interface.

use std::sync::Arc;

use stella_assets::ka3d::BitmapFont;

use super::SystemFontRenderBinding;

#[derive(Debug, Clone)]
pub struct TextRenderCommand {
    pub order: u64,
    pub text: String,
    pub font: String,
    /// Exact native IFont object selected when Purple enters its draw virtual.
    /// The desktop host expands text later, so a name-only lookup would let a
    /// replacement or release rebind an already submitted draw.
    pub font_binding: Option<TextFontBinding>,
    pub x: f64,
    pub y: f64,
    /// Float32 coordinates passed to the selected IFont virtual before the
    /// current GL_Context transform. UIKit SystemFont truncates its anchored
    /// coordinates to signed integers before submitting the cached label;
    /// retaining this local origin lets a deferred backend preserve that
    /// ordering under rotation or a non-uniform matrix.
    pub native_system_origin: Option<[f32; 2]>,
    pub scale_x: f64,
    pub scale_y: f64,
    pub angle: f64,
    /// Optional exact 2D linear transform `[m00, m01, m10, m11]` applied to
    /// bitmap-font local coordinates. `drawUITextNative` installs Purple's
    /// Scale * Rotation basis, which cannot be reconstructed from the
    /// compatibility scale/angle fields as Rotation * Scale when X/Y scales
    /// differ.
    pub matrix: Option<[f64; 4]>,
    /// Optional position-only basis for the local anchor/cursor offset.
    ///
    /// Purple's BitmapFont moves each glyph through the live context pivot,
    /// so its cursor and glyph quad share `matrix`. UIKit SystemFont instead
    /// anchors its float x/y before `GL_Image::draw`: that anchored position
    /// is only axis-scaled, while the cached label quad still uses `matrix`
    /// for its orientation. Keeping the bases separate preserves that native
    /// distinction under non-zero rotation.
    pub position_matrix: Option<[f64; 4]>,
    pub alpha: f64,
    pub horizontal_anchor: String,
    pub vertical_anchor: String,
    pub projection_3d: Option<TextProjection3D>,
    pub clip_rect: Option<[i32; 4]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextFontBinding {
    Bitmap {
        /// Shared constructed IFont value. Purple's ResourceManager and every
        /// submitted glyph retain the same BitmapFont/AtlasSprite ownership;
        /// deferred wgpu commands must not deep-copy the glyph tree.
        font: Arc<BitmapFont>,
        /// Constructor-resolved atlas source retained with that IFont value.
        texture_source: String,
    },
    /// Snapshot of the UIKit-backed IFont implementation selected at native
    /// submission time. Font bytes are shared between deferred commands, but
    /// every field that participates in LabelPool lookup remains immutable.
    System(SystemFontRenderBinding),
}

/// Perspective and optional model transform retained by the native context.
/// Image overloads select raw local vertices or normalized 2D vertices.
/// A disabled model represents identity while perspective stays active.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TextProjection3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_x: f32,
    pub custom_model: bool,
}
