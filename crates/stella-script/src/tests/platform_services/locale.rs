//! Full shipped-script locale selection in isolated child processes.
//!
//! Environment overrides belong only to Command children, never the parallel
//! parent test process. Each child has its own empty appdata directory.

use super::*;
use stella_assets::ka3d::LocalizationTable;

const CHILD_DATA: &str = "STELLA_LOCALE_CHILD_DATA";
const CHILD_EXPECTED: &str = "STELLA_LOCALE_CHILD_EXPECTED";
const CHILD_TEST: &str = "tests::platform_services::locale::shipped_locale_subprocess_probe";

#[test]
fn shipped_locale_subprocess_probe() {
    let Some(data_root) = std::env::var_os(CHILD_DATA) else {
        return;
    };
    let data_root = std::path::PathBuf::from(data_root);
    let expected_locale = std::env::var(CHILD_EXPECTED).unwrap();
    let input_locale = std::env::var("STELLA_LOCALE").unwrap();

    // Resource locale normalization must not rewrite the shared BCP-47 list
    // consumed separately by system-font fallback.
    assert_eq!(
        crate::preferred_languages::host_preferred_languages(),
        vec![input_locale]
    );

    let bytes = fs::read(data_root.join("localization/TEXTS_BASIC.dat")).unwrap();
    let table = LocalizationTable::parse(&bytes).unwrap();
    let locale_index = table
        .locales
        .iter()
        .position(|locale| locale == &expected_locale)
        .expect("expected fixture locale exists in shipped TEXTS_BASIC");
    let text_index = table
        .ids
        .iter()
        .position(|id| id == "TEXT_LEVEL_COMPLETE")
        .expect("level-complete text exists in shipped TEXTS_BASIC");
    let expected_text = &table.translations[locale_index][text_index];
    assert!(!expected_text.is_empty());

    let runtime = StellaLua::new_with_resolution(&data_root, 1024, 768).unwrap();
    // Do not call useLocale/loadLocale here: the original entry point and its
    // refreshCurrentLocale call must select and load the translation themselves.
    runtime.boot("scripts/game.lua").unwrap();
    runtime
        .execute_source(
            r#"
                locale_boot_selected = res.getLocale()
                locale_boot_level_complete = res.getString(
                    "TEXTS_BASIC", "TEXT_LEVEL_COMPLETE"
                )
            "#,
        )
        .unwrap();
    let environment = game_environment(runtime.lua()).unwrap();
    assert_eq!(
        environment.get::<String>("locale_boot_selected").unwrap(),
        expected_locale
    );
    assert_eq!(
        environment
            .get::<String>("locale_boot_level_complete")
            .unwrap(),
        *expected_text
    );
    println!("locale-boot-probe-ok:{expected_locale}");
}

#[test]
fn shipped_locale_boot_uses_chinese_script_japanese_and_two_letter_preferences() {
    let parent_locale = std::env::var_os("STELLA_LOCALE");
    for (input, expected) in [
        ("zh-Hans-CN", "zh_CN"),
        ("zh-Hant-TW", "zh_TW"),
        ("ja-CN", "ja_JP"),
        ("fr", "fr_FR"),
    ] {
        let sandbox = ShippedDataSandbox::new("locale-shipped-boot");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD_TEST, "--nocapture"])
            .env(CHILD_DATA, &sandbox.data_root)
            .env(CHILD_EXPECTED, expected)
            .env("STELLA_LOCALE", input)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "isolated locale boot {input} -> {expected} failed:\n{stdout}\n{stderr}"
        );
        // A misspelled exact test filter must not pass with zero tests run.
        let completion_marker = format!("locale-boot-probe-ok:{expected}");
        assert!(
            stdout.lines().any(|line| line == completion_marker),
            "locale child did not execute its assertions: {stdout}"
        );
        assert_eq!(std::env::var_os("STELLA_LOCALE"), parent_locale);
    }
}
