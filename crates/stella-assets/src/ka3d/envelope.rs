use crate::AssetError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ka3dEnvelope<'a> {
    pub container_type: [u8; 4],
    pub resource_type: [u8; 4],
    pub payload: &'a [u8],
}

impl<'a> Ka3dEnvelope<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, AssetError> {
        if bytes.len() < 16 {
            return Err(AssetError::InvalidKa3d("header is truncated"));
        }
        if &bytes[..4] != b"KA3D" && &bytes[..4] != b"RVIO" {
            return Err(AssetError::InvalidKa3d("root tag is not KA3D/RVIO"));
        }
        let container_type = bytes[..4].try_into().unwrap();
        let root_len = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
        if root_len > bytes.len() - 8 {
            return Err(AssetError::InvalidKa3d(
                "root length exceeds remaining bytes",
            ));
        }
        let resource_type = bytes[8..12].try_into().unwrap();
        let resource_len = u32::from_be_bytes(bytes[12..16].try_into().unwrap()) as usize;
        if resource_len > bytes.len() - 16 {
            return Err(AssetError::InvalidKa3d(
                "resource length exceeds remaining bytes",
            ));
        }
        Ok(Self {
            container_type,
            resource_type,
            payload: &bytes[16..16 + resource_len],
        })
    }

    /// Find a declared chunk by tag for host-side catalog classification.
    /// Format-specific loaders still use the native physical reader because
    /// Purple ignores the declared length of chunks it recognizes.
    pub fn find(bytes: &'a [u8], expected: &[u8; 4]) -> Result<Self, AssetError> {
        if bytes.len() < 8 {
            return Err(AssetError::InvalidKa3d("header is truncated"));
        }
        if &bytes[..4] != b"KA3D" && &bytes[..4] != b"RVIO" {
            return Err(AssetError::InvalidKa3d("root tag is not KA3D/RVIO"));
        }
        let container_type = bytes[..4].try_into().unwrap();
        let root_len = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
        if root_len > bytes.len() - 8 {
            return Err(AssetError::InvalidKa3d(
                "root length exceeds remaining bytes",
            ));
        }
        let mut offset = 8usize;
        while offset < bytes.len() {
            let header_end = offset
                .checked_add(8)
                .ok_or(AssetError::InvalidKa3d("KA3D offset overflow"))?;
            let header = bytes
                .get(offset..header_end)
                .ok_or(AssetError::InvalidKa3d("KA3D chunk header is truncated"))?;
            let resource_type = header[..4].try_into().unwrap();
            let resource_len = u32::from_be_bytes(header[4..8].try_into().unwrap()) as usize;
            let payload_end = header_end
                .checked_add(resource_len)
                .ok_or(AssetError::InvalidKa3d("KA3D offset overflow"))?;
            let payload = bytes
                .get(header_end..payload_end)
                .ok_or(AssetError::InvalidKa3d(
                    "resource length exceeds remaining bytes",
                ))?;
            if &resource_type == expected {
                return Ok(Self {
                    container_type,
                    resource_type,
                    payload,
                });
            }
            offset = payload_end;
        }
        Err(AssetError::InvalidKa3d("resource chunk was not found"))
    }

    pub fn container_type_str(&self) -> &str {
        std::str::from_utf8(&self.container_type).unwrap_or("????")
    }

    pub fn resource_type_str(&self) -> &str {
        std::str::from_utf8(&self.resource_type).unwrap_or("????")
    }
}
