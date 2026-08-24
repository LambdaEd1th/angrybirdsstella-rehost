use crate::AssetError;

use super::reader::NativeContainerReader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitmapFont {
    pub texture: String,
    /// Extra vertical spacing stored in the FONT header. Purple exposes this
    /// verbatim through `getFontLeading`.
    pub leading: i16,
    /// Horizontal spacing added after each rendered glyph.
    pub tracking: i16,
    pub glyphs: Vec<FontGlyph>,
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
        let mut container = NativeContainerReader::parse(bytes)?;
        if container.container_type() != b"KA3D" {
            return Err(AssetError::InvalidKa3d("FONT root is not KA3D"));
        }
        let mut found = false;
        let mut texture = String::new();
        let mut leading = 0;
        let mut tracking = 0;
        let mut glyphs = Vec::new();
        while let Some(chunk) = container.next_chunk()? {
            if &chunk.tag != b"FONT" {
                container.skip(chunk.declared_len)?;
                continue;
            }
            found = true;
            let reader = container.body();
            let version = reader.u16()?;
            if !matches!(version, 1 | 2) {
                continue;
            }
            texture = reader.string()?;
            leading = reader.i16()?;
            tracking = reader.i16()?;
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
        if !found {
            return Err(AssetError::InvalidKa3d("resource is not a FONT atlas"));
        }
        Ok(Self {
            texture,
            leading,
            tracking,
            glyphs,
        })
    }

    /// Native `std::map<int, Sprite *>` lookup used by every BitmapFont
    /// virtual. Missing codepoints deliberately have no replacement glyph.
    pub fn glyph(&self, codepoint: u32) -> Option<&FontGlyph> {
        self.glyphs
            .iter()
            .rev()
            .find(|glyph| glyph.codepoint == codepoint)
    }

    /// Whole-string form of BitmapFont's width virtual. Arithmetic is kept in
    /// 32-bit wrapping lanes, matching the AArch64 `ADD`/`MADD W...` sequence.
    pub fn native_string_width(&self, text: &str) -> i32 {
        let character_count = text.chars().count() as i32;
        if character_count == 0 {
            return 0;
        }
        let glyph_width = text.chars().fold(0_i32, |width, character| {
            self.glyph(character as u32)
                .map_or(width, |glyph| width.wrapping_add(i32::from(glyph.width)))
        });
        glyph_width
            .wrapping_add(i32::from(self.tracking).wrapping_mul(character_count.wrapping_sub(1)))
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
    pub fn native_string_height(&self, text: &str) -> i32 {
        text.chars()
            .filter_map(|character| self.glyph(character as u32))
            .map(|glyph| i32::from(glyph.height))
            .max()
            .unwrap_or(0)
    }

    /// Integer anchor applied by `BitmapFont::draw` before the individual
    /// glyph pivot is subtracted. Unknown/HPIVOT and BASELINE/VPIVOT values
    /// take the native default branches.
    pub fn native_draw_anchor(
        &self,
        text: &str,
        horizontal_anchor: &str,
        vertical_anchor: &str,
    ) -> [i32; 2] {
        let width = self.native_string_width(text);
        let horizontal = match horizontal_anchor {
            "HCENTER" => (width >> 1).wrapping_neg(),
            "RIGHT" => width.wrapping_neg(),
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
        [horizontal, vertical]
    }

    /// Wide-string BitmapFont `getBounds` virtual after substring selection.
    /// Its top edge additionally subtracts the largest glyph pivot in the
    /// requested substring, whereas `draw` subtracts each glyph's own pivot.
    pub fn native_string_bounds(
        &self,
        text: &str,
        horizontal_anchor: &str,
        vertical_anchor: &str,
    ) -> [i32; 4] {
        let width = self.native_string_width(text);
        let height = self.native_string_height(text);
        let [left, vertical] = self.native_draw_anchor(text, horizontal_anchor, vertical_anchor);
        let maximum_pivot = text
            .chars()
            .filter_map(|character| self.glyph(character as u32))
            .map(|glyph| i32::from(glyph.pivot_y))
            .max()
            .unwrap_or(0)
            .max(0);
        let top = vertical.wrapping_sub(maximum_pivot);
        [
            left,
            top,
            left.wrapping_add(width),
            top.wrapping_add(height),
        ]
    }
}
