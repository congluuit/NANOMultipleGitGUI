use serde::Serialize;
use specta::Type;
use tauri::State;

use crate::error::AppResult;
use crate::settings::{self, AppSettings, SettingsPatch};
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub os: String,
    pub arch: String,
    /// `https://github.com/<owner>/<repo>` of the project, when the build embedded one.
    pub repository_url: Option<String>,
}

impl AppInfo {
    pub fn current(name: &str, version: &str) -> Self {
        Self {
            name: name.to_owned(),
            version: version.to_owned(),
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
            repository_url: github_repo().map(|r| format!("https://github.com/{r}")),
        }
    }
}

/// The project's GitHub `owner/repo`, embedded at build time from `NANOMGG_GITHUB_REPO`
/// (environment or `<workspace>/.env`; CI sets it to `github.repository`).
pub fn github_repo() -> Option<&'static str> {
    option_env!("NANOMGG_GITHUB_REPO").filter(|r| valid_repo_slug(r))
}

fn valid_repo_slug(slug: &str) -> bool {
    let ok = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    matches!(slug.split_once('/'), Some((owner, repo)) if ok(owner) && ok(repo))
}

#[tauri::command]
#[specta::specta]
pub fn app_info(app: tauri::AppHandle) -> AppInfo {
    let pkg = app.package_info();
    AppInfo::current(&pkg.name, &pkg.version.to_string())
}

#[tauri::command]
#[specta::specta]
pub fn get_settings(state: State<'_, AppState>) -> AppResult<AppSettings> {
    settings::load(&state.storage)
}

#[tauri::command]
#[specta::specta]
pub fn update_settings(state: State<'_, AppState>, patch: SettingsPatch) -> AppResult<AppSettings> {
    tracing::debug!(?patch, "update_settings");
    settings::apply(&state.storage, patch)
}

#[cfg(test)]
mod tests {
    use super::valid_repo_slug;

    #[test]
    fn repo_slug_is_owner_slash_repo() {
        assert!(valid_repo_slug("nanomgg/nanomultiplegitgui"));
        assert!(valid_repo_slug("NANO-Pret/repo.name_2"));
        for bad in [
            "", "owner", "owner/", "/repo", "a/b/c", "../x", "o/..", "o w/r", "o/r?x=1",
        ] {
            assert!(!valid_repo_slug(bad), "{bad}");
        }
    }
}
