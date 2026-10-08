//! Compressed file Images must survive Lua construction and reach real GPU
//! sprite and glyph draws with the same pixels as independent SDK fixtures.

use super::*;

const WIDTH: u16 = 16;
const HEIGHT: u16 = 16;

fn compressed_file(bits: u32, alpha: bool, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for word in [
        52,
        u32::from(HEIGHT),
        u32::from(WIDTH),
        0,
        if bits == 2 { 0x18 } else { 0x19 } | if alpha { 1 << 15 } else { 0 },
        payload.len() as u32,
        bits,
        0,
        0,
        0,
        0,
        u32::from_le_bytes(*b"PVR!"),
        1,
    ] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes.extend_from_slice(payload);
    bytes
}

fn resources(files: &Files, name: &str, image: &str) {
    let mut sheet = 1_u16.to_be_bytes().to_vec();
    string(&mut sheet, image);
    sheet.extend_from_slice(&1_u16.to_be_bytes());
    string(&mut sheet, name);
    for value in [0_u16, 0, WIDTH, HEIGHT, 0, 0] {
        sheet.extend_from_slice(&value.to_be_bytes());
    }
    std::fs::write(
        files.data().join(format!("{name}.dat")),
        envelope(b"SPRT", sheet),
    )
    .unwrap();

    let mut font = 1_u16.to_be_bytes().to_vec();
    string(&mut font, image);
    font.extend_from_slice(&0_i16.to_be_bytes());
    font.extend_from_slice(&(HEIGHT as i16).to_be_bytes());
    font.extend_from_slice(&1_u16.to_be_bytes());
    for value in [u16::from(b'A'), 0, 0, WIDTH, HEIGHT, 0] {
        font.extend_from_slice(&value.to_be_bytes());
    }
    std::fs::write(
        files.data().join(format!("F{name}.dat")),
        envelope(b"FONT", font),
    )
    .unwrap();
}

#[test]
fn pvrtc_sprites_and_bitmap_glyphs_match_sdk_reference_on_gpu() {
    for (bits, payload, expected) in [
        (
            2,
            include_bytes!("../../../../../stella-assets/src/pvr/pvrtc/fixtures/2bpp-16x16.bin")
                .as_slice(),
            include_bytes!("../../../../../stella-assets/src/pvr/pvrtc/fixtures/2bpp-16x16.rgba")
                .as_slice(),
        ),
        (
            4,
            include_bytes!("../../../../../stella-assets/src/pvr/pvrtc/fixtures/4bpp-16x16.bin")
                .as_slice(),
            include_bytes!("../../../../../stella-assets/src/pvr/pvrtc/fixtures/4bpp-16x16.rgba")
                .as_slice(),
        ),
    ] {
        for alpha in [true, false] {
            let files = Files::new();
            std::fs::write(
                files.data().join("compressed.pvr"),
                compressed_file(bits, alpha, payload),
            )
            .unwrap();
            let mut expected = expected.to_vec();
            if !alpha {
                for pixel in expected.as_chunks_mut::<4>().0 {
                    pixel[3] = 255;
                }
            }
            RgbaImage::from_raw(u32::from(WIDTH), u32::from(HEIGHT), expected.clone())
                .unwrap()
                .save(files.data().join("reference.png"))
                .unwrap();
            resources(&files, "C", "compressed.pvr");
            resources(&files, "R", "reference.png");
            let size = GameResolution {
                width: u32::from(WIDTH) * 2,
                height: u32::from(HEIGHT),
            };
            let runtime =
                StellaLua::new_with_resolution(files.data(), size.width, size.height).unwrap();
            runtime.execute_source("res.createSpriteSheet('C.dat'); res.createSpriteSheet('R.dat'); res.createBitmapFont('FC.dat'); res.createBitmapFont('FR.dat')").unwrap();
            // File Image construction, rather than a later asset fallback,
            // must already retain both the sprite and font image pixels.
            std::fs::remove_file(files.data().join("compressed.pvr")).unwrap();
            std::fs::remove_file(files.data().join("reference.png")).unwrap();
            let mut assets = assets(&files);
            assets
                .apply_sprite_catalog_snapshot(runtime.sprite_catalog_snapshot_since(0).unwrap())
                .unwrap();
            let mut renderer = GpuRenderer::headless(size).unwrap();
            eprintln!(
                "PVRTC {bits}bpp alpha={alpha}: {:?}",
                renderer.adapter.get_info()
            );
            let mut rendered = Vec::new();
            for name in ["C", "R"] {
                runtime.execute_source(&format!("res.drawSprite('{name}',0,0); res.useFont('F{name}'); res.drawString('','A',16,0)")).unwrap();
                let frame = assets
                    .prepare_gpu_frame_at_resolution(
                        size,
                        &runtime.take_render_commands(),
                        &runtime.take_text_commands(),
                        &runtime.take_rect_commands(),
                        &runtime.take_capture_commands(),
                    )
                    .unwrap();
                rendered.push(
                    renderer
                        .render_to_rgba(&assets, &frame, [0, 255, 0])
                        .unwrap(),
                );
            }
            assert_eq!(rendered[0], rendered[1], "{bits}bpp alpha={alpha}");
            assert!(
                rendered[0]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|pixel| pixel != &[0, 255, 0, 255])
            );
            if !alpha {
                let expected: Vec<u8> = expected
                    .chunks_exact(usize::from(WIDTH) * 4)
                    .flat_map(|row| row.iter().chain(row.iter()).copied())
                    .collect();
                assert_eq!(rendered[0], expected, "opaque sprite and glyph pixels");
            }
        }
    }
}
