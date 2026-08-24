//! In-memory extraction of the 7z wrapper used by encrypted resources.

use std::io::Cursor;

use sevenz_rust2::{ArchiveReader, Password};

use crate::AssetError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    pub name: String,
    pub bytes: Vec<u8>,
}

pub fn unpack_7z(input: &[u8]) -> Result<Vec<ArchiveEntry>, AssetError> {
    let mut reader = ArchiveReader::new(Cursor::new(input), Password::empty())?;
    let mut entries = Vec::new();
    reader.for_each_entries(|entry, stream| {
        if entry.is_directory() {
            return Ok(true);
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        stream.read_to_end(&mut bytes)?;
        entries.push(ArchiveEntry {
            name: entry.name().to_owned(),
            bytes,
        });
        Ok(true)
    })?;
    Ok(entries)
}
