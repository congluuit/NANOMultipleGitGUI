use nanomgg_storage::Storage;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::error::AppResult;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ThemeMode {
    Light,
    Dark,
    #[default]
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Language {
    Vi,
    En,
}

/// User preferences persisted in SQLite. `language: None` means "follow the OS locale".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: ThemeMode,
    pub language: Option<Language>,
}

/// Partial update; omitted fields keep their stored value.
#[derive(Debug, Clone, Default, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    #[serde(default)]
    #[specta(optional)]
    pub theme: Option<ThemeMode>,
    #[serde(default)]
    #[specta(optional)]
    pub language: Option<Language>,
}

const KEY_THEME: &str = "theme";
const KEY_LANGUAGE: &str = "language";

pub fn load(db: &Storage) -> AppResult<AppSettings> {
    Ok(AppSettings {
        theme: db.get_setting(KEY_THEME)?.unwrap_or_default(),
        language: db.get_setting(KEY_LANGUAGE)?,
    })
}

pub fn apply(db: &Storage, patch: SettingsPatch) -> AppResult<AppSettings> {
    if let Some(theme) = patch.theme {
        db.set_setting(KEY_THEME, &theme)?;
    }
    if let Some(language) = patch.language {
        db.set_setting(KEY_LANGUAGE, &language)?;
    }
    load(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_nothing_stored() {
        let db = Storage::open_in_memory().unwrap();
        assert_eq!(load(&db).unwrap(), AppSettings::default());
    }

    #[test]
    fn patch_updates_only_given_fields() {
        let db = Storage::open_in_memory().unwrap();
        let s = apply(
            &db,
            SettingsPatch {
                theme: Some(ThemeMode::Dark),
                language: None,
            },
        )
        .unwrap();
        assert_eq!(s.theme, ThemeMode::Dark);
        assert_eq!(s.language, None);

        let s = apply(
            &db,
            SettingsPatch {
                theme: None,
                language: Some(Language::Vi),
            },
        )
        .unwrap();
        assert_eq!(s.theme, ThemeMode::Dark);
        assert_eq!(s.language, Some(Language::Vi));
    }

    #[test]
    fn stored_as_camel_case_strings() {
        let db = Storage::open_in_memory().unwrap();
        apply(
            &db,
            SettingsPatch {
                theme: Some(ThemeMode::Light),
                language: Some(Language::En),
            },
        )
        .unwrap();
        let raw = db.settings().unwrap();
        assert_eq!(raw["theme"], serde_json::json!("light"));
        assert_eq!(raw["language"], serde_json::json!("en"));
    }
}
