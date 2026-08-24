use super::*;
use std::path::PathBuf;

fn configuration(
    channels: u16,
    bits_per_sample: u16,
    buffer_bytes: u32,
) -> NativeMixerConfiguration {
    NativeMixerConfiguration {
        channels,
        bits_per_sample,
        sample_rate: 44_100,
        buffer_bytes,
    }
}

fn playback(
    handle: i64,
    bytes: &[u8],
    channels: u16,
    bits_per_sample: u16,
    volume: f32,
    looping: bool,
) -> NativePlayback {
    NativePlayback::new(
        AudioPlaybackState {
            handle,
            source: None,
            duration: None,
            sample_frames: None,
            source_channels: None,
            source_bits_per_sample: None,
            volume,
            looping,
            track: 0,
        },
        NativeClip {
            reader: NativeReader::Memory {
                bytes: Arc::from(bytes),
                cursor: 0,
            },
            channels,
            bits_per_sample,
            sample_rate: 16_000,
        },
    )
}

#[test]
fn sixteen_bit_mixer_quantizes_gain_accumulates_and_saturates() {
    let mut mixer = MixerState::new(configuration(1, 16, 4));
    mixer.started = true;
    mixer
        .playbacks
        .insert(1, playback(1, &i16::MAX.to_le_bytes(), 1, 16, 1.0, true));
    mixer
        .playbacks
        .insert(2, playback(2, &i16::MAX.to_le_bytes(), 1, 16, 0.5, true));
    let block = mixer.fill_block();
    assert_eq!(block, [i16::MAX as f32 / 32_768.0; 2]);
}

#[test]
fn mono_and_stereo_conversion_match_recovered_integer_shifts() {
    let sample = 4_096_i16;
    let mut stereo = MixerState::new(configuration(2, 16, 8));
    stereo.started = true;
    stereo
        .playbacks
        .insert(1, playback(1, &sample.to_le_bytes(), 1, 16, 1.0, true));
    assert_eq!(stereo.fill_block(), [0.125, 0.125, 0.125, 0.125]);

    let mut mono = MixerState::new(configuration(1, 16, 4));
    mono.started = true;
    mono.playbacks.insert(
        1,
        playback(
            1,
            &[sample.to_le_bytes(), sample.to_le_bytes()].concat(),
            2,
            16,
            1.0,
            true,
        ),
    );
    assert_eq!(mono.fill_block(), [0.125, 0.125]);
}

#[test]
fn muted_instance_short_read_then_zero_read_then_removal_matches_three_blocks() {
    let mut mixer = MixerState::new(configuration(1, 16, 4));
    mixer.started = true;
    mixer
        .playbacks
        .insert(7, playback(7, &0_i16.to_le_bytes(), 1, 16, 0.0, false));
    assert_eq!(mixer.fill_block(), [0.0, 0.0]);
    assert!(!mixer.playbacks[&7].finished);
    assert_eq!(mixer.fill_block(), [0.0, 0.0]);
    assert!(mixer.playbacks[&7].finished);
    assert_eq!(mixer.fill_block(), [0.0, 0.0]);
    assert!(!mixer.playbacks.contains_key(&7));
    assert!(mixer.completed.contains(&7));
}

#[test]
fn eight_bit_conversion_preserves_target_unsigned_centering_bug() {
    let mut mixer = MixerState::new(configuration(2, 8, 2));
    mixer.started = true;
    mixer
        .playbacks
        .insert(1, playback(1, &[128], 1, 8, 1.0, true));
    // Mono-to-stereo omits `sample - 128`, so unsigned center saturates
    // to a positive full-scale byte in the original binary as well.
    assert_eq!(mixer.fill_block(), [127.0 / 128.0, 127.0 / 128.0]);
    assert_eq!(native_u8_saturate(127), 254);
    assert_eq!(native_u8_saturate(128), 255);
}

#[test]
fn source_sample_rate_is_metadata_only_and_does_not_resample() {
    let mut mixer = MixerState::new(configuration(1, 16, 4));
    mixer.started = true;
    mixer
        .playbacks
        .insert(1, playback(1, &1_024_i16.to_le_bytes(), 1, 16, 1.0, true));
    assert_eq!(mixer.fill_block(), [0.03125, 0.03125]);
}

