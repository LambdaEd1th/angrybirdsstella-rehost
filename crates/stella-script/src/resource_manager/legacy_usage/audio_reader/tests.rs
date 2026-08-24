use super::*;
use crate::resource_manager::legacy_usage::sprite_sheet_textures;

#[test]
fn wav_counter_uses_data_chunk_not_container_size() {
    let mut bytes = b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0".to_vec();
    bytes.extend_from_slice(&[1, 0, 1, 0, 0x80, 0x3e, 0, 0, 0, 0x7d, 0, 0, 2, 0, 16, 0]);
    bytes.extend_from_slice(b"data\x04\0\0\0\0\0\0\0");
    assert_eq!(wav_reader_info(&bytes).unwrap().data_bytes, Some(4));
    assert_eq!(
        wav_reader_info(&bytes).unwrap().stream,
        Some(AudioStreamInfo {
            samples: 2,
            sample_rate: 16_000,
        })
    );
}

#[test]
fn shipped_lame_mp3_matches_mpg123_gapless_pcm_length() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/extracted/data/audio/sfx/metal_hit_01.mp3");
    // Purple's static branch drains mpg123 into a memory stream. This
    // value independently matches `mpg123 -s` for the shipped file.
    let static_info = audio_file_info(&path, false).unwrap();
    assert_eq!(static_info.resident_bytes, Some(18_250));
    let AudioAssetSource::PcmData {
        data,
        channels,
        bits_per_sample,
        sample_rate,
        ..
    } = static_info.source
    else {
        panic!("static MP3 must own decoded PCM")
    };
    assert_eq!(data.len(), 18_250);
    assert_eq!((channels, bits_per_sample, sample_rate), (1, 16, 16_000));
    assert_eq!(fnv1a64(&data), 0xf289_a09d_98ab_9098);
    let streaming_info = audio_file_info(&path, true).unwrap();
    assert_eq!(streaming_info.resident_bytes, None);
    assert!(matches!(
        streaming_info.source,
        AudioAssetSource::EncodedFile {
            streaming: true,
            ..
        }
    ));
    assert_eq!(
        audio_duration(&path),
        Some(Duration::from_nanos(570_312_500))
    );

    let tagged_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../build/extracted/data/audio/sfx/character_stella_hit_01.mp3");
    let tagged_info = audio_file_info(&tagged_path, false).unwrap();
    let AudioAssetSource::PcmData { data, .. } = tagged_info.source else {
        panic!("tagged static MP3 must own decoded PCM")
    };
    // This resource exercises mpg123's 529-sample encoder-delay removal.
    assert_eq!(data.len(), 21_982);
    assert_eq!(fnv1a64(&data), 0xdde0_711c_f0b3_76c2);
    assert_eq!(tagged_info.sample_frames, Some(10_991));
    assert_eq!(
        audio_file_info(&tagged_path, true).unwrap().sample_frames,
        Some(10_991)
    );
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}

#[test]
fn every_shipped_mp3_decodes_to_a_static_native_memory_clip() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/extracted/data/audio");
    let mut pending = vec![root];
    let mut decoded = 0;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if !path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("mp3"))
            {
                continue;
            }
            let info = audio_file_info(&path, false)
                .unwrap_or_else(|| panic!("failed to decode {}", path.display()));
            assert!(info.duration.is_some(), "duration for {}", path.display());
            assert!(info.resident_bytes.is_some(), "PCM for {}", path.display());
            assert!(matches!(info.source, AudioAssetSource::PcmData { .. }));
            decoded += 1;
        }
    }
    assert_eq!(decoded, 538);
}

#[test]
fn purple_detection_prefers_magic_and_uses_unknown_type_as_raw_pcm() {
    assert_eq!(
        native_file_type(Path::new("renamed.png"), b"RIFF\0\0\0\0WAVE"),
        NativeFileType::Wav
    );
    assert_eq!(
        native_file_type(Path::new("renamed.bin"), b"OggSnoise"),
        NativeFileType::Raw
    );
    assert_eq!(
        native_file_type(Path::new("renamed.ogg"), b"not ogg"),
        NativeFileType::Ogg
    );
    assert_eq!(
        native_file_type(Path::new("samples.raw"), b"\0\0\0\0"),
        NativeFileType::Unsupported
    );
    assert_eq!(
        native_file_type(Path::new("samples.unknown"), b"\0\0\0\0"),
        NativeFileType::Raw
    );
}

#[test]
fn wav_reader_accepts_clean_eof_without_fmt_but_not_data_before_fmt() {
    let empty = wav_reader_info(b"RIFF\x04\0\0\0WAVE").unwrap();
    assert_eq!(empty.stream, None);
    assert_eq!(empty.data_bytes, None);

    let mut data_first = b"RIFF\x10\0\0\0WAVEdata".to_vec();
    data_first.extend_from_slice(&[0, 0, 0, 0]);
    assert_eq!(wav_reader_info(&data_first), None);

    let trailing_partial_header = b"RIFF\x05\0\0\0WAVEx";
    assert_eq!(wav_reader_info(trailing_partial_header), None);
}

#[test]
fn static_wav_keeps_declared_zero_tail_after_short_input_read() {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("stella-short-data-{unique}.wav"));
    let mut bytes = b"RIFF\x28\0\0\0WAVEfmt \x10\0\0\0".to_vec();
    bytes.extend_from_slice(&[1, 0, 1, 0, 0x80, 0x3e, 0, 0, 0, 0x7d, 0, 0, 2, 0, 16, 0]);
    bytes.extend_from_slice(b"data\x08\0\0\0\x34\x12");
    fs::write(&path, bytes).unwrap();

    let info = audio_file_info(&path, false).unwrap();
    assert_eq!(info.resident_bytes, Some(8));
    let AudioAssetSource::PcmData { data, .. } = info.source else {
        panic!("static WAV must own decoded PCM")
    };
    assert_eq!(data.as_ref(), &[0x34, 0x12, 0, 0, 0, 0, 0, 0]);
    let _ = fs::remove_file(path);
}

#[test]
fn shipped_sprt_reports_its_pvr_payload_upload() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/extracted/data");
    let textures = sprite_sheet_textures(&root, "images/1024x768/CONNECTION_SCREEN_SHEET_0.dat");
    assert_eq!(textures.len(), 1);
    assert_eq!(textures[0].uploaded_bytes, 96_350);
}
