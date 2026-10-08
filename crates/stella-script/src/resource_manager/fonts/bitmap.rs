//! Metrics from the constructed native bitmap font allocation.

use stella_assets::ka3d::BitmapFont;

use super::FontMetric;

pub(crate) fn bitmap_font_string_width(font: &BitmapFont, text: &str) -> crate::LuaResult<i32> {
    font.native_string_width(text)
        .map_err(|error| crate::runtime_error(error.to_string()))
}

pub(crate) fn bitmap_font_metric(font: &BitmapFont, metric: FontMetric) -> crate::LuaResult<i32> {
    let ascending = font.native_max_ascending();
    let descending = font.native_max_descending();
    Ok(match metric {
        FontMetric::MaxAscending => ascending,
        FontMetric::MaxDescending => descending,
        FontMetric::Leading => i32::from(font.native_leading().map_err(crate::runtime_error)?),
        FontMetric::Tracking => i32::from(font.native_tracking().map_err(crate::runtime_error)?),
        FontMetric::Height => ascending.wrapping_add(descending),
    })
}
