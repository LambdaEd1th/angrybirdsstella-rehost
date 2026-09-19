//! Purple 1.1.6 encrypted-file core and fusion.registry format wrapper.
//!
//! AES-256-CBC with a zero IV and native PadMode 1. This is legacy format
//! compatibility, not authenticated encryption or secure credential storage.
//! No function logs, formats, or retains an input in an error.

use aes::{
    Aes256,
    cipher::{BlockModeDecrypt, BlockModeEncrypt, KeyIvInit, block_padding::NoPadding},
};
use std::fmt;

// The fixed file-format key is assembled as four little-endian words at
// 10056289C..1005628D8 and again at 100562FB8..100562FF4. It is not a user's
// credential. Keep it private and out of diagnostics/documentation.
const REGISTRY_KEY: [u8; 32] = [
    0x3a, 0x7d, 0x2e, 0x03, 0x79, 0xe6, 0x49, 0x85, 0xa0, 0x1f, 0xa8, 0x01, 0x04, 0xd5, 0xd7, 0x7d,
    0xa1, 0xbc, 0x7a, 0xe7, 0x03, 0x63, 0x24, 0x8e, 0x7a, 0xc9, 0xc0, 0xad, 0x5f, 0x46, 0x60, 0xea,
];
const ZERO_IV: [u8; 16] = [0; 16];
const BLOCK_SIZE: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RegistryCodecError {
    InvalidLength,
    InvalidPadding,
    RandomUnavailable,
    SizeOverflow,
}

impl fmt::Display for RegistryCodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidLength => "encrypted file ciphertext length is invalid",
            Self::InvalidPadding => "encrypted file padding is invalid",
            Self::RandomUnavailable => "encrypted file padding randomness is unavailable",
            Self::SizeOverflow => "encrypted file data size is unsupported",
        })
    }
}

impl std::error::Error for RegistryCodecError {}

/// The empty-input special case belongs to wrapper 100557F40, not the raw
/// AES routine. The native padding check deliberately accepts a final zero,
/// and checks neither the random fill nor equal-valued PKCS#7 fill.
pub(super) fn decode_registry(ciphertext: &[u8]) -> Result<Vec<u8>, RegistryCodecError> {
    decode_with_key(ciphertext, &REGISTRY_KEY)
}

pub(super) fn decode_with_key(
    ciphertext: &[u8],
    key: &[u8; 32],
) -> Result<Vec<u8>, RegistryCodecError> {
    if ciphertext.is_empty() {
        return Ok(Vec::new());
    }
    if !ciphertext.len().is_multiple_of(BLOCK_SIZE) {
        return Err(RegistryCodecError::InvalidLength);
    }
    let mut plaintext = cbc::Decryptor::<Aes256>::new(key.into(), (&ZERO_IV).into())
        .decrypt_padded_vec::<NoPadding>(ciphertext)
        .map_err(|_| RegistryCodecError::InvalidLength)?;
    let padding = usize::from(*plaintext.last().expect("nonempty aligned input"));
    // 1005585C4..1005585E8: last-byte <=16, then SUB W11,W10,W8 / signed
    // negative check. Keep the low-W wrap instead of inventing a large-buffer
    // acceptance rule. The disk adapter supplies its own resource-size limit.
    let remaining_low = (plaintext.len() as u32).wrapping_sub(padding as u32) as i32;
    if padding > BLOCK_SIZE || remaining_low < 0 || padding > plaintext.len() {
        return Err(RegistryCodecError::InvalidPadding);
    }
    plaintext.truncate(plaintext.len() - padding);
    Ok(plaintext)
}

/// Native encrypt wrapper 100557F1C skips empty input. Nonempty aligned data
/// receives an additional complete padding block; there is no IV/header/tag.
pub(super) fn encode_registry(plaintext: &[u8]) -> Result<Vec<u8>, RegistryCodecError> {
    encode_with_key(plaintext, &REGISTRY_KEY)
}

