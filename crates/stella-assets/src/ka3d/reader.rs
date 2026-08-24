use crate::AssetError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ChunkHeader {
    pub(super) tag: [u8; 4],
    pub(super) declared_len: usize,
}

/// Purple's KA3D loaders keep one reader over the physical file. The root
/// length is validated as an upper bound, but does not create a bounded child
/// reader. Known chunks likewise consume their fields directly; only unknown
/// chunks use their declared length to seek forward.
pub(super) struct NativeContainerReader<'a> {
    container_type: [u8; 4],
    body: BeReader<'a>,
}

impl<'a> NativeContainerReader<'a> {
    pub(super) fn parse(bytes: &'a [u8]) -> Result<Self, AssetError> {
        let mut reader = BeReader::new(bytes);
        let container_type = reader.tag()?;
        if &container_type != b"KA3D" && &container_type != b"RVIO" {
            return Err(AssetError::InvalidKa3d("root tag is not KA3D/RVIO"));
        }
        let declared_len = reader.u32()? as usize;
        if declared_len > reader.remaining_len() {
            return Err(AssetError::InvalidKa3d(
                "root length exceeds remaining bytes",
            ));
        }
        Ok(Self {
            container_type,
            body: reader,
        })
    }

    pub(super) fn container_type(&self) -> &[u8; 4] {
        &self.container_type
    }

    pub(super) fn next_chunk(&mut self) -> Result<Option<ChunkHeader>, AssetError> {
        if self.body.remaining_len() == 0 {
            return Ok(None);
        }
        Ok(Some(ChunkHeader {
            tag: self.body.tag()?,
            declared_len: self.body.u32()? as usize,
        }))
    }

    pub(super) fn body(&mut self) -> &mut BeReader<'a> {
        &mut self.body
    }

    pub(super) fn skip(&mut self, length: usize) -> Result<(), AssetError> {
        self.body.skip(length)
    }
}

pub(super) struct BeReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> BeReader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn u8(&mut self) -> Result<u8, AssetError> {
        let value = *self
            .bytes
            .get(self.offset)
            .ok_or(AssetError::InvalidKa3d("KA3D byte is truncated"))?;
        self.offset += 1;
        Ok(value)
    }

    pub(super) fn tag(&mut self) -> Result<[u8; 4], AssetError> {
        let end = self
            .offset
            .checked_add(4)
            .ok_or(AssetError::InvalidKa3d("KA3D offset overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(AssetError::InvalidKa3d("KA3D chunk tag is truncated"))?;
        self.offset = end;
        Ok(bytes.try_into().unwrap())
    }

    pub(super) fn u16(&mut self) -> Result<u16, AssetError> {
        let end = self
            .offset
            .checked_add(2)
            .ok_or(AssetError::InvalidKa3d("KA3D offset overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(AssetError::InvalidKa3d("KA3D u16 is truncated"))?;
        self.offset = end;
        Ok(u16::from_be_bytes(bytes.try_into().unwrap()))
    }

    pub(super) fn u32(&mut self) -> Result<u32, AssetError> {
        let end = self
            .offset
            .checked_add(4)
            .ok_or(AssetError::InvalidKa3d("KA3D offset overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(AssetError::InvalidKa3d("KA3D u32 is truncated"))?;
        self.offset = end;
        Ok(u32::from_be_bytes(bytes.try_into().unwrap()))
    }

    pub(super) fn i16(&mut self) -> Result<i16, AssetError> {
        Ok(self.u16()? as i16)
    }

    pub(super) fn f32(&mut self) -> Result<f32, AssetError> {
        let end = self
            .offset
            .checked_add(4)
            .ok_or(AssetError::InvalidKa3d("KA3D offset overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(AssetError::InvalidKa3d("COMP float is truncated"))?;
        self.offset = end;
        Ok(f32::from_be_bytes(bytes.try_into().unwrap()))
    }

    pub(super) fn string(&mut self) -> Result<String, AssetError> {
        let length = self.u16()? as usize;
        let end = self
            .offset
            .checked_add(length)
            .ok_or(AssetError::InvalidKa3d("KA3D offset overflow"))?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or(AssetError::InvalidKa3d("KA3D string is truncated"))?;
        self.offset = end;
        String::from_utf8(bytes.to_vec())
            .map_err(|_| AssetError::InvalidKa3d("KA3D string is not UTF-8"))
    }

    pub(super) fn skip(&mut self, length: usize) -> Result<(), AssetError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(AssetError::InvalidKa3d("KA3D offset overflow"))?;
        if end > self.bytes.len() {
            return Err(AssetError::InvalidKa3d("KA3D chunk payload is truncated"));
        }
        self.offset = end;
        Ok(())
    }

    pub(super) fn remaining_len(&self) -> usize {
        self.bytes.len() - self.offset
    }
}
