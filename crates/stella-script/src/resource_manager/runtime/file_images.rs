//! File Images load pixels at construction, not at deferred draw time.
use std::{path::Path, sync::Arc};

use stella_assets::{image_source::image_source_path, native_image::DecodedNativeImage};

use super::ResourceRuntime;

impl ResourceRuntime {
    pub(super) fn snapshot_file_image(&mut self, source: &str) -> Option<Arc<DecodedNativeImage>> {
        let path = image_source_path(source);
        let bytes = std::fs::read(path).ok()?;
        let image = stella_assets::native_image::decode_native_texture(
            &bytes,
            Path::new(path).extension().and_then(|value| value.to_str()),
        )
        .ok()?;
        // Sharing immutable pixels is independent of native Image identity.
        // Always read the constructor input, then compare actual decoded data;
        // a file timestamp or filename cannot establish pixel equivalence.
        if let Some(previous) = self
            .file_image_cache
            .get(path)
            .and_then(|value| value.upgrade())
            && *previous == image
        {
            return Some(previous);
        }
        let image = Arc::new(image);
        self.file_image_cache
            .insert(path.to_owned(), Arc::downgrade(&image));
        Some(image)
    }
}
