//! `sub_1000512D8` raw/encrypted resource-byte pipeline.

use std::{fs, path::Path};

use super::paths::resolve_text_table;
use crate::{ScriptError, app_data_path, runtime_error};

pub(super) fn load_text_bytes(
    data_root: &Path,
    requested: &str,
    encrypted: bool,
    alternate_key: bool,
    decompress: bool,
) -> Result<Vec<u8>, ScriptError> {
    if !encrypted {
        return read_optional_file(&app_data_path(data_root, requested)?);
    }

    // The encrypted branch uses Purple's virtual resource stream: downloaded
    // AppData assets shadow their shipped bundle fallback.
    let app_data = app_data_path(data_root, requested)?;
    let path = if app_data.is_file() {
        app_data
    } else if let Some(path) = resolve_text_table(data_root, requested) {
        path
    } else {
        return Ok(Vec::new());
    };
    let input = read_optional_file(&path)?;
    if input.is_empty() {
        return Ok(Vec::new());
    }

    // Extracted config JSON is the already-decoded form of the original
    // encrypted `.dat` container. All other input follows the native AES path.
    let predecoded = std::str::from_utf8(&input).is_ok_and(|text| !text.contains('\0'));
    let mut bytes = if predecoded {
        input
    } else if alternate_key {
        stella_assets::decrypt_text_file(&input)?
    } else {
        stella_assets::decrypt_resource_data(&input)?
    };
    if decompress && (!predecoded || bytes.starts_with(&stella_assets::SEVEN_Z_SIGNATURE)) {
        bytes = stella_assets::unpack_7z(&bytes)?
            .into_iter()
            .next()
            .ok_or_else(|| ScriptError::Lua(runtime_error("text archive contains no files")))?
            .bytes;
    }
    Ok(bytes)
}

fn read_optional_file(path: &Path) -> Result<Vec<u8>, ScriptError> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}
