//! Deferred host bindings for independently allocated native Image objects.
//!
//! A filename is an input to an Image constructor, not the Image's identity.
//! Keep the constructor identity in the cache key and unwrap only for file I/O.

pub fn sheet_image_source(sheet_identity: u64, texture_index: usize, path: &str) -> String {
    format!("<image:sheet:{sheet_identity}:{texture_index}>{path}")
}

pub fn image_source_path(source: &str) -> &str {
    let Some(suffix) = source.strip_prefix("<image:sheet:") else {
        return source;
    };
    let Some((identity, path)) = suffix.split_once('>') else {
        return source;
    };
    let Some((sheet, texture)) = identity.split_once(':') else {
        return source;
    };
    if sheet.parse::<u64>().is_err() || texture.parse::<usize>().is_err() {
        return source;
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_source_separates_native_identity_from_arbitrary_file_paths() {
        for path in ["/tmp/atlas.png", r"D:\game\atlas.png", "images/a>b:c.webp"] {
            let first = sheet_image_source(7, 0, path);
            assert_eq!(image_source_path(&first), path);
            assert_ne!(first, sheet_image_source(8, 0, path));
            assert_ne!(first, sheet_image_source(7, 1, path));
        }
        for source in ["normal.png", "<capture:image:1>", "<image:sheet:x:0>file"] {
            assert_eq!(image_source_path(source), source);
        }
    }
}
