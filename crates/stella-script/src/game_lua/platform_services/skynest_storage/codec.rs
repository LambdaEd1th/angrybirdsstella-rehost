//! Storage SDKv2: URL-safe padded Base64 over a 13-byte LZMA1 envelope.
//!
//! Purple 1.1.6: 0x1006FF040 / 0x100703708 select the codec; 0x1006617E0
//! writes the five properties bytes, little-endian input length, and LZMA payload.
//! Resource limits and explicit malformed-input errors are host safety boundaries.

use base64::{Engine as _, engine::general_purpose::URL_SAFE};
use lzma_rust2::{EncodeMode, LzmaOptions, LzmaReader, LzmaWriter, MfType};
use std::io::{self, Cursor, Read, Write};

const MAX_VALUE_BYTES: usize = 8 * 1024 * 1024;
const MAX_ENCODED_BYTES: usize = 8 * 1024 * 1024;
const MAX_COMPRESSED_BYTES: usize = MAX_ENCODED_BYTES / 4 * 3;
const DECODER_MEMORY_KIB: u32 = 16 * 1024;
const HEADER_BYTES: usize = 13;
const TOO_LARGE: &str = "Storage value exceeds codec resource limits";
const INVALID_VALUE: &str = "Invalid compressed storage value";
const ENCODE_FAILED: &str = "Storage value compression failed";

pub(super) fn encode(value: &str) -> Result<String, &'static str> {
    if value.len() > MAX_VALUE_BYTES {
        return Err(TOO_LARGE);
    }
    let compressed = compress(value.as_bytes())?;
    Ok(URL_SAFE.encode(compressed))
}

pub(super) fn decode(value: &str, encoding: &str) -> Result<String, &'static str> {
    // Native tests only this exact spelling; every other label takes the codec path.
    if encoding == "SDKv1" {
        return if value.len() <= MAX_VALUE_BYTES {
            Ok(value.to_owned())
        } else {
            Err(TOO_LARGE)
        };
    }
    if value.len() > MAX_ENCODED_BYTES {
        return Err(TOO_LARGE);
    }
    let compressed = URL_SAFE.decode(value).map_err(|_| INVALID_VALUE)?;
    if compressed.len() < HEADER_BYTES + 5 {
        return Err(INVALID_VALUE);
    }
    let declared_size = u64::from_le_bytes(
        compressed[5..HEADER_BYTES]
            .try_into()
            .map_err(|_| INVALID_VALUE)?,
    );
    // SDKv2 always stores a known size. Do not allocate from untrusted u64 headers
    // or accept the generic LZMA "unknown length" marker as an unbounded stream.
    if declared_size > MAX_VALUE_BYTES as u64 {
        return Err(TOO_LARGE);
    }
    let reader = LzmaReader::new_mem_limit(Cursor::new(compressed), DECODER_MEMORY_KIB, None)
        .map_err(|_| INVALID_VALUE)?;
    let mut decoded = Vec::new();
    decoded
        .try_reserve_exact(declared_size as usize)
        .map_err(|_| TOO_LARGE)?;
    reader
        .take(declared_size + 1)
        .read_to_end(&mut decoded)
        .map_err(|_| INVALID_VALUE)?;
    if decoded.len() as u64 != declared_size {
        return Err(INVALID_VALUE);
    }
    String::from_utf8(decoded).map_err(|_| INVALID_VALUE)
}

fn compress(value: &[u8]) -> Result<Vec<u8>, &'static str> {
    // PropsInit(level=5) + the SDK's 16-KiB dictionary override, normalized by
    // 0x1004E94E8: lc=3, lp=0, pb=2, normal/Bt4, nice length=32, depth=32.
    let options = LzmaOptions::new(0x4000, 3, 0, 2, EncodeMode::Normal, 32, MfType::Bt4, 32);
    let output = BoundedOutput {
        bytes: Vec::new(),
        limit: MAX_COMPRESSED_BYTES,
    };
    // The native LzmaEncode argument at 0x1006618B0 enables the end marker even
    // though the wrapper also records a known input size. new_use_header() would
    // suppress that marker for Some(size), so use the explicit constructor.
    let mut writer = LzmaWriter::new(output, &options, true, true, Some(value.len() as u64))
        .map_err(|_| ENCODE_FAILED)?;
    writer.write_all(value).map_err(|_| ENCODE_FAILED)?;
    writer
        .finish()
        .map(|output| output.bytes)
        .map_err(|_| ENCODE_FAILED)
}

