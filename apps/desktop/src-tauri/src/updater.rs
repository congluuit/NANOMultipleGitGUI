//! Check for Update (PLAN 3B.4): new versions published on the project's GitHub Releases.
//!
//! The updater is configured at runtime, not in `tauri.conf.json`, because the repository and
//! the minisign public key are only known to builds that embed `NANOMGG_GITHUB_REPO` and
//! `NANOMGG_UPDATER_PUBKEY` (CI, or a maintainer's `.env`). Builds without them report
//! "not configured" and never touch the network.
//!
//! Channels read static `latest.json` manifests that `release.yml` uploads:
//! - `stable`: `releases/latest/download/latest.json`. GitHub's `latest` alias only resolves
//!   to a **published** release that is not a pre-release, so drafts are never offered.
//! - `beta`: additionally `releases/download/updater-beta/latest.json`, a fixed pre-release
//!   whose manifest CI replaces when a beta is published. Both manifests are checked and the
//!   highest version wins, so beta users still get stable releases.
//!
//! Settings: `updater.channel`, `updater.skippedVersion` ("Bỏ qua bản này"),
//! `updater.lastCheck` (unix seconds; the startup check runs at most once a day).

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nanomgg_storage::Storage;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager, Runtime, Url};
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const KEY_CHANNEL: &str = "updater.channel";
pub const KEY_SKIPPED: &str = "updater.skippedVersion";
pub const KEY_LAST_CHECK: &str = "updater.lastCheck";

/// Tag of the pre-release that carries the beta channel's `latest.json`.
pub const BETA_TAG: &str = "updater-beta";

const CHECK_INTERVAL_SECS: i64 = 24 * 60 * 60;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Per read, not for the whole download: installers are tens of MB on slow links.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

/// What this build was compiled with.
#[derive(Debug, Clone, Copy)]
pub struct UpdaterBuild {
    pub repo: Option<&'static str>,
    pub pubkey: Option<&'static str>,
}

impl UpdaterBuild {
    pub fn current() -> Self {
        Self {
            repo: crate::commands::github_repo(),
            pubkey: option_env!("NANOMGG_UPDATER_PUBKEY")
                .map(str::trim)
                .filter(|k| !k.is_empty()),
        }
    }

    pub fn enabled(&self) -> bool {
        self.repo.is_some() && self.pubkey.is_some()
    }

    pub fn releases_url(&self) -> Option<String> {
        self.repo
            .map(|r| format!("https://github.com/{r}/releases"))
    }

    fn release_url(&self, version: &str) -> Option<String> {
        self.repo
            .map(|r| format!("https://github.com/{r}/releases/tag/v{version}"))
    }

    /// Manifests to check for `channel`; empty when the updater is not configured.
    pub fn manifests(&self, channel: UpdateChannel) -> Vec<Url> {
        let Some(repo) = self.repo.filter(|_| self.enabled()) else {
            return Vec::new();
        };
        let mut urls = vec![format!(
            "https://github.com/{repo}/releases/latest/download/latest.json"
        )];
        if channel == UpdateChannel::Beta {
            urls.push(format!(
                "https://github.com/{repo}/releases/download/{BETA_TAG}/latest.json"
            ));
        }
        urls.iter().filter_map(|u| Url::parse(u).ok()).collect()
    }
}

/// The updater plugin refuses to start without a `plugins.updater` section, which
/// `tauri.conf.json` deliberately lacks (see the module docs); give it an empty one.
/// The public key and endpoints are set per check.
pub fn ensure_plugin_config(config: &mut tauri::Config) {
    let entry = config
        .plugins
        .0
        .entry("updater".to_owned())
        .or_insert(serde_json::Value::Null);
    if !entry.is_object() {
        *entry = serde_json::json!({});
    }
    if let Some(obj) = entry.as_object_mut() {
        obj.entry("pubkey")
            .or_insert_with(|| serde_json::Value::String(String::new()));
    }
}

