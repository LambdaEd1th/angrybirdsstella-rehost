//! Locale-refresh member `sub_100050948`, registered at `0x10002E880`.

use std::fs;

use crate::preferred_languages::host_preferred_languages;
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

fn select_supported_locale(
    candidates: impl IntoIterator<Item = String>,
    available: &[String],
) -> String {
    for host_language in candidates {
        // pf::Locale::LocaleImpl::getPreferedLanguages (10053E408) runs
        // BEFORE GameLua's 100050948. Keep this resource-name conversion here,
        // not in the shared BCP-47 list used by system-font fallback.
        let mut candidate = native_platform_locale(&host_language);
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
        // Current desktop APIs include region after script (zh-Hans-CN),
        // unlike the older iOS labels handled by the native exact mapping.
        // Resolve this candidate before advancing to a lower-priority language.
        if let Some(chinese) = modern_chinese_locale(&host_language)
            && available.iter().any(|locale| locale == chinese)
        {
            return chinese.to_owned();
        }
    }
    "en_EN".to_owned()
}

fn native_platform_locale(language: &str) -> String {
    if language.len() == 2 {
        if language == "pt" {
            return "pt_BR".to_owned();
        }
        return format!(
            "{}_{}",
            language.to_ascii_lowercase(),
            language.to_ascii_uppercase()
        );
    }
    match language.replace('-', "_").as_str() {
        "zh_Hans" => "zh_CN".to_owned(),
        "zh_Hant" => "zh_TW".to_owned(),
        locale => locale.to_owned(),
    }
}

fn modern_chinese_locale(language: &str) -> Option<&'static str> {
    let mut subtags = language.split(['-', '_']);
    if !subtags.next()?.eq_ignore_ascii_case("zh") {
        return None;
    }
    let mut script = None;
    let mut region = None;
    for subtag in subtags {
        // Extension/private-use subtags are not language script/region data.
        if subtag.len() == 1 {
            break;
        }
        if subtag.len() == 4 && subtag.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            script = Some(subtag);
        } else if subtag.len() == 2 && subtag.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            region = Some(subtag);
        }
    }
    // An explicit script wins even when it differs from the region's default;
    // never turn Latin-script Chinese into a Han-script translation implicitly.
    if let Some(script) = script {
        return if script.eq_ignore_ascii_case("Hans") {
            Some("zh_CN")
        } else if script.eq_ignore_ascii_case("Hant") {
            Some("zh_TW")
        } else {
            None
        };
    }
    match region?.to_ascii_uppercase().as_str() {
        "CN" | "SG" => Some("zh_CN"),
        "TW" | "HK" | "MO" => Some("zh_TW"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::select_supported_locale;

    #[test]
    fn native_locale_selection_uses_first_available_and_english_fallback() {
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

    #[test]
    fn native_locale_selection_applies_the_upstream_platform_conversion() {
        let available = [
            "en_EN", "ja_JP", "ko_KR", "fr_FR", "de_DE", "pt_BR", "zh_CN", "zh_TW",
        ]
        .map(str::to_owned);
        for (input, expected) in [
            ("fr", "fr_FR"),
            ("fr-FR", "fr_FR"),
            ("de", "de_DE"),
            ("pt", "pt_BR"),
            ("pt-BR", "pt_BR"),
            ("en-GB", "en_EN"),
            ("zh-Hans", "zh_CN"),
            ("zh-Hant", "zh_TW"),
            ("zh-CN", "zh_CN"),
            ("zh-TW", "zh_TW"),
            ("ja-CN", "ja_JP"),
            ("ko", "ko_KR"),
        ] {
            assert_eq!(
                select_supported_locale([input.to_owned()], &available),
                expected,
                "{input}"
            );
        }
    }

    #[test]
    fn native_locale_selection_resolves_modern_chinese_before_next_preference() {
        let available = ["en_EN", "ja_JP", "zh_CN", "zh_TW"].map(str::to_owned);
        for (input, expected) in [
            ("zh-Hans-CN", "zh_CN"),
            ("zh_Hans_CN", "zh_CN"),
            ("zh-hans-cn", "zh_CN"),
            ("zh-Hans-TW", "zh_CN"),
            ("zh-Hant-TW", "zh_TW"),
            ("zh-Hant-CN", "zh_TW"),
            ("zh-SG", "zh_CN"),
            ("zh-HK", "zh_TW"),
            ("zh-MO", "zh_TW"),
            ("zh-Hans-CN-u-ca-chinese", "zh_CN"),
            ("zh-Latn-CN", "ja_JP"),
            ("zh-x-Hans-CN", "ja_JP"),
        ] {
            assert_eq!(
                select_supported_locale([input, "ja-CN"].map(str::to_owned), &available),
                expected,
                "{input}"
            );
        }
        assert_eq!(
            select_supported_locale(["ja-CN", "zh-Hans-CN"].map(str::to_owned), &available),
            "ja_JP"
        );
        assert_eq!(
            select_supported_locale(
                ["zh-Hans-CN", "ja-CN"].map(str::to_owned),
                &["en_EN", "ja_JP"].map(str::to_owned)
            ),
            "ja_JP"
        );
    }
}
