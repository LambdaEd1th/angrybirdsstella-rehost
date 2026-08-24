//! Deferred sprite-command payloads matching Purple's ResourceManager draws.

use std::collections::BTreeMap;

use stella_assets::ka3d::{CompositePart, SpriteRegion};

/// Active ResourceManager catalog exported to the deferred wgpu host.
/// Purple draws immediately through pointers; the Rust host prepares GPU
/// commands later, so it snapshots the same last-entry-wins catalog whenever
/// its native lifetime revision changes.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteCatalogSnapshot {
    pub revision: u64,
    pub regions: BTreeMap<String, SpriteCatalogRegion>,
    pub composites: BTreeMap<String, Vec<CompositePart>>,
    pub masked_textures: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteCatalogRegion {
    /// Deterministic host identity for the retained native `SpriteSheet*`.
    /// Purple uses this pointer as the second key of its z-ordered draw map.
    /// The id changes when a same-named sheet is reconstructed, while regions
    /// retained by existing objects keep the old allocation identity.
    pub native_sheet_id: u64,
    /// Resolved host source used as both the decoder path and GPU-cache key.
    pub texture_source: String,
    pub sprite: SpriteRegion,
}

/// One native CompoSprite record paired with the AtlasSprite pointer frozen
/// by the COMP loader. Keeping the pair on a deferred command prevents a
/// later sheet shadow/release from rebinding an existing scene object.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundCompositePart {
    pub part: CompositePart,
    pub region: SpriteCatalogRegion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskedTextureBinding {
    /// The native lookup returned null at submission time. The safe desktop
    /// host keeps the command/order record but must never bind a later image.
    Missing,
    Source(String),
}

#[derive(Debug, Clone, Copy)]
pub struct RenderQuad {
    /// Vertex order consumed by the immediate renderer. `PreparedFrame`
    /// expands this as 0,1,2 / 2,1,3, matching Purple's triangle list.
    pub positions: [[f64; 2]; 4],
    /// Full-texture normalized coordinates in the same vertex order.
    pub uv: [[f64; 2]; 4],
}

#[derive(Debug, Clone, Copy)]
pub struct RenderState {
    pub translate_x: f64,
    pub translate_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub angle: f64,
    /// Exact column-major 2D linear transform `[m00, m01, m10, m11]`.
    /// Animation scenes set this when parent composition produces shear that
    /// cannot be represented by the scale/angle compatibility fields.
    pub matrix: Option<[f64; 4]>,
    /// Override the atlas pivot for native paths that have already anchored
    /// raw sprite coordinates themselves. SpriteComponent and
    /// SpriteComponentCustom leave this unset and use the pivot stored in
    /// their bound SPRT record.
    pub sprite_pivot: Option<[f64; 2]>,
    pub pivot_x: f64,
    pub pivot_y: f64,
    /// Optional destination size from the seven/eight-argument native
    /// `drawSprite` overload. It stretches atlas sprites but is ignored for
    /// composite sprites.
    pub draw_size: Option<[f64; 2]>,
    /// Native helpers such as `renderMaskedImageNative` submit an arbitrary
    /// four-corner mesh instead of transforming an atlas rectangle.
    pub explicit_quad: Option<RenderQuad>,
    /// Exact screen-space corners for a native atlas-sprite quad, in the
    /// renderer's TL, TR, BL, BR order. Unlike `explicit_quad`, the UVs still
    /// come from the named atlas region. Purple's rubber-band and textured-
    /// line helpers build their four float32 vertices independently, so
    /// reducing them to one affine matrix loses observable edge rounding.
    pub native_sprite_quad: Option<[[f64; 2]; 4]>,
    pub alpha: f64,
    /// Native renderer scissor edges `[left, top, right, bottom]` captured at
    /// draw submission time. `None` means the full current drawable.
    pub clip_rect: Option<[i32; 4]>,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            angle: 0.0,
            matrix: None,
            sprite_pivot: None,
            pivot_x: 0.0,
            pivot_y: 0.0,
            draw_size: None,
            explicit_quad: None,
            native_sprite_quad: None,
            alpha: 1.0,
            clip_rect: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderCommand {
    /// Monotonic order of the original immediate renderer submission.
    pub order: u64,
    pub sprite: String,
    pub texture: Option<String>,
    pub texture_scale: f64,
    /// Submission-time image pointer used by alpha-masked sprite programs.
    /// `None` retains the dynamic-name path for components whose ownership
    /// has not selected an explicit image at command creation.
    pub masked_texture_binding: Option<MaskedTextureBinding>,
    /// Atlas resource pointer retained by a native scene component or copied
    /// when an immediate draw is submitted. This preserves same-frame
    /// draw-then-release and same-name shadowing in the deferred wgpu host.
    pub bound_region: Option<SpriteCatalogRegion>,
    /// CompoSprite pointer retained by a native scene component. Every child
    /// carries the AtlasSprite pointer resolved when its COMP file loaded.
    pub bound_composite: Option<Vec<BoundCompositePart>>,
    pub shader: Option<SpriteShader>,
    pub clip_holes: Vec<RenderHole>,
    /// Native DirtMechanics replaces the object's ordinary sprite callback
    /// with a background polygon followed by its clipped foreground polygons.
    pub dirt: Option<DirtRenderCommand>,
    pub x: f64,
    pub y: f64,
    pub state: RenderState,
    pub world_space: bool,
}

#[derive(Debug, Clone)]
pub struct DirtRenderCommand {
    pub background_texture: String,
    pub foreground_texture: String,
    /// Image pointers resolved once by the native Dirt constructor. They do
    /// not follow later ResourceManager shadowing or release by name.
    pub background_texture_binding: MaskedTextureBinding,
    pub foreground_texture_binding: MaskedTextureBinding,
    /// Triangle positions are in the scene object's local physics units.
    /// Purple multiplies them by 20 immediately before projection and also
    /// passes the unscaled pair through as repeating texture coordinates.
    pub background_triangles: Vec<Vec<RenderTriangle>>,
    pub foreground_triangles: Vec<Vec<RenderTriangle>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderTriangle {
    pub vertices: [[f64; 2]; 3],
}

/// Script-defined sprite shader state copied by Purple when a shader is bound
/// to a sprite or animation scene. The bundled shader names are process-unique
/// suffixed variants, so the renderer matches their stable name prefix.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteShader {
    pub name: String,
    pub diffuse: [f64; 4],
    pub lightness: f64,
    pub saturation: f64,
    pub highlight: f64,
}

impl Default for SpriteShader {
    fn default() -> Self {
        Self {
            name: String::new(),
            diffuse: [1.0; 4],
            lightness: 0.0,
            saturation: 1.0,
            highlight: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RenderHole {
    /// Hole center in unscaled sprite-local pixels relative to its pivot.
    pub x: f64,
    pub y: f64,
    /// Circumradius of Purple's eight-point collision cut, in local pixels.
    pub radius: f64,
}