#[test]
fn composite_child_end_returns_short_before_next_child_on_non_looping_read() {
    let first = NativeClip {
        reader: NativeReader::Memory {
            bytes: Arc::from(1_i16.to_le_bytes()),
            cursor: 0,
        },
        channels: 1,
        bits_per_sample: 16,
        sample_rate: 16_000,
    };
    let second = NativeClip {
        reader: NativeReader::Memory {
            bytes: Arc::from(2_i16.to_le_bytes()),
            cursor: 0,
        },
        channels: 1,
        bits_per_sample: 16,
        sample_rate: 16_000,
    };
    let mut playback = NativePlayback::new(
        AudioPlaybackState {
            handle: 9,
            source: None,
            duration: None,
            sample_frames: None,
            source_channels: None,
            source_bits_per_sample: None,
            volume: 1.0,
            looping: false,
            track: 0,
        },
        NativeClip {
            reader: NativeReader::Sequence {
                parts: vec![first, second],
                index: 0,
            },
            channels: 1,
            bits_per_sample: 16,
            sample_rate: 16_000,
        },
    );
    assert_eq!(playback.read(4), 1_i16.to_le_bytes());
    assert!(!playback.finished);
    assert_eq!(playback.read(4), 2_i16.to_le_bytes());
    assert!(!playback.finished);
    assert!(playback.read(4).is_empty());
    assert!(playback.finished);
}

#[test]
fn looping_reader_retries_short_reads_and_wraps_inside_one_request() {
    let mut playback = playback(3, &7_i16.to_le_bytes(), 1, 16, 1.0, true);
    assert_eq!(
        playback.read(6),
        [
            7_i16.to_le_bytes(),
            7_i16.to_le_bytes(),
            7_i16.to_le_bytes()
        ]
        .concat()
    );
    assert!(!playback.finished);
}

#[test]
fn output_source_prefills_six_native_blocks_before_first_sample() {
    let configuration = configuration(1, 16, 4);
    let control = create(configuration);
    {
        let mut mixer = control.shared.lock().unwrap();
        mixer.started = true;
        mixer
            .playbacks
            .insert(4, playback(4, &3_i16.to_le_bytes(), 1, 16, 1.0, false));
    }
    let mut source = control.source(6);
    assert_eq!(
        source.current_block.len() + source.queued_blocks.iter().map(Vec::len).sum::<usize>(),
        12
    );
    assert!(!control.shared.lock().unwrap().playbacks.contains_key(&4));
    assert!(control.shared.lock().unwrap().completed.contains(&4));
    assert_eq!(source.next(), Some(3.0 / 32_768.0));
}

#[test]
fn output_source_refills_only_after_two_processed_buffers_at_a_worker_poll() {
    let configuration = NativeMixerConfiguration {
        channels: 1,
        bits_per_sample: 16,
        sample_rate: 100,
        buffer_bytes: 2,
    };
    let control = create(configuration);
    {
        let mut mixer = control.shared.lock().unwrap();
        mixer.started = true;
        mixer
            .playbacks
            .insert(4, playback(4, &[0; 40], 1, 16, 1.0, false));
    }
    let mut source = control.source(6);
    let cursor = || {
        let mixer = control.shared.lock().unwrap();
        match &mixer.playbacks[&4].clip.reader {
            NativeReader::Memory { cursor, .. } => *cursor,
            _ => panic!("test playback is not a memory reader"),
        }
    };
    assert_eq!(cursor(), 12);
    assert_eq!(source.next(), Some(0.0));
    assert_eq!(cursor(), 12);
    assert_eq!(source.next(), Some(0.0));
    assert_eq!(cursor(), 16);
}

#[test]
fn gain_conversion_keeps_arm_integer_indefinite_for_nan_and_overflow() {
    assert_eq!(native_gain(f32::NAN, 1.0, 4_096.0), i32::MIN);
    assert_eq!(native_gain(f32::INFINITY, 1.0, 4_096.0), i32::MIN);
    assert_eq!(native_gain(0.9999, 1.0, 4_096.0), 4_095);
}

#[test]
fn shipped_mp3_streams_match_native_gapless_pcm() {
    for (name, expected_len, expected_hash) in [
        ("metal_hit_01.mp3", 18_250, 0xf289_a09d_98ab_9098),
        ("character_stella_hit_01.mp3", 21_982, 0xdde0_711c_f0b3_76c2),
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../build/extracted/data/audio/sfx")
            .join(name);
        let clip = decode_asset(&AudioAssetSource::File(path)).unwrap();
        assert_eq!((clip.channels, clip.bits_per_sample), (1, 16));
        let mut playback = NativePlayback::new(
            AudioPlaybackState {
                handle: 1,
                source: None,
                duration: None,
                sample_frames: None,
                source_channels: None,
                source_bits_per_sample: None,
                volume: 1.0,
                looping: false,
                track: 0,
            },
            clip,
        );
        let mut decoded = Vec::new();
        while !playback.finished {
            decoded.extend(playback.read(4_096));
        }
        assert_eq!(decoded.len(), expected_len, "{name}");
        assert_eq!(fnv1a64(&decoded), expected_hash, "{name}");
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}
