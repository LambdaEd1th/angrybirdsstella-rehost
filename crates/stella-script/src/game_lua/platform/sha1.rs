//! Recovered SHA-1 implementation and uppercase Base16 rendering.

pub(crate) fn sha1_upper_hex(input: &[u8]) -> String {
    upper_hex(&sha1_digest(input))
}

pub(crate) fn sha1_digest(input: &[u8]) -> [u8; 20] {
    let bit_len = (input.len() as u64) * 8;
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    let mut hash = [
        0x6745_2301_u32,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let mut words = [0_u32; 80];
    for chunk in padded.as_chunks::<64>().0 {
        for (index, bytes) in chunk.as_chunks::<4>().0.iter().enumerate() {
            words[index] = u32::from_be_bytes(*bytes);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }

        let [mut a, mut b, mut c, mut d, mut e] = hash;
        for (index, word) in words.iter().copied().enumerate() {
            let (function, constant) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(function)
                .wrapping_add(e)
                .wrapping_add(constant)
                .wrapping_add(word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        for (slot, value) in hash.iter_mut().zip([a, b, c, d, e]) {
            *slot = slot.wrapping_add(value);
        }
    }

    let mut digest = [0; 20];
    for (bytes, word) in digest.as_chunks_mut::<4>().0.iter_mut().zip(hash) {
        bytes.copy_from_slice(&word.to_be_bytes());
    }
    digest
}

pub(crate) fn upper_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02X}").expect("writing hexadecimal to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_digest_preserves_padding_boundaries_and_multiple_blocks() {
        // Independent hashlib.sha1 vectors for repetitions of byte 0x61.
        for (length, expected) in [
            (0, "DA39A3EE5E6B4B0D3255BFEF95601890AFD80709"),
            (55, "C1C8BBDC22796E28C0E15163D20899B65621D65A"),
            (56, "C2DB330F6083854C99D4B5BFB6E8F29F201BE699"),
            (63, "03F09F5B158A7A8CDAD920BDDC29B81C18A551F5"),
            (64, "0098BA824B5C16427BD7A1122A5A442A25EC644D"),
            (65, "11655326C708D70319BE2610E8A57D9A5B959D3B"),
            (1000, "291E9A6C66994949B57BA5E650361E98FC36B1BA"),
        ] {
            assert_eq!(sha1_upper_hex(&vec![b'a'; length]), expected);
        }
    }
}
