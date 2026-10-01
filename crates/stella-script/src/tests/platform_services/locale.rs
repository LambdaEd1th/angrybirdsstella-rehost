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
fn shipped_host_language_change_retranslates_cached_text_and_fonts() {
    let sandbox = ShippedDataSandbox::new("host-language-fonts");
    let runtime = StellaLua::new_with_resolution(&sandbox.data_root, 1024, 768).unwrap();
    runtime.enable_local_services().unwrap();
    runtime.set_preferred_language("ja-JP").unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    runtime.set_application_active(true).unwrap();
    runtime.post_application_resumed();
    for _ in 0..600 {
        runtime.update(1.0 / 60.0).unwrap();
        runtime.draw().unwrap();
    }
    runtime.set_cursor(972.0, 727.0, true).unwrap();
    runtime.update(1.0 / 60.0).unwrap();
    runtime.draw().unwrap();
    runtime.set_cursor(972.0, 727.0, false).unwrap();
    for _ in 0..120 {
        runtime.update(1.0 / 60.0).unwrap();
        runtime.draw().unwrap();
    }
    runtime.draw().unwrap();
    let before = runtime.take_text_commands();
    assert!(
        before
            .iter()
            .any(|command| command.text.contains("プライバシー")),
        "the original Japanese settings panel is open: {before:?}"
    );
    // Original score animations assign .text directly after Text initialization.
    // A locale refresh must reclip the current score, not restore its old value.
    runtime
        .execute_source(
            r#"
        local label = ui.Text:new{name = "hostLocaleScoreProbe", text = "123"}
        menuManager.currentRoot:addChild(label)
        label.text = "17030"
        label:clip()
        local heading = ui.Text:new{
            name = "hostLocaleHeadingProbe", text = "TEXT_LEVEL_COMPLETE", substitutes = {"1"}
        }
        menuManager.currentRoot:addChild(heading)
    "#,
        )
        .unwrap();
    for language in ["zh-CN", "ja-JP", "zh-CN"] {
        runtime.set_preferred_language(language).unwrap();
        runtime.update(1.0 / 60.0).unwrap();
        runtime.draw().unwrap();
        let commands = runtime.take_text_commands();
        let text = commands
            .iter()
            .map(|command| command.text.as_str())
            .collect::<String>();
        if language == "zh-CN" {
            for label in ["连接", "隐私政策", "用户协议"] {
                assert!(
                    text.contains(label),
                    "the settings label retranslated: {text}"
                );
            }
        } else {
            assert!(text.contains("プライバシー"), "Japanese restored: {text}");
        }
        assert!(text.contains("17030"), "the current score survives: {text}");
        for command in commands {
            let Some(TextFontBinding::Bitmap { font, .. }) = &command.font_binding else {
                panic!("the original settings label uses a bitmap font");
            };
            for character in command.text.chars() {
                assert!(
                    font.glyph(character as u32).is_some(),
                    "{} lacks {character}",
                    command.font
                );
            }
        }
    }
}

#[test]
fn shipped_host_language_selection_covers_all_locales_and_survives_resume() {
    let sandbox = ShippedDataSandbox::new("host-language-selection");
    let table = LocalizationTable::parse(
        &fs::read(sandbox.data_root.join("localization/TEXTS_BASIC.dat")).unwrap(),
    )
    .unwrap();
    let text_index = table
        .ids
        .iter()
        .position(|id| id == "TEXT_LEVEL_COMPLETE")
        .unwrap();
    let runtime = StellaLua::new(&sandbox.data_root).unwrap();
    runtime.set_preferred_language("ja-JP").unwrap();
    runtime.boot("scripts/game.lua").unwrap();
    assert_eq!(runtime.current_locale().unwrap(), "ja_JP");
    for (index, locale) in table.locales.iter().enumerate() {
        runtime.set_preferred_language(locale).unwrap();
        runtime.post_application_resumed();
        runtime.update(1.0 / 60.0).unwrap();
        assert_eq!(runtime.current_locale().unwrap(), *locale);
        let translated: String = runtime
            .lua()
            .load("return res.getString('TEXTS_BASIC', 'TEXT_LEVEL_COMPLETE')")
            .set_environment(game_environment(runtime.lua()).unwrap())
            .eval()
            .unwrap();
        assert_eq!(
            translated, table.translations[index][text_index],
            "{locale}"
        );
    }
    runtime.set_preferred_language("unsupported").unwrap();
    assert_eq!(runtime.current_locale().unwrap(), "en_EN");
    assert!(runtime.fallback_calls.lock().unwrap().is_empty());
    assert!(runtime.compatibility_bindings.lock().unwrap().is_empty());
}

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