struct BoundedOutput {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other(TOO_LARGE));
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(|_| io::Error::other(TOO_LARGE))?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE_TEXT: &str = "Synthetic storage fixture: hello, Stella! 12345";
    // Independent xz/liblzma raw LZMA1 fixture, not produced by this Rust encoder:
    // printf '%s' 'Synthetic storage fixture: hello, Stella! 12345' |
    // xz --format=raw --lzma1=dict=16KiB,lc=3,lp=0,pb=2,mode=normal,nice=32,mf=bt4,depth=32
    // Prefix 5d00400000 + 47u64 LE, then encode with the native URL-safe alphabet.
    const FIXTURE: &str = "XQBAAAAvAAAAAAAAAAApnknHd6rD4nTarNBfhvDMUxQGwUWLVjJEtyV14nW4cCV_65cbqeZ-WuJijlcxxMrMf1_-OXAA";

    #[test]
    fn sdkv1_is_exact_plaintext_passthrough() {
        for value in ["", "not base64", "{\"locale\":\"简体中文\"}", "a\0b"] {
            assert_eq!(decode(value, "SDKv1"), Ok(value.to_owned()));
        }
    }

    #[test]
    fn independent_lzma_fixture_decodes_and_unknown_labels_use_same_path() {
        for encoding in ["SDKv2", "", "sdkv1", "future-label"] {
            assert_eq!(decode(FIXTURE, encoding), Ok(FIXTURE_TEXT.to_owned()));
        }
        assert!(FIXTURE.contains('-') && FIXTURE.contains('_'));
    }

    #[test]
    fn sdkv2_header_and_text_roundtrips_match_native_envelope() {
        for value in ["", "a", "ab", "abc", "a\0b", "Stella 简体中文 日本語 🙂"] {
            let encoded = encode(value).unwrap();
            let bytes = URL_SAFE.decode(&encoded).unwrap();
            assert_eq!(&bytes[..5], &[0x5d, 0x00, 0x40, 0x00, 0x00]);
            assert_eq!(&bytes[5..13], &(value.len() as u64).to_le_bytes());
            assert_eq!(encoded.len() % 4, 0);
            assert_eq!(decode(&encoded, "SDKv2"), Ok(value.to_owned()));
        }
        let repeated = "state-data-".repeat(16 * 1024);
        assert_eq!(decode(&encode(&repeated).unwrap(), "SDKv2"), Ok(repeated));
    }

    #[test]
    fn rejects_malformed_base64_properties_and_truncated_payload() {
        for invalid in ["", "%%%", "XQBAAAAv", "not plain SDKv2 text"] {
            assert_eq!(decode(invalid, "SDKv2"), Err(INVALID_VALUE));
        }
        let fixture = URL_SAFE.decode(FIXTURE).unwrap();
        for length in [0, 5, 12, 13, 17, 20] {
            assert_eq!(
                decode(&URL_SAFE.encode(&fixture[..length]), "SDKv2"),
                Err(INVALID_VALUE)
            );
        }
        let mut invalid_props = fixture;
        invalid_props[0] = 0xff;
        assert_eq!(
            decode(&URL_SAFE.encode(invalid_props), "SDKv2"),
            Err(INVALID_VALUE)
        );
    }

    #[test]
    fn rejects_untrusted_size_and_dictionary_before_large_allocation() {
        for size in [(MAX_VALUE_BYTES as u64) + 1, u64::MAX] {
            let mut bytes = URL_SAFE.decode(FIXTURE).unwrap();
            bytes[5..13].copy_from_slice(&size.to_le_bytes());
            assert_eq!(decode(&URL_SAFE.encode(bytes), "SDKv2"), Err(TOO_LARGE));
        }
        for dictionary_size in [32 * 1024 * 1024_u32, u32::MAX] {
            let mut bytes = URL_SAFE.decode(FIXTURE).unwrap();
            bytes[1..5].copy_from_slice(&dictionary_size.to_le_bytes());
            assert_eq!(decode(&URL_SAFE.encode(bytes), "SDKv2"), Err(INVALID_VALUE));
        }
    }

    #[test]
    fn utf8_boundary_rejects_binary_payload_without_echoing_it() {
        let value = URL_SAFE.encode(compress(&[0xff, 0xfe, 0x00]).unwrap());
        assert_eq!(decode(&value, "SDKv2"), Err(INVALID_VALUE));
    }

    #[test]
    fn limits_plaintext_and_encoded_input() {
        let oversized = "x".repeat(MAX_VALUE_BYTES + 1);
        assert_eq!(encode(&oversized), Err(TOO_LARGE));
        assert_eq!(decode(&oversized, "SDKv1"), Err(TOO_LARGE));
        assert_eq!(decode(&oversized, "SDKv2"), Err(TOO_LARGE));
    }

    #[test]
    fn compressed_output_is_bounded_without_partial_appends() {
        let mut output = BoundedOutput {
            bytes: Vec::new(),
            limit: 4,
        };
        output.write_all(&[1, 2, 3]).unwrap();
        assert!(output.write_all(&[4, 5]).is_err());
        assert_eq!(output.bytes, [1, 2, 3]);
        output.write_all(&[4]).unwrap();
        assert!(output.write_all(&[5]).is_err());
        assert_eq!(output.bytes, [1, 2, 3, 4]);
    }
}