// ----- DTOs ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdaterStatus {
    /// The build embeds a repository and a public key.
    pub configured: bool,
    pub current_version: String,
    pub channel: UpdateChannel,
    /// Unix seconds of the last check (startup or manual).
    pub last_check: Option<i64>,
    pub skipped_version: Option<String>,
    /// `https://github.com/<owner>/<repo>/releases` when the build knows its repository.
    pub releases_url: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UpdateCheckStatus {
    NotConfigured,
    /// Startup check skipped: the last one was less than a day ago.
    NotDue,
    UpToDate,
    Available,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AvailableUpdate {
    pub version: String,
    /// Unix seconds, from the manifest's `pub_date`.
    pub date: Option<i64>,
    /// Plain text (Markdown source) — never rendered as HTML.
    pub notes: Option<String>,
    /// The user chose "Bỏ qua bản này" for this version.
    pub skipped: bool,
    pub release_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheck {
    pub status: UpdateCheckStatus,
    pub current_version: String,
    pub releases_url: Option<String>,
    pub update: Option<AvailableUpdate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UpdatePhase {
    Downloading,
    Installing,
    Restarting,
}

/// Progress of `updater_install`, at most ~7 per second (phase changes always sent).
#[derive(Debug, Clone, Serialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub phase: UpdatePhase,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub percent: Option<u8>,
}

/// Failure of a check or an install, serialized as `{ kind, message }`; `message` is the
/// technical detail (for "Chi tiết" / logs), the UI explains each kind in plain words.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Type)]
#[serde(tag = "kind", content = "message", rename_all = "camelCase")]
pub enum UpdateError {
    /// No connection to GitHub (offline, DNS, proxy, timeout).
    #[error("offline: {0}")]
    Offline(String),
    /// No published release carries a `latest.json` yet (404).
    #[error("no release: {0}")]
    NoRelease(String),
    /// The release has no update package for this OS / CPU.
    #[error("no build for this platform: {0}")]
    NoBuildForPlatform(String),
    #[error("invalid update manifest: {0}")]
    BadManifest(String),
    /// The package is not signed with the key built into the app.
    #[error("bad signature: {0}")]
    BadSignature(String),
    #[error("download failed: {0}")]
    DownloadFailed(String),
    #[error("not configured: {0}")]
    NotConfigured(String),
    /// `updater_install` without a matching successful check (or after a channel change).
    #[error("no pending update: {0}")]
    NoPendingUpdate(String),
    /// An install is already running.
    #[error("busy: {0}")]
    Busy(String),
    /// The OS password prompt (Linux packages) was dismissed.
    #[error("cancelled: {0}")]
    Cancelled(String),
    #[error("{0}")]
    Failed(String),
}

impl From<AppError> for UpdateError {
    fn from(e: AppError) -> Self {
        Self::Failed(e.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Check,
    Download,
    Install,
}

fn classify(err: &tauri_plugin_updater::Error, stage: Stage) -> UpdateError {
    use tauri_plugin_updater::Error as E;
    let msg = nanomgg_logging::redact(&err.to_string()).into_owned();
    match err {
        E::Reqwest(e) if e.is_connect() || e.is_timeout() => UpdateError::Offline(msg),
        E::Reqwest(e) if e.is_decode() && stage == Stage::Check => UpdateError::BadManifest(msg),
        E::Reqwest(_) | E::Network(_) if stage != Stage::Check => UpdateError::DownloadFailed(msg),
        E::Reqwest(_) => UpdateError::Offline(msg),
        E::ReleaseNotFound => UpdateError::NoRelease(msg),
        E::TargetNotFound(_) | E::TargetsNotFound(_) | E::UnsupportedOs | E::UnsupportedArch => {
            UpdateError::NoBuildForPlatform(msg)
        }
        E::Serialization(_) | E::Semver(_) | E::UrlParse(_) => UpdateError::BadManifest(msg),
        E::Minisign(_)
        | E::Base64(_)
        | E::SignatureUtf8(_)
        | E::SignedVersionMismatch { .. }
        | E::MissingSignedVersion => UpdateError::BadSignature(msg),
        E::EmptyEndpoints => UpdateError::NotConfigured(msg),
        E::AuthenticationFailed => UpdateError::Cancelled(msg),
        _ => UpdateError::Failed(msg),
    }
}

// ----- Pure logic ----------------------------------------------------------------------------

/// The startup check runs when there was none yet, a day has passed, or the clock went back.
pub fn check_due(last_check: Option<i64>, now: i64) -> bool {
    match last_check {
        None => true,
        Some(last) => last > now || now - last >= CHECK_INTERVAL_SECS,
    }
}

fn parse_version(v: &str) -> Option<semver::Version> {
    semver::Version::parse(v.trim().trim_start_matches('v')).ok()
}

/// `version` is the one the user chose to skip (`v` prefix and build metadata ignored).
pub fn is_skipped(skipped: Option<&str>, version: &str) -> bool {
    let Some(skipped) = skipped else {
        return false;
    };
    match (parse_version(skipped), parse_version(version)) {
        (Some(a), Some(b)) => a.cmp_precedence(&b).is_eq(),
        _ => skipped.trim() == version.trim(),
    }
}

/// Highest version among `candidates` (semver precedence; unparsable versions lose).
fn pick_latest<T>(candidates: Vec<T>, version: impl Fn(&T) -> &str) -> Option<T> {
    candidates.into_iter().max_by(|a, b| {
        match (parse_version(version(a)), parse_version(version(b))) {
            (Some(a), Some(b)) => a.cmp_precedence(&b),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => std::cmp::Ordering::Equal,
        }
    })
}

fn percent(downloaded: u64, total: Option<u64>) -> Option<u8> {
    total
        .filter(|t| *t > 0)
        .map(|t| (downloaded.min(t) * 100 / t) as u8)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

// ----- Settings ------------------------------------------------------------------------------

struct Stored {
    channel: UpdateChannel,
    last_check: Option<i64>,
    skipped: Option<String>,
}

fn load_stored(db: &Storage) -> AppResult<Stored> {
    Ok(Stored {
        channel: db.get_setting(KEY_CHANNEL)?.unwrap_or_default(),
        last_check: db.get_setting(KEY_LAST_CHECK)?,
        skipped: db.get_setting(KEY_SKIPPED)?,
    })
}

fn status_of(db: &Storage, build: UpdaterBuild, current_version: &str) -> AppResult<UpdaterStatus> {
    let s = load_stored(db)?;
    Ok(UpdaterStatus {
        configured: build.enabled(),
        current_version: current_version.to_owned(),
        channel: s.channel,
        last_check: s.last_check,
        skipped_version: s.skipped,
        releases_url: build.releases_url(),
    })
}

fn set_skipped(db: &Storage, version: Option<&str>) -> AppResult<()> {
    match version.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => db.set_setting(KEY_SKIPPED, v)?,
        None => {
            db.delete_setting(KEY_SKIPPED)?;
        }
    }
    Ok(())
}

// ----- Check / download ----------------------------------------------------------------------

/// The update found by the last check, waiting for "Cập nhật & khởi động lại".
static PENDING: Mutex<Option<Update>> = Mutex::new(None);
static INSTALLING: AtomicBool = AtomicBool::new(false);

fn pending() -> std::sync::MutexGuard<'static, Option<Update>> {
    PENDING.lock().unwrap_or_else(|e| e.into_inner())
}

fn configure_client(b: reqwest::ClientBuilder) -> reqwest::ClientBuilder {
    b.connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
}

/// Checks every manifest and returns the newest release above the running version.
/// A manifest that fails is ignored when another one answered; otherwise the first error wins
/// (the stable manifest comes first).
async fn check_manifests<R: Runtime>(
    app: &AppHandle<R>,
    manifests: &[Url],
    pubkey: &str,
) -> Result<Option<Update>, UpdateError> {
    if manifests.is_empty() {
        return Err(UpdateError::NotConfigured(
            "this build has no update source".into(),
        ));
    }
    let mut found = Vec::new();
    let mut answered = false;
    let mut first_error = None;
    for url in manifests {
        let result = async {
            app.updater_builder()
                .pubkey(pubkey)
                .endpoints(vec![url.clone()])?
                .configure_client(configure_client)
                .build()?
                .check()
                .await
        }
        .await;
        match result {
            Ok(update) => {
                answered = true;
                found.extend(update);
            }
            Err(e) => {
                tracing::info!(%url, error = %e, "update manifest check failed");
                first_error.get_or_insert_with(|| classify(&e, Stage::Check));
            }
        }
    }
    match (pick_latest(found, |u| u.version.as_str()), first_error) {
        (Some(update), _) => Ok(Some(update)),
        (None, Some(e)) if !answered => Err(e),
        (None, _) => Ok(None),
    }
}

/// Downloads and verifies the package, reporting `(downloaded, total)` as it goes.
async fn download(
    update: &Update,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<Vec<u8>, UpdateError> {
    let mut downloaded = 0u64;
    update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                on_progress(downloaded, total);
            },
            || {},
        )
        .await
        .map_err(|e| classify(&e, Stage::Download))
}

fn emit_progress<R: Runtime>(app: &AppHandle<R>, progress: UpdateProgress) {
    if let Err(e) = progress.emit(app) {
        tracing::warn!(error = %e, "failed to emit update progress");
    }
}

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&AppState) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

fn app_version(app: &AppHandle) -> String {
    app.package_info().version.to_string()
}

// ----- Commands ------------------------------------------------------------------------------

#[tauri::command]
#[specta::specta]
pub async fn updater_status(app: AppHandle) -> AppResult<UpdaterStatus> {
    let version = app_version(&app);
    blocking(app, move |st| {
        status_of(&st.storage, UpdaterBuild::current(), &version)
    })
    .await
}

/// Looks for a newer published release. `startup: true` honours the once-a-day limit; the
/// caller decides what to do with a skipped version (`update.skipped`).
#[tauri::command]
#[specta::specta]
pub async fn updater_check(app: AppHandle, startup: bool) -> Result<UpdateCheck, UpdateError> {
    let build = UpdaterBuild::current();
    let current_version = app_version(&app);
    let reply = |status, update| UpdateCheck {
        status,
        current_version: current_version.clone(),
        releases_url: build.releases_url(),
        update,
    };
    let (Some(pubkey), true) = (build.pubkey, build.enabled()) else {
        return Ok(reply(UpdateCheckStatus::NotConfigured, None));
    };
    let now = now_secs();
    let stored = blocking(app.clone(), move |st| {
        let stored = load_stored(&st.storage)?;
        if !startup || check_due(stored.last_check, now) {
            // Recorded before the request so an offline machine does not retry on every launch.
            st.storage.set_setting(KEY_LAST_CHECK, &now)?;
        }
        Ok(stored)
    })
    .await?;
    if startup && !check_due(stored.last_check, now) {
        return Ok(reply(UpdateCheckStatus::NotDue, None));
    }

    let started = Instant::now();
    let result = check_manifests(&app, &build.manifests(stored.channel), pubkey).await;
    tracing::info!(
        channel = ?stored.channel,
        startup,
        found = result.as_ref().ok().and_then(|u| u.as_ref()).map(|u| u.version.as_str()),
        error = result.as_ref().err().map(tracing::field::display),
        ms = started.elapsed().as_millis() as u64,
        "update check finished"
    );
    let found = result?;
    let Some(update) = found else {
        *pending() = None;
        return Ok(reply(UpdateCheckStatus::UpToDate, None));
    };
    let available = AvailableUpdate {
        version: update.version.clone(),
        date: update.date.map(|d| d.unix_timestamp()),
        notes: update.body.clone().filter(|n| !n.trim().is_empty()),
        skipped: is_skipped(stored.skipped.as_deref(), &update.version),
        release_url: build.release_url(&update.version),
    };
    *pending() = Some(update);
    Ok(reply(UpdateCheckStatus::Available, Some(available)))
}

/// Downloads, verifies and installs `version` (found by the last check), then restarts the
/// app. Progress arrives as [`UpdateProgress`]. Only returns on failure: on Windows the
/// installer closes the app and starts it again, elsewhere the app restarts itself.
#[tauri::command]
#[specta::specta]
pub async fn updater_install(app: AppHandle, version: String) -> Result<(), UpdateError> {
    let update = pending()
        .as_ref()
        .filter(|u| u.version == version)
        .cloned()
        .ok_or_else(|| UpdateError::NoPendingUpdate(version.clone()))?;
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err(UpdateError::Busy(version));
    }
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            INSTALLING.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset;

    tracing::info!(from = %update.current_version, to = %update.version, "installing update");
    emit_progress(
        &app,
        UpdateProgress {
            phase: UpdatePhase::Downloading,
            downloaded: 0,
            total: None,
            percent: Some(0),
        },
    );
    let mut last_emit: Option<Instant> = None;
    let bytes = download(&update, |downloaded, total| {
        let now = Instant::now();
        if last_emit.is_some_and(|at| now.duration_since(at) < PROGRESS_INTERVAL) {
            return;
        }
        last_emit = Some(now);
        emit_progress(
            &app,
            UpdateProgress {
                phase: UpdatePhase::Downloading,
                downloaded,
                total,
                percent: percent(downloaded, total),
            },
        );
    })
    .await
    .inspect_err(|e| tracing::warn!(error = %e, "update download failed"))?;

    let size = bytes.len() as u64;
    emit_progress(
        &app,
        UpdateProgress {
            phase: UpdatePhase::Installing,
            downloaded: size,
            total: Some(size),
            percent: Some(100),
        },
    );
    tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|e| UpdateError::Failed(e.to_string()))?
        .map_err(|e| classify(&e, Stage::Install))
        .inspect_err(|e| tracing::warn!(error = %e, "update install failed"))?;

    tracing::info!("update installed, restarting");
    emit_progress(
        &app,
        UpdateProgress {
            phase: UpdatePhase::Restarting,
            downloaded: size,
            total: Some(size),
            percent: Some(100),
        },
    );
    app.restart()
}

