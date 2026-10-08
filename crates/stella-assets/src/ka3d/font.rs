use crate::AssetError;

use super::reader::NativeContainerReader;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BitmapFont {
    pub texture: String,
    /// Extra vertical spacing stored in the FONT header. Purple exposes this
    /// verbatim through `getFontLeading`.
    pub leading: i16,
    /// Horizontal spacing added after each rendered glyph.
    pub tracking: i16,
    /// Native +0x58/+0x64 are assigned only by a supported FONT record.
    /// Zero host storage in an empty allocation is not a valid native metric.
    pub spacing_initialized: bool,
    pub glyphs: Vec<FontGlyph>,
    /// The last valid FONT record owns a fresh private SpriteSheet. Earlier
    /// records remain in the native glyph tree as raw, released Sprite pointers.
    /// Keep their geometry for cached metrics, but reject pointer dereferences.
    pub current_atlas_glyph_start: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontGlyph {
    /// FONT v1 stores this as an unsigned 16-bit value, while v2 stores the
    /// complete UTF-32 value used by the native glyph tree.
    pub codepoint: u32,
    pub x: i16,
    pub y: i16,
    pub width: i16,
    pub height: i16,
    /// Vertical atlas pivot/baseline. Horizontal advance is `width + tracking`
    /// in the native BitmapFont implementation.
    pub pivot_y: i16,
}

impl BitmapFont {
    /// Parse the big-endian `FONT` glyph atlas metadata. Version 1 glyphs use
    /// a 16-bit codepoint; version 2 glyphs use a 32-bit codepoint. The five
    /// remaining record fields are signed 16-bit AtlasSprite geometry.
    pub fn parse(bytes: &[u8]) -> Result<Self, AssetError> {
        Self::parse_with_image_loader(bytes, |_| Ok(()))
    }

    /// FONT's native constructor loads each atlas before the leading,
    /// tracking and glyph records, including when those later fields fail.
    pub fn parse_with_image_loader<E: From<AssetError>>(
        bytes: &[u8],
        mut load_image: impl FnMut(&str) -> Result<(), E>,
    ) -> Result<Self, E> {
        let Some(mut container) = NativeContainerReader::parse_ka3d(bytes)? else {
            return Ok(Self::default());
        };
        let mut texture = String::new();
        let mut leading = 0;
        let mut tracking = 0;
        let mut spacing_initialized = false;
        let mut glyphs = Vec::new();
        let mut current_atlas_glyph_start = 0;
        while let Some(chunk) = container.next_chunk()? {
            if &chunk.tag != b"FONT" {
                container.skip(chunk.declared_len)?;
                continue;
            }
            let reader = container.body();
            let version = reader.u16()?;
            if !matches!(version, 1 | 2) {
                continue;
            }
            texture = reader.string()?;
            load_image(&texture)?;
            current_atlas_glyph_start = glyphs.len();
            leading = reader.i16()?;
            tracking = reader.i16()?;
            spacing_initialized = true;
            let glyph_count = reader.u16()? as usize;
            glyphs.reserve(glyph_count);
            for _ in 0..glyph_count {
                glyphs.push(FontGlyph {
                    codepoint: if version == 1 {
                        u32::from(reader.u16()?)
                    } else {
                        reader.u32()?
                    },
                    x: reader.i16()?,
                    y: reader.i16()?,
                    width: reader.i16()?,
                    height: reader.i16()?,
                    pivot_y: reader.i16()?,
                });
            }
        }
        Ok(Self {
            texture,
            leading,
            tracking,
            spacing_initialized,
            glyphs,
            current_atlas_glyph_start,
        })
    }

    pub fn native_leading(&self) -> Result<i16, AssetError> {
        if !self.spacing_initialized {
            return Err(AssetError::UninitializedFontMetric { metric: "leading" });
        }
        Ok(self.leading)
    }

    pub fn native_tracking(&self) -> Result<i16, AssetError> {
        if !self.spacing_initialized {
            return Err(AssetError::UninitializedFontMetric { metric: "tracking" });
        }
        Ok(self.tracking)
    }

    /// Metadata lookup, including geometry whose native Sprite has been freed.
    /// Drawing and virtual metrics must use `live_glyph` instead.
    pub fn glyph(&self, codepoint: u32) -> Option<&FontGlyph> {
        self.glyphs
            .iter()
            .rev()
            .find(|glyph| glyph.codepoint == codepoint)
    }

    /// Native map lookup followed by a Sprite pointer dereference. Translate
    /// its undefined freed-pointer access to an explicit host error.
    pub fn live_glyph(&self, codepoint: u32) -> Result<Option<&FontGlyph>, AssetError> {
        let Some((index, glyph)) = self
            .glyphs
            .iter()
            .enumerate()
            .rev()
            .find(|(_, glyph)| glyph.codepoint == codepoint)
        else {
            return Ok(None);
        };
        if index < self.current_atlas_glyph_start {
            return Err(AssetError::ReleasedFontGlyph { codepoint });
        }
        Ok(Some(glyph))
    }

    /// Locate the first invalid dereference without rejecting a live prefix.
    /// A normal single-record font needs no additional string scan.
    pub fn first_released_glyph(&self, text: &str) -> Option<(usize, AssetError)> {
        if self.current_atlas_glyph_start == 0 {
            return None;
        }
        text.char_indices().find_map(|(offset, character)| {
            self.live_glyph(character as u32)
                .err()
                .map(|error| (offset, error))
        })
    }

    /// Whole-string form of BitmapFont's width virtual. Arithmetic is kept in
    /// 32-bit wrapping lanes, matching the AArch64 `ADD`/`MADD W...` sequence.
    pub fn native_string_width(&self, text: &str) -> Result<i32, AssetError> {
        let mut character_count = 0_i32;
        let mut glyph_width = 0_i32;
        for character in text.chars() {
            character_count = character_count.wrapping_add(1);
            if let Some(glyph) = self.live_glyph(character as u32)? {
                glyph_width = glyph_width.wrapping_add(i32::from(glyph.width));
            }
        }
        if character_count == 0 {
            return Ok(0);
        }
        // 42BBB0's MADD has a zero spacing multiplier for one character.
        // Its result is known even when the allocated tracking bytes are not.
        if character_count == 1 {
            return Ok(glyph_width);
        }
        Ok(glyph_width.wrapping_add(
            i32::from(self.native_tracking()?).wrapping_mul(character_count.wrapping_sub(1)),
        ))
    }

    /// Constructor-cached ascender at object offset `+0x5c`.
    pub fn native_max_ascending(&self) -> i32 {
        self.glyphs
            .iter()
            .map(|glyph| i32::from(glyph.pivot_y))
            .max()
            .unwrap_or(0)
            .max(0)
    }

    /// Constructor-cached descender at object offset `+0x60`.
    pub fn native_max_descending(&self) -> i32 {
        self.glyphs
            .iter()
            .map(|glyph| i32::from(glyph.height).wrapping_sub(i32::from(glyph.pivot_y)))
            .max()
            .unwrap_or(0)
            .max(0)
    }

    /// Substring-height virtual for the whole string. Unlike the public font
    /// height metric, this is the tallest glyph in the requested string.
    pub fn native_string_height(&self, text: &str) -> Result<i32, AssetError> {
        let mut maximum = None;
        for character in text.chars() {
            if let Some(glyph) = self.live_glyph(character as u32)? {
                maximum = Some(maximum.map_or(i32::from(glyph.height), |height: i32| {
                    height.max(i32::from(glyph.height))
                }));
            }
        }
        Ok(maximum.unwrap_or(0))
    }

    /// Integer anchor applied by `BitmapFont::draw` before the individual
    /// glyph pivot is subtracted. Unknown/HPIVOT and BASELINE/VPIVOT values
    /// take the native default branches.
    pub fn native_draw_anchor(
        &self,
        text: &str,
        horizontal_anchor: &str,
        vertical_anchor: &str,
    ) -> Result<[i32; 2], AssetError> {
        let horizontal = match horizontal_anchor {
            "HCENTER" => (self.native_string_width(text)? >> 1).wrapping_neg(),
            "RIGHT" => self.native_string_width(text)?.wrapping_neg(),
            _ => 0,
        };
        let ascending = self.native_max_ascending();
        let descending = self.native_max_descending();
        let vertical = match vertical_anchor {
            "TOP" => ascending,
            "VCENTER" => ascending.wrapping_sub(ascending.wrapping_add(descending) >> 1),
            "BOTTOM" => descending.wrapping_neg(),
            _ => 0,
        };
        Ok([horizontal, vertical])
    }

    /// Wide-string BitmapFont `getBounds` virtual after substring selection.
    /// Its top edge additionally subtracts the largest glyph pivot in the
    /// requested substring, whereas `draw` subtracts each glyph's own pivot.
    pub fn native_string_bounds(
        &self,
        text: &str,
        horizontal_anchor: &str,
        vertical_anchor: &str,
    ) -> Result<[i32; 4], AssetError> {
        let width = self.native_string_width(text)?;
        let height = self.native_string_height(text)?;
        let [left, vertical] = self.native_draw_anchor(text, horizontal_anchor, vertical_anchor)?;
        let maximum_pivot = text
            .chars()
            .filter_map(|character| self.glyph(character as u32))
            .map(|glyph| i32::from(glyph.pivot_y))
            .max()
            .unwrap_or(0)
            .max(0);
        let top = vertical.wrapping_sub(maximum_pivot);
        Ok([
            left,
            top,
            left.wrapping_add(width),
            top.wrapping_add(height),
        ])
    }
}
