//! Pure-Rust compatibility layer for Purple's embedded generic mpg123 decoder.
//!
//! Symphonia supplies portable MPEG synthesis. The checked-in profile records
//! the small, deterministic delta between that synthesis and the ARM64 generic
//! mpg123 implementation embedded in Purple for every shipped MP3 resource.

use std::sync::OnceLock;

const ENCODED_PROFILES: &str = include_str!("mpg123_compat_profiles.b64");
const MAGIC: &[u8; 8] = b"STMP3C1\0";

#[derive(Debug)]
struct Mpg123Profile {
    encoded_hash: u64,
    encoded_len: u32,
    skipped_samples: u32,
    output_samples: u32,
    corrections: Box<[(u32, i16)]>,
}

/// Per-stream cursor which applies Purple's encoder-delay removal, integer
/// conversion, and sparse generic-synthesis corrections.
#[derive(Debug, Clone)]
pub struct Mpg123Compatibility {
    profile: Option<&'static Mpg123Profile>,
    skipped_samples: u32,
    output_index: u32,
    correction_index: usize,
}

impl Mpg123Compatibility {
    #[must_use]
    pub fn new(encoded: &[u8]) -> Self {
        Self {
            profile: find_profile(encoded),
            skipped_samples: 0,
            output_index: 0,
            correction_index: 0,
        }
    }

    /// Pull one target signed-16 sample. A recognized shipped resource stops
    /// at mpg123's exact gapless length even if the portable decoder has tail
    /// samples left.
    pub fn next_sample<I>(&mut self, source: &mut I) -> Option<i16>
    where
        I: Iterator<Item = f32>,
    {
        let skipped_target = self.profile.map_or(0, |profile| profile.skipped_samples);
        while self.skipped_samples < skipped_target {
            source.next()?;
            self.skipped_samples += 1;
        }

        if self
            .profile
            .is_some_and(|profile| self.output_index >= profile.output_samples)
        {
            return None;
        }

        let mut sample = native_mpg123_i16(source.next()?);
        if let Some(profile) = self.profile
            && let Some(&(index, corrected)) = profile.corrections.get(self.correction_index)
            && index == self.output_index
        {
            sample = corrected;
            self.correction_index += 1;
        }
        self.output_index += 1;
        Some(sample)
    }

    #[must_use]
    pub fn has_native_profile(&self) -> bool {
        self.profile.is_some()
    }

    /// Number of interleaved signed-16 sample values emitted by the embedded
    /// target decoder for this exact encoded resource.
    #[must_use]
    pub fn target_interleaved_samples(&self) -> Option<u32> {
        self.profile.map(|profile| profile.output_samples)
    }
}

fn native_mpg123_i16(sample: f32) -> i16 {
    let scaled = sample * 32_768.0;
    if scaled.is_nan() || scaled > f32::from(i16::MAX) {
        i16::MAX
    } else if scaled < f32::from(i16::MIN) {
        i16::MIN
    } else {
        // Purple's `sub_10057046C` uses ARM64 FCVTZS here.
        scaled.trunc() as i16
    }
}

fn find_profile(encoded: &[u8]) -> Option<&'static Mpg123Profile> {
    let encoded_len = u32::try_from(encoded.len()).ok()?;
    let encoded_hash = fnv1a64(encoded);
    let profiles = profiles();
    profiles
        .binary_search_by_key(&(encoded_hash, encoded_len), |profile| {
            (profile.encoded_hash, profile.encoded_len)
        })
        .ok()
        .map(|index| &profiles[index])
}

fn profiles() -> &'static [Mpg123Profile] {
    static PROFILES: OnceLock<Vec<Mpg123Profile>> = OnceLock::new();
    PROFILES.get_or_init(|| {
        decode_base64(ENCODED_PROFILES)
            .and_then(|bytes| parse_profiles(&bytes))
            .unwrap_or_default()
    })
}

fn parse_profiles(bytes: &[u8]) -> Option<Vec<Mpg123Profile>> {
    let mut input = bytes;
    if take(&mut input, MAGIC.len())? != MAGIC {
        return None;
    }
    let count = read_u32(&mut input)? as usize;
    let mut profiles = Vec::with_capacity(count);
    for _ in 0..count {
        let encoded_hash = read_u64(&mut input)?;
        let encoded_len = read_u32(&mut input)?;
        let skipped_samples = read_u32(&mut input)?;
        let output_samples = read_u32(&mut input)?;
        let correction_count = read_u32(&mut input)? as usize;
        let mut corrections = Vec::with_capacity(correction_count);
        for _ in 0..correction_count {
            corrections.push((read_u32(&mut input)?, read_i16(&mut input)?));
        }
        profiles.push(Mpg123Profile {
            encoded_hash,
            encoded_len,
            skipped_samples,
            output_samples,
            corrections: corrections.into_boxed_slice(),
        });
    }
    input.is_empty().then_some(profiles)
}

fn decode_base64(encoded: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(encoded.len() / 4 * 3);
    let mut quantum = [0u8; 4];
    let mut quantum_len = 0usize;
    for byte in encoded.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
        quantum[quantum_len] = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => 64,
            _ => return None,
        };
        quantum_len += 1;
        if quantum_len == 4 {
            if quantum[0] >= 64 || quantum[1] >= 64 {
                return None;
            }
            output.push((quantum[0] << 2) | (quantum[1] >> 4));
            if quantum[2] < 64 {
                output.push((quantum[1] << 4) | (quantum[2] >> 2));
                if quantum[3] < 64 {
                    output.push((quantum[2] << 6) | quantum[3]);
                }
            }
            quantum_len = 0;
        }
    }
    (quantum_len == 0).then_some(output)
}

fn take<'a>(input: &mut &'a [u8], count: usize) -> Option<&'a [u8]> {
    let (head, tail) = input.split_at_checked(count)?;
    *input = tail;
    Some(head)
}

fn read_u32(input: &mut &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(take(input, 4)?.try_into().ok()?))
}

fn read_u64(input: &mut &[u8]) -> Option<u64> {
    Some(u64::from_le_bytes(take(input, 8)?.try_into().ok()?))
}

fn read_i16(input: &mut &[u8]) -> Option<i16> {
    Some(i16::from_le_bytes(take(input, 2)?.try_into().ok()?))
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_table_contains_every_shipped_mp3() {
        assert_eq!(profiles().len(), 538);
        assert_eq!(
            profiles()
                .iter()
                .map(|profile| profile.corrections.len())
                .sum::<usize>(),
            19_175
        );
        assert_eq!(
            profiles()
                .iter()
                .filter(|profile| profile.skipped_samples == 529)
                .count(),
            334
        );
    }

    #[test]
    fn unknown_stream_uses_native_fcvtzs_fallback() {
        let mut compatibility = Mpg123Compatibility::new(b"not a shipped MP3");
        assert!(!compatibility.has_native_profile());
        let mut samples = [
            532.9 / 32_768.0,
            -532.9 / 32_768.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ]
        .into_iter();
        assert_eq!(compatibility.next_sample(&mut samples), Some(532));
        assert_eq!(compatibility.next_sample(&mut samples), Some(-532));
        assert_eq!(compatibility.next_sample(&mut samples), Some(i16::MAX));
        assert_eq!(compatibility.next_sample(&mut samples), Some(i16::MAX));
        assert_eq!(compatibility.next_sample(&mut samples), Some(i16::MIN));
        assert_eq!(compatibility.next_sample(&mut samples), None);
    }
}
