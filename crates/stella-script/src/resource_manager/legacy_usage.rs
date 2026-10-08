//! Resource byte counters recovered from Purple's legacy ResourceManager.
//!
//! SpriteSheet allocation accounting comes from its constructor bindings;
//! AudioReader decoding remains separate in the native executable.

mod audio_reader;

pub(super) use audio_reader::{audio_file_info, audio_file_path};
