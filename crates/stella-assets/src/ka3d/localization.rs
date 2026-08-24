use crate::AssetError;

use super::reader::{BeReader, NativeContainerReader};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizationTable {
    pub locales: Vec<String>,
    pub ids: Vec<String>,
    pub translations: Vec<Vec<String>>,
}

impl LocalizationTable {
    /// Parse Stella's chunked `TEXT` localization table (`LDAT`, `LIDS`, then
    /// one `TXGP` string group per locale).
    pub fn parse(bytes: &[u8]) -> Result<Self, AssetError> {
        if bytes.get(..4) != Some(b"KA3D") {
            return parse_legacy(bytes);
        }
        let locales = parse_locales(bytes)?;
        let mut ids = Vec::new();
        let mut translations = Vec::with_capacity(locales.len());
        for locale_index in 0..locales.len() {
            let (loaded_ids, values) = parse_translation_group(bytes, locale_index)?;
            if !loaded_ids.is_empty() {
                ids = loaded_ids;
            }
            translations.push(values);
        }
        Ok(Self {
            locales,
            ids,
            translations,
        })
    }
}

/// Pre-KA3D TextGroupSet layout selected by Purple after rewinding a stream
/// whose first big-endian u32 is not `KA3D`. The locale section owns a byte
/// length; the per-locale u32 table contains forward offsets to each string
/// group from immediately after the selected offset entry.
fn parse_legacy(bytes: &[u8]) -> Result<LocalizationTable, AssetError> {
    let mut locale_reader = BeReader::new(bytes);
    let _version = locale_reader.u8()?;
    let _locale_section_len = locale_reader.u32()?;
    let locale_count = locale_reader.u8()? as i8;
    if locale_count < 0 {
        return Err(AssetError::InvalidKa3d(
            "legacy TEXT locale count is negative",
        ));
    }
    let mut locales = Vec::with_capacity(locale_count as usize);
    for _ in 0..locale_count {
        locales.push(locale_reader.string()?);
    }

    let mut ids = Vec::new();
    let mut translations = Vec::with_capacity(locales.len());
    for locale_index in 0..locales.len() {
        let mut reader = BeReader::new(bytes);
        let _version = reader.u8()?;
        let locale_section_len = reader.u32()? as usize;
        reader.skip(locale_section_len)?;
        let id_count = reader.u16()? as usize;
        let mut loaded_ids = Vec::with_capacity(id_count);
        for _ in 0..id_count {
            loaded_ids.push(reader.string()?);
        }
        reader.skip(locale_index.checked_mul(4).ok_or(AssetError::InvalidKa3d(
            "legacy TEXT locale offset overflow",
        ))?)?;
        let group_offset = reader.u32()? as usize;
        reader.skip(group_offset)?;
        let mut values = Vec::with_capacity(id_count);
        for _ in 0..id_count {
            values.push(reader.string()?);
        }
        ids = loaded_ids;
        translations.push(values);
    }
    Ok(LocalizationTable {
        locales,
        ids,
        translations,
    })
}

fn ka3d_text_reader(bytes: &[u8]) -> Result<NativeContainerReader<'_>, AssetError> {
    let reader = NativeContainerReader::parse(bytes)?;
    if reader.container_type() != b"KA3D" {
        return Err(AssetError::InvalidKa3d("TEXT root is not KA3D"));
    }
    Ok(reader)
}

fn parse_locales(bytes: &[u8]) -> Result<Vec<String>, AssetError> {
    let mut container = ka3d_text_reader(bytes)?;
    let mut found = false;
    let mut locales = Vec::new();
    while let Some(chunk) = container.next_chunk()? {
        if &chunk.tag != b"TEXT" {
            container.skip(chunk.declared_len)?;
            continue;
        }
        found = true;
        if container.body().u16()? != 1 {
            continue;
        }
        while let Some(nested) = container.next_chunk()? {
            if &nested.tag != b"LDAT" {
                container.skip(nested.declared_len)?;
                continue;
            }
            let count = container.body().u16()? as usize;
            let mut loaded = Vec::with_capacity(count);
            for _ in 0..count {
                loaded.push(container.body().string()?);
            }
            locales = loaded;
        }
    }
    if !found {
        return Err(AssetError::InvalidKa3d("resource is not a TEXT table"));
    }
    Ok(locales)
}

fn parse_translation_group(
    bytes: &[u8],
    requested_index: usize,
) -> Result<(Vec<String>, Vec<String>), AssetError> {
    let mut container = ka3d_text_reader(bytes)?;
    while let Some(chunk) = container.next_chunk()? {
        if &chunk.tag != b"TEXT" {
            container.skip(chunk.declared_len)?;
            continue;
        }
        if container.body().u16()? != 1 {
            continue;
        }
        let mut ids = Vec::new();
        let mut group_index = 0;
        while let Some(nested) = container.next_chunk()? {
            match &nested.tag {
                b"LIDS" => {
                    let count = container.body().u16()? as usize;
                    let mut loaded = Vec::with_capacity(count);
                    for _ in 0..count {
                        loaded.push(container.body().string()?);
                    }
                    ids = loaded;
                }
                b"TXGP" => {
                    if ids.is_empty() {
                        return Err(AssetError::InvalidKa3d(
                            "missing LIDS chunk before TXGP chunk",
                        ));
                    }
                    if group_index == requested_index {
                        let mut values = Vec::with_capacity(ids.len());
                        for _ in 0..ids.len() {
                            values.push(container.body().string()?);
                        }
                        return Ok((ids, values));
                    }
                    container.skip(nested.declared_len)?;
                    group_index += 1;
                }
                _ => container.skip(nested.declared_len)?,
            }
        }
        return Ok((ids, Vec::new()));
    }
    Ok((Vec::new(), Vec::new()))
}