fn fill_host_padding(padding: &mut [u8]) -> Result<(), RegistryCodecError> {
    if padding.is_empty() {
        return Ok(());
    }
    // 1005582A0 uses rand()%255, so every fill byte is in 0..=254.
    // OS randomness replaces the process-global libc rand sequence explicitly;
    // the resulting bytes remain accepted by the original decoder.
    let mut random = [0_u8; 4 * (BLOCK_SIZE - 1)];
    getrandom::fill(&mut random[..4 * padding.len()])
        .map_err(|_| RegistryCodecError::RandomUnavailable)?;
    for (output, bytes) in padding.iter_mut().zip(random.as_chunks::<4>().0) {
        let random_word = u32::from_le_bytes(*bytes);
        *output = ((random_word & i32::MAX as u32) % 255) as u8;
    }
    Ok(())
}

pub(super) fn encode_with_key(
    plaintext: &[u8],
    key: &[u8; 32],
) -> Result<Vec<u8>, RegistryCodecError> {
    encode_with_key_and_padding(plaintext, key, fill_host_padding)
}

#[cfg(test)]
fn encode_with_padding(
    plaintext: &[u8],
    fill_padding: impl FnOnce(&mut [u8]) -> Result<(), RegistryCodecError>,
) -> Result<Vec<u8>, RegistryCodecError> {
    encode_with_key_and_padding(plaintext, &REGISTRY_KEY, fill_padding)
}

