//! wgpu device, surface, render-pass, capture, and texture synchronization stage.

mod capture;
pub(in crate::gpu) mod device_state;
pub(in crate::gpu) mod initialization;
mod pass;
mod presentation;
mod readback;
mod streams;
pub(in crate::gpu) mod surface_acquisition;
mod textures;
pub(in crate::gpu) mod window_overlay;

fn window_target_format(format: wgpu::TextureFormat) -> wgpu::TextureFormat {
    // Purple presents an ordinary RGBA8/RGB565 EAGL drawable. Both the game
    // target and the premultiplied host UI already contain encoded color
    // channels. An sRGB attachment would encode them again and blend the UI
    // in a different space. Use the unorm alias even on an sRGB-only surface.
    format.remove_srgb_suffix()
}

fn window_target_view(texture: &wgpu::Texture) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(window_target_format(texture.format())),
        ..Default::default()
    })
}

fn configure_window_surface(
    surface: &wgpu::Surface<'_>,
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
) -> anyhow::Result<()> {
    surface.configure(device, config);

    #[cfg(target_os = "macos")]
    if config.color_space == wgpu::SurfaceColorSpace::Srgb {
        use anyhow::Context;
        use objc2_core_graphics::{CGColorSpace, kCGColorSpaceSRGB};

        // wgpu-hal 30.0.1 sets the sRGB Metal layer's colorspace to nil,
        // disabling Core Animation color matching on wide-gamut displays.
        // Apply the declaration from upstream wgpu PR #10286 after every
        // configuration, including resize and lost-surface recovery.
        // SAFETY: The HAL guard keeps the live surface borrowed. The layer
        // mutex protects its retained handle; only presentation metadata is
        // changed. No surface, drawable or GPU resource is destroyed.
        if let Some(metal) = unsafe { surface.as_hal::<wgpu::hal::api::Metal>() } {
            // SAFETY: CoreGraphics exports an immutable, process-lifetime name.
            let name = unsafe { kCGColorSpaceSRGB };
            let colorspace = CGColorSpace::with_name(Some(name))
                .context("create macOS sRGB window color space")?;
            metal.render_layer().lock().setColorspace(Some(&colorspace));
        }
    }
    Ok(())
}
