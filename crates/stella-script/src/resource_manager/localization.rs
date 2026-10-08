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
