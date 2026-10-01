use std::io::Cursor;

use super::{
    AssetError, DecodedNativeImage, ImageSurfaceLayout, PNG_SIGNATURE, SurfaceFormat,
    valid_png_bit_depth,
};

#[cfg(test)]
mod tests;

fn reader(bytes: &[u8]) -> Result<::png::Reader<Cursor<&[u8]>>, AssetError> {
    if bytes.get(..8) != Some(PNG_SIGNATURE) {
        return Err(AssetError::InvalidPng("signature mismatch"));
    }
    if bytes.len() < 29 {
        return Err(AssetError::InvalidPng("truncated IHDR"));
    }
    if bytes[8..12] != 13u32.to_be_bytes() || &bytes[12..16] != b"IHDR" {
        return Err(AssetError::InvalidPng("IHDR is not the first chunk"));
    }
    let depth = bytes[24];
    let color = bytes[25];
    if depth > 8 {
        return Err(AssetError::UnsupportedPngBitDepth(depth));
    }
    if !valid_png_bit_depth(color, depth) {
        return Err(AssetError::InvalidPng("invalid bit depth for color type"));
    }
    let mut decoder = ::png::Decoder::new(Cursor::new(bytes));
    decoder.set_limits(::png::Limits {
        bytes: 512 * 1024 * 1024,
    });
    // 1004D62B8 expands only depths below8, before selecting the surface.
    // 1004B0F74 sets PNG_EXPAND|PNG_EXPAND_tRNS, not PNG_PACK.
    if depth < 8 {
        decoder.set_transformations(::png::Transformations::EXPAND);
    }
    let reader = decoder
        .read_info()
        .map_err(|_| AssetError::InvalidPng("header decoding failed"))?;
    if depth == 8 && matches!(color, 0 | 2) && reader.info().trns.is_some() {
        // Without expansion, 1004AFF8C returns a null alpha table and count1.
        // The native palette-copy loop dereferences it. Keep a checked failure.
        return Err(AssetError::InvalidPng(
            "native color-key transparency has no alpha table",
        ));
    }
    Ok(reader)
}

fn layout(color: ::png::ColorType) -> ImageSurfaceLayout {
    use ::png::ColorType;
    match color {
        ColorType::Grayscale => ImageSurfaceLayout::direct(SurfaceFormat::L8),
        ColorType::Rgb => ImageSurfaceLayout::direct(SurfaceFormat::B8G8R8),
        ColorType::Indexed => ImageSurfaceLayout {
            pixels: SurfaceFormat::P8,
            palette: Some(SurfaceFormat::A8R8G8B8),
        },
        ColorType::GrayscaleAlpha => ImageSurfaceLayout::direct(SurfaceFormat::A8L8),
        ColorType::Rgba => ImageSurfaceLayout::direct(SurfaceFormat::A8B8G8R8),
    }
}

pub(super) fn probe(bytes: &[u8]) -> Result<(u32, u32, ImageSurfaceLayout), AssetError> {
    // 1004D62B8 calls png_read_image during reader construction, so even an
    // extent/layout query must reject damaged pixel data before succeeding.
    let decoded = decode(bytes, false)?;
    Ok((decoded.width, decoded.height, decoded.layout))
}

pub(super) fn decode(bytes: &[u8], texture: bool) -> Result<DecodedNativeImage, AssetError> {
    let mut reader = reader(bytes)?;
    let (color, depth) = reader.output_color_type();
    if depth != ::png::BitDepth::Eight {
        return Err(AssetError::InvalidPng("unexpected native output depth"));
    }
    let layout = layout(color);
    if texture {
        layout.pixels.gl_texture_format(true)?;
    }
    let width = reader.info().width;
    let height = reader.info().height;
    image::Limits::default()
        .reserve_buffer(width, height, image::ColorType::Rgba8)
        .map_err(|_| AssetError::InvalidPng("decoded image exceeds allocation limit"))?;
    let mut palette = [[255; 4]; 256];
    if color == ::png::ColorType::Indexed {
        let entries = reader
            .info()
            .palette
            .as_deref()
            .ok_or(AssetError::InvalidPng("palette missing"))?;
        if entries.len() > 768 {
            return Err(AssetError::InvalidPng("palette exceeds native table"));
        }
        for (entry, output) in entries.as_chunks::<3>().0.iter().zip(&mut palette) {
            output[..3].copy_from_slice(entry);
        }
        if let Some(alpha) = reader.info().trns.as_deref() {
            if alpha.len() > 256 {
                return Err(AssetError::InvalidPng("alpha exceeds native palette"));
            }
            for (&alpha, output) in alpha.iter().zip(&mut palette) {
                output[3] = alpha;
            }
        }
    }
    let length = reader
        .output_buffer_size()
        .ok_or(AssetError::InvalidPng("decoded image size overflow"))?;
    let mut source = Vec::new();
    source
        .try_reserve_exact(length)
        .map_err(|_| AssetError::InvalidPng("decoded image allocation failed"))?;
    source.resize(length, 0);
    let output = reader
        .next_frame(&mut source)
        .map_err(|_| AssetError::InvalidPng("pixel decoding failed"))?;
    if output.width != width || output.height != height || output.color_type != color {
        return Err(AssetError::InvalidPng(
            "unexpected native image extent or layout",
        ));
    }
    source.truncate(output.buffer_size());
    let length = usize::try_from(u64::from(width) * u64::from(height) * 4)
        .map_err(|_| AssetError::InvalidPng("decoded image size overflow"))?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(length)
        .map_err(|_| AssetError::InvalidPng("decoded image allocation failed"))?;
    for pixel in source.chunks_exact(color.samples()) {
        rgba.extend_from_slice(&match color {
            ::png::ColorType::Grayscale => [pixel[0], pixel[0], pixel[0], 255],
            ::png::ColorType::Rgb => [pixel[0], pixel[1], pixel[2], 255],
            ::png::ColorType::Indexed => palette[usize::from(pixel[0])],
            ::png::ColorType::GrayscaleAlpha => [pixel[0], pixel[0], pixel[0], pixel[1]],
            ::png::ColorType::Rgba => [pixel[0], pixel[1], pixel[2], pixel[3]],
        });
    }
    Ok(DecodedNativeImage {
        width,
        height,
        rgba,
        layout,
    })
}
