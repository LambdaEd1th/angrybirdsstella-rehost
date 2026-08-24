//! Locale-refresh member `sub_100050948`, registered at `0x10002E880`.

use std::fs;

use crate::*;
use mlua::Function;
use stella_assets::ka3d::LocalizationTable;

pub(super) fn install_refresh_current_locale(
    lua: &Lua,
    globals: &mlua::Table,
    resources: Arc<Mutex<ResourceRuntime>>,
    data_root: Arc<PathBuf>,
) -> LuaResult<()> {
    globals.set(
        "refreshCurrentLocale",
        lua.create_function(move |lua, _: MultiValue| {
            let available = available_basic_locales(&resources, &data_root);
            let locale = select_supported_locale(host_preferred_languages(), &available);
            let environment = game_environment(lua)?;
            let set_locale: Function = environment.get("setLocale")?;
            set_locale.call::<()>(locale)?;
            Ok(())
        })?,
    )
}

fn available_basic_locales(
    resources: &Arc<Mutex<ResourceRuntime>>,
    data_root: &Path,
) -> Vec<String> {
    if let Some(locales) = resources
        .lock()
        .expect("resource runtime lock poisoned")
        .text_group_set_tables
        .get("TEXTS_BASIC")
        .and_then(Option::as_ref)
        .map(|table| table.locales.clone())
    {
        return locales;
    }
    fs::read(data_root.join("localization/TEXTS_BASIC.dat"))
        .ok()
        .and_then(|bytes| LocalizationTable::parse(&bytes).ok())
        .map(|table| table.locales)
        .unwrap_or_default()
}

fn host_preferred_languages() -> Vec<String> {
    if let Some(locale) = std::env::var_os("STELLA_LOCALE")
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty() && value != "auto")
    {
        return vec![locale];
    }

    let mut candidates = Vec::new();
    for name in ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"] {
        let Some(value) = std::env::var_os(name).and_then(|value| value.into_string().ok()) else {
            continue;
        };
        for locale in value.split(':').filter(|locale| !locale.is_empty()) {
            let locale = locale.split(['.', '@']).next().unwrap_or(locale).to_owned();
            if !candidates.contains(&locale) {
                candidates.push(locale);
            }
        }
    }
    candidates
}

fn select_supported_locale(
    candidates: impl IntoIterator<Item = String>,
    available: &[String],
) -> String {
    for mut candidate in candidates {
        if candidate.starts_with("ja") {
            candidate = "ja_JP".to_owned();
        } else if candidate.starts_with("ko") {
            candidate = "ko_KR".to_owned();
        } else if candidate.starts_with("en") {
            candidate = "en_EN".to_owned();
        }
        if available.contains(&candidate) {
            return candidate;
        }
    }
    "en_EN".to_owned()
}

#[cfg(test)]
mod tests {
    use super::select_supported_locale;

    #[test]
    fn native_locale_selection_normalizes_only_ja_ko_en_and_uses_first_available() {
        let available = ["en_EN", "ja_JP", "ko_KR", "fr_FR"].map(str::to_owned);
        assert_eq!(
            select_supported_locale(
                ["zz_ZZ", "ja-anything", "fr_FR"].map(str::to_owned),
                &available,
            ),
            "ja_JP"
        );
        assert_eq!(
            select_supported_locale(["fr_FR".to_owned()], &available),
            "fr_FR"
        );
        assert_eq!(
            select_supported_locale(["unsupported".to_owned()], &available),
            "en_EN"
        );
    }
}
