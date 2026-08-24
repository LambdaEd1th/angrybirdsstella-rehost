//! Ogg/Vorbis granule accounting for Purple's static decode path.

use super::AudioStreamInfo;

pub(super) fn stream_info(bytes: &[u8]) -> Option<AudioStreamInfo> {
    parsed_stream(bytes).map(|(stream, _)| stream)
}

fn parsed_stream(bytes: &[u8]) -> Option<(AudioStreamInfo, u32)> {
    let identification = bytes
        .windows(7)
        .position(|window| window == b"\x01vorbis")?;
    let channels = u32::from(*bytes.get(identification + 11)?);
    if channels == 0 {
        return None;
    }
    let sample_rate = u32::from_le_bytes(
        bytes
            .get(identification + 12..identification + 16)?
            .try_into()
            .ok()?,
    );
    if sample_rate == 0 {
        return None;
    }
    let mut cursor = 0usize;
    let mut final_granule = 0u64;
    while cursor.checked_add(27)? <= bytes.len() {
        if bytes.get(cursor..cursor + 4) != Some(b"OggS") {
            break;
        }
        let granule = u64::from_le_bytes(bytes.get(cursor + 6..cursor + 14)?.try_into().ok()?);
        if granule != u64::MAX {
            final_granule = final_granule.max(granule);
        }
        let segment_count = *bytes.get(cursor + 26)? as usize;
        let table = bytes.get(cursor + 27..cursor + 27 + segment_count)?;
        let payload_len = table.iter().map(|size| *size as usize).sum::<usize>();
        cursor = cursor.checked_add(27 + segment_count + payload_len)?;
    }
    (final_granule > 0).then_some((
        AudioStreamInfo {
            samples: final_granule,
            sample_rate,
        },
        channels,
    ))
}
