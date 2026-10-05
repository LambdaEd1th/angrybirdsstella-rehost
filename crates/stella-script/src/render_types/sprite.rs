//! Deferred sprite-command payloads matching Purple's ResourceManager draws.

use std::{
    collections::BTreeMap,
    fmt,
    ops::Deref,
    sync::{Arc, RwLock},
};

use stella_assets::ka3d::{CompositePart, SpriteRegion};

/// Active ResourceManager catalog exported to the deferred wgpu host.
/// Purple draws immediately through pointers; the Rust host prepares GPU
/// commands later, so it snapshots the same last-entry-wins catalog whenever
/// its native lifetime revision changes.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteCatalogSnapshot {
    pub revision: u64,
    /// Currently retained Images, including hidden sheets and empty sheets.
    /// The host records weak owners rather than retaining this map.
    pub image_owners: BTreeMap<String, Arc<super::NativeImageOwner>>,
    pub regions: BTreeMap<String, SpriteCatalogRegion>,
    pub composites: BTreeMap<String, Vec<CompositePart>>,
    pub masked_textures: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteCatalogRegion {
    /// Present on retained sprite objects, absent on immutable draw snapshots.
    pub sheet_image: Option<super::SpriteSheetImageBinding>,
    /// Extent used by AtlasSprite's constructor to normalize its stored UVs.
    /// Later SPRT records replace the sheet's Image without changing these.
    pub uv_image_dimensions: Option<[u32; 2]>,
    /// Submitted Image ownership, absent on the live sprite's sheet borrow.
    pub image_owner: Option<Arc<super::NativeImageOwner>>,
    /// Pixels frozen at the immediate draw or catalog-export boundary.
    /// Live sprite objects resolve their sheet instead of retaining pixels.
    pub decoded_image: Option<Arc<stella_assets::native_image::DecodedNativeImage>>,
    /// Deterministic host identity for the retained native `SpriteSheet*`.
    /// Purple uses this pointer as the second key of its z-ordered draw map.
    /// The id changes when a same-named sheet is reconstructed, while regions
    /// retained by existing objects keep the old allocation identity.
    pub native_sheet_id: u64,
    /// Opaque native Image binding and GPU-cache key. Ordinary sheet images
    /// include the concrete current Image identity plus its resolved
    /// file path; `stella_assets::image_source::image_source_path` unwraps it
    /// only for I/O. Captures use an allocation identity with no file source.
    /// On a live sprite this is the constructor source; snapshot_image reads
    /// the current source from its sheet before publishing a deferred command.
    pub texture_source: String,
    pub sprite: SpriteRegion,
}

impl SpriteCatalogRegion {
    pub(crate) fn borrow_image(
        &self,
    ) -> crate::LuaResult<(MaskedTextureBinding, Option<[u32; 2]>)> {
        let Some(sheet) = &self.sheet_image else {
            return Ok((
                MaskedTextureBinding::with_image(
                    self.texture_source.clone(),
                    self.decoded_image.clone(),
                    self.image_owner.clone(),
                ),
                self.decoded_image
                    .as_ref()
                    .map(|image| [image.width, image.height])
                    .or(self.uv_image_dimensions),
            ));
        };
        let image = sheet.snapshot().map_err(|reason| {
            crate::runtime_error(format!(
                "Native sprite '{}' uses {reason}",
                self.sprite.name
            ))
        })?;
        Ok((
            MaskedTextureBinding::Borrowed {
                source: image.source,
                image: super::NativeImageBorrow::new(sheet.clone(), image.owner.identity()),
            },
            image.dimensions,
        ))
    }

