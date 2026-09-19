//! NSBundle-style .lproj strings for the original platform account artwork.

use std::{collections::HashMap, path::Path};

#[derive(Default)]
pub(super) struct Strings(HashMap<String, String>, bool);

impl Strings {
    pub(super) fn load(data: &Path, preferences: &[String]) -> Self {
        for language in preferences {
            let normalized = language.replace('_', "-");
            let normalized = normalized.split(['.', '@']).next().unwrap_or("en");
            let lower = normalized.to_ascii_lowercase();
            let base = normalized.split('-').next().unwrap_or(normalized);
            let chinese = if lower.starts_with("zh-hant") || lower == "zh-tw" || lower == "zh-hk" {
                Some("zh-Hant")
            } else if lower.starts_with("zh") {
                Some("zh-Hans")
            } else {
                None
            };
            for candidate in [chinese.unwrap_or(normalized), base] {
                // No language preference may escape its .lproj directory.
                if !candidate
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
                {
                    continue;
                }
                if candidate == "en" {
                    return Self::default();
                }
                let path = data
                    .join("skynestdata")
                    .join(format!("{candidate}.lproj/Localizable.strings"));
                if let Ok(text) = std::fs::read_to_string(path) {
                    return Self(parse(&text), candidate == "ru");
                }
            }
        }
        Self::default()
    }

    pub(super) fn get<'a>(&'a self, key: &str, fallback: &'a str) -> &'a str {
        self.0.get(key).map_or(fallback, String::as_str)
    }

    pub(super) fn is_russian(&self) -> bool {
        self.1
    }
}

fn parse(source: &str) -> HashMap<String, String> {
    let mut chars = source.chars().peekable();
    let mut pairs = HashMap::new();
    loop {
        skip_space(&mut chars);
        let Some(key) = quoted(&mut chars) else { break };
        skip_space(&mut chars);
        if chars.next() != Some('=') {
            break;
        }
        skip_space(&mut chars);
        let Some(value) = quoted(&mut chars) else {
            break;
        };
        pairs.insert(key, value);
        skip_space(&mut chars);
        if chars.next() != Some(';') {
            break;
        }
    }
    pairs
}

fn skip_space(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) {
    loop {
        while chars
            .peek()
            .is_some_and(|ch| ch.is_whitespace() || *ch == '\u{feff}')
        {
            chars.next();
        }
        let mut look = chars.clone();
        if look.next() != Some('/') {
            return;
        }
        match look.next() {
            Some('/') => {
                chars.next();
                chars.next();
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        break;
                    }
                }
            }
            Some('*') => {
                chars.next();
                chars.next();
                let mut star = false;
                for ch in chars.by_ref() {
                    if star && ch == '/' {
                        break;
                    }
                    star = ch == '*';
                }
            }
            _ => return,
        }
    }
}

fn quoted(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> Option<String> {
    if chars.next()? != '"' {
        return None;
    }
    let mut output = String::new();
    loop {
        match chars.next()? {
            '"' => return Some(output),
            '\\' => match chars.next()? {
                'n' => output.push('\n'),
                'r' => output.push('\r'),
                't' => output.push('\t'),
                ch => output.push(ch),
            },
            ch => output.push(ch),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_preserve_unicode_escapes_and_comment_boundaries() {
        let strings = parse(
            r#"/* quoted " ignored */ "email" = "邮件";
            // another "ignored"
            "message" = "first\n\"second\""; "slash" = "https://example/";"#,
        );
        assert_eq!(strings["email"], "邮件");
        assert_eq!(strings["message"], "first\n\"second\"");
        assert_eq!(strings["slash"], "https://example/");
    }

    #[test]
    fn shipped_locale_selection_uses_lproj_names_and_english_fallback() {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/data");
        if !data.join("skynestdata/zh-Hans.lproj").is_dir() {
            return;
        }
        assert_eq!(
            Strings::load(&data, &["zh_CN".into()]).get("rovio_id_sign_up", "Sign up"),
            "注册"
        );
        assert_eq!(
            Strings::load(&data, &["en-US".into(), "zh-CN".into()])
                .get("rovio_id_sign_up", "Sign up"),
            "Sign up"
        );
    }
}
