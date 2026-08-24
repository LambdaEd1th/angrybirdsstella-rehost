//! Shipped FONT asset loading and bitmap-font metrics.

use std::{collections::BTreeMap, fs, path::Path};

use stella_assets::ka3d::BitmapFont;

use super::FontMetric;

pub(crate) fn load_bitmap_fonts(data_root: &Path) -> BTreeMap<String, BitmapFont> {
    let font_root = data_root.join("fonts/1024x768");
    let mut paths = match fs::read_dir(font_root) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("dat"))
            .collect::<Vec<_>>(),
        Err(_) => return BTreeMap::new(),
    };
    paths.sort();
    let mut fonts = BTreeMap::new();
    for path in paths {
        let Some(name) = path
            .file_stem()
            .and_then(|value| value.to_str())
            .map(str::to_owned)
        else {
            continue;
        };
        let Ok(bytes) = fs::read(path) else {
            continue;
        };
        if let Ok(font) = BitmapFont::parse(&bytes) {
            fonts.insert(name, font);
        }
    }
    fonts
}

pub(crate) fn bitmap_font_string_width(font: &BitmapFont, text: &str) -> i32 {
    font.native_string_width(text)
}

pub(crate) fn bitmap_font_metric(font: &BitmapFont, metric: FontMetric) -> i32 {
    let ascending = font.native_max_ascending();
    let descending = font.native_max_descending();
    match metric {
        FontMetric::MaxAscending => ascending,
        FontMetric::MaxDescending => descending,
        FontMetric::Leading => i32::from(font.leading),
        FontMetric::Tracking => i32::from(font.tracking),
        FontMetric::Height => ascending.wrapping_add(descending),
    }
}