    pub(crate) fn borrow_texture(
        self: &Arc<Self>,
    ) -> crate::LuaResult<(MaskedTextureBinding, Option<super::NativeTextureBorrow>)> {
        let image = self.snapshot_image()?;
        let borrow = self
            .sheet_image
            .as_ref()
            .zip(image.image_owner.as_ref())
            .map(|(sheet, owner)| super::NativeTextureBorrow::new(sheet.clone(), owner.identity()));
        let binding = if borrow.is_some() {
            MaskedTextureBinding::Source(image.texture_source.clone())
        } else {
            // Immutable diagnostic regions do not have a native sheet lifetime.
            MaskedTextureBinding::with_image(
                image.texture_source.clone(),
                image.decoded_image.clone(),
                image.image_owner.clone(),
            )
        };
        Ok((binding, borrow))
    }

    pub(crate) fn snapshot_image(self: &Arc<Self>) -> crate::LuaResult<Arc<Self>> {
        let Some(binding) = &self.sheet_image else {
            return Ok(Arc::clone(self));
        };
        let image = binding.snapshot().map_err(|reason| {
            crate::runtime_error(format!(
                "Native sprite '{}' uses {reason}",
                self.sprite.name
            ))
        })?;
        Ok(Arc::new(Self {
            sheet_image: None,
            image_owner: Some(image.owner),
            decoded_image: image.image,
            texture_source: image.source,
            ..self.as_ref().clone()
        }))
    }
}

/// One native CompoSprite record paired with the AtlasSprite pointer frozen
/// by the COMP loader. Keeping the pair on a deferred command prevents a
/// later sheet shadow/release from rebinding an existing scene object.
#[derive(Debug, Clone, PartialEq)]
pub struct BoundCompositePart {
    /// Shared old-ABI COW label retained by the concrete CompoSprite Entry.
    /// The parsed record remains available for Lua query/mutation semantics.
    pub sprite: SharedSpriteName,
    pub part: CompositePart,
    pub region: Arc<SpriteCatalogRegion>,
}

/// Shared native-style CompoSprite object retained by ResourceManager,
/// particles and scene components.
///
/// Purple mutates Entry records on the concrete CompoSprite after other
/// objects have retained its pointer. The deferred host therefore shares one
/// live owner, but publishes an immutable Arc snapshot at each immediate draw
/// boundary so a later same-frame mutation cannot rewrite an earlier command.
#[derive(Debug)]
pub(crate) struct CompositeSpriteOwner {
    parts: RwLock<Arc<Vec<BoundCompositePart>>>,
}

impl CompositeSpriteOwner {
    pub(crate) fn new(parts: Vec<BoundCompositePart>) -> Self {
        Self {
            parts: RwLock::new(Arc::new(parts)),
        }
    }

    pub(crate) fn snapshot(&self) -> Arc<Vec<BoundCompositePart>> {
        Arc::clone(
            &self
                .parts
                .read()
                .expect("composite sprite owner lock poisoned"),
        )
    }