/// "Bỏ qua bản này": the startup check stays quiet about `version` (`null` clears it).
#[tauri::command]
#[specta::specta]
pub async fn updater_skip_version(
    app: AppHandle,
    version: Option<String>,
) -> AppResult<UpdaterStatus> {
    let current = app_version(&app);
    blocking(app, move |st| {
        set_skipped(&st.storage, version.as_deref())?;
        status_of(&st.storage, UpdaterBuild::current(), &current)
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn updater_set_channel(
    app: AppHandle,
    channel: UpdateChannel,
) -> AppResult<UpdaterStatus> {
    let current = app_version(&app);
    *pending() = None;
    blocking(app, move |st| {
        st.storage.set_setting(KEY_CHANNEL, &channel)?;
        status_of(&st.storage, UpdaterBuild::current(), &current)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Base64 of minisign's own published public key: well-formed, but nothing in these tests
    /// is signed with it.
    const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEU3NjIwRjE4NDJCNEU4MUYKUldRZjZMUkNHQTlpNTNtbFllY080SXpUNTFUR1Bwdld1Y05TQ2gxQ0JNMFFUYUxuNzNZN0dGTzMK";

    const BUILD: UpdaterBuild = UpdaterBuild {
        repo: Some("octo/app"),
        pubkey: Some("key"),
    };

    #[test]
    fn manifests_follow_channel_and_configuration() {
        assert_eq!(
            BUILD
                .manifests(UpdateChannel::Stable)
                .iter()
                .map(Url::as_str)
                .collect::<Vec<_>>(),
            ["https://github.com/octo/app/releases/latest/download/latest.json"]
        );
        assert_eq!(
            BUILD
                .manifests(UpdateChannel::Beta)
                .iter()
                .map(Url::as_str)
                .collect::<Vec<_>>(),
            [
                "https://github.com/octo/app/releases/latest/download/latest.json",
                "https://github.com/octo/app/releases/download/updater-beta/latest.json",
            ]
        );
        assert_eq!(
            BUILD.releases_url().as_deref(),
            Some("https://github.com/octo/app/releases")
        );
        assert_eq!(
            BUILD.release_url("1.2.0").as_deref(),
            Some("https://github.com/octo/app/releases/tag/v1.2.0")
        );

        for build in [
            UpdaterBuild {
                repo: None,
                pubkey: Some("key"),
            },
            UpdaterBuild {
                repo: Some("octo/app"),
                pubkey: None,
            },
        ] {
            assert!(!build.enabled());
            assert!(build.manifests(UpdateChannel::Beta).is_empty());
        }
        // Without a key the Releases page is still worth linking.
        assert_eq!(
            UpdaterBuild {
                repo: Some("octo/app"),
                pubkey: None
            }
            .releases_url()
            .as_deref(),
            Some("https://github.com/octo/app/releases")
        );
    }

    #[test]
    fn plugin_config_is_added_without_clobbering_an_existing_one() {
        let mut config = mock_context::<MockRuntime, _>(noop_assets())
            .config()
            .clone();
        config.plugins.0.remove("updater");
        ensure_plugin_config(&mut config);
        assert_eq!(
            config.plugins.0["updater"],
            serde_json::json!({ "pubkey": "" })
        );

        config.plugins.0.insert(
            "updater".into(),
            serde_json::json!({ "pubkey": "abc", "windows": { "installMode": "quiet" } }),
        );
        ensure_plugin_config(&mut config);
        assert_eq!(config.plugins.0["updater"]["pubkey"], "abc");
        assert_eq!(
            config.plugins.0["updater"]["windows"]["installMode"],
            "quiet"
        );
    }

    #[test]
    fn startup_check_runs_at_most_once_a_day() {
        let now = 1_800_000_000;
        assert!(check_due(None, now));
        assert!(!check_due(Some(now - 60), now));
        assert!(!check_due(Some(now - CHECK_INTERVAL_SECS + 1), now));
        assert!(check_due(Some(now - CHECK_INTERVAL_SECS), now));
        // Clock moved back (or a bogus value): check rather than stay silent for years.
        assert!(check_due(Some(now + 3600), now));
    }

    #[test]
    fn skipped_version_matches_by_semver() {
        assert!(is_skipped(Some("1.2.0"), "1.2.0"));
        assert!(is_skipped(Some("v1.2.0"), "1.2.0"));
        assert!(is_skipped(Some("1.2.0+build.5"), "1.2.0"));
        assert!(!is_skipped(Some("1.2.0"), "1.2.1"));
        assert!(!is_skipped(Some("1.2.0-beta.1"), "1.2.0"));
        assert!(!is_skipped(None, "1.2.0"));
        assert!(is_skipped(Some("weird"), "weird"));
    }

    #[test]
    fn latest_is_picked_by_semver_precedence() {
        let pick = |v: Vec<&'static str>| pick_latest(v, |s| s);
        assert_eq!(pick(vec!["0.9.0", "0.10.0", "0.2.0"]), Some("0.10.0"));
        assert_eq!(pick(vec!["1.0.0-beta.2", "1.0.0"]), Some("1.0.0"));
        assert_eq!(pick(vec!["1.1.0-beta.1", "1.0.0"]), Some("1.1.0-beta.1"));
        assert_eq!(pick(vec!["garbage", "0.1.0"]), Some("0.1.0"));
        assert_eq!(pick(vec![]), None);
    }

    #[test]
    fn percent_is_clamped() {
        assert_eq!(percent(50, Some(200)), Some(25));
        assert_eq!(percent(300, Some(200)), Some(100));
        assert_eq!(percent(10, None), None);
        assert_eq!(percent(10, Some(0)), None);
    }

    #[test]
    fn settings_round_trip() {
        let db = Storage::open_in_memory().unwrap();
        let s = status_of(&db, BUILD, "0.3.0").unwrap();
        assert!(s.configured);
        assert_eq!(s.channel, UpdateChannel::Stable);
        assert_eq!(s.last_check, None);
        assert_eq!(s.skipped_version, None);

        db.set_setting(KEY_CHANNEL, &UpdateChannel::Beta).unwrap();
        db.set_setting(KEY_LAST_CHECK, &1_800_000_000i64).unwrap();
        set_skipped(&db, Some(" 0.4.0 ")).unwrap();
        let s = status_of(&db, BUILD, "0.3.0").unwrap();
        assert_eq!(s.channel, UpdateChannel::Beta);
        assert_eq!(s.last_check, Some(1_800_000_000));
        assert_eq!(s.skipped_version.as_deref(), Some("0.4.0"));
        assert_eq!(
            db.settings().unwrap()[KEY_CHANNEL],
            serde_json::json!("beta")
        );

        set_skipped(&db, None).unwrap();
        assert_eq!(
            status_of(&db, BUILD, "0.3.0").unwrap().skipped_version,
            None
        );
        let s = status_of(
            &db,
            UpdaterBuild {
                repo: None,
                pubkey: None,
            },
            "0.3.0",
        )
        .unwrap();
        assert!(!s.configured);
        assert_eq!(s.releases_url, None);
    }

    #[test]
    fn errors_serialize_as_kind_and_message() {
        assert_eq!(
            serde_json::to_value(UpdateError::BadSignature("x".into())).unwrap(),
            serde_json::json!({ "kind": "badSignature", "message": "x" })
        );
        use tauri_plugin_updater::Error as E;
        assert!(matches!(
            classify(&E::ReleaseNotFound, Stage::Check),
            UpdateError::NoRelease(_)
        ));
        assert!(matches!(
            classify(
                &E::TargetsNotFound(vec!["windows-x86_64".into()]),
                Stage::Check
            ),
            UpdateError::NoBuildForPlatform(_)
        ));
        assert!(matches!(
            classify(&E::Network("status 500".into()), Stage::Download),
            UpdateError::DownloadFailed(_)
        ));
        assert!(matches!(
            classify(&E::MissingSignedVersion, Stage::Install),
            UpdateError::BadSignature(_)
        ));
        assert!(matches!(
            classify(&E::AuthenticationFailed, Stage::Install),
            UpdateError::Cancelled(_)
        ));
        assert!(matches!(
            classify(&E::EmptyEndpoints, Stage::Check),
            UpdateError::NotConfigured(_)
        ));
    }

    // ----- Against a fake release server --------------------------------------------------

    fn mock_app() -> tauri::App<MockRuntime> {
        let mut ctx = mock_context(noop_assets());
        ensure_plugin_config(ctx.config_mut());
        mock_builder()
            .plugin(tauri_plugin_updater::Builder::new().build())
            .build(ctx)
            .expect("mock app")
    }

    /// `{os}-{arch}`, the fallback key the updater looks up in `platforms`.
    fn platform_key() -> String {
        let os = match std::env::consts::OS {
            "macos" => "darwin",
            os => os,
        };
        format!("{os}-{}", std::env::consts::ARCH)
    }

    fn manifest(server: &MockServer, version: &str) -> serde_json::Value {
        serde_json::json!({
            "version": version,
            "notes": "- Sửa lỗi <b>đẹp</b>",
            "pub_date": "2026-10-01T12:00:00Z",
            "platforms": {
                platform_key(): {
                    "url": format!("{}/asset", server.uri()),
                    "signature": "bm90IGEgc2lnbmF0dXJl",
                }
            }
        })
    }

    async fn serve(server: &MockServer, at: &str, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(response)
            .mount(server)
            .await;
    }

    fn url(server: &MockServer, at: &str) -> Url {
        Url::parse(&format!("{}{at}", server.uri())).unwrap()
    }

    #[tokio::test]
    async fn newer_release_is_found_and_older_ignored() {
        let app = mock_app();
        let server = MockServer::start().await;
        // The mock app runs version 0.1.0.
        serve(
            &server,
            "/new.json",
            ResponseTemplate::new(200).set_body_json(manifest(&server, "0.2.0")),
        )
        .await;
        serve(
            &server,
            "/old.json",
            ResponseTemplate::new(200).set_body_json(manifest(&server, "0.1.0")),
        )
        .await;

        let update = check_manifests(app.handle(), &[url(&server, "/new.json")], PUBKEY)
            .await
            .unwrap()
            .expect("update");
        assert_eq!(update.version, "0.2.0");
        assert_eq!(update.body.as_deref(), Some("- Sửa lỗi <b>đẹp</b>"));
        assert_eq!(update.date.map(|d| d.unix_timestamp()), Some(1_790_856_000));

        let none = check_manifests(app.handle(), &[url(&server, "/old.json")], PUBKEY)
            .await
            .unwrap();
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn beta_channel_takes_the_highest_of_both_manifests() {
        let app = mock_app();
        let server = MockServer::start().await;
        serve(
            &server,
            "/stable.json",
            ResponseTemplate::new(200).set_body_json(manifest(&server, "0.3.0")),
        )
        .await;
        serve(
            &server,
            "/beta.json",
            ResponseTemplate::new(200).set_body_json(manifest(&server, "0.4.0-beta.1")),
        )
        .await;
        serve(
            &server,
            "/old-beta.json",
            ResponseTemplate::new(200).set_body_json(manifest(&server, "0.3.0-beta.2")),
        )
        .await;

        let pick = |beta: &'static str| {
            let manifests = [url(&server, "/stable.json"), url(&server, beta)];
            let handle = app.handle().clone();
            async move { check_manifests(&handle, &manifests, PUBKEY).await }
        };
        assert_eq!(
            pick("/beta.json").await.unwrap().unwrap().version,
            "0.4.0-beta.1"
        );
        // A stale beta manifest must not hide a newer stable release.
        assert_eq!(
            pick("/old-beta.json").await.unwrap().unwrap().version,
            "0.3.0"
        );
        // No beta manifest yet (404): the stable answer still counts.
        assert_eq!(
            pick("/missing.json").await.unwrap().unwrap().version,
            "0.3.0"
        );
    }

    #[tokio::test]
    async fn failures_are_classified() {
        let app = mock_app();
        let server = MockServer::start().await;
        serve(
            &server,
            "/broken.json",
            ResponseTemplate::new(200).set_body_string("{ not json"),
        )
        .await;
        let mut other_platform = manifest(&server, "0.2.0");
        other_platform["platforms"] = serde_json::json!({
            "plan9-mips": { "url": format!("{}/asset", server.uri()), "signature": "x" }
        });
        serve(
            &server,
            "/other.json",
            ResponseTemplate::new(200).set_body_json(other_platform),
        )
        .await;

        let check = |at: Url| {
            let handle = app.handle().clone();
            async move { check_manifests(&handle, &[at], PUBKEY).await.err() }
        };
        assert!(matches!(
            check(url(&server, "/missing.json")).await,
            Some(UpdateError::NoRelease(_))
        ));
        assert!(matches!(
            check(url(&server, "/broken.json")).await,
            Some(UpdateError::BadManifest(_))
        ));
        assert!(matches!(
            check(url(&server, "/other.json")).await,
            Some(UpdateError::NoBuildForPlatform(_))
        ));

        // Nothing listens on a port that was just released.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let offline = Url::parse(&format!("http://127.0.0.1:{port}/latest.json")).unwrap();
        assert!(matches!(
            check(offline).await,
            Some(UpdateError::Offline(_))
        ));

        assert!(matches!(
            check_manifests(app.handle(), &[], PUBKEY).await,
            Err(UpdateError::NotConfigured(_))
        ));
    }

    #[tokio::test]
    async fn download_reports_progress_and_rejects_a_bad_signature() {
        let app = mock_app();
        let server = MockServer::start().await;
        serve(
            &server,
            "/latest.json",
            ResponseTemplate::new(200).set_body_json(manifest(&server, "0.2.0")),
        )
        .await;
        serve(
            &server,
            "/asset",
            ResponseTemplate::new(200).set_body_bytes(vec![7u8; 64 * 1024]),
        )
        .await;

        let update = check_manifests(app.handle(), &[url(&server, "/latest.json")], PUBKEY)
            .await
            .unwrap()
            .unwrap();
        let mut last = (0, None);
        let err = download(&update, |done, total| last = (done, total))
            .await
            .unwrap_err();
        assert!(matches!(err, UpdateError::BadSignature(_)), "{err:?}");
        assert_eq!(last, (64 * 1024, Some(64 * 1024)));
    }

    #[tokio::test]
    async fn missing_asset_is_a_download_failure() {
        let app = mock_app();
        let server = MockServer::start().await;
        serve(
            &server,
            "/latest.json",
            ResponseTemplate::new(200).set_body_json(manifest(&server, "0.2.0")),
        )
        .await;

        let update = check_manifests(app.handle(), &[url(&server, "/latest.json")], PUBKEY)
            .await
            .unwrap()
            .unwrap();
        let err = download(&update, |_, _| {}).await.unwrap_err();
        assert!(matches!(err, UpdateError::DownloadFailed(_)), "{err:?}");
    }
}
