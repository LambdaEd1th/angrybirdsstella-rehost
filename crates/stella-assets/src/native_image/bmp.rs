use super::{AssetError, DecodedNativeImage, ImageSurfaceLayout, SurfaceFormat};

#[cfg(test)]
mod tests;

pub(super) struct Header {
    pub width: u32,
    pub height: u32,
    pub layout: ImageSurfaceLayout,
    depth: u16,
    palette_offset: usize,
    palette_entries: usize,
    palette_entry_size: usize,
    pixel_offset: usize,
    row_stride: usize,
}

impl Header {
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, AssetError> {
        if bytes.len() < 26 || !bytes.starts_with(b"BM") {
            return Err(AssetError::InvalidBmp("truncated or missing header"));
        }
        let word =
            |offset: usize| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
        let dword =
            |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let dib = dword(14) as usize;
        if !matches!(dib, 12 | 40 | 64) {
            return Err(AssetError::InvalidBmp("unsupported DIB header"));
        }
        if bytes.len() < 14 + dib {
            return Err(AssetError::InvalidBmp("truncated header"));
        }
        // 1004D4128 calls the 16-bit accessor even for INFO/OS2 dimensions.
        let width = u32::from(word(18));
        let height = u32::from(word(if dib == 12 { 20 } else { 22 }));
        let depth = word(if dib == 12 { 24 } else { 28 });
        if width == 0 || height == 0 {
            return Err(AssetError::InvalidBmp("zero image dimension"));
        }
        if dib == 12 {
            // The CORE branch discards the planes accessor and only admits8/24.
            if !matches!(depth, 8 | 24) {
                return Err(AssetError::InvalidBmp("unsupported CORE pixel depth"));
            }
        } else {
            if word(26) != 1 {
                return Err(AssetError::InvalidBmp("invalid planes"));
            }
            if dword(30) != 0 {
                return Err(AssetError::InvalidBmp("unsupported compression"));
            }
            if !matches!(depth, 4 | 8 | 16 | 24 | 32) {
                return Err(AssetError::InvalidBmp("unsupported pixel depth"));
            }
        }
        let layout = match depth {
            4 | 8 => ImageSurfaceLayout {
                pixels: if depth == 4 {
                    SurfaceFormat::P4
                } else {
                    SurfaceFormat::P8
                },
                palette: Some(SurfaceFormat::X8B8G8R8),
            },
            16 => ImageSurfaceLayout::direct(SurfaceFormat::R5G5B5),
            24 => ImageSurfaceLayout::direct(SurfaceFormat::R8G8B8),
            32 => ImageSurfaceLayout::direct(SurfaceFormat::X8R8G8B8),
            _ => unreachable!(),
        };
        let palette_entries = if depth <= 8 {
            let used = if dib == 12 { 0 } else { dword(46) as i32 };
            let entries = if used >= 1 {
                used as usize
            } else {
                1usize << depth
            };
            if entries > 256 {
                return Err(AssetError::InvalidBmp(
                    "palette exceeds native 256-entry table",
                ));
            }
            entries
        } else {
            0
        };
        let palette_entry_size = if dib == 12 { 3 } else { 4 };
        let palette_offset = 14 + dib;
        let pixel_offset = dword(10) as usize;
        if pixel_offset < palette_offset + palette_entries * palette_entry_size
            || pixel_offset > bytes.len()
        {
            return Err(AssetError::InvalidBmp("invalid pixel offset"));
        }
        // Native floors width*depth to bytes before aligning, including P4.
        let row_stride = (((width as usize * usize::from(depth)) >> 3) + 3) & !3;
        Ok(Self {
            width,
            height,
            layout,
            depth,
            palette_offset,
            palette_entries,
            palette_entry_size,
            pixel_offset,
            row_stride,
        })
    }
}

pub(super) fn decode(bytes: &[u8], header: Header) -> Result<DecodedNativeImage, AssetError> {
    let Header {
        width,
        height,
        layout,
        depth,
        palette_offset,
        palette_entries,
        palette_entry_size,
        pixel_offset,
        row_stride,
    } = header;
    let needed = (width as usize * usize::from(depth)).div_ceil(8);
    if needed > row_stride * 2 {
        // A one-pixel P4 row has zero native storage. Avoid undefined reads.
        return Err(AssetError::InvalidBmp(
            "native row buffer cannot hold visible pixels",
        ));
    }
    let mut palette = [[0, 0, 0, 255]; 256];
    for (index, output) in palette.iter_mut().take(palette_entries).enumerate() {
        let offset = palette_offset + index * palette_entry_size;
        let color = &bytes[offset..offset + palette_entry_size];
        *output = [color[2], color[1], color[0], 255];
    }
    image::Limits::default()
        .reserve_buffer(width, height, image::ColorType::Rgba8)
        .map_err(|_| AssetError::InvalidBmp("decoded image exceeds allocation limit"))?;
    let length = usize::try_from(u64::from(width) * u64::from(height) * 4)
        .map_err(|_| AssetError::InvalidBmp("decoded image size overflow"))?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(length)
        .map_err(|_| AssetError::InvalidBmp("decoded image allocation failed"))?;
    rgba.resize(length, 0);
    let mut input = &bytes[pixel_offset..];
    let output_stride = width as usize * 4;
    for source_row in 0..height as usize {
        let (source, rest) = input
            .split_at_checked(row_stride)
            .ok_or(AssetError::InvalidBmp("truncated pixels"))?;
        input = rest;
        // 1004D377C always reverses BMP rows. The second half of its doubled
        // row buffer is zero-filled, not populated from another input row.
        let row = height as usize - 1 - source_row;
        let output = &mut rgba[row * output_stride..(row + 1) * output_stride];
        for (x, output) in output.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            *output = match depth {
                4 => {
                    let packed = source.get(x / 2).copied().unwrap_or(0);
                    palette[usize::from(if x.is_multiple_of(2) {
                        packed >> 4
                    } else {
                        packed & 15
                    })]
                }
                8 => palette[usize::from(source[x])],
                16 => super::rgb555_color(&source[x * 2..x * 2 + 2]),
                24 | 32 => {
                    let offset = x * usize::from(depth / 8);
                    [source[offset + 2], source[offset + 1], source[offset], 255]
                }
                _ => unreachable!(),
            };
        }
    }
    Ok(DecodedNativeImage {
        width,
        height,
        rgba,
        layout,
    })
}