    pub(crate) fn replace(&self, parts: Vec<BoundCompositePart>) {
        *self
            .parts
            .write()
            .expect("composite sprite owner lock poisoned") = Arc::new(parts);
    }

    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.parts
            .read()
            .expect("composite sprite owner lock poisoned")
            .is_empty()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.parts
            .read()
            .expect("composite sprite owner lock poisoned")
            .len()
    }

    pub(crate) fn first_native_sheet_id(&self) -> Option<u64> {
        self.parts
            .read()
            .expect("composite sprite owner lock poisoned")
            .first()
            .map(|part| part.region.native_sheet_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskedTextureBinding {
    /// The native lookup returned null at submission time. The safe desktop
    /// host keeps the command/order record but must never bind a later image.
    Missing,
    Source(String),
    /// A scene object's raw Image pointer. Resolve only when that masked
    /// branch actually submits visible geometry, then publish Retained.
    Borrowed {
        source: String,
        image: super::NativeImageBorrow,
    },
    Retained {
        source: String,
        image: Option<Arc<stella_assets::native_image::DecodedNativeImage>>,
        image_owner: Option<Arc<super::NativeImageOwner>>,
    },
}

impl MaskedTextureBinding {
    pub(crate) fn native_dimensions(&self) -> crate::LuaResult<Option<[u32; 2]>> {
        match self {
            Self::Borrowed { source, image } => image
                .snapshot()
                .map(|image| image.dimensions)
                .map_err(|reason| {
                    crate::runtime_error(format!("Native masked Image '{source}' uses {reason}"))
                }),
            Self::Retained { image, .. } => {
                Ok(image.as_ref().map(|image| [image.width, image.height]))
            }
            Self::Missing | Self::Source(_) => Ok(None),
        }
    }

    pub(crate) fn snapshot(&self) -> crate::LuaResult<Self> {
        let Self::Borrowed { source, image } = self else {
            return Ok(self.clone());
        };
        let image = image.snapshot().map_err(|reason| {
            crate::runtime_error(format!("Native masked Image '{source}' uses {reason}"))
        })?;
        Ok(Self::with_image(
            image.source,
            image.image,
            Some(image.owner),
        ))
    }

    pub(crate) fn with_image(
        source: String,
        image: Option<Arc<stella_assets::native_image::DecodedNativeImage>>,
        image_owner: Option<Arc<super::NativeImageOwner>>,
    ) -> Self {
        if image.is_some() || image_owner.is_some() {
            Self::Retained {
                source,
                image,
                image_owner,
            }
        } else {
            Self::Source(source)
        }
    }

    pub fn source(&self) -> Option<&str> {
        match self {
            Self::Missing => None,
            Self::Source(source)
            | Self::Borrowed { source, .. }
            | Self::Retained { source, .. } => Some(source),
        }
    }

    pub fn image(&self) -> Option<&Arc<stella_assets::native_image::DecodedNativeImage>> {
        match self {
            Self::Retained { image, .. } => image.as_ref(),
            Self::Missing | Self::Source(_) | Self::Borrowed { .. } => None,
        }
    }

    pub fn image_owner(&self) -> Option<&Arc<super::NativeImageOwner>> {
        match self {
            Self::Retained { image_owner, .. } => image_owner.as_ref(),
            Self::Missing | Self::Source(_) | Self::Borrowed { .. } => None,
        }
    }
}

/// Retained inputs for Purple's optional alpha-masked texture branch.
///
/// `sub_10008D428` receives the fill image as one pointer, resolves the mask
/// image from the AtlasSprite passed beside it, and consumes both axis scales
/// only on that branch. Ordinary sprite submissions therefore must not carry
/// an inline string, scale and binding payload. The deferred host retains the
/// same selected image state behind one pointer instead.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteTextureSubmission {
    pub name: Arc<str>,
    pub scale: f64,
    pub binding: MaskedTextureBinding,
    /// Geometry appended to a named TexturizedSprite before its later flush.
    pub native_quad: Option<Arc<NativeMaskedQuad>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeMaskedQuad {
    /// Raw NDC vertices from sub_10008D428, before the flush-time model/projection.
    pub positions: [[f32; 2]; 4],
    pub mask_uv: [[f32; 2]; 4],
    pub fill_uv: [[f32; 2]; 4],
}

impl SpriteTextureSubmission {
    pub(crate) fn snapshot(self: &Arc<Self>) -> crate::LuaResult<Arc<Self>> {
        if !matches!(self.binding, MaskedTextureBinding::Borrowed { .. }) {
            return Ok(Arc::clone(self));
        }
        Ok(Arc::new(Self {
            binding: self.binding.snapshot()?,
            ..self.as_ref().clone()
        }))
    }
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
    /// Context+0x4c/0x50 custom model, independent of the 2D affine fields.
    pub custom_model: Option<super::TextProjection3D>,
    pub translate_x: f64,
    pub translate_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub angle: f64,
    /// Exact column-major 2D linear transform `[m00, m01, m10, m11]`.
    /// Animation scenes set this when parent composition produces shear that
    /// cannot be represented by the scale/angle compatibility fields.
    pub matrix: Option<[f64; 4]>,
    /// Optional unprojected affine transform used by Purple's
    /// `2d-sprite-alpha-masked` path. The six values are
    /// `[tx, ty, m00, m01, m10, m11]` in world pixels, before camera
    /// translation/zoom. Keeping this separate from `matrix` is required for
    /// themed terrain: the mask follows the projected sprite while its fill
    /// coordinates remain continuous between neighbouring world objects.
    pub masked_texture_matrix: Option<[f64; 6]>,
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
    pub alpha: f64,
    /// Native renderer scissor edges `[left, top, right, bottom]` captured at
    /// draw submission time. `None` means the full current drawable.
    pub clip_rect: Option<[i32; 4]>,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            custom_model: None,
            translate_x: 0.0,
            translate_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            angle: 0.0,
            matrix: None,
            masked_texture_matrix: None,
            sprite_pivot: None,
            pivot_x: 0.0,
            pivot_y: 0.0,
            draw_size: None,
            alpha: 1.0,
            clip_rect: None,
        }
    }
}

