//! Native Image allocation lifetime, independent of shared immutable pixels.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

static NEXT_IMAGE_IDENTITY: AtomicU64 = AtomicU64::new(1);

/// Host lifetime marker shared by resource bindings and deferred snapshots.
/// Caches keep weak references and cannot extend their lifetime. Equal
/// decoded pixels do not imply the same native Image allocation. AtlasSprite
/// reads SpriteSheet's current Image; this marker is not its native refcount.
#[derive(Debug, PartialEq, Eq)]
pub struct NativeImageOwner {
    identity: u64,
}

impl NativeImageOwner {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            identity: NEXT_IMAGE_IDENTITY.fetch_add(1, Ordering::Relaxed),
        })
    }

    pub(crate) const fn identity(&self) -> u64 {
        self.identity
    }
}
