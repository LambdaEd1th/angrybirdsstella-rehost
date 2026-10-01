use super::{AssetError, DecodedNativeImage, ImageSurfaceLayout, SurfaceFormat};

#[cfg(test)]
mod tests;

/// The row copier `1004D377C` uses only vertical origin and calls the RLE
/// reader `1004DAEDC` separately for every scanline, discarding packet overflow.
pub(super) fn decode(
    bytes: &[u8],
    width: u32,
    height: u32,
    layout: ImageSurfaceLayout,
) -> Result<DecodedNativeImage, AssetError> {
    let mut input = bytes
        .get(18 + usize::from(bytes[0])..)
        .ok_or(AssetError::InvalidTga("truncated image ID"))?;
    let format = match layout.pixels {
        SurfaceFormat::P8 => {
            let palette_length = usize::from(u16::from_le_bytes([bytes[5], bytes[6]]));
            let entry_size = usize::from(bytes[7] / 8);
            PixelFormat::Indexed {
                palette: take(&mut input, palette_length * entry_size)?,
                entry_size,
            }
        }
        SurfaceFormat::R5G5B5 => PixelFormat::Rgb555,
        SurfaceFormat::R8G8B8 => PixelFormat::Bgr,
        SurfaceFormat::A8R8G8B8 => PixelFormat::Bgra,
        _ => return Err(AssetError::InvalidTga("unsupported source format")),
    };
    let pixel_size = usize::from(bytes[16] / 8);

    // Apply the same default decoded-image budget as the other image readers.
    image::Limits::default()
        .reserve_buffer(width, height, image::ColorType::Rgba8)
        .map_err(|_| AssetError::InvalidTga("decoded image exceeds allocation limit"))?;
    let length = usize::try_from(u64::from(width) * u64::from(height) * 4)
        .map_err(|_| AssetError::InvalidTga("decoded image size overflow"))?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(length)
        .map_err(|_| AssetError::InvalidTga("decoded image allocation failed"))?;
    rgba.resize(length, 0);
    let row_bytes = width as usize * 4;
    for source_row in 0..height as usize {
        let row = if bytes[17] & 0x20 == 0 {
            height as usize - 1 - source_row
        } else {
            source_row
        };
        // The native reader only uses descriptor bit 5; bit 4 does not mirror.
        let output = &mut rgba[row * row_bytes..(row + 1) * row_bytes];
        if bytes[2] <= 8 {
            let source = take(&mut input, width as usize * pixel_size)?;
            for (pixel, source) in output
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(source.chunks_exact(pixel_size))
            {
                *pixel = format.color(source)?;
            }
        } else {
            let mut x = 0;
            while x < width as usize {
                let header = take(&mut input, 1)?[0];
                let count = usize::from(header & 0x7f) + 1;
                let visible = count.min(width as usize - x);
                let output = &mut output[x * 4..(x + visible) * 4];
                if header & 0x80 != 0 {
                    let color = format.color(take(&mut input, pixel_size)?)?;
                    for pixel in output.as_chunks_mut::<4>().0 {
                        *pixel = color;
                    }
                } else {
                    let source = take(&mut input, count * pixel_size)?;
                    for (pixel, source) in output
                        .as_chunks_mut::<4>()
                        .0
                        .iter_mut()
                        .zip(source.chunks_exact(pixel_size))
                    {
                        *pixel = format.color(source)?;
                    }
                }
                x += visible;
            }
        }
    }
    Ok(DecodedNativeImage {
        width,
        height,
        rgba,
        layout,
    })
}

fn take<'a>(input: &mut &'a [u8], count: usize) -> Result<&'a [u8], AssetError> {
    let (head, tail) = input
        .split_at_checked(count)
        .ok_or(AssetError::InvalidTga("truncated palette or pixels"))?;
    *input = tail;
    Ok(head)
}

enum PixelFormat<'a> {
    Indexed {
        palette: &'a [u8],
        entry_size: usize,
    },
    Rgb555,
    Bgr,
    Bgra,
}

impl PixelFormat<'_> {
    fn color(&self, bytes: &[u8]) -> Result<[u8; 4], AssetError> {
        Ok(match *self {
            Self::Indexed {
                palette,
                entry_size,
            } => {
                let offset = usize::from(bytes[0]) * entry_size;
                let color = palette
                    .get(offset..offset + entry_size)
                    .ok_or(AssetError::InvalidTga("pixel index outside color map"))?;
                // 1004D3FC8 creates an opaque X8B8G8R8 palette at either depth.
                [color[2], color[1], color[0], 255]
            }
            Self::Rgb555 => super::rgb555_color(bytes),
            Self::Bgr => [bytes[2], bytes[1], bytes[0], 255],
            Self::Bgra => [bytes[2], bytes[1], bytes[0], bytes[3]],
        })
    }
}