impl RenderState {
    /// Reconstruct the camera-free fill-coordinate transform assembled by
    /// `sub_10008D428`. Purple first rotates a mask vertex around the live
    /// GL-context pivot and only then applies the independent X/Y texture
    /// scales, so the linear part is `Scale * Rotation`.
    ///
    /// `relative_pivot_*` is the live context pivot after subtracting the
    /// atlas pivot already represented by the host region's local vertices.
    pub(crate) fn native_masked_texture_matrix(
        position_x_pixels: f32,
        position_y_pixels: f32,
        scale_x: f32,
        scale_y: f32,
        angle: f32,
        relative_pivot_x: f32,
        relative_pivot_y: f32,
    ) -> [f64; 6] {
        let (sine, cosine) = angle.sin_cos();
        let rotation_m00 = cosine;
        let rotation_m01 = -sine;
        let rotation_m10 = sine;
        let rotation_m11 = cosine;

        // The native call receives position*20/scale and later multiplies the
        // complete rotated coordinate by that axis' scale. Keep the division
        // and multiplication boundaries instead of cancelling them so the
        // float32 origin follows Purple for awkward authored scales.
        let local_origin_x = rotation_m00 * -relative_pivot_x + rotation_m01 * -relative_pivot_y;
        let local_origin_y = rotation_m10 * -relative_pivot_x + rotation_m11 * -relative_pivot_y;
        let origin_x = (position_x_pixels / scale_x + local_origin_x) * scale_x;
        let origin_y = (position_y_pixels / scale_y + local_origin_y) * scale_y;

        [
            f64::from(origin_x),
            f64::from(origin_y),
            f64::from(scale_x * rotation_m00),
            f64::from(scale_x * rotation_m01),
            f64::from(scale_y * rotation_m10),
            f64::from(scale_y * rotation_m11),
        ]
    }
}

/// Float32 snapshot copied across Purple's immediate renderer boundary.
///
/// Lua-facing compatibility code may keep widened values while assembling a
/// call, but `setRenderState` stores floats in the native GL context and the
/// ordinary sprite member receives every scalar in `s` registers. Deferred
/// commands therefore retain the post-boundary values rather than copying a
/// host-double representation for every submitted sprite.
#[derive(Debug, Clone, Copy)]
pub struct RenderSubmissionState {
    pub translate_x: f32,
    pub translate_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub angle: f32,
    pub matrix: Option<[f32; 4]>,
    pub masked_texture_matrix: Option<[f32; 6]>,
    pub sprite_pivot: Option<[f32; 2]>,
    pub pivot_x: f32,
    pub pivot_y: f32,
    pub draw_size: Option<[f32; 2]>,
    pub alpha: f32,
    pub clip_rect: Option<[i32; 4]>,
}

