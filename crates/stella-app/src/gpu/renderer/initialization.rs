//! wgpu constructor facade over device, fixed-target, native-program and
//! window-presentation ownership stages.

use super::super::resources::upload_texture;
use super::super::*;

mod programs;
pub(in crate::gpu) mod target;
mod window;

const STELLA_WGPU_BACKENDS: wgpu::Backends = wgpu::Backends::PRIMARY;

impl GpuRenderer {
    pub(crate) fn for_window(window: Arc<Window>, resolution: GameResolution) -> Result<Self> {
        pollster::block_on(Self::new(Some(window), resolution))
    }

    pub(crate) fn headless(resolution: GameResolution) -> Result<Self> {
        pollster::block_on(Self::new(None, resolution))
    }

    async fn new(window: Option<Arc<Window>>, resolution: GameResolution) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            // Prefer Metal on macOS, DX12 on Windows and Vulkan on Linux.
            // The secondary GL backend is intentionally not a rehost path.
            backends: STELLA_WGPU_BACKENDS,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = window
            .map(|window| instance.create_surface(window))
            .transpose()
            .context("create wgpu surface")?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: surface.as_ref(),
                apply_limit_buckets: false,
            })
            .await
            .context("request wgpu adapter")?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Stella wgpu device"),
                ..Default::default()
            })
            .await
            .context("request wgpu device")?;

        let target::NativeTarget {
            game_texture,
            game_view,
            base_sampler,
            fill_sampler,
            sprite_storage_layout,
            sprite_texture_layout,
        } = target::create(&device, resolution);
        let (capture_pipeline, capture_layout) = super::capture::create_pipeline(&device);
        let capture_bind_group =
            super::capture::bind_framebuffer(&device, &capture_layout, &game_view);
        let programs::NativePrograms {
            plain,
            plain_alpha,
            sprite,
            sprite_alpha,
            sprite_alpha_masked,
        } = programs::create(&device, &sprite_storage_layout, &sprite_texture_layout);
        let super::streams::NativeStreams {
            draw_storage_buffer,
            draw_storage_bind_group,
            draw_storage_capacity,
            vertex_buffer,
            vertex_capacity,
        } = super::streams::create(&device, &sprite_storage_layout);

        let mut textures = HashMap::new();
        textures.insert(
            WHITE_TEXTURE.to_owned(),
            upload_texture(
                &device,
                &queue,
                WHITE_TEXTURE,
                &RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255])),
            ),
        );
        let window::WindowPresentation {
            surface_config,
            blit_pipeline,
            blit_bind_group,
            blit_layout,
            blit_sampler,
        } = window::create(surface.as_ref(), &adapter, &device, &game_view, resolution)?;

        Ok(Self {
            _instance: instance,
            surface,
            surface_config,
            device,
            queue,
            resolution,
            game_texture,
            game_view,
            capture_pipeline,
            capture_layout,
            capture_bind_group,
            sprite_storage_layout,
            sprite_texture_layout,
            draw_storage_buffer,
            draw_storage_bind_group,
            draw_storage_capacity,
            vertex_buffer,
            vertex_capacity,
            plain_program: plain,
            plain_alpha_program: plain_alpha,
            sprite_program: sprite,
            sprite_alpha_program: sprite_alpha,
            sprite_alpha_masked_program: sprite_alpha_masked,
            base_sampler,
            fill_sampler,
            textures,
            texture_bind_groups: HashMap::new(),
            retired_textures: HashSet::new(),
            blit_pipeline,
            blit_bind_group,
            blit_layout,
            blit_sampler,
            window_overlay: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_enables_only_wgpu_primary_backends() {
        assert_eq!(STELLA_WGPU_BACKENDS, wgpu::Backends::PRIMARY);
        assert!(!STELLA_WGPU_BACKENDS.intersects(wgpu::Backends::SECONDARY));
    }
}
