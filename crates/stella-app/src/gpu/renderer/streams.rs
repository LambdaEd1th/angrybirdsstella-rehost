//! Persistent, grow-only GPU streams for Purple's immediate-mode draw list.

use super::super::*;

const INITIAL_DRAW_STORAGE_CAPACITY: u64 = 2 * 1024 * 1024;
const INITIAL_VERTEX_CAPACITY: u64 = 1024 * 1024;

pub(super) struct NativeStreams {
    pub(super) draw_storage_buffer: wgpu::Buffer,
    pub(super) draw_storage_bind_group: wgpu::BindGroup,
    pub(super) draw_storage_capacity: u64,
    pub(super) vertex_buffer: wgpu::Buffer,
    pub(super) vertex_capacity: u64,
}

fn create_buffer(
    device: &wgpu::Device,
    label: &'static str,
    size: u64,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_storage_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Stella persistent per-draw storage bind group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: buffer.as_entire_binding(),
        }],
    })
}

pub(super) fn create(
    device: &wgpu::Device,
    storage_layout: &wgpu::BindGroupLayout,
) -> NativeStreams {
    let draw_storage_buffer = create_buffer(
        device,
        "Stella persistent per-draw storage",
        INITIAL_DRAW_STORAGE_CAPACITY,
        wgpu::BufferUsages::STORAGE,
    );
    let draw_storage_bind_group =
        create_storage_bind_group(device, storage_layout, &draw_storage_buffer);
    let vertex_buffer = create_buffer(
        device,
        "Stella persistent sprite vertex stream",
        INITIAL_VERTEX_CAPACITY,
        wgpu::BufferUsages::VERTEX,
    );
    NativeStreams {
        draw_storage_buffer,
        draw_storage_bind_group,
        draw_storage_capacity: INITIAL_DRAW_STORAGE_CAPACITY,
        vertex_buffer,
        vertex_capacity: INITIAL_VERTEX_CAPACITY,
    }
}

fn grown_capacity(current: u64, required: u64) -> Result<u64> {
    if required <= current {
        return Ok(current);
    }
    required
        .checked_next_power_of_two()
        .ok_or_else(|| anyhow!("GPU stream size overflow for {required} bytes"))
}

impl GpuRenderer {
    pub(in crate::gpu) fn ensure_stream_capacity(
        &mut self,
        storage_required: u64,
        vertex_required: u64,
    ) -> Result<()> {
        let storage_capacity = grown_capacity(self.draw_storage_capacity, storage_required.max(1))?;
        let storage_limit = self.device.limits().max_storage_buffer_binding_size;
        if storage_capacity > storage_limit {
            return Err(anyhow!(
                "Stella draw storage needs {storage_required} bytes, device limit is {storage_limit}"
            ));
        }
        if storage_capacity != self.draw_storage_capacity {
            self.draw_storage_buffer = create_buffer(
                &self.device,
                "Stella grown persistent per-draw storage",
                storage_capacity,
                wgpu::BufferUsages::STORAGE,
            );
            self.draw_storage_bind_group = create_storage_bind_group(
                &self.device,
                &self.sprite_storage_layout,
                &self.draw_storage_buffer,
            );
            self.draw_storage_capacity = storage_capacity;
        }

        let vertex_capacity = grown_capacity(self.vertex_capacity, vertex_required.max(1))?;
        if vertex_capacity != self.vertex_capacity {
            self.vertex_buffer = create_buffer(
                &self.device,
                "Stella grown persistent sprite vertex stream",
                vertex_capacity,
                wgpu::BufferUsages::VERTEX,
            );
            self.vertex_capacity = vertex_capacity;
        }
        Ok(())
    }
}
