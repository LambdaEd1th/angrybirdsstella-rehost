//! AtlasSprite's raw SpriteSheet pointer, separate from a submitted Image.

use std::sync::{Arc, RwLock, Weak};

use stella_assets::native_image::DecodedNativeImage;

use super::NativeImageOwner;

#[derive(Debug, Clone)]
pub(crate) struct SheetImageSnapshot {
    pub(crate) source: String,
    pub(crate) image: Option<Arc<DecodedNativeImage>>,
    pub(crate) owner: Arc<NativeImageOwner>,
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
