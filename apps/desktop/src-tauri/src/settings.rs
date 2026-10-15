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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: ThemeMode,
    pub language: Option<Language>,
    /// Show CI results (GitHub Actions / GitLab pipelines). Off by default: most people only
    /// care whether push / pull worked.
    pub ci_enabled: bool,
    /// Load author avatars for the commit graph from GitHub / Gravatar.
    pub author_avatars: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: ThemeMode::default(),
            language: None,
            ci_enabled: false,
            author_avatars: true,
        }
    }
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
    #[serde(default)]
    #[specta(optional)]
    pub ci_enabled: Option<bool>,
    #[serde(default)]
    #[specta(optional)]
    pub author_avatars: Option<bool>,
}

const KEY_THEME: &str = "theme";
const KEY_LANGUAGE: &str = "language";
const KEY_CI_ENABLED: &str = "ciEnabled";
const KEY_AUTHOR_AVATARS: &str = "authorAvatars";

pub fn load(db: &Storage) -> AppResult<AppSettings> {
    let defaults = AppSettings::default();
    Ok(AppSettings {
        theme: db.get_setting(KEY_THEME)?.unwrap_or_default(),
        language: db.get_setting(KEY_LANGUAGE)?,
        ci_enabled: db
            .get_setting(KEY_CI_ENABLED)?
            .unwrap_or(defaults.ci_enabled),
        author_avatars: db
            .get_setting(KEY_AUTHOR_AVATARS)?
            .unwrap_or(defaults.author_avatars),
    })
}

pub fn apply(db: &Storage, patch: SettingsPatch) -> AppResult<AppSettings> {
    if let Some(theme) = patch.theme {
        db.set_setting(KEY_THEME, &theme)?;
    }
    if let Some(language) = patch.language {
        db.set_setting(KEY_LANGUAGE, &language)?;
    }
    if let Some(on) = patch.ci_enabled {
        db.set_setting(KEY_CI_ENABLED, &on)?;
    }
    if let Some(on) = patch.author_avatars {
        db.set_setting(KEY_AUTHOR_AVATARS, &on)?;
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
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(s.theme, ThemeMode::Dark);
        assert_eq!(s.language, None);

        let s = apply(
            &db,
            SettingsPatch {
                language: Some(Language::Vi),
                ..Default::default()
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
                ..Default::default()
            },
        )
        .unwrap();
        let raw = db.settings().unwrap();
        assert_eq!(raw["theme"], serde_json::json!("light"));
        assert_eq!(raw["language"], serde_json::json!("en"));
    }

    #[test]
    fn ci_is_off_and_avatars_on_by_default() {
        let db = Storage::open_in_memory().unwrap();
        let s = load(&db).unwrap();
        assert!(!s.ci_enabled);
        assert!(s.author_avatars);
        let s = apply(
            &db,
            SettingsPatch {
                ci_enabled: Some(true),
                author_avatars: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(s.ci_enabled);
        assert!(!s.author_avatars);
        assert_eq!(s.theme, ThemeMode::System);
    }
}
