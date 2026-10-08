//! File Images load pixels at construction, not at deferred draw time.
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use stella_assets::{
    image_source::{font_image_source, image_source_path, sheet_image_source},
    native_image::DecodedNativeImage,
};

use super::ResourceRuntime;
use crate::resource_manager::{resource_join_path, resource_normalized_path};
use crate::{LuaResult, NativeImageOwner, SheetImageSnapshot, runtime_error};

/// Only the current Image survives each successful native SPRT image load.
/// Earlier AtlasSprites retain their constructor UV extent, not those pixels.
#[derive(Default)]
pub(crate) struct PreparedSheetImages {
    pub(crate) texture_sources: Vec<String>,
    pub(super) constructor_dimensions: Vec<Option<[u32; 2]>>,
    pub(super) current_image: Option<SheetImageSnapshot>,
    pub(super) allocation_bytes: u32,
}

impl PreparedSheetImages {
    pub(crate) fn push(&mut self, image: SheetImageSnapshot) {
        if let Some(pixels) = &image.image {
            self.allocation_bytes = self.allocation_bytes.wrapping_add(
                pixels
                    .layout
                    .pixels
                    .for_gl_upload(true)
                    .allocation_bytes(pixels.width as i32, pixels.height as i32),
            );
        }
        self.texture_sources.push(image.source.clone());
        self.constructor_dimensions.push(image.dimensions);
        // The replacement is fully constructed before its predecessor drops.
        self.current_image = Some(image);
    }
}

impl ResourceRuntime {
    pub(super) fn snapshot_file_image(
        &mut self,
        source: &str,
    ) -> LuaResult<Arc<DecodedNativeImage>> {
        let path = image_source_path(source);
        // FileInputStream::Impl (100506A58) rejects a final separator before
        // opening the file. Lexical FilePath normalization turns '\\' into '/'.
        if matches!(path.as_bytes().last(), Some(b'/') | Some(b'\\')) {
            return Err(runtime_error(format!(
                "Failed to open image '{path}': path ends with a separator"
            )));
        }
        let bytes = std::fs::read(path)
            .map_err(|error| runtime_error(format!("Failed to open image '{path}': {error}")))?;
        let image = stella_assets::native_image::decode_native_texture(
            &bytes,
            Path::new(path).extension().and_then(|value| value.to_str()),
        )
        .map_err(|error| runtime_error(format!("Failed to load image '{path}': {error}")))?;
        // Sharing immutable pixels is independent of native Image identity.
        // Always read the constructor input, then compare actual decoded data;
        // a file timestamp or filename cannot establish pixel equivalence.
        if let Some(previous) = self
            .file_image_cache
            .get(path)
            .and_then(|value| value.upgrade())
            && *previous == image
        {
            return Ok(previous);
        }
        let image = Arc::new(image);
        self.file_image_cache
            .insert(path.to_owned(), Arc::downgrade(&image));
        Ok(image)
    }

    pub(crate) fn load_sheet_file_image(
        &mut self,
        data_root: &Path,
        descriptor: Option<&PathBuf>,
        texture: &str,
        texture_index: usize,
    ) -> LuaResult<SheetImageSnapshot> {
        let path = resolve_file_image_path(data_root, descriptor, texture);
        let owner = NativeImageOwner::new();
        let source = sheet_image_source(owner.identity(), texture_index, &path);
        let image = self.snapshot_file_image(&source)?;
        Ok(SheetImageSnapshot {
            source,
            dimensions: Some([image.width, image.height]),
            image: Some(image),
            owner,
        })
    }

    pub(crate) fn load_font_file_image(
        &mut self,
        data_root: &Path,
        descriptor: &PathBuf,
        texture: &str,
    ) -> LuaResult<SheetImageSnapshot> {
        let path = resolve_file_image_path(data_root, Some(descriptor), texture);
        let owner = NativeImageOwner::new();
        let source = font_image_source(owner.identity(), &path);
        let image = self.snapshot_file_image(&source)?;
        Ok(SheetImageSnapshot {
            source,
            dimensions: Some([image.width, image.height]),
            image: Some(image),
            owner,
        })
    }
}

/// SPRT/FONT/JSON join their embedded image to the descriptor's parent with
/// FilePath (1004610F0 / 10042A780 / 1004637FC). They do not search for an
/// existing alternative. Explicit file-stream inputs have no descriptor base.
/// Capture images are published separately and never parsed as filenames.
pub(super) fn resolve_file_image_path(
    data_root: &Path,
    descriptor: Option<&PathBuf>,
    texture: &str,
) -> String {
    let selected = if let Some(descriptor) = descriptor {
        let descriptor = resource_normalized_path(&descriptor.to_string_lossy());
        let parent = descriptor.rsplit_once('/').map_or("", |(parent, _)| parent);
        resource_join_path(parent, texture)
    } else if Path::new(texture).is_absolute() {
        resource_normalized_path(texture)
    } else {
        resource_join_path(&data_root.to_string_lossy(), texture)
    };
    // FileInputStream retains this requested name (100506294), and the image
    // reader consults its extension (1004FB534). Resolving a symlink here would
    // change decoder selection as well as add filesystem work before opening.
    #[cfg(windows)]
    {
        // Preserve Windows namespace prefixes supplied by the host data root.
        selected.replace('/', "\\")
    }
    #[cfg(not(windows))]
    {
        selected
    }
}
