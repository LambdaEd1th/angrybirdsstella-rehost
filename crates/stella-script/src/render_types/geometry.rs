//! Native color-mesh and framebuffer-capture command payloads.

#[derive(Debug, Clone)]
pub struct RectRenderCommand {
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
}
