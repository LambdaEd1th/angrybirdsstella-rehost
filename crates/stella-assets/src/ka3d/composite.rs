use crate::AssetError;

use super::reader::{BeReader, NativeContainerReader};

#[derive(Debug, Clone, PartialEq)]
pub struct CompositeSpriteSet {
    pub sprites: Vec<CompositeSprite>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompositeSprite {
    pub name: String,
    pub parts: Vec<CompositePart>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompositePart {
    /// Entry-map name. A non-empty RVIO/JSON id is retained as
    /// `atlas_sprite#id`, while atlas lookup uses the prefix before `#`.
    pub sprite: String,
    pub x: f32,
    pub y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    /// Native Entry fields at +56/+60. Loaders preserve JSON multipliers;
    /// RVIO flag bytes become exactly +1 or -1.
    pub flip_x: f32,
    pub flip_y: f32,
    /// Native Entry angle at +64, stored in radians.
    pub angle: f32,
    /// Initialized true by the binary loader and mutable through the native
    /// composite-entry Lua API.
    pub visible: bool,
}

impl CompositeSpriteSet {
    /// Parse the big-endian `COMP` payload used to assemble named sprites
    /// from atlas regions.
    pub fn parse(bytes: &[u8]) -> Result<Self, AssetError> {
        let mut container = NativeContainerReader::parse(bytes)?;
        let is_rvio = container.container_type() == b"RVIO";
        let mut found = false;
        let mut sprites: Vec<CompositeSprite> = Vec::new();
        while let Some(chunk) = container.next_chunk()? {
            if &chunk.tag != b"COMP" {
                container.skip(chunk.declared_len)?;
                continue;
            }
            found = true;
            let reader = container.body();
            let version = reader.u16()?;
            if (is_rvio && version == 0) || (!is_rvio && !matches!(version, 1 | 2)) {
                continue;
            }
            let sprite_count = reader.u16()? as usize;
            for _ in 0..sprite_count {
                let sprite = parse_composite(reader, is_rvio, version)?;
                if let Some(existing) = sprites
                    .iter_mut()
                    .find(|existing| existing.name == sprite.name)
                {
                    *existing = sprite;
                } else {
                    sprites.push(sprite);
                }
            }
        }
        if !found {
            return Err(AssetError::InvalidKa3d("resource is not a COMP set"));
        }
        Ok(Self { sprites })
    }
}

fn parse_composite(
    reader: &mut BeReader<'_>,
    is_rvio: bool,
    version: u16,
) -> Result<CompositeSprite, AssetError> {
    let name = reader.string()?;
    let part_count = reader.u16()? as usize;
    let mut parts = Vec::with_capacity(part_count);
    for _ in 0..part_count {
        let atlas_sprite = reader.string()?;
        let (sprite, x, y, scale_x, scale_y, flip_x, flip_y, angle) = if is_rvio {
            let id = reader.string()?;
            let sprite = if id.is_empty() {
                atlas_sprite
            } else {
                format!("{atlas_sprite}#{id}")
            };
            let x = f32::from(reader.i16()?);
            let y = f32::from(reader.i16()?);
            let scale_x = reader.f32()?;
            let scale_y = reader.f32()?;
            let angle = reader.f32()? * (std::f32::consts::PI / 180.0);
            let flip_x = if reader.u8()? == 0 { 1.0 } else { -1.0 };
            let flip_y = if reader.u8()? == 0 { 1.0 } else { -1.0 };
            (sprite, x, y, scale_x, scale_y, flip_x, flip_y, angle)
        } else {
            (
                atlas_sprite,
                f32::from(reader.i16()?),
                f32::from(reader.i16()?),
                1.0,
                1.0,
                1.0,
                1.0,
                0.0,
            )
        };
        parts.push(CompositePart {
            sprite,
            x,
            y,
            scale_x,
            scale_y,
            flip_x,
            flip_y,
            angle,
            visible: true,
        });
    }
    if !is_rvio && version == 2 {
        // Purple consumes this KA3D v2 attachment list after every
        // composite. The shipped sets use zero entries, but non-zero
        // lists are valid and contain a name plus two u16 values.
        let attachment_count = reader.u16()? as usize;
        for _ in 0..attachment_count {
            let _name = reader.string()?;
            let _x = reader.u16()?;
            let _y = reader.u16()?;
        }
    }
    Ok(CompositeSprite { name, parts })
}
