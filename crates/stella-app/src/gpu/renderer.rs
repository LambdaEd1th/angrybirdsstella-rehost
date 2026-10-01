//! wgpu device, surface, render-pass, capture, and texture synchronization stage.

mod capture;
pub(in crate::gpu) mod device_state;
pub(in crate::gpu) mod initialization;
mod pass;
mod presentation;
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
