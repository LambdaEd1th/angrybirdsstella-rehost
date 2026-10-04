//! Native color-mesh and framebuffer-capture command payloads.

/// TexturizedSprite rejects clip-space quads before dereferencing Images.
/// Preserve its inclusive lower and strict upper edges and float32 FMADDs.
pub fn native_masked_quad_visible(positions: &[[f32; 2]; 4], dimensions: [u32; 2]) -> bool {
    let x_scale = 2.0_f32 / dimensions[0] as f32;
    let y_scale = -2.0_f32 / dimensions[1] as f32;
    let clip = positions.map(|[x, y]| [x.mul_add(x_scale, -1.0), y.mul_add(y_scale, 1.0)]);
    let min_x = clip.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let min_y = clip.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let max_x = clip.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
    let max_y = clip.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
    max_x >= -1.0 && max_y >= -1.0 && min_x < 1.0 && min_y < 1.0
}

#[derive(Debug, Clone)]
pub struct RectRenderCommand {
    pub projection_3d: Option<super::TextProjection3D>,
    pub order: u64,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    /// Exact native GL program selected before submission. Purple keeps the
    /// plain and plain-alpha programs as independent cached objects even
    /// though both consume the same vertex-color stream.
    pub color_program: ColorProgram,
    /// When present, this is Purple's native color-mesh vertex stream.
    /// Native `drawRect` also uses this path whenever the live GL state is not
    /// the identity transform.
    pub vertices: Option<Vec<[f64; 2]>>,
    pub mesh_topology: ColorMeshTopology,
    pub clip_rect: Option<[i32; 4]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorProgram {
    Plain,
    PlainAlpha,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMeshTopology {
    TriangleFan,
    TriangleStrip,
    TriangleList,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureRenderCommand {
    pub order: u64,
    pub name: String,
    /// Stable native Image identity. Capturing an existing sheet updates its
    /// image without replacing its atlas geometry or global name priority.
    pub texture_source: String,
    pub decoded_image: Option<std::sync::Arc<stella_assets::native_image::DecodedNativeImage>>,
    pub image_owner: Option<std::sync::Arc<super::NativeImageOwner>>,
    /// Existing sheet with a released/null Image: native capture creates and
    /// immediately drops a temporary image, without restoring the sheet.
    pub temporary: bool,
}
