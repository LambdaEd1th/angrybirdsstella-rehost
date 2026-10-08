//! GPU vertex/uniform ABI and ordered frame operations shared by desktop and Web.

use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use stella_script::ScreenshotShareRequest;

use crate::gpu::program::NativeProgram;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(in crate::gpu) struct GpuVertex {
    pub(in crate::gpu) position: [f32; 2],
    pub(in crate::gpu) uv: [f32; 2],
    pub(in crate::gpu) source: [f32; 2],
    pub(in crate::gpu) clip_position: [f32; 4],
    pub(in crate::gpu) draw_index: u32,
    pub(in crate::gpu) padding: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(in crate::gpu) struct DrawUniform {
    pub(in crate::gpu) header: [f32; 4],
    pub(in crate::gpu) diffuse: [f32; 4],
    pub(in crate::gpu) params: [f32; 4],
    pub(in crate::gpu) fill: [f32; 4],
}

impl Default for DrawUniform {
    fn default() -> Self {
        Self::zeroed()
    }
}

pub(in crate::gpu) struct PreparedDraw {
    pub(in crate::gpu) vertices: Range<u32>,
    pub(in crate::gpu) texture_pair: usize,
    pub(in crate::gpu) program: NativeProgram,
    pub(in crate::gpu) scissor: Option<[u32; 4]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::gpu) enum PreparedOperation {
    Draw(usize),
    Capture(String),
    ScreenshotShare(ScreenshotShareRequest),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    #[test]
    fn native_sprite_stream_keeps_gpu_and_browser_packet_layout() {
        // renderer vertex attributes and web/renderer.js consume this ABI.
        assert_eq!(size_of::<GpuVertex>(), 48);
        assert_eq!(align_of::<GpuVertex>(), 4);
        assert_eq!(offset_of!(GpuVertex, position), 0);
        assert_eq!(offset_of!(GpuVertex, uv), 8);
        assert_eq!(offset_of!(GpuVertex, source), 16);
        assert_eq!(offset_of!(GpuVertex, clip_position), 24);
        assert_eq!(offset_of!(GpuVertex, draw_index), 40);
        assert_eq!(offset_of!(GpuVertex, padding), 44);
        // Purple uploads four float4 rows; Web reads 16 floats per draw.
        // DirtMechanics triangulates holes outside this sprite stream.
        assert_eq!(size_of::<DrawUniform>(), 64);
        assert_eq!(align_of::<DrawUniform>(), 4);
        assert_eq!(offset_of!(DrawUniform, header), 0);
        assert_eq!(offset_of!(DrawUniform, diffuse), 16);
        assert_eq!(offset_of!(DrawUniform, params), 32);
        assert_eq!(offset_of!(DrawUniform, fill), 48);
        assert_eq!(bytemuck::bytes_of(&DrawUniform::default()), &[0; 64]);
    }
}
