//! IFont allocation ownership and ResourceManager's non-owning selection.

use super::*;
use crate::{
    FontMetric, LuaResult, NativeImageOwner, bitmap_font_metric, bitmap_font_string_width,
    runtime_error, system_font_metric, system_font_string_width,
};

#[derive(Debug)]
pub(crate) enum NativeFontValue {
    Bitmap {
        font: Arc<BitmapFont>,
        texture_source: String,
        decoded_image: Option<Arc<stella_assets::native_image::DecodedNativeImage>>,
        image_owner: Option<Arc<NativeImageOwner>>,
    },
    System(SystemFontState),
}

impl NativeFontValue {
    pub(crate) fn render_binding(&self) -> TextFontBinding {
        match self {
            Self::Bitmap {
                font,
                texture_source,
                decoded_image,
                image_owner,
            } => TextFontBinding::Bitmap {
                font: Arc::clone(font),
                texture_source: texture_source.clone(),
                decoded_image: decoded_image.clone(),
                image_owner: image_owner.clone(),
            },
            Self::System(font) => TextFontBinding::System(font.render_binding()),
        }
    }

    pub(crate) fn string_width(&self, text: &str) -> LuaResult<i32> {
        match self {
            Self::Bitmap { font, .. } => bitmap_font_string_width(font, text),
            Self::System(font) => Ok(system_font_string_width(font, text)),
        }
    }

    pub(crate) fn metric(&self, metric: FontMetric) -> LuaResult<f64> {
        match self {
            Self::Bitmap { font, .. } => bitmap_font_metric(font, metric).map(f64::from),
            Self::System(font) => Ok(system_font_metric(font, metric)),
        }
    }
}

impl ResourceRuntime {
    pub(crate) fn register_bitmap_font_value(&mut self, name: &str) {
        let value = NativeFontValue::Bitmap {
            font: Arc::clone(&self.bitmap_font_values[name]),
            texture_source: self.bitmap_font_texture_sources[name].clone(),
            decoded_image: self.bitmap_font_decoded_images.get(name).cloned(),
            image_owner: self.bitmap_font_image_owners.get(name).cloned(),
        };
        self.native_font_values
            .insert(name.to_owned(), Arc::new(value));
    }

    pub(crate) fn register_system_font_value(&mut self, name: &str) {
        let value = NativeFontValue::System(self.system_fonts[name].clone());
        self.native_font_values
            .insert(name.to_owned(), Arc::new(value));
    }

    pub(crate) fn select_native_font(&mut self, name: &str) {
        // 45BAC4 writes +0x48/+0x50 only on a successful exact map lookup.
        // Missing names leave even a retired selected pointer unchanged.
        if let Some(value) = self.native_font_values.get(name) {
            self.current_font_value = Some(Arc::downgrade(value));
            self.current_font = Some(name.to_owned());
        }
    }

    pub(crate) fn current_native_font(&self) -> LuaResult<Option<Arc<NativeFontValue>>> {
        let Some(value) = &self.current_font_value else {
            return Ok(None);
        };
        // A queued host draw retains immutable glyph/label data, not an IFont
        // allocation. Never let that snapshot or a same-name replacement
        // resurrect the raw pointer used by 45C1FC and the metric virtuals.
        value.upgrade().map(Some).ok_or_else(|| {
            runtime_error(format!(
                "Native selected font '{}' uses a released IFont",
                self.current_font.as_deref().unwrap_or("")
            ))
        })
    }
}
