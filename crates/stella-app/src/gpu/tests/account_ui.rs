//! Native account window presentation uses the desktop GPU surface formats.

use super::*;
use std::{fs, path::PathBuf};

#[test]
fn native_account_window_color_preserves_raster_on_srgb_surfaces() {
    let Some(layer) = crate::account_ui::native_account_window_color_fixture() else {
        return;
    };
    let background = RgbaImage::from_fn(1024, 768, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8, 255])
    });
    let mut renderer = crate::gpu::GpuRenderer::headless(crate::GameResolution {
        width: 1024,
        height: 768,
    })
    .unwrap();
    let mut expected: Option<Vec<u8>> = None;
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ] {
        renderer.set_window_overlay(None).unwrap();
        renderer
            .configure_window_target_for_test(&[format], 1024, 768)
            .unwrap();
        renderer.set_window_overlay(Some(&layer)).unwrap();
        let presented = renderer.composite_window_overlay_for_test(&background);
        if let Some(directory) = std::env::var_os("STELLA_WINDOW_COLOR_AUDIT_DIR") {
            let directory = PathBuf::from(directory);
            fs::create_dir_all(&directory).unwrap();
            presented
                .save(directory.join(format!("native-account-{format:?}.png")))
                .unwrap();
        }
        if let Some(expected) = &expected {
            let differing_channel = presented
                .as_raw()
                .iter()
                .zip(expected)
                .position(|(actual, expected)| actual != expected);
            assert!(
                differing_channel.is_none(),
                "{format:?} changed original-artwork account byte {differing_channel:?}"
            );
        } else {
            expected = Some(presented.into_raw());
        }
    }
}
