//! Native audio stream-format validation and output-buffer sizing.

use crate::*;

pub(super) fn audio_io_configuration(
    [channels, bits_per_sample, samples_per_second]: [i32; 3],
    kind: &str,
) -> LuaResult<AudioIoConfiguration> {
    if !(1..=2).contains(&channels) {
        return Err(runtime_error(format!(
            "Unsupported count of channels while creating {kind}"
        )));
    }
    if !matches!(bits_per_sample, 8 | 16) {
        return Err(runtime_error(format!(
            "Unsupported bits per sample while creating {kind}"
        )));
    }
    if !matches!(
        samples_per_second,
        8_000
            | 11_025
            | 12_000
            | 16_000
            | 22_050
            | 24_000
            | 32_000
            | 44_100
            | 48_000
            | 64_000
            | 88_200
            | 96_000
    ) {
        return Err(runtime_error(format!(
            "Unsupported samples per second while creating {kind}"
        )));
    }
    let buffer_bytes = if kind == "AudioOutput" {
        let bytes_per_frame = (bits_per_sample / 8 * channels) as u32;
        let quarter_tenth = bytes_per_frame
            .wrapping_mul(samples_per_second as u32)
            .wrapping_div(40);
        let frame_aligned = quarter_tenth
            .wrapping_add(bytes_per_frame - 1)
            .wrapping_div(bytes_per_frame)
            .wrapping_mul(bytes_per_frame);
        frame_aligned.next_power_of_two()
    } else {
        0
    };
    Ok(AudioIoConfiguration {
        channels,
        bits_per_sample,
        samples_per_second,
        buffer_bytes,
    })
}
