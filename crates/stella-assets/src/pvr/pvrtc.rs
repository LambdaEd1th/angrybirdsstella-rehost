// SPDX-License-Identifier: MIT
// Copyright (c) Imagination Technologies Ltd.
// Adapted from PowerVR Native SDK PVRTDecompress.cpp, commit
// fa7396af369a3c803be43545504f82c7d5cfa4a9. See pvrtc/LICENSE.MIT.
// Pure Rust PVRTC1 decoding; no native-library dependency or padded RGBA copy.

use crate::AssetError;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Rate {
    Two,
    Four,
}

impl Rate {
    const fn word_width(self) -> usize {
        match self {
            Self::Two => 8,
            Self::Four => 4,
        }
    }
}

#[derive(Clone, Copy)]
struct Word {
    modulation: u32,
    colors: u32,
}

type Color = [i32; 4];

pub(super) fn decode(
    data: &[u8],
    width: u32,
    height: u32,
    rate: Rate,
    alpha: bool,
) -> Result<Vec<u8>, AssetError> {
    let width = width as usize;
    let height = height as usize;
    let word_width = rate.word_width();
    let storage_width = width.max(word_width * 2);
    let storage_height = height.max(8);
    if !storage_width.is_power_of_two() || !storage_height.is_power_of_two() {
        return Err(AssetError::InvalidPvr(
            "PVRTC storage dimensions must be powers of two",
        ));
    }
    let words_x = storage_width / word_width;
    let words_y = storage_height / 4;
    let encoded_size = words_x
        .checked_mul(words_y)
        .and_then(|count| count.checked_mul(8))
        .ok_or(AssetError::InvalidPvr("PVRTC base mip size overflow"))?;
    let data = data
        .get(..encoded_size)
        .ok_or(AssetError::InvalidPvr("PVRTC base mip is truncated"))?;
    let rgba_size = width
        .checked_mul(height)
        .and_then(|count| count.checked_mul(4))
        .ok_or(AssetError::InvalidPvr("decoded texture size overflow"))?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(rgba_size)
        .map_err(|_| AssetError::InvalidPvr("decoded texture allocation failed"))?;
    rgba.resize(rgba_size, 0);

    let word_at = |x, y| {
        let offset = morton_index(words_x, words_y, x, y) * 8;
        Word {
            modulation: u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()),
            colors: u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap()),
        }
    };
    for y in 0..words_y {
        let next_y = (y + 1) % words_y;
        for x in 0..words_x {
            let next_x = (x + 1) % words_x;
            let pixels = decode_words(
                [
                    word_at(x, y),
                    word_at(next_x, y),
                    word_at(x, next_y),
                    word_at(next_x, next_y),
                ],
                rate,
                alpha,
            );
            // Each 2x2-word neighborhood supplies one quarter of each word.
            // Scatter directly into the requested extent, including small mips.
            for row in 0..2 {
                for column in 0..word_width / 2 {
                    for (out_x, out_y, source) in [
                        (
                            x * word_width + column + word_width / 2,
                            y * 4 + row + 2,
                            row * word_width + column,
                        ),
                        (
                            next_x * word_width + column,
                            y * 4 + row + 2,
                            row * word_width + column + word_width / 2,
                        ),
                        (
                            x * word_width + column + word_width / 2,
                            next_y * 4 + row,
                            (row + 2) * word_width + column,
                        ),
                        (
                            next_x * word_width + column,
                            next_y * 4 + row,
                            (row + 2) * word_width + column + word_width / 2,
                        ),
                    ] {
                        if out_x < width && out_y < height {
                            let offset = (out_y * width + out_x) * 4;
                            rgba[offset..offset + 4].copy_from_slice(&pixels[source]);
                        }
                    }
                }
            }
        }
    }
    Ok(rgba)
}

fn morton_index(width: usize, height: usize, x: usize, y: usize) -> usize {
    let mut index = 0;
    let mut source_bit = 1;
    let mut destination_bit = 1;
    let mut shift = 0;
    while source_bit < width.min(height) {
        if y & source_bit != 0 {
            index |= destination_bit;
        }
        if x & source_bit != 0 {
            index |= destination_bit << 1;
        }
        source_bit <<= 1;
        destination_bit <<= 2;
        shift += 1;
    }
    index | (((if height < width { x } else { y }) >> shift) << (2 * shift))
}

fn color_a(data: u32) -> Color {
    if data & 0x8000 != 0 {
        [
            ((data & 0x7c00) >> 10) as i32,
            ((data & 0x3e0) >> 5) as i32,
            ((data & 0x1e) | ((data & 0x1e) >> 4)) as i32,
            15,
        ]
    } else {
        [
            (((data & 0xf00) >> 7) | ((data & 0xf00) >> 11)) as i32,
            (((data & 0xf0) >> 3) | ((data & 0xf0) >> 7)) as i32,
            (((data & 0xe) << 1) | ((data & 0xe) >> 2)) as i32,
            ((data & 0x7000) >> 11) as i32,
        ]
    }
}

