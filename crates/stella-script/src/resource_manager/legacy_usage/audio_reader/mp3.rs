//! MPEG frame accounting for Purple's static mpg123 decode path.

use super::AudioStreamInfo;

#[derive(Debug, Clone, Copy)]
struct Frame {
    byte_len: usize,
    samples: u32,
    channels: u32,
    sample_rate: u32,
    side_info: usize,
    has_crc: bool,
}

pub(super) fn stream_info(bytes: &[u8]) -> Option<AudioStreamInfo> {
    let stream = decoded_stream(bytes)?;
    Some(AudioStreamInfo {
        samples: stream.samples,
        sample_rate: stream.sample_rate,
    })
}

#[derive(Debug, Clone, Copy)]
struct DecodedStream {
    samples: u64,
    sample_rate: u32,
}

fn decoded_stream(bytes: &[u8]) -> Option<DecodedStream> {
    let start = skip_id3v2(bytes)?;
    let first_offset = (start..bytes.len().saturating_sub(3))
        .find(|&offset| bytes.get(offset..).and_then(parse_frame).is_some())?;
    let first = parse_frame(bytes.get(first_offset..)?)?;

    if let Some((frames, delay, padding)) = xing_gapless(bytes, first_offset, first) {
        let samples = u64::from(frames)
            .checked_mul(u64::from(first.samples))?
            .checked_sub(u64::from(delay) + u64::from(padding))?;
        return Some(DecodedStream {
            samples,
            sample_rate: first.sample_rate,
        });
    }

    let mut cursor = first_offset;
    let mut samples = 0u64;
    while let Some(frame) = bytes.get(cursor..).and_then(parse_frame) {
        if frame.channels != first.channels || frame.byte_len == 0 {
            break;
        }
        samples = samples.checked_add(u64::from(frame.samples))?;
        cursor = cursor.checked_add(frame.byte_len)?;
    }
    Some(DecodedStream {
        samples,
        sample_rate: first.sample_rate,
    })
}

fn skip_id3v2(bytes: &[u8]) -> Option<usize> {
    if bytes.get(..3) != Some(b"ID3") {
        return Some(0);
    }
    let size = bytes.get(6..10)?;
    if size.iter().any(|byte| byte & 0x80 != 0) {
        return None;
    }
    let body = ((size[0] as usize) << 21)
        | ((size[1] as usize) << 14)
        | ((size[2] as usize) << 7)
        | size[3] as usize;
    10usize.checked_add(body)
}

fn parse_frame(bytes: &[u8]) -> Option<Frame> {
    let header = u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?);
    if header & 0xffe0_0000 != 0xffe0_0000 {
        return None;
    }
    let version = (header >> 19) & 3;
    let layer_bits = (header >> 17) & 3;
    let bitrate_index = ((header >> 12) & 0xf) as usize;
    let sample_index = ((header >> 10) & 3) as usize;
    if version == 1
        || layer_bits == 0
        || bitrate_index == 0
        || bitrate_index == 15
        || sample_index == 3
    {
        return None;
    }

    let layer = 4 - layer_bits;
    let version_one = version == 3;
    let bitrate_row = match (version_one, layer) {
        (true, 1) => 0,
        (true, 2) => 1,
        (true, 3) => 2,
        (false, 1) => 3,
        (false, 2 | 3) => 4,
        _ => return None,
    };
    const BITRATES: [[u32; 16]; 5] = [
        [
            0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0,
        ],
        [
            0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0,
        ],
        [
            0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
        ],
        [
            0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0,
        ],
        [
            0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
        ],
    ];
    const SAMPLE_RATES: [[u32; 3]; 3] = [
        [11_025, 12_000, 8_000],
        [22_050, 24_000, 16_000],
        [44_100, 48_000, 32_000],
    ];
    let sample_row = match version {
        0 => 0,
        2 => 1,
        3 => 2,
        _ => return None,
    };
    let bitrate = BITRATES[bitrate_row][bitrate_index] * 1_000;
    let sample_rate = SAMPLE_RATES[sample_row][sample_index];
    let padding = (header >> 9) & 1;
    let byte_len = match layer {
        1 => (((12 * bitrate / sample_rate) + padding) * 4) as usize,
        2 => ((144 * bitrate / sample_rate) + padding) as usize,
        3 if version_one => ((144 * bitrate / sample_rate) + padding) as usize,
        3 => ((72 * bitrate / sample_rate) + padding) as usize,
        _ => return None,
    };
    let samples = match layer {
        1 => 384,
        2 => 1_152,
        3 if version_one => 1_152,
        3 => 576,
        _ => return None,
    };
    let channels = if (header >> 6) & 3 == 3 { 1 } else { 2 };
    let side_info = if layer == 3 {
        match (version_one, channels) {
            (true, 1) => 17,
            (true, _) => 32,
            (false, 1) => 9,
            (false, _) => 17,
        }
    } else {
        0
    };
    Some(Frame {
        byte_len,
        samples,
        channels,
        sample_rate,
        side_info,
        has_crc: header & (1 << 16) == 0,
    })
}

fn xing_gapless(bytes: &[u8], offset: usize, frame: Frame) -> Option<(u32, u16, u16)> {
    let xing = offset.checked_add(4 + usize::from(frame.has_crc) * 2 + frame.side_info)?;
    if !matches!(bytes.get(xing..xing + 4), Some(b"Xing" | b"Info")) {
        return None;
    }
    let flags = u32::from_be_bytes(bytes.get(xing + 4..xing + 8)?.try_into().ok()?);
    let mut cursor = xing + 8;
    let frames = if flags & 1 != 0 {
        let value = u32::from_be_bytes(bytes.get(cursor..cursor + 4)?.try_into().ok()?);
        cursor += 4;
        value
    } else {
        return None;
    };
    if flags & 2 != 0 {
        cursor += 4;
    }
    if flags & 4 != 0 {
        cursor += 100;
    }
    if flags & 8 != 0 {
        cursor += 4;
    }

    let frame_end = offset.checked_add(frame.byte_len)?.min(bytes.len());
    let search_end = cursor.saturating_add(32).min(frame_end);
    let lame = bytes
        .get(cursor..search_end)?
        .windows(4)
        .position(|window| window == b"LAME")?
        + cursor;
    let packed = bytes.get(lame + 21..lame + 24)?;
    let delay = (u16::from(packed[0]) << 4) | u16::from(packed[1] >> 4);
    let padding = (u16::from(packed[1] & 0xf) << 8) | u16::from(packed[2]);
    Some((frames, delay, padding))
}
