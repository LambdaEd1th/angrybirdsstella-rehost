use std::{
    env, fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use rodio::Decoder;

fn main() {
    let mut args = env::args_os().skip(1);
    let mpg123 = PathBuf::from(
        args.next()
            .expect("usage: stella-mp3-audit MPG123 AUDIO_ROOT"),
    );
    let audio_root = PathBuf::from(args.next().expect("missing AUDIO_ROOT"));
    let emit_profile = args.next().is_some_and(|value| value == "--emit-profile");
    let mut paths = Vec::new();
    collect_mp3s(&audio_root, &mut paths);
    paths.sort();

    let mut total_samples = 0usize;
    let mut total_differences = 0usize;
    let mut maximum_differences = (0usize, PathBuf::new());
    let mut failed = 0usize;
    let mut trimmed_start = 0usize;
    let mut trimmed_end = 0usize;
    let mut profiles = Vec::with_capacity(paths.len());
    for (number, path) in paths.iter().enumerate() {
        let encoded = fs::read(path).expect("read MP3");
        let encoded_hash = fnv1a64(&encoded);
        let encoded_len = u32::try_from(encoded.len()).expect("encoded length");
        let target = Command::new(&mpg123)
            .args(["--quiet", "--gapless", "-s"])
            .arg(path)
            .output()
            .expect("run mpg123");
        if !target.status.success() {
            eprintln!("target decode failed: {}", path.display());
            failed += 1;
            continue;
        }
        let decoder = Decoder::builder()
            .with_data(Cursor::new(Arc::<[u8]>::from(encoded)))
            .with_hint("mp3")
            .with_gapless(true)
            .build()
            .expect("decode MP3");
        let actual = decoder
            .map(native_mpg123_i16)
            .flat_map(i16::to_le_bytes)
            .collect::<Vec<_>>();
        let (actual, skipped_samples) = if actual.len() == target.stdout.len() {
            (actual.as_slice(), 0)
        } else if actual.len() > target.stdout.len()
            && (actual.len() - target.stdout.len()).is_multiple_of(2)
        {
            let extra = actual.len() - target.stdout.len();
            let prefix = &actual[..target.stdout.len()];
            let suffix = &actual[extra..];
            let prefix_differences = count_differences(prefix, &target.stdout);
            let suffix_differences = count_differences(suffix, &target.stdout);
            if prefix_differences <= suffix_differences {
                trimmed_end += 1;
                (prefix, 0)
            } else {
                trimmed_start += 1;
                (suffix, extra / 2)
            }
        } else {
            eprintln!(
                "unhandled length mismatch: {} Rust={} mpg123={}",
                path.display(),
                actual.len(),
                target.stdout.len()
            );
            failed += 1;
            continue;
        };
        let corrections = actual
            .chunks_exact(2)
            .zip(target.stdout.chunks_exact(2))
            .enumerate()
            .filter_map(|(index, (actual, expected))| {
                (actual != expected).then_some((
                    u32::try_from(index).expect("sample index"),
                    i16::from_le_bytes([expected[0], expected[1]]),
                ))
            })
            .collect::<Vec<_>>();
        let differences = corrections.len();
        total_samples += actual.len() / 2;
        total_differences += differences;
        profiles.push(Profile {
            encoded_hash,
            encoded_len,
            skipped_samples: u32::try_from(skipped_samples).expect("skipped samples"),
            output_samples: u32::try_from(target.stdout.len() / 2).expect("output samples"),
            corrections,
        });
        if differences > maximum_differences.0 {
            maximum_differences = (differences, path.clone());
        }
        if !emit_profile && ((number + 1).is_multiple_of(50) || number + 1 == paths.len()) {
            eprintln!(
                "audited {}/{} files; differences={total_differences}",
                number + 1,
                paths.len()
            );
        }
    }

    if emit_profile {
        profiles.sort_by_key(|profile| (profile.encoded_hash, profile.encoded_len));
        println!("PROFILE_BEGIN");
        for line in base64_lines(&encode_profiles(&profiles), 96) {
            println!("{line}");
        }
        println!("PROFILE_END");
    } else {
        println!("files={} failed={failed}", paths.len());
        println!("trimmed_start={trimmed_start} trimmed_end={trimmed_end}");
        println!("samples={total_samples} differences={total_differences}");
        println!(
            "difference_rate={:.6}%",
            100.0 * total_differences as f64 / total_samples as f64
        );
        println!(
            "maximum={} file={}",
            maximum_differences.0,
            maximum_differences.1.display()
        );
    }
}

struct Profile {
    encoded_hash: u64,
    encoded_len: u32,
    skipped_samples: u32,
    output_samples: u32,
    corrections: Vec<(u32, i16)>,
}

fn encode_profiles(profiles: &[Profile]) -> Vec<u8> {
    let mut bytes = b"STMP3C1\0".to_vec();
    bytes.extend_from_slice(
        &u32::try_from(profiles.len())
            .expect("profile count")
            .to_le_bytes(),
    );
    for profile in profiles {
        bytes.extend_from_slice(&profile.encoded_hash.to_le_bytes());
        bytes.extend_from_slice(&profile.encoded_len.to_le_bytes());
        bytes.extend_from_slice(&profile.skipped_samples.to_le_bytes());
        bytes.extend_from_slice(&profile.output_samples.to_le_bytes());
        bytes.extend_from_slice(
            &u32::try_from(profile.corrections.len())
                .expect("correction count")
                .to_le_bytes(),
        );
        for &(index, sample) in &profile.corrections {
            bytes.extend_from_slice(&index.to_le_bytes());
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
    }
    bytes
}

fn base64_lines(bytes: &[u8], width: usize) -> Vec<String> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        encoded.push(ALPHABET[(a >> 2) as usize] as char);
        encoded.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            ALPHABET[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            ALPHABET[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    encoded
        .as_bytes()
        .chunks(width)
        .map(|line| String::from_utf8(line.to_vec()).expect("base64"))
        .collect()
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}

fn count_differences(actual: &[u8], expected: &[u8]) -> usize {
    actual
        .chunks_exact(2)
        .zip(expected.chunks_exact(2))
        .filter(|(actual, expected)| actual != expected)
        .count()
}

fn collect_mp3s(path: &Path, output: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(path).expect("read audio directory") {
        let path = entry.expect("read directory entry").path();
        if path.is_dir() {
            collect_mp3s(&path, output);
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp3"))
        {
            output.push(path);
        }
    }
}

fn native_mpg123_i16(sample: f32) -> i16 {
    let scaled = sample * 32_768.0;
    if scaled.is_nan() || scaled > f32::from(i16::MAX) {
        i16::MAX
    } else if scaled < f32::from(i16::MIN) {
        i16::MIN
    } else {
        scaled.trunc() as i16
    }
}