fn color_b(data: u32) -> Color {
    if data & 0x8000_0000 != 0 {
        [
            ((data & 0x7c00_0000) >> 26) as i32,
            ((data & 0x3e0_0000) >> 21) as i32,
            ((data & 0x1f_0000) >> 16) as i32,
            15,
        ]
    } else {
        [
            (((data & 0xf00_0000) >> 23) | ((data & 0xf00_0000) >> 27)) as i32,
            (((data & 0xf0_0000) >> 19) | ((data & 0xf0_0000) >> 23)) as i32,
            (((data & 0xf_0000) >> 15) | ((data & 0xf_0000) >> 19)) as i32,
            ((data & 0x7000_0000) >> 27) as i32,
        ]
    }
}

fn interpolate([mut p, q, mut r, s]: [Color; 4], rate: Rate) -> [Color; 32] {
    let word_width = rate.word_width();
    let delta_x_p: Color = std::array::from_fn(|i| q[i] - p[i]);
    let delta_x_r: Color = std::array::from_fn(|i| s[i] - r[i]);
    for i in 0..4 {
        p[i] *= word_width as i32;
        r[i] *= word_width as i32;
    }
    let mut output = [[0; 4]; 32];
    for outer in 0..word_width {
        let mut value: Color = std::array::from_fn(|i| 4 * p[i]);
        let delta_y: Color = std::array::from_fn(|i| r[i] - p[i]);
        for inner in 0..4 {
            let index = if rate == Rate::Two {
                inner * word_width + outer
            } else {
                outer * word_width + inner
            };
            for i in 0..4 {
                output[index][i] = match (rate, i) {
                    (Rate::Two, 3) => (value[i] >> 5) + (value[i] >> 1),
                    (Rate::Two, _) => (value[i] >> 7) + (value[i] >> 2),
                    (Rate::Four, 3) => (value[i] >> 4) + value[i],
                    (Rate::Four, _) => (value[i] >> 6) + (value[i] >> 1),
                };
                value[i] += delta_y[i];
            }
        }
        for i in 0..4 {
            p[i] += delta_x_p[i];
            r[i] += delta_x_r[i];
        }
    }
    output
}

#[derive(Default)]
struct Modulations {
    values: [[u8; 8]; 16],
    modes: [[u8; 8]; 16],
}

impl Modulations {
    fn unpack(&mut self, word: Word, offset_x: usize, offset_y: usize, rate: Rate) {
        let mut mode = (word.colors & 1) as u8;
        let mut bits = word.modulation;
        if rate == Rate::Two && mode != 0 {
            if bits & 1 != 0 {
                mode = if bits & (1 << 20) != 0 { 3 } else { 2 };
                bits = (bits & !(1 << 20)) | ((bits & (1 << 21)) >> 1);
            }
            bits = (bits & !1) | ((bits & 2) >> 1);
        }
        for y in 0..4 {
            for x in 0..rate.word_width() {
                if rate == Rate::Four {
                    // The SDK's 4bpp modulation and output orientations cancel.
                    self.values[y + offset_y][x + offset_x] = if mode == 0 {
                        [0, 3, 5, 8]
                    } else {
                        [0, 4, 14, 8]
                    }[(bits & 3) as usize];
                    bits >>= 2;
                } else {
                    self.modes[x + offset_x][y + offset_y] = mode;
                    if mode == 0 {
                        self.values[x + offset_x][y + offset_y] = if bits & 1 == 0 { 0 } else { 3 };
                        bits >>= 1;
                    } else if (x ^ y) & 1 == 0 {
                        self.values[x + offset_x][y + offset_y] = (bits & 3) as u8;
                        bits >>= 2;
                    }
                }
            }
        }
    }

    fn weight(&self, x: usize, y: usize, rate: Rate) -> i32 {
        if rate == Rate::Four {
            return i32::from(self.values[x][y]);
        }
        let value = |x: usize, y: usize| [0, 3, 5, 8][self.values[x][y] as usize];
        if self.modes[x][y] == 0 || (x ^ y) & 1 == 0 {
            return value(x, y);
        }
        match self.modes[x][y] {
            1 => (value(x, y - 1) + value(x, y + 1) + value(x - 1, y) + value(x + 1, y) + 2) / 4,
            2 => (value(x - 1, y) + value(x + 1, y) + 1) / 2,
            _ => (value(x, y - 1) + value(x, y + 1) + 1) / 2,
        }
    }
}

fn decode_words(words: [Word; 4], rate: Rate, alpha: bool) -> [[u8; 4]; 32] {
    let word_width = rate.word_width();
    let mut modulation = Modulations::default();
    for (word, (x, y)) in words
        .into_iter()
        .zip([(0, 0), (word_width, 0), (0, 4), (word_width, 4)])
    {
        modulation.unpack(word, x, y, rate);
    }
    let colors_a = interpolate(words.map(|word| color_a(word.colors)), rate);
    let colors_b = interpolate(words.map(|word| color_b(word.colors)), rate);
    let mut output = [[0; 4]; 32];
    for y in 0..4 {
        for x in 0..word_width {
            let mut weight = modulation.weight(x + word_width / 2, y + 2, rate);
            let punchthrough = weight > 10;
            if punchthrough {
                weight -= 10;
            }
            let source = y * word_width + x;
            let target = if rate == Rate::Two { source } else { y + x * 4 };
            output[target] = std::array::from_fn(|i| {
                if i == 3 && !alpha {
                    255
                } else if i == 3 && punchthrough {
                    0
                } else {
                    ((colors_a[source][i] * (8 - weight) + colors_b[source][i] * weight) / 8) as u8
                }
            });
        }
    }
    output
}
