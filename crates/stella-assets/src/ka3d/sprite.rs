use crate::AssetError;

use super::reader::NativeContainerReader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteSheet {
    pub textures: Vec<String>,
    pub sprites: Vec<SpriteRegion>,
    /// Texture pointer selected when each native Sprite was constructed.
    /// Purple may encounter more than one SPRT chunk and replaces the sheet's
    /// current texture before constructing the following chunk's sprites.
    pub sprite_texture_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteRegion {
    pub name: String,
    pub x: i16,
    pub y: i16,
    pub width: i16,
    pub height: i16,
    pub pivot_x: i16,
    pub pivot_y: i16,
    /// Atlas UV permutation selected by the final Sprite constructor
    /// argument. Binary SPRT records use zero; TexturePacker's `rotated`
    /// boolean maps to one. The constructor also recognizes two and three.
    pub atlas_rotation: u8,
}

impl SpriteRegion {
    /// Four atlas-space corners cached by `Sprite::Sprite` at offsets
    /// `+0x34..+0x4c`, in native top-left, top-right, bottom-left,
    /// bottom-right vertex order.
    pub fn native_atlas_corners(&self) -> [[f32; 2]; 4] {
        let left = f32::from(self.x);
        let top = f32::from(self.y);
        let (right, bottom) = if self.atlas_rotation == 1 {
            (left + f32::from(self.height), top + f32::from(self.width))
        } else {
            (left + f32::from(self.width), top + f32::from(self.height))
        };
        match self.atlas_rotation {
            1 => [[right, top], [right, bottom], [left, top], [left, bottom]],
            2 => [[right, top], [left, top], [right, bottom], [left, bottom]],
            3 => [[left, bottom], [right, bottom], [left, top], [right, top]],
            _ => [[left, top], [right, top], [left, bottom], [right, bottom]],
        }
    }

    pub fn native_uvs(&self, texture_width: f32, texture_height: f32) -> [[f32; 2]; 4] {
        let texture_width = texture_width.max(1.0);
        let texture_height = texture_height.max(1.0);
        self.native_atlas_corners()
            .map(|[x, y]| [x / texture_width, y / texture_height])
    }
}

impl SpriteSheet {
    pub fn texture_for(&self, sprite: &SpriteRegion) -> Option<&str> {
        let index = self
            .sprites
            .iter()
            .position(|candidate| candidate.name == sprite.name)?;
        let texture_index = *self.sprite_texture_indices.get(index)?;
        self.textures.get(texture_index).map(String::as_str)
    }

    pub fn current_texture(&self) -> Option<&str> {
        self.textures.last().map(String::as_str)
    }

    /// Parse the big-endian `SPRT` payload used by Stella's atlas metadata.
    pub fn parse(bytes: &[u8]) -> Result<Self, AssetError> {
        let mut container = NativeContainerReader::parse(bytes)?;
        if container.container_type() != b"KA3D" {
            return Err(AssetError::InvalidKa3d("SPRT root is not KA3D"));
        }
        let mut found = false;
        let mut textures = Vec::new();
        let mut sprites: Vec<SpriteRegion> = Vec::new();
        let mut sprite_texture_indices = Vec::new();
        while let Some(chunk) = container.next_chunk()? {
            if &chunk.tag != b"SPRT" {
                container.skip(chunk.declared_len)?;
                continue;
            }
            found = true;
            let reader = container.body();
            if reader.u16()? != 1 {
                continue;
            }
            let texture_index = textures.len();
            textures.push(reader.string()?);
            let sprite_count = reader.u16()? as usize;
            for _ in 0..sprite_count {
                let sprite = SpriteRegion {
                    name: reader.string()?,
                    x: reader.i16()?,
                    y: reader.i16()?,
                    width: reader.i16()?,
                    height: reader.i16()?,
                    pivot_x: reader.i16()?,
                    pivot_y: reader.i16()?,
                    atlas_rotation: 0,
                };
                if let Some(index) = sprites
                    .iter()
                    .position(|existing| existing.name == sprite.name)
                {
                    sprites[index] = sprite;
                    sprite_texture_indices[index] = texture_index;
                } else {
                    sprites.push(sprite);
                    sprite_texture_indices.push(texture_index);
                }
            }
        }
        if !found {
            return Err(AssetError::InvalidKa3d("resource is not an SPRT sheet"));
        }
        Ok(Self {
            textures,
            sprites,
            sprite_texture_indices,
        })
    }
}
