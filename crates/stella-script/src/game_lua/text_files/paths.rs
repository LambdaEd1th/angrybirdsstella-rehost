//! Shipped-resource and extracted-config path resolution.

use std::path::{Path, PathBuf};

use crate::resolve_data_file;

pub(super) fn resolve_text_table(data_root: &Path, requested: &str) -> Option<PathBuf> {
    if let Ok(path) = resolve_data_file(data_root, requested) {
        return Some(path);
    }
    let requested = Path::new(requested.trim_start_matches('/'));
    let file_name = requested.file_name()?;
    let direct_config = data_root.join("config").join(file_name);
    if direct_config.is_file() {
        return Some(direct_config);
    }
    let json_name = Path::new(file_name).with_extension("json");
    let json_config = data_root.join("config").join(json_name);
    json_config.is_file().then_some(json_config)
}
