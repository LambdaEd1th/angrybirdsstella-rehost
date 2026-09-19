//! Portable equivalent of pf::UUID's CFUUIDCreate/CreateString pair.

pub(crate) fn generate_uuid_v4() -> Result<String, getrandom::Error> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes)?;
    Ok(uuid_v4_from_bytes(bytes))
}

fn uuid_v4_from_bytes(mut bytes: [u8; 16]) -> String {
    use std::fmt::Write as _;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let mut value = String::with_capacity(36);
    for (index, byte) in bytes.into_iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            value.push('-');
        }
        write!(value, "{byte:02X}").expect("writing a UUID to String cannot fail");
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_bytes_keep_native_text_case_and_version_variant_bits() {
        assert_eq!(
            uuid_v4_from_bytes([0; 16]),
            "00000000-0000-4000-8000-000000000000"
        );
        assert_eq!(
            uuid_v4_from_bytes([255; 16]),
            "FFFFFFFF-FFFF-4FFF-BFFF-FFFFFFFFFFFF"
        );
    }
}
