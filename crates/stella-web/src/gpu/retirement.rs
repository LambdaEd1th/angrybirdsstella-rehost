//! Physical GL textures release after every native owner and input frame.
use std::{
    collections::{HashMap, HashSet},
    sync::Weak,
};

pub(super) fn retired_texture_names(
    explicitly_retired: &HashSet<String>,
    uploaded: &HashSet<String>,
    lifetimes: &HashMap<String, Weak<()>>,
    required: &HashSet<String>,
    capture_targets: &HashSet<String>,
) -> HashSet<String> {
    // A prepared stream can be discarded before packet export. Consider
    // every uploaded/captured physical generation as well, so its release
    // is delivered after the last input-frame lease disappears.
    explicitly_retired
        .iter()
        .chain(uploaded.iter().filter(|name| {
            name.starts_with("<file-image-generation:") || name.starts_with("<capture-generation:")
        }))
        .filter(|name| {
            !lifetimes
                .get(*name)
                .is_some_and(|lease| lease.strong_count() != 0)
                && !required.contains(*name)
                && !capture_targets.contains(*name)
        })
        .cloned()
        .collect()
}
