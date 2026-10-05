//! AtlasSprite's raw SpriteSheet pointer, separate from a submitted Image.

use std::sync::{Arc, RwLock, Weak};

use stella_assets::native_image::DecodedNativeImage;

use super::NativeImageOwner;

#[derive(Debug, Clone)]
pub(crate) struct SheetImageSnapshot {
    pub(crate) source: String,
    pub(crate) image: Option<Arc<DecodedNativeImage>>,
    pub(crate) owner: Arc<NativeImageOwner>,
    pub(crate) dimensions: Option<[u32; 2]>,
}

/// Only ResourceRuntime owns this cell. Sprites borrow it; a sheet release
/// must not be extended by scene objects, animation targets or Compo entries.
#[derive(Debug, Default)]
pub(crate) struct SpriteSheetImageCell {
    current: RwLock<Option<SheetImageSnapshot>>,
}

impl SpriteSheetImageCell {
    pub(crate) fn bind(self: &Arc<Self>) -> SpriteSheetImageBinding {
        SpriteSheetImageBinding(Arc::downgrade(self))
    }

    pub(crate) fn replace(&self, image: Option<SheetImageSnapshot>) {
        *self.current.write().expect("sheet Image lock poisoned") = image;
    }
}

/// Non-owning AtlasSprite -> SpriteSheet Image access. It never retains the
/// sheet or Image. A draw command resolves it at the immediate draw boundary.
#[derive(Debug, Clone)]
pub struct SpriteSheetImageBinding(Weak<SpriteSheetImageCell>);

impl PartialEq for SpriteSheetImageBinding {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for SpriteSheetImageBinding {}

impl SpriteSheetImageBinding {
    pub(crate) fn snapshot(&self) -> Result<SheetImageSnapshot, &'static str> {
        let sheet = self.0.upgrade().ok_or("a released SpriteSheet")?;
        sheet
            .current
            .read()
            .expect("sheet Image lock poisoned")
            .clone()
            .ok_or("a SpriteSheet with no current Image")
    }
}

/// A raw Image obtained once from a resource sheet, as by native_setTexture.
/// It borrows the resource allocation rather than retaining submitted pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeImageBorrow {
    sheet_image: SpriteSheetImageBinding,
    image_identity: u64,
}

impl NativeImageBorrow {
    pub(crate) fn new(sheet_image: SpriteSheetImageBinding, image_identity: u64) -> Self {
        Self {
            sheet_image,
            image_identity,
        }
    }

    pub(crate) fn snapshot(&self) -> Result<SheetImageSnapshot, &'static str> {
        let image = self.sheet_image.snapshot()?;
        if image.owner.identity() != self.image_identity {
            return Err("a replaced native Image");
        }
        Ok(image)
    }
}

/// Dirt stores Image::getTexture's raw result, without retaining the Image,
/// sheet or Texture. A later sheet Image must not replace that pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeTextureBorrow(NativeImageBorrow);

impl NativeTextureBorrow {
    pub(crate) fn new(sheet_image: SpriteSheetImageBinding, image_identity: u64) -> Self {
        Self(NativeImageBorrow::new(sheet_image, image_identity))
    }

    pub(crate) fn snapshot(&self) -> Result<SheetImageSnapshot, &'static str> {
        self.0.snapshot().map_err(|reason| match reason {
            "a replaced native Image" => "a replaced native Texture",
            reason => reason,
        })
    }
}
