//! Text draw commands submitted through Purple's IFont interface.

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
    pub alpha: f64,
    pub horizontal_anchor: String,
    pub vertical_anchor: String,
    pub projection_3d: Option<TextProjection3D>,
    pub clip_rect: Option<[i32; 4]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextFontBinding {
    Bitmap {
        font: BitmapFont,
        /// Resolved atlas source retained by the constructed BitmapFont.
        texture_source: String,
    },
    /// Snapshot of the UIKit-backed IFont implementation selected at native
    /// submission time. Font bytes are shared between deferred commands, but
    /// every field that participates in LabelPool lookup remains immutable.
    System(SystemFontRenderBinding),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextProjection3D {
    pub z: f64,
    pub rotation_x: f64,
}
