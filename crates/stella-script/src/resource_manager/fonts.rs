//! Font resource facade following bitmap, clipText and SystemFont ownership.

mod bitmap;
mod clipping;
mod system;

pub(crate) use bitmap::{bitmap_font_metric, bitmap_font_string_width, load_bitmap_fonts};
pub(crate) use clipping::native_clip_text_lines;
pub(crate) use system::{
    SystemFontState, create_system_font_state, platform_system_font_names,
    system_font_color_from_lua, system_font_metric, system_font_string_width,
};

#[derive(Clone, Copy)]
pub(crate) enum FontMetric {
    MaxAscending,
    MaxDescending,
    Leading,
    Tracking,
    Height,
}
