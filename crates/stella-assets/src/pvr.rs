//! PVR v2 texture parsing and decoding for formats used by Purple 1.1.6.

use std::path::Path;

use crate::AssetError;

const PVR_V2_HEADER_SIZE: usize = 52;
const PVR_TAG: u32 = u32::from_le_bytes(*b"PVR!");

pub const OGL_RGBA_4444: u8 = 0x10;
pub const OGL_RGBA_8888: u8 = 0x12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PvrV2Header {
    pub height: u32,
    pub width: u32,
    pub mipmap_count: u32,
    pub flags: u32,
    pub data_length: u32,
    pub bits_per_pixel: u32,
    pub red_mask: u32,
    pub green_mask: u32,
    pub blue_mask: u32,
    pub alpha_mask: u32,
    pub surface_count: u32,
}

impl PvrV2Header {
    pub fn pixel_format(&self) -> u8 {
        (self.flags & 0xff) as u8
    }
}

#[derive(Debug, Clone)]
pub struct DecodedPvr {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

pub fn parse_header(bytes: &[u8]) -> Result<PvrV2Header, AssetError> {
    if bytes.len() < PVR_V2_HEADER_SIZE {
        return Err(AssetError::InvalidPvr("header is truncated"));
    }
    let mut words = [0u32; 13];
    for (index, word) in words.iter_mut().enumerate() {
        let offset = index * 4;
        *word = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    }
    if words[0] as usize != PVR_V2_HEADER_SIZE {
        return Err(AssetError::InvalidPvr("unexpected header size"));
    }
    if words[11] != PVR_TAG {
        return Err(AssetError::InvalidPvr("PVR! tag is missing"));
    }
    if words[1] == 0 || words[2] == 0 {
        return Err(AssetError::InvalidPvr("zero texture dimension"));
    }
    if bytes.len() < PVR_V2_HEADER_SIZE + words[5] as usize {
        return Err(AssetError::InvalidPvr("texture data is truncated"));
    }
    Ok(PvrV2Header {
        height: words[1],
        width: words[2],
        mipmap_count: words[3],
        flags: words[4],
        data_length: words[5],
        bits_per_pixel: words[6],
        red_mask: words[7],
        green_mask: words[8],
        blue_mask: words[9],
        alpha_mask: words[10],
        surface_count: words[12],
    })
}

pub fn decode_rgba8(bytes: &[u8]) -> Result<DecodedPvr, AssetError> {
    let header = parse_header(bytes)?;
    let pixel_count = (header.width as usize)
        .checked_mul(header.height as usize)
        .ok_or(AssetError::InvalidPvr("texture dimensions overflow"))?;
    let bytes_per_pixel = (header.bits_per_pixel / 8) as usize;
    let base_size = pixel_count
        .checked_mul(bytes_per_pixel)
        .ok_or(AssetError::InvalidPvr("base mip size overflow"))?;
    let data = bytes
        .get(PVR_V2_HEADER_SIZE..PVR_V2_HEADER_SIZE + base_size)
        .ok_or(AssetError::InvalidPvr("base mip is truncated"))?;
    let mut rgba8 = Vec::with_capacity(pixel_count * 4);

    match header.pixel_format() {
        OGL_RGBA_4444 if header.bits_per_pixel == 16 => {
            for pixel in data.as_chunks::<2>().0 {
                let value = u16::from_le_bytes(*pixel) as u32;
                rgba8.extend_from_slice(&[
                    channel(value, header.red_mask),
                    channel(value, header.green_mask),
                    channel(value, header.blue_mask),
                    channel(value, header.alpha_mask),
                ]);
            }
        }
        OGL_RGBA_8888 if header.bits_per_pixel == 32 => {
            for pixel in data.as_chunks::<4>().0 {
                let value = u32::from_le_bytes(*pixel);
                rgba8.extend_from_slice(&[
                    channel(value, header.red_mask),
                    channel(value, header.green_mask),
                    channel(value, header.blue_mask),
                    channel(value, header.alpha_mask),
                ]);
            }
        }
        format => return Err(AssetError::UnsupportedPvr(format)),
    }

    Ok(DecodedPvr {
        width: header.width,
        height: header.height,
        rgba8,
    })
}

pub fn save_png(bytes: &[u8], destination: impl AsRef<Path>) -> Result<(), AssetError> {
    let decoded = decode_rgba8(bytes)?;
    image::save_buffer_with_format(
        destination,
        &decoded.rgba8,
        decoded.width,
        decoded.height,
        image::ColorType::Rgba8,
        image::ImageFormat::Png,
    )
    .map_err(|_| AssetError::InvalidPvr("PNG encoder failed"))
}

fn channel(value: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 255;
    }
    let shift = mask.trailing_zeros();
    let maximum = mask >> shift;
    let sample = (value & mask) >> shift;
    ((sample * 255 + maximum / 2) / maximum) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_rgba4444() {
        let mut bytes = Vec::new();
        for word in [
            52u32,
            1,
            1,
            0,
            OGL_RGBA_4444 as u32,
            2,
            16,
            0xf000,
            0x0f00,
            0x00f0,
            0x000f,
            PVR_TAG,
            1,
        ] {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        bytes.extend_from_slice(&0xf00fu16.to_le_bytes());
        let decoded = decode_rgba8(&bytes).unwrap();
        assert_eq!(decoded.rgba8, [255, 0, 0, 255]);
    }
}
