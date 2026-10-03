//! wgpu constructor facade over device, fixed-target, native-program and
//! window-presentation ownership stages.

use super::super::resources::upload_texture;
use super::super::*;

mod programs;
pub(in crate::gpu) mod target;
mod window;

const STELLA_WGPU_BACKENDS: wgpu::Backends = wgpu::Backends::PRIMARY;

impl GpuRenderer {
    #[cfg(test)]
    pub(crate) fn configure_window_target_for_test(
        &mut self,
        formats: &[wgpu::TextureFormat],
        width: u32,
        height: u32,
    ) -> Result<()> {
        // Exercise the production format selection and pipeline creation on
        // a copyable substitute surface, without requiring a display server.
        let mut config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: GAME_FORMAT,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: Vec::new(),
        };
        window::configure_color_format(&mut config, formats)?;
        let presentation = window::create_blit_presentation(&self.device, &self.game_view, config);
        self.replace_window_presentation(presentation);
        Ok(())
    }

    fn replace_window_presentation(&mut self, presentation: window::WindowPresentation) {
        if let Some(config) = &presentation.surface_config {
            self.reconfigure_window_overlay(super::window_target_format(config.format));
        }
        self.surface_config = presentation.surface_config;
        self.blit_pipeline = presentation.blit_pipeline;
        self.blit_bind_group = presentation.blit_bind_group;
        self.blit_layout = presentation.blit_layout;
        self.blit_sampler = presentation.blit_sampler;
    }

    pub(super) fn recover_window_surface(
        &mut self,
        recovery: super::surface_acquisition::SurfaceRecovery,
        width: u32,
        height: u32,
    ) -> Result<()> {
        use super::surface_acquisition::SurfaceRecovery;

        self.check_device()?;
        match recovery {
            SurfaceRecovery::Reconfigure => {
                let surface = self
                    .surface
                    .as_ref()
                    .ok_or_else(|| anyhow!("window renderer has no wgpu surface"))?;
                let config = self
                    .surface_config
                    .as_mut()
                    .ok_or_else(|| anyhow!("window renderer has no surface configuration"))?;
                config.width = width;
                config.height = height;
                surface.configure(&self.device, config);
            }
            SurfaceRecovery::Recreate => {
                let window = self
                    .surface_window
                    .as_ref()
                    .ok_or_else(|| anyhow!("window renderer has no window for surface recovery"))?;
                // Release the lost surface before attaching its replacement to
                // the same native window. No acquired frame survives here.
                self.surface = None;
                let surface = self
                    .instance
                    .create_surface(Arc::clone(window))
                    .context("recreate lost wgpu surface")?;
                let presentation = window::create(
                    Some(&surface),
                    &self.adapter,
                    &self.device,
                    &self.game_view,
                    width,
                    height,
                )?;
                self.replace_window_presentation(presentation);
                self.surface = Some(surface);
                // Game/capture allocations and uploaded UI pixels belong to
                // the healthy device, independently of the window surface.
            }
        }
        self.device_state.check()
    }

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
            .as_ref()
            .map(|window| instance.create_surface(Arc::clone(window)))
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
        let device_state = super::device_state::DeviceState::register(&device);

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
        } = window::create(
            surface.as_ref(),
            &adapter,
            &device,
            &game_view,
            resolution.width,
            resolution.height,
        )?;

        let renderer = Self {
            screenshot_shares: Vec::new(),
            instance,
            adapter,
            surface_window: window,
            surface,
            surface_config,
            surface_recovery: None,
            device,
            device_state,
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
        };
        renderer.check_device()?;
        Ok(renderer)
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
