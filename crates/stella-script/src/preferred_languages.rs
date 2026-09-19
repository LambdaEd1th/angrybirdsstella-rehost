//! Process language preferences shared by locale selection and native text fallback.

use std::ffi::OsString;

/// Freeze the host's ordered language preferences for subsystems whose native
/// counterpart resolves locale-sensitive resources once per process.
pub(crate) fn host_preferred_languages() -> Vec<String> {
    preferred_languages_from_sources(
        std::env::var_os("STELLA_LOCALE"),
        platform_preferred_languages(),
        ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(std::env::var_os),
    )
}

fn preferred_languages_from_sources(
    stella_locale: Option<OsString>,
    platform: impl IntoIterator<Item = String>,
    environment: impl IntoIterator<Item = OsString>,
) -> Vec<String> {
    if let Some(locale) = stella_locale
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty() && value != "auto")
    {
        return vec![locale];
    }

    let mut candidates = Vec::new();
    for language in platform {
        push_language_tokens(&mut candidates, &language);
    }
    for value in environment {
        let Some(value) = value.into_string().ok() else {
            continue;
        };
        push_language_tokens(&mut candidates, &value);
    }
    candidates
}

fn push_language_tokens(candidates: &mut Vec<String>, value: &str) {
    for locale in value.split(':').filter(|locale| !locale.is_empty()) {
        let locale = locale.split(['.', '@']).next().unwrap_or(locale);
        if !locale.is_empty() && !candidates.iter().any(|candidate| candidate == locale) {
            candidates.push(locale.to_owned());
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_preferred_languages() -> Vec<String> {
    macos::preferred_languages()
}

#[cfg(target_os = "windows")]
fn platform_preferred_languages() -> Vec<String> {
    windows::preferred_languages()
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_preferred_languages() -> Vec<String> {
    Vec::new()
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::{CStr, c_char, c_void};

    type CfIndex = isize;
    const UTF8_ENCODING: u32 = 0x0800_0100;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFLocaleCopyPreferredLanguages() -> *const c_void;
        fn CFArrayGetCount(array: *const c_void) -> CfIndex;
        fn CFArrayGetValueAtIndex(array: *const c_void, index: CfIndex) -> *const c_void;
        fn CFStringGetLength(string: *const c_void) -> CfIndex;
        fn CFStringGetMaximumSizeForEncoding(length: CfIndex, encoding: u32) -> CfIndex;
        fn CFStringGetCString(
            string: *const c_void,
            buffer: *mut c_char,
            buffer_size: CfIndex,
            encoding: u32,
        ) -> u8;
        fn CFRelease(value: *const c_void);
    }

    pub(super) fn preferred_languages() -> Vec<String> {
        // SAFETY: CoreFoundation returns a retained CFArray whose members are
        // borrowed CFStrings. Every pointer is checked before use and the array
        // is released exactly once after all strings have been copied.
        unsafe {
            let array = CFLocaleCopyPreferredLanguages();
            if array.is_null() {
                return Vec::new();
            }
            let mut languages = Vec::new();
            let count = CFArrayGetCount(array).max(0);
            for index in 0..count {
                let string = CFArrayGetValueAtIndex(array, index);
                if string.is_null() {
                    continue;
                }
                let length = CFStringGetLength(string);
                let maximum = CFStringGetMaximumSizeForEncoding(length, UTF8_ENCODING);
                let Some(capacity) = maximum
                    .checked_add(1)
                    .and_then(|value| usize::try_from(value).ok())
                    .filter(|capacity| *capacity > 0)
                else {
                    continue;
                };
                let mut buffer = vec![0_u8; capacity];
                if CFStringGetCString(
                    string,
                    buffer.as_mut_ptr().cast(),
                    capacity as CfIndex,
                    UTF8_ENCODING,
                ) == 0
                {
                    continue;
                }
                if let Ok(language) = CStr::from_ptr(buffer.as_ptr().cast()).to_str() {
                    languages.push(language.to_owned());
                }
            }
            CFRelease(array);
            languages
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::ptr;

    const MUI_LANGUAGE_NAME: u32 = 0x8;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetUserPreferredUILanguages(
            flags: u32,
            language_count: *mut u32,
            language_buffer: *mut u16,
            buffer_characters: *mut u32,
        ) -> i32;
    }

    pub(super) fn preferred_languages() -> Vec<String> {
        // SAFETY: the first call requests the exact MULTI_SZ length; the second
        // owns a buffer of that size and parsing stays within the returned slice.
        unsafe {
            let mut count = 0_u32;
            let mut characters = 0_u32;
            let _ = GetUserPreferredUILanguages(
                MUI_LANGUAGE_NAME,
                &mut count,
                ptr::null_mut(),
                &mut characters,
            );
            let Ok(capacity) = usize::try_from(characters) else {
                return Vec::new();
            };
            if capacity == 0 {
                return Vec::new();
            }
            let mut buffer = vec![0_u16; capacity];
            if GetUserPreferredUILanguages(
                MUI_LANGUAGE_NAME,
                &mut count,
                buffer.as_mut_ptr(),
                &mut characters,
            ) == 0
            {
                return Vec::new();
            }

            let used = usize::try_from(characters)
                .ok()
                .map_or(buffer.len(), |used| used.min(buffer.len()));
            buffer[..used]
                .split(|unit| *unit == 0)
                .filter(|language| !language.is_empty())
                .filter_map(|language| String::from_utf16(language).ok())
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_stella_locale_replaces_platform_and_environment_preferences() {
        assert_eq!(
            preferred_languages_from_sources(
                Some(OsString::from("ja_JP")),
                ["zh-Hans-CN".to_owned()],
                [OsString::from("fr_FR:de_DE")],
            ),
            ["ja_JP"]
        );
    }

    #[test]
    fn automatic_locale_keeps_native_order_then_existing_environment_semantics() {
        assert_eq!(
            preferred_languages_from_sources(
                Some(OsString::from("auto")),
                ["zh-Hans-CN".to_owned(), "ja-CN".to_owned()],
                [
                    OsString::from("ja_JP:fr_FR.UTF-8"),
                    OsString::from("de_DE@euro"),
                ],
            ),
            ["zh-Hans-CN", "ja-CN", "ja_JP", "fr_FR", "de_DE"]
        );
    }
}
