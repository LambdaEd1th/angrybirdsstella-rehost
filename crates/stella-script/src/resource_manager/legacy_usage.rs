//! Resource byte counters recovered from Purple's legacy ResourceManager.
//!
//! Native SpriteSheet upload accounting and AudioReader decoding are separate
//! owners in the executable, so the facade only retains their shared API.

mod audio_reader;
mod sprite_sheet;

pub(super) use audio_reader::{audio_file_info, audio_file_path};
pub(super) use sprite_sheet::sprite_sheet_textures;