impl From<RenderState> for RenderSubmissionState {
    fn from(state: RenderState) -> Self {
        Self {
            translate_x: state.translate_x as f32,
            translate_y: state.translate_y as f32,
            scale_x: state.scale_x as f32,
            scale_y: state.scale_y as f32,
            angle: state.angle as f32,
            matrix: state.matrix.map(|matrix| matrix.map(|value| value as f32)),
            masked_texture_matrix: state
                .masked_texture_matrix
                .map(|matrix| matrix.map(|value| value as f32)),
            sprite_pivot: state
                .sprite_pivot
                .map(|pivot| pivot.map(|value| value as f32)),
            pivot_x: state.pivot_x as f32,
            pivot_y: state.pivot_y as f32,
            draw_size: state.draw_size.map(|size| size.map(|value| value as f32)),
            alpha: state.alpha as f32,
            clip_rect: state.clip_rect,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderCommand {
    /// Rare native perspective snapshot. Keep it out of the per-sprite
    /// compact Copy state, like shader and explicit-geometry payloads.
    pub projection_3d: Option<Arc<super::TextProjection3D>>,
    /// Monotonic order of the original immediate renderer submission.
    pub order: u64,
    /// Stable sprite label associated with the retained native resource.
    /// The executable's old libstdc++ string copies are reference-counted;
    /// sharing the immutable label preserves that ownership boundary without
    /// allocating again for every deferred command.
    pub sprite: SharedSpriteName,
    /// Optional fill-image state supplied as a pointer to Purple's masked
    /// sprite member. Keeping the complete rare branch out-of-line makes the
    /// overwhelmingly common untextured command pointer-sized here.
    pub texture: Option<Arc<SpriteTextureSubmission>>,
    /// AtlasSprite pointer retained by a native scene component or immediate
    /// draw submission. Deferred commands share that immutable owner instead
    /// of copying its texture path and region name on every frame. This also
    /// preserves same-frame draw-then-release and same-name shadowing.
    pub bound_region: Option<Arc<SpriteCatalogRegion>>,
    /// Immutable child-array snapshot taken from the retained CompoSprite at
    /// this native immediate-draw boundary. Every child carries the
    /// AtlasSprite pointer resolved by that live object. A later Entry
    /// mutation replaces the live owner's snapshot without rewriting this
    /// already deferred command.
    pub bound_composite: Option<Arc<Vec<BoundCompositePart>>>,
    /// Rare per-call vertex payload. Purple passes these arrays directly to
    /// its draw member; they are not part of the copied 0x9c-byte GL state.
    /// The deferred host owns them separately so ordinary commands stay
    /// compact and command clones retain rather than duplicate the vertices.
    pub geometry: Option<SpriteGeometrySubmission>,
    /// Optional shader object supplied as a pointer to the native draw call.
    /// Keep it retained out-of-line so its string and parameter block do not
    /// enlarge every unshaded scene command.
    pub shader: Option<Arc<SpriteShader>>,
    /// Native DirtMechanics replaces the object's ordinary sprite callback
    /// with a background polygon followed by its clipped foreground polygons.
    /// Cached DrawablePolygon pair retained by DirtMechanics. The native
    /// component owns these meshes between cuts instead of rebuilding or
    /// embedding them in every ordinary draw submission.
    pub dirt: Option<Arc<DirtRenderCommand>>,
    /// Float arguments passed beside the copied GL state in `s0`/`s1`.
    pub x: f32,
    pub y: f32,
    pub state: RenderSubmissionState,
    pub world_space: bool,
}

impl RenderCommand {
    pub(crate) fn masked_quad_visible(&self, dimensions: [u32; 2]) -> bool {
        let Some(region) = &self.bound_region else {
            return true;
        };
        let state = self.state;
        let (sine, cosine) = state.angle.sin_cos();
        let [m00, m01, m10, m11] = state.matrix.unwrap_or_else(|| {
            if self.world_space {
                [
                    cosine * state.scale_x,
                    -sine * state.scale_y,
                    sine * state.scale_x,
                    cosine * state.scale_y,
                ]
            } else {
                [
                    state.scale_x * cosine,
                    -state.scale_x * sine,
                    state.scale_y * sine,
                    state.scale_y * cosine,
                ]
            }
        });
        let mut x = state.translate_x + self.x;
        let mut y = state.translate_y + self.y;
        if !self.world_space {
            x *= state.scale_x;
            y *= state.scale_y;
            x = state.scale_x.mul_add(
                (-cosine).mul_add(state.pivot_x, sine.mul_add(state.pivot_y, state.pivot_x)),
                x,
            );
            y = state.scale_y.mul_add(
                (-sine).mul_add(
                    state.pivot_x,
                    (-cosine).mul_add(state.pivot_y, state.pivot_y),
                ),
                y,
            );
        }
        let [pivot_x, pivot_y] = state.sprite_pivot.unwrap_or([
            f32::from(region.sprite.pivot_x),
            f32::from(region.sprite.pivot_y),
        ]);
        let [width, height] = state.draw_size.unwrap_or([
            f32::from(region.sprite.width),
            f32::from(region.sprite.height),
        ]);
        let positions =
            [[0.0, 0.0], [width, 0.0], [0.0, height], [width, height]].map(|[px, py]| {
                let local_x = px - pivot_x;
                let local_y = py - pivot_y;
                [
                    x + m00.mul_add(local_x, m01 * local_y),
                    y + m10.mul_add(local_x, m11 * local_y),
                ]
            });
        super::native_masked_quad_visible(&positions, dimensions)
    }

    pub fn texture_name(&self) -> Option<&str> {
        self.texture.as_deref().map(|texture| texture.name.as_ref())
    }

    pub fn texture_scale(&self) -> f64 {
        self.texture.as_deref().map_or(1.0, |texture| texture.scale)
    }

    pub fn masked_texture_binding(&self) -> Option<&MaskedTextureBinding> {
        self.texture.as_deref().map(|texture| &texture.binding)
    }
}

#[derive(Debug, Clone)]
pub enum SpriteGeometrySubmission {
    /// Arbitrary positions and UVs emitted by `renderMaskedImageNative`.
    ExplicitQuad(Arc<RenderQuad>),
    /// Exact independently rounded atlas corners emitted by textured-line and
    /// rubber-band helpers. UVs still come from the bound atlas region.
    NativeAtlasQuad(Arc<[[f64; 2]; 4]>),
    /// GL_Image's width/height overload under an active custom model submits
    /// local xyz=(x,y,0), without the ordinary affine or viewport conversion.
    /// Retain these rare corners separately from explicit normalized quads.
    RawAtlasQuad(Arc<[[f64; 2]; 4]>),
}

/// Reference-counted counterpart of Purple's copy-on-write libstdc++ sprite
/// strings. The small wrapper keeps the existing string-facing render API
/// while making command clones retain a pointer instead of copying bytes.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SharedSpriteName(Arc<str>);

impl SharedSpriteName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_arc(&self) -> &Arc<str> {
        &self.0
    }
}

impl Deref for SharedSpriteName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl fmt::Display for SharedSpriteName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl From<Arc<str>> for SharedSpriteName {
    fn from(value: Arc<str>) -> Self {
        Self(value)
    }
}

impl From<String> for SharedSpriteName {
    fn from(value: String) -> Self {
        Self(value.into())
    }
}

impl From<&str> for SharedSpriteName {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

impl PartialEq<&str> for SharedSpriteName {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<SharedSpriteName> for &str {
    fn eq(&self, other: &SharedSpriteName) -> bool {
        *self == other.as_str()
    }
}

impl PartialEq<String> for SharedSpriteName {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<SharedSpriteName> for String {
    fn eq(&self, other: &SharedSpriteName) -> bool {
        self == other.as_str()
    }
}

#[cfg(test)]
mod deferred_payload_tests {
    use super::*;

    #[test]
    fn rare_render_payloads_stay_out_of_the_common_command_storage() {
        // The native GL state is copied for every scene submission, while
        // these arrays exist only on masked/line/rubber-band calls. Keep the
        // common state compact and retain a rare payload through one pointer.
        assert!(std::mem::size_of::<RenderState>() < 2 * std::mem::size_of::<RenderQuad>());
        assert_eq!(
            std::mem::size_of::<Option<SpriteGeometrySubmission>>(),
            2 * std::mem::size_of::<usize>()
        );
        assert_eq!(
            std::mem::size_of::<Option<Arc<SpriteShader>>>(),
            std::mem::size_of::<usize>()
        );
        assert_eq!(
            std::mem::size_of::<Option<Arc<DirtRenderCommand>>>(),
            std::mem::size_of::<usize>()
        );
        assert_eq!(
            std::mem::size_of::<Option<Arc<SpriteTextureSubmission>>>(),
            std::mem::size_of::<usize>()
        );
        assert_eq!(std::mem::size_of::<RenderSubmissionState>(), 124);
        assert_eq!(
            std::mem::size_of::<Option<Arc<super::super::TextProjection3D>>>(),
            std::mem::size_of::<usize>()
        );
        // Dirt holes belong to the retained DirtMechanics component.  The
        // original sprite submission has no per-command analytic-hole list.
        assert_eq!(std::mem::size_of::<RenderCommand>(), 224);
        assert!(
            std::mem::size_of::<RenderCommand>()
                < std::mem::size_of::<RenderState>()
                    + std::mem::size_of::<SpriteShader>()
                    + std::mem::size_of::<DirtRenderCommand>()
        );
    }
}

#[derive(Debug, Clone)]
pub struct DirtRenderCommand {
    pub background_texture: String,
    pub foreground_texture: String,
    /// Texture pointers resolved once by the native Dirt constructor. Live
    /// components borrow them; submitted commands keep immutable pixels.
    pub background_texture_binding: MaskedTextureBinding,
    pub foreground_texture_binding: MaskedTextureBinding,
    pub background_texture_borrow: Option<super::NativeTextureBorrow>,
    pub foreground_texture_borrow: Option<super::NativeTextureBorrow>,
    /// Triangle positions are in the scene object's local physics units.
    /// Purple multiplies them by 20 immediately before projection and also
    /// passes the unscaled pair through as repeating texture coordinates.
    pub background_triangles: Arc<Vec<Vec<RenderTriangle>>>,
    pub foreground_triangles: Arc<Vec<Vec<RenderTriangle>>>,
}

impl DirtRenderCommand {
    pub(crate) fn snapshot_textures(self: &Arc<Self>) -> crate::LuaResult<Arc<Self>> {
        if self.background_texture_borrow.is_none() && self.foreground_texture_borrow.is_none() {
            return Ok(Arc::clone(self));
        }
        let resolve = |binding: &MaskedTextureBinding,
                       borrow: &Option<super::NativeTextureBorrow>,
                       triangles: &[Vec<RenderTriangle>]|
         -> crate::LuaResult<MaskedTextureBinding> {
            // DrawablePolygon::render (0x100024A08) skips an empty mesh before
            // its raw BASEMAP pointer is passed to the shader.
            if let Some(borrow) = borrow
                && triangles.iter().any(|triangles| !triangles.is_empty())
            {
                let image = borrow.snapshot().map_err(|reason| {
                    crate::runtime_error(format!(
                        "Native Dirt texture '{}' uses {reason}",
                        binding.source().unwrap_or_default()
                    ))
                })?;
                Ok(MaskedTextureBinding::with_image(
                    image.source,
                    image.image,
                    Some(image.owner),
                ))
            } else {
                Ok(binding.clone())
            }
        };
        Ok(Arc::new(Self {
            background_texture_binding: resolve(
                &self.background_texture_binding,
                &self.background_texture_borrow,
                &self.background_triangles,
            )?,
            foreground_texture_binding: resolve(
                &self.foreground_texture_binding,
                &self.foreground_texture_borrow,
                &self.foreground_triangles,
            )?,
            background_texture_borrow: None,
            foreground_texture_borrow: None,
            ..self.as_ref().clone()
        }))
    }
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
