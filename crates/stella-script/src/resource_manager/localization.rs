//! ResourceManager localization table loading and resource-name normalization.

use std::{collections::BTreeMap, fs, path::Path};

use stella_assets::ka3d::LocalizationTable;

#[derive(Debug, Default)]
pub(crate) struct LocaleRuntime {
    pub(crate) current: String,
    /// locale -> text-group name -> localized key/value table.
    pub(crate) loaded: BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>>,
}

pub(crate) fn load_localized_strings(data_root: &Path, locale: &str) -> BTreeMap<String, String> {
    load_localized_string_group(data_root, "TEXTS_BASIC", locale)
}

pub(crate) fn resource_file_stem(path: &str) -> String {
    // game::FilePath::setPath at sub_1004FCEDC first canonicalises both
    // separator spellings, then treats everything before the first colon as
    // the drive and everything after the final slash as the filename. Its
    // `+0x18` field is the bytes before the final dot. `std::path::Path`
    // cannot be used here because its separator and dot-file semantics vary
    // with the host platform and disagree with Purple for `.name`.
    let normalized = resource_normalized_path(path);
    let without_drive = normalized
        .split_once(':')
        .map_or(normalized.as_str(), |(_, remainder)| remainder);
    let filename = without_drive.rsplit('/').next().unwrap_or(without_drive);
    filename
        .rfind('.')
        .map_or(filename, |dot| &filename[..dot])
        .to_owned()
}

/// Return `game::FilePath`'s exact final suffix, including its leading dot.
///
/// The resource constructors compare this field byte-for-byte with `.dat`
/// and `.json`; in particular, `.DAT` is not accepted by Purple's dispatch.
pub(crate) fn resource_file_extension(path: &str) -> String {
    let normalized = resource_normalized_path(path);
    let without_drive = normalized
        .split_once(':')
        .map_or(normalized.as_str(), |(_, remainder)| remainder);
    let filename = without_drive.rsplit('/').next().unwrap_or(without_drive);
    filename
        .rfind('.')
        .map_or("", |dot| &filename[dot..])
        .to_owned()
}

/// Sprite-sheet and CompoSpriteSet maps use a doubly parsed FilePath stem.
///
/// `sub_100457E38`, `sub_1004586AC`, `sub_10045ABC4`, and `sub_10045AF5C`
/// construct a FilePath from the caller path, copy its `+0x18` stem into a
/// string, construct a second FilePath from that string, and use the second
/// `+0x18` stem as the map key.
pub(crate) fn resource_double_file_stem(path: &str) -> String {
    resource_file_stem(&resource_file_stem(path))
}

/// Join ResourceManager's current path and a resource filename like
/// `game::FilePath(base, child)` (`sub_1004FD2F0`).
pub(crate) fn resource_join_path(base: &str, child: &str) -> String {
    let child = if !base.is_empty() && matches!(child.as_bytes().first(), Some(b'/') | Some(b'\\'))
    {
        &child[1..]
    } else {
        child
    };
    let mut joined = String::with_capacity(base.len() + child.len() + 1);
    joined.push_str(base);
    if !base.is_empty() && !matches!(base.as_bytes().last(), Some(b'/') | Some(b'\\')) {
        joined.push('/');
    }
    joined.push_str(child);
    resource_normalized_path(&joined)
}

/// Reproduce the path spelling pass in `sub_1004FCEDC` without consulting the
/// host filesystem. Besides separator and drive normalisation, Purple removes
/// `./` and a preceding segment followed by `../`. A leading `../` stops that
/// pass and remains untouched.
pub(crate) fn resource_normalized_path(path: &str) -> String {
    let mut bytes = path.replace('\\', "/").into_bytes();
    if bytes.contains(&b':') && bytes.first().is_some_and(u8::is_ascii_lowercase) {
        bytes[0] = bytes[0].to_ascii_uppercase();
    }

    let mut cursor = 0usize;
    while cursor + 1 < bytes.len() {
        if bytes[cursor] != b'.' {
            cursor += 1;
            continue;
        }
        if bytes[cursor + 1] == b'/' {
            let remove_from = bytes[..cursor]
                .iter()
                .rposition(|byte| *byte == b'/')
                .map_or(0, |slash| slash + 1);
            bytes.drain(remove_from..cursor + 2);
            cursor = remove_from.saturating_sub(1);
            continue;
        }
        if bytes[cursor + 1] != b'.' || cursor + 2 >= bytes.len() {
            cursor += 1;
            continue;
        }
        if cursor == 0 {
            break;
        }

        let prefix_without_separator = if bytes.get(cursor - 1) == Some(&b'/') {
            cursor - 1
        } else {
            cursor
        };
        let remove_from = bytes[..prefix_without_separator]
            .iter()
            .rposition(|byte| *byte == b'/')
            .map_or(0, |slash| slash + 1);
        // The native routine advances three bytes after `..`; valid resource
        // paths put a slash in the third slot.
        bytes.drain(remove_from..cursor + 3);
        cursor = remove_from.saturating_sub(1);
    }

    String::from_utf8(bytes).expect("removing ASCII path segments preserves UTF-8")
}

