//! Parser for the portable part of a Lua 5.1 binary chunk header.

use crate::AssetError;

pub const LUA_SIGNATURE: [u8; 4] = [0x1b, b'L', b'u', b'a'];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LuaChunkHeader {
    pub version: u8,
    pub format: u8,
    pub little_endian: bool,
    pub int_size: u8,
    pub size_t_size: u8,
    pub instruction_size: u8,
    pub number_size: u8,
    pub integral_numbers: bool,
}

impl LuaChunkHeader {
    pub const SIZE: usize = 12;

    pub fn parse(bytes: &[u8]) -> Result<Self, AssetError> {
        if bytes.len() < Self::SIZE {
            return Err(AssetError::InvalidLua("header is truncated"));
        }
        if bytes[..4] != LUA_SIGNATURE {
            return Err(AssetError::InvalidLua("signature mismatch"));
        }
        if bytes[4] != 0x51 {
            return Err(AssetError::InvalidLua("only Lua 5.1 chunks are supported"));
        }
        Ok(Self {
            version: bytes[4],
            format: bytes[5],
            little_endian: bytes[6] == 1,
            int_size: bytes[7],
            size_t_size: bytes[8],
            instruction_size: bytes[9],
            number_size: bytes[10],
            integral_numbers: bytes[11] != 0,
        })
    }
}

/// Convert Purple's portable Lua chunk representation (32-bit string lengths
/// and 32-bit floating-point numbers) into the Lua 5.1 representation expected
/// by the current host. VM instructions retain their 32-bit values while every
/// serialized scalar is re-encoded in the host's byte order and string lengths
/// and numeric constants are widened to the host ABI.
pub fn transcode_for_host(bytes: &[u8]) -> Result<Vec<u8>, AssetError> {
    transcode_for_abi(
        bytes,
        cfg!(target_endian = "little"),
        size_of::<usize>() as u8,
    )
}

fn transcode_for_abi(
    bytes: &[u8],
    target_little_endian: bool,
    target_size_t_size: u8,
) -> Result<Vec<u8>, AssetError> {
    let header = LuaChunkHeader::parse(bytes)?;
    if header.format != 0
        || !header.little_endian
        || header.int_size != 4
        || header.instruction_size != 4
        || header.size_t_size != 4
        || header.number_size != 4
        || header.integral_numbers
    {
        return Err(AssetError::InvalidLua(
            "unsupported source chunk representation",
        ));
    }
    if !matches!(target_size_t_size, 4 | 8) {
        return Err(AssetError::InvalidLua(
            "unsupported target size_t representation",
        ));
    }

    let mut reader = Reader::new(&bytes[LuaChunkHeader::SIZE..]);
    let mut output = Vec::with_capacity(bytes.len() + bytes.len() / 4);
    output.extend_from_slice(&LUA_SIGNATURE);
    output.extend_from_slice(&[
        0x51,
        0,
        u8::from(target_little_endian),
        4,
        target_size_t_size,
        4,
        8,
        0,
    ]);
    let target = TargetAbi {
        little_endian: target_little_endian,
        size_t_size: target_size_t_size,
    };
    transcode_function(&mut reader, &mut output, target)?;
    if !reader.remaining().is_empty() {
        return Err(AssetError::InvalidLua("trailing bytes after root function"));
    }
    Ok(output)
}

/// Return a chunk that can be loaded by Lua 5.1 on this host. Chunks already
/// matching the host representation are left untouched.
pub fn prepare_for_host(bytes: &[u8]) -> Result<Vec<u8>, AssetError> {
    let header = LuaChunkHeader::parse(bytes)?;
    if header.format == 0
        && header.little_endian == cfg!(target_endian = "little")
        && header.int_size == 4
        && header.size_t_size as usize == size_of::<usize>()
        && header.instruction_size == 4
        && header.number_size == 8
        && !header.integral_numbers
    {
        Ok(bytes.to_vec())
    } else {
        transcode_for_host(bytes)
    }
}

#[derive(Clone, Copy)]
struct TargetAbi {
    little_endian: bool,
    size_t_size: u8,
}

fn transcode_function(
    reader: &mut Reader<'_>,
    output: &mut Vec<u8>,
    target: TargetAbi,
) -> Result<(), AssetError> {
    transcode_string(reader, output, target)?;
    transcode_u32(reader, output, target)?; // line defined
    transcode_u32(reader, output, target)?; // last line defined
    output.extend_from_slice(reader.read(4)?); // nups, params, vararg, stack

    let code_count = read_count(reader)?;
    write_u32(output, code_count as u32, target);
    for _ in 0..code_count {
        transcode_u32(reader, output, target)?;
    }

    let constant_count = read_count(reader)?;
    write_u32(output, constant_count as u32, target);
    for _ in 0..constant_count {
        let tag = reader.byte()?;
        output.push(tag);
        match tag {
            0 => {}
            1 => output.push(reader.byte()?),
            3 => {
                let value = f32::from_le_bytes(reader.read(4)?.try_into().unwrap()) as f64;
                write_f64(output, value, target);
            }
            4 => transcode_string(reader, output, target)?,
            _ => return Err(AssetError::InvalidLua("unknown constant type tag")),
        }
    }

    let prototype_count = read_count(reader)?;
    write_u32(output, prototype_count as u32, target);
    for _ in 0..prototype_count {
        transcode_function(reader, output, target)?;
    }

    let line_count = read_count(reader)?;
    write_u32(output, line_count as u32, target);
    for _ in 0..line_count {
        transcode_u32(reader, output, target)?;
    }

    let local_count = read_count(reader)?;
    write_u32(output, local_count as u32, target);
    for _ in 0..local_count {
        transcode_string(reader, output, target)?;
        transcode_u32(reader, output, target)?;
        transcode_u32(reader, output, target)?;
    }

    let upvalue_count = read_count(reader)?;
    write_u32(output, upvalue_count as u32, target);
    for _ in 0..upvalue_count {
        transcode_string(reader, output, target)?;
    }
    Ok(())
}