fn encode_with_key_and_padding(
    plaintext: &[u8],
    key: &[u8; 32],
    fill_padding: impl FnOnce(&mut [u8]) -> Result<(), RegistryCodecError>,
) -> Result<Vec<u8>, RegistryCodecError> {
    if plaintext.is_empty() {
        return Ok(Vec::new());
    }
    let padding = BLOCK_SIZE - plaintext.len() % BLOCK_SIZE;
    let padded_len = plaintext
        .len()
        .checked_add(padding)
        .ok_or(RegistryCodecError::SizeOverflow)?;
    let mut padded = Vec::new();
    padded
        .try_reserve_exact(padded_len)
        .map_err(|_| RegistryCodecError::SizeOverflow)?;
    padded.extend_from_slice(plaintext);
    padded.resize(padded_len, 0);
    fill_padding(&mut padded[plaintext.len()..padded_len - 1])?;
    padded[padded_len - 1] = padding as u8;
    Ok(cbc::Encryptor::<Aes256>::new(key.into(), (&ZERO_IV).into())
        .encrypt_padded_vec::<NoPadding>(&padded))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: [u8; 32] = [
        0xeb, 0x10, 0x3a, 0xa0, 0x33, 0x87, 0xd7, 0x30, 0xea, 0xa8, 0xa3, 0x12, 0xd4, 0x52, 0x1d,
        0xc2, 0x41, 0xa9, 0x2c, 0x38, 0x72, 0xca, 0x46, 0xdf, 0x70, 0x95, 0xd2, 0x06, 0xd6, 0xdf,
        0xd6, 0xe5,
    ];

    fn raw_encrypt(plaintext: &[u8]) -> Vec<u8> {
        assert!(plaintext.len().is_multiple_of(BLOCK_SIZE));
        cbc::Encryptor::<Aes256>::new((&REGISTRY_KEY).into(), (&ZERO_IV).into())
            .encrypt_padded_vec::<NoPadding>(plaintext)
    }

    fn raw_decrypt(ciphertext: &[u8]) -> Vec<u8> {
        cbc::Decryptor::<Aes256>::new((&REGISTRY_KEY).into(), (&ZERO_IV).into())
            .decrypt_padded_vec::<NoPadding>(ciphertext)
            .unwrap()
    }

    #[test]
    fn registry_codec_matches_independent_openssl_fixture() {
        // Independently computed with OpenSSL AES-256-CBC / zero IV / no
        // automatic padding: 16 ASCII input bytes, fill 0..14, final byte 16.
        // A fixed expected ciphertext also catches key byte-order and CBC/ECB
        // mistakes that a same-implementation round trip would conceal.
        let ciphertext = encode_with_padding(b"0123456789abcdef", |padding| {
            for (index, byte) in padding.iter_mut().enumerate() {
                *byte = index as u8;
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(ciphertext, FIXTURE);
        assert_eq!(decode_registry(&FIXTURE).unwrap(), b"0123456789abcdef");
    }

    #[test]
    fn registry_codec_covers_every_native_padding_length_including_zero() {
        for padding in 0_u8..=16 {
            let mut padded = [0xA5; 32];
            padded[31] = padding;
            let ciphertext = raw_encrypt(&padded);
            let decoded = decode_registry(&ciphertext).unwrap();
            assert_eq!(
                decoded,
                padded[..32 - usize::from(padding)],
                "padding {padding}"
            );
        }
        // The original does not inspect any preceding padding byte. This
        // intentionally accepts a non-PKCS#7 block and full-block removal.
        let mut padded = [0xFF; 16];
        padded[15] = 16;
        assert!(decode_registry(&raw_encrypt(&padded)).unwrap().is_empty());
    }

    #[test]
    fn registry_codec_encoder_uses_exact_length_fill_and_terminal_count() {
        for length in 1..=64 {
            let plaintext = vec![0x42; length];
            let padding = 16 - length % 16;
            let ciphertext = encode_with_padding(&plaintext, |fill| {
                assert_eq!(fill.len(), padding - 1);
                fill.fill(0xA7);
                Ok(())
            })
            .unwrap();
            assert_eq!(ciphertext.len(), length + padding);
            let padded = raw_decrypt(&ciphertext);
            assert_eq!(&padded[..length], &plaintext);
            assert!(
                padded[length..padded.len() - 1]
                    .iter()
                    .all(|&byte| byte == 0xA7)
            );
            assert_eq!(padded.last(), Some(&(padding as u8)));
            assert_eq!(decode_registry(&ciphertext).unwrap(), plaintext);
        }
    }

    #[test]
    fn registry_codec_empty_wrappers_do_not_require_aes_or_randomness() {
        assert!(decode_registry(&[]).unwrap().is_empty());
        assert!(encode_registry(&[]).unwrap().is_empty());
        assert!(
            encode_with_padding(&[], |_| {
                panic!("empty wrapper must not ask for random padding")
            })
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn registry_codec_rejects_misalignment_and_out_of_range_last_bytes() {
        for length in [1, 15, 17, 31, 33] {
            assert_eq!(
                decode_registry(&vec![0; length]),
                Err(RegistryCodecError::InvalidLength)
            );
        }
        for padding in [17, 31, 127, 128, 255] {
            let mut padded = [0; 16];
            padded[15] = padding;
            assert_eq!(
                decode_registry(&raw_encrypt(&padded)),
                Err(RegistryCodecError::InvalidPadding)
            );
        }
    }

    #[test]
    fn registry_codec_host_random_fill_is_native_compatible() {
        let ciphertext = encode_registry(b"registry fixture").unwrap();
        let padded = raw_decrypt(&ciphertext);
        assert_eq!(&padded[..16], b"registry fixture");
        assert!(padded[16..31].iter().all(|&byte| byte < 255));
        assert_eq!(padded[31], 16);
        assert_eq!(decode_registry(&ciphertext).unwrap(), b"registry fixture");
    }

    #[test]
    fn registry_codec_random_failure_is_state_only() {
        assert_eq!(
            encode_with_padding(b"synthetic secret", |_| Err(
                RegistryCodecError::RandomUnavailable
            )),
            Err(RegistryCodecError::RandomUnavailable),
        );
        for error in [
            RegistryCodecError::InvalidLength,
            RegistryCodecError::InvalidPadding,
            RegistryCodecError::RandomUnavailable,
            RegistryCodecError::SizeOverflow,
        ] {
            assert!(!format!("{error:?} {error}").contains("synthetic secret"));
        }
    }
}
