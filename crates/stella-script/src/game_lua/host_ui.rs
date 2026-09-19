//! Services for platform-owned views, outside Lua's framebuffer/resources.

use crate::*;

impl StellaLua {
    /// Current native Apprater alert, outside the game's framebuffer.
    pub fn app_rating_prompt(&self) -> Option<AppRatingPrompt> {
        self.apprater.prompt()
    }

    /// Answer the retained alert. A stale owner returns false without writes.
    /// Rate records the user's choice and queues the native store URL; it is
    /// not a claim that a review was submitted or that the URL opened.
    pub fn answer_app_rating(&self, id: u64, choice: AppRatingChoice) -> Result<bool, ScriptError> {
        Ok(self.apprater.answer(id, choice)?)
    }

    /// Desktop ownership handoff when a native-style modal intercepts input.
    /// Clear pending/held platform input without firing gamePaused/gameResumed
    /// or synthesizing a click on the obscured game. This is an embedding
    /// boundary, not a claim that UIKit calls GameApp's activation virtual.
    pub fn clear_platform_input_for_modal(&self) -> Result<(), ScriptError> {
        self.set_touches(&[])?;
        *self
            .native_keys
            .lock()
            .expect("native key-buffer lock poisoned") = NativeKeyBuffers::default();
        Ok(())
    }

    /// Account-view fonts retain their face independently of game Lua font
    /// handles and LabelPool lifetime. OpenSans uses the original bundle;
    /// UIKit's private regular face uses an explicit host sans adaptation, not
    /// reconstructed iOS font bytes. Never registers a Lua resource or inserts
    /// entered text into the game's label cache.
    pub fn platform_ui_font(
        &self,
        family: &str,
        size: i32,
        color: [u8; 4],
    ) -> Result<SystemFontRenderBinding, ScriptError> {
        let filename = match family {
            "OpenSans" => "OpenSans-Regular.ttf",
            "OpenSans-CondensedBold" => "OpenSans-CondBold.ttf",
            ".HelveticaNeueInterface-Regular" => {
                return Ok(crate::resource_manager::create_platform_ui_regular_font(
                    size, color,
                )?);
            }
            _ => return Err(runtime_error("unsupported platform UI font").into()),
        };
        let path = self.data_root.join("skynestdata/fonts").join(filename);
        let bytes = std::fs::read(&path)
            .map_err(|_| runtime_error(format!("missing platform UI font: {}", path.display())))?;
        Ok(crate::resource_manager::create_platform_ui_font(
            family,
            size,
            color,
            bytes.into(),
        )?)
    }

    /// Ordered host language preferences also drive UIKit's localized views.
    /// The account wrapper has its own service-locale mapping; filenames here
    /// intentionally retain the original .lproj language identifiers.
    pub fn platform_ui_languages(&self) -> Vec<String> {
        crate::preferred_languages::host_preferred_languages()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_ui_regular_font_keeps_game_catalog_and_label_pool_unchanged() {
        let runtime = StellaLua::new(std::env::temp_dir()).unwrap();
        let before = {
            let resources = runtime.resource_runtime.lock().unwrap();
            (
                resources.system_fonts.keys().cloned().collect::<Vec<_>>(),
                resources.bitmap_fonts.clone(),
                resources.current_font.clone(),
                resources.system_font_label_pool_epoch,
                resources.sprite_catalog_snapshot(&runtime.data_root),
            )
        };
        let names = platform_system_font_names().to_vec();
        let binding = runtime
            .platform_ui_font(".HelveticaNeueInterface-Regular", 21, [29, 30, 31, 255])
            .unwrap();
        assert_eq!(binding.size, 21);
        assert_eq!(binding.fill_rgba, [29, 30, 31, 255]);
        assert_eq!(binding.label_pool_epoch, 0);
        assert_ne!(binding.family, ".HelveticaNeueInterface-Regular");
        let layout = binding
            .native_system_font_layout("September 05 2000")
            .unwrap();
        assert!(layout.width > 0);
        assert!(
            layout.lines[0]
                .glyphs
                .iter()
                .all(|glyph| glyph.glyph_id != 0)
        );
        drop(binding);
        let resources = runtime.resource_runtime.lock().unwrap();
        let after = (
            resources.system_fonts.keys().cloned().collect::<Vec<_>>(),
            resources.bitmap_fonts.clone(),
            resources.current_font.clone(),
            resources.system_font_label_pool_epoch,
            resources.sprite_catalog_snapshot(&runtime.data_root),
        );
        assert_eq!(before, after);
        assert_eq!(platform_system_font_names(), names);
    }

    #[test]
    fn platform_ui_regular_font_does_not_replace_missing_bundled_faces() {
        let missing_root = std::env::temp_dir().join(format!(
            "stella-font-missing-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let runtime = StellaLua::new(missing_root).unwrap();
        for family in ["OpenSans", "OpenSans-CondensedBold"] {
            let error = runtime
                .platform_ui_font(family, 21, [0, 0, 0, 255])
                .unwrap_err()
                .to_string();
            assert!(error.contains("missing platform UI font"), "{error}");
            assert!(error.contains("skynestdata"), "{error}");
        }
        assert!(
            runtime
                .platform_ui_font("HelveticaNeue", 21, [0; 4])
                .is_err()
        );
    }
}