fn transcode_string(
    reader: &mut Reader<'_>,
    output: &mut Vec<u8>,
    target: TargetAbi,
) -> Result<(), AssetError> {
    let length = reader.u32()? as usize;
    write_usize(output, length, target);
    output.extend_from_slice(reader.read(length)?);
    Ok(())
}

fn transcode_u32(
    reader: &mut Reader<'_>,
    output: &mut Vec<u8>,
    target: TargetAbi,
) -> Result<(), AssetError> {
    write_u32(output, reader.u32()?, target);
    Ok(())
}

fn read_count(reader: &mut Reader<'_>) -> Result<usize, AssetError> {
    Ok(reader.u32()? as usize)
}

fn write_u32(output: &mut Vec<u8>, value: u32, target: TargetAbi) {
    let bytes = if target.little_endian {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    };
    output.extend_from_slice(&bytes);
}

fn write_f64(output: &mut Vec<u8>, value: f64, target: TargetAbi) {
    let bytes = if target.little_endian {
        value.to_le_bytes()
    } else {
        value.to_be_bytes()
    };
    output.extend_from_slice(&bytes);
}

fn write_usize(output: &mut Vec<u8>, value: usize, target: TargetAbi) {
    if target.size_t_size == 4 {
        write_u32(output, value as u32, target);
    } else {
        let value = value as u64;
        let bytes = if target.little_endian {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        };
        output.extend_from_slice(&bytes);
    }
}

struct Reader<'a> {
    remaining: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn read(&mut self, length: usize) -> Result<&'a [u8], AssetError> {
        if self.remaining.len() < length {
            return Err(AssetError::InvalidLua("chunk is truncated"));
        }
        let (head, tail) = self.remaining.split_at(length);
        self.remaining = tail;
        Ok(head)
    }

    fn byte(&mut self) -> Result<u8, AssetError> {
        Ok(self.read(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, AssetError> {
        Ok(u32::from_le_bytes(self.read(4)?.try_into().unwrap()))
    }

    fn remaining(&self) -> &'a [u8] {
        self.remaining
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_purple_chunk_header() {
        let bytes = [0x1b, b'L', b'u', b'a', 0x51, 0, 1, 4, 4, 4, 4, 0];
        let header = LuaChunkHeader::parse(&bytes).unwrap();
        assert!(header.little_endian);
        assert_eq!(header.version, 0x51);
        assert_eq!(header.number_size, 4);
    }

    #[test]
    fn transcodes_an_empty_root_prototype() {
        let mut bytes = [0x1b, b'L', b'u', b'a', 0x51, 0, 1, 4, 4, 4, 4, 0].to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes()); // source name
        bytes.extend_from_slice(&0u32.to_le_bytes()); // first line
        bytes.extend_from_slice(&0u32.to_le_bytes()); // last line
        bytes.extend_from_slice(&[0, 0, 2, 2]);
        for _ in 0..6 {
            bytes.extend_from_slice(&0u32.to_le_bytes());
        }
        let output = transcode_for_host(&bytes).unwrap();
        assert_eq!(output[8], size_of::<usize>() as u8);
        assert_eq!(output[10], 8);
    }

    #[test]
    fn transcodes_every_scalar_field_for_a_big_endian_runtime() {
        let mut bytes = [0x1b, b'L', b'u', b'a', 0x51, 0, 1, 4, 4, 4, 4, 0].to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes()); // source name
        bytes.extend_from_slice(&0x0102_0304u32.to_le_bytes()); // first line
        bytes.extend_from_slice(&0x0506_0708u32.to_le_bytes()); // last line
        bytes.extend_from_slice(&[0, 0, 2, 2]);
        bytes.extend_from_slice(&1u32.to_le_bytes()); // instruction count
        bytes.extend_from_slice(&0x1122_3344u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes()); // constant count
        bytes.push(3); // number tag
        bytes.extend_from_slice(&1.5f32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // prototype count
        bytes.extend_from_slice(&1u32.to_le_bytes()); // line info count
        bytes.extend_from_slice(&0x99aa_bbccu32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes()); // local count
        bytes.extend_from_slice(&0u32.to_le_bytes()); // local name
        bytes.extend_from_slice(&0x0a0b_0c0du32.to_le_bytes());
        bytes.extend_from_slice(&0x0101_0101u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // upvalue count

        let output = transcode_for_abi(&bytes, false, 8).unwrap();
        assert_eq!(
            &output[..12],
            &[0x1b, b'L', b'u', b'a', 0x51, 0, 0, 4, 8, 4, 8, 0]
        );
        assert_eq!(&output[12..20], &0u64.to_be_bytes());
        assert_eq!(&output[20..24], &0x0102_0304u32.to_be_bytes());
        assert_eq!(&output[24..28], &0x0506_0708u32.to_be_bytes());
        assert_eq!(&output[32..36], &1u32.to_be_bytes());
        assert_eq!(&output[36..40], &0x1122_3344u32.to_be_bytes());
        assert_eq!(output[44], 3);
        assert_eq!(&output[45..53], &1.5f64.to_be_bytes());
        assert_eq!(&output[61..65], &0x99aa_bbccu32.to_be_bytes());
        assert_eq!(&output[69..77], &0u64.to_be_bytes());
        assert_eq!(&output[77..81], &0x0a0b_0c0du32.to_be_bytes());
        assert_eq!(&output[81..85], &0x0101_0101u32.to_be_bytes());
        assert_eq!(output.len(), 89);
    }
}