pub(crate) fn load_localized_string_group(
    data_root: &Path,
    group: &str,
    locale: &str,
) -> BTreeMap<String, String> {
    let file_name = if Path::new(group).extension().is_some() {
        group.to_owned()
    } else {
        format!("{group}.dat")
    };
    let Ok(bytes) = fs::read(data_root.join("localization").join(file_name)) else {
        return BTreeMap::new();
    };
    let Ok(table) = LocalizationTable::parse(&bytes) else {
        return BTreeMap::new();
    };
    let locale_index = table
        .locales
        .iter()
        .position(|known| known == locale)
        .unwrap_or(0);
    let Some(values) = table.translations.get(locale_index) else {
        return BTreeMap::new();
    };
    table.ids.into_iter().zip(values.iter().cloned()).collect()
}

pub(crate) fn localized_string_groups_from_table(
    table: &LocalizationTable,
    locale: &str,
) -> Option<BTreeMap<String, BTreeMap<String, String>>> {
    let indices = if locale == "ALL" {
        (0..table.locales.len()).collect::<Vec<_>>()
    } else {
        vec![table.locales.iter().position(|known| known == locale)?]
    };
    let mut groups = BTreeMap::new();
    for index in indices {
        let values = table.translations.get(index)?;
        groups.insert(
            table.locales[index].clone(),
            table
                .ids
                .iter()
                .cloned()
                .zip(values.iter().cloned())
                .collect(),
        );
    }
    Some(groups)
}

pub(crate) fn localization_table_has_locale(table: &LocalizationTable, locale: &str) -> bool {
    table.locales.iter().any(|known| known == locale)
}

#[cfg(test)]
mod tests {
    use super::{
        resource_double_file_stem, resource_file_extension, resource_file_stem, resource_join_path,
        resource_normalized_path,
    };

    #[test]
    fn native_file_stems_are_host_independent() {
        assert_eq!(
            resource_file_stem("fonts/1024x768/FONT_BASIC.dat"),
            "FONT_BASIC"
        );
        assert_eq!(
            resource_file_stem(r"fonts\1024x768\FONT.PROFILE.dat"),
            "FONT.PROFILE"
        );
        assert_eq!(resource_file_stem(r"c:\fonts\FONT.dat"), "FONT");
        assert_eq!(resource_file_stem("images/.hidden"), "");
        assert_eq!(resource_file_stem("NAME"), "NAME");
        assert_eq!(resource_file_extension("images/.hidden"), ".hidden");
        assert_eq!(resource_file_extension("images/SHEET.dat"), ".dat");
        assert_eq!(resource_file_extension("images/SHEET.DAT"), ".DAT");
        assert_eq!(resource_file_extension("images/SHEET"), "");
        assert_eq!(resource_double_file_stem("images/MENU.PROFILE.dat"), "MENU");
        assert_eq!(
            resource_double_file_stem(r"images\MENU.PROFILE.json"),
            "MENU"
        );
        assert_eq!(
            resource_join_path(r"c:\game\images", r"\1024x768\MENU.dat"),
            "C:/game/images/1024x768/MENU.dat"
        );
        assert_eq!(resource_join_path("", "/MENU.dat"), "/MENU.dat");
        assert_eq!(
            resource_normalized_path(r"c:\game\.\images\old\..\MENU.dat"),
            "C:/game/images/MENU.dat"
        );
        assert_eq!(
            resource_normalized_path("../old/./MENU.dat"),
            "../old/./MENU.dat"
        );
        assert_eq!(resource_normalized_path("directory./MENU.dat"), "MENU.dat");
    }
}
