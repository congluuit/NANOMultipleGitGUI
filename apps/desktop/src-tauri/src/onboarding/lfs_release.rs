//! git-lfs from its official GitHub release, for Macs where Homebrew cannot install it: Intel
//! Macs (Homebrew has no git-lfs bottle for them any more and building from source fails) and
//! Macs without Homebrew.
//!
//! The newest `git-lfs-darwin-<arch>-<tag>.zip` is downloaded, checked against the release's
//! `sha256sums.asc`, unpacked with `/usr/bin/unzip` and its `git-lfs` binary copied to
//! `/usr/local/bin` when that is writable (it is on `PATH` everywhere), else `~/.local/bin`.
//! Then `git lfs install --skip-repo` sets up the global filter, as the release's own
//! `install.sh` does. No administrator password is ever asked for.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{
    ProgressTracker, SetupInstallOutcome, SetupInstallPhase, SetupInstallProgress,
    SetupInstallResult, SetupPackageManager, SetupToolId,
};

pub(crate) const RELEASES_PAGE: &str = "https://github.com/git-lfs/git-lfs/releases/latest";
pub(crate) const LATEST_API: &str = "https://api.github.com/repos/git-lfs/git-lfs/releases/latest";

/// A download that sends nothing for this long is given up.
const STALL: Duration = Duration::from_secs(60);
/// How often a waiting download looks at the cancel flag.
const CANCEL_POLL: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

/// The archive for this Mac and the checksum list of the same release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Picked {
    pub version: String,
    pub archive: String,
    pub archive_url: String,
    pub sums_url: String,
}

/// `arch` is `amd64` (Intel) or `arm64` (Apple silicon).
pub(crate) fn pick(release: &Release, arch: &str) -> Option<Picked> {
    let tag = release.tag_name.trim();
    let want = format!("git-lfs-darwin-{arch}-{tag}.zip");
    let archive = release.assets.iter().find(|a| a.name == want)?;
    let sums = ["sha256sums.asc", "sha256sums"]
        .iter()
        .find_map(|n| release.assets.iter().find(|a| a.name == *n))?;
    Some(Picked {
        version: tag.trim_start_matches('v').to_owned(),
        archive: archive.name.clone(),
        archive_url: archive.browser_download_url.clone(),
        sums_url: sums.browser_download_url.clone(),
    })
}

/// The SHA-256 the checksum list gives `name` (`<hex>  <name>` lines, possibly inside a PGP
/// signed message).
pub(crate) fn expected_sha(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut parts = l.split_whitespace();
        let (hash, file) = (parts.next()?, parts.next()?);
        (file.trim_start_matches('*') == name
            && hash.len() == 64
            && hash.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| hash.to_ascii_lowercase())
    })
}

#[derive(Debug)]
pub(crate) enum Fail {
    Cancelled,
    Network(String),
    NotFound(String),
    NoPermission(String),
    Other(String),
}

impl Fail {
    fn outcome(&self) -> SetupInstallOutcome {
        match self {
            Self::Cancelled => SetupInstallOutcome::Cancelled,
            Self::Network(_) => SetupInstallOutcome::Network,
            Self::NotFound(_) => SetupInstallOutcome::PackageNotFound,
            Self::NoPermission(_) => SetupInstallOutcome::NeedsAdmin,
            Self::Other(_) => SetupInstallOutcome::Failed,
        }
    }

    fn message(&self) -> &str {
        match self {
            Self::Cancelled => "",
            Self::Network(m) | Self::NotFound(m) | Self::NoPermission(m) | Self::Other(m) => m,
        }
    }
}

fn net(e: reqwest::Error) -> Fail {
    Fail::Network(e.to_string())
}

fn io(context: &str, e: std::io::Error) -> Fail {
    let msg = format!("{context}: {e}");
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        Fail::NoPermission(msg)
    } else {
        Fail::Other(msg)
    }
}

async fn get(http: &reqwest::Client, url: &str) -> Result<reqwest::Response, Fail> {
    let resp = http.get(url).send().await.map_err(net)?;
    match resp.status() {
        s if s.is_success() => Ok(resp),
        reqwest::StatusCode::NOT_FOUND => Err(Fail::NotFound(format!("404 {url}"))),
        s => Err(Fail::Network(format!("HTTP {s} {url}"))),
    }
}

/// Body of `url`, reporting the percent when the size is known. Stops on `cancel`.
async fn download(
    http: &reqwest::Client,
    url: &str,
    cancel: &AtomicBool,
    mut on_percent: impl FnMut(Option<u8>),
) -> Result<Vec<u8>, Fail> {
    let mut resp = get(http, url).await?;
    let total = resp.content_length().filter(|t| *t > 0);
    let mut body = Vec::with_capacity(total.unwrap_or(0).min(64 << 20) as usize);
    let mut last_data = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(Fail::Cancelled);
        }
        match tokio::time::timeout(CANCEL_POLL, resp.chunk()).await {
            Err(_) if last_data.elapsed() > STALL => {
                return Err(Fail::Network(format!(
                    "no data for {}s: {url}",
                    STALL.as_secs()
                )));
            }
            Err(_) => continue,
            Ok(Err(e)) => return Err(net(e)),
            Ok(Ok(None)) => break,
            Ok(Ok(Some(chunk))) => {
                last_data = Instant::now();
                body.extend_from_slice(&chunk);
                on_percent(total.map(|t| (body.len() as u64 * 100 / t).min(100) as u8));
            }
        }
    }
    Ok(body)
}

/// Finds the newest release for `arch`, downloads its archive and checks its SHA-256.
pub(crate) async fn fetch_verified(
    http: &reqwest::Client,
    api_url: &str,
    arch: &str,
    cancel: &AtomicBool,
    tracker: &mut ProgressTracker,
    emit: &mut (dyn FnMut(SetupInstallProgress) + Send),
) -> Result<(Picked, Vec<u8>), Fail> {
    tracker.note(&format!("GET {api_url}"));
    emit(ev(tracker));
    let raw = get(http, api_url).await?.bytes().await.map_err(net)?;
    let release: Release = serde_json::from_slice(&raw)
        .map_err(|e| Fail::Other(format!("unexpected answer from GitHub: {e}")))?;
    let picked = pick(&release, arch).ok_or_else(|| {
        Fail::NotFound(format!(
            "release {} has no git-lfs-darwin-{arch} archive",
            release.tag_name
        ))
    })?;
    if cancel.load(Ordering::Relaxed) {
        return Err(Fail::Cancelled);
    }

    tracker.set_phase(SetupInstallPhase::Downloading);
    tracker.note(&format!("Downloading {}", picked.archive_url));
    emit(ev(tracker));
    tracker.emitted();
    let zip = download(http, &picked.archive_url, cancel, |p| {
        if p != tracker.percent() {
            tracker.set_percent(p);
            if tracker.due() || p == Some(100) {
                emit(ev(tracker));
                tracker.emitted();
            }
        }
    })
    .await?;

    tracker.set_phase(SetupInstallPhase::Verifying);
    tracker.note("Verifying the SHA-256 checksum");
    emit(ev(tracker));
    let sums = get(http, &picked.sums_url)
        .await?
        .text()
        .await
        .map_err(net)?;
    let want = expected_sha(&sums, &picked.archive).ok_or_else(|| {
        Fail::Other(format!(
            "{} is not in the release checksums",
            picked.archive
        ))
    })?;
    let got = hex(&Sha256::digest(&zip));
    if got != want {
        return Err(Fail::Other(format!(
            "checksum mismatch for {}: expected {want}, got {got}",
            picked.archive
        )));
    }
    tracker.note(&format!("Checksum OK ({})", &got[..12]));
    emit(ev(tracker));
    Ok((picked, zip))
}

fn ev(t: &ProgressTracker) -> SetupInstallProgress {
    SetupInstallProgress::of(SetupToolId::GitLfs, t)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether a file can be created in `dir` (without sudo).
fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".nanomgg-write-test-{}", std::process::id()));
    let ok = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
        .is_ok();
    if ok {
        let _ = std::fs::remove_file(&probe);
    }
    ok
}

/// `/usr/local/bin` if it exists and is writable (Intel Homebrew makes it so), else
/// `~/.local/bin`.
pub(crate) fn target_dir(usr_local: &Path, home: Option<&Path>) -> Option<PathBuf> {
    if usr_local.is_dir() && writable(usr_local) {
        return Some(usr_local.to_path_buf());
    }
    home.map(|h| h.join(".local").join("bin"))
}

/// The `git-lfs` binary inside the unpacked archive (`git-lfs-3.x.y/git-lfs`).
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn find_binary(dir: &Path) -> Option<PathBuf> {
    let direct = dir.join("git-lfs");
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path().join("git-lfs"))
        .find(|p| p.is_file())
}

/// Unpacks `zip` and copies its binary to `dest_dir`; returns the installed path.
#[cfg(unix)]
fn place(zip: &[u8], dest_dir: &Path, cancel: &AtomicBool) -> Result<PathBuf, Fail> {
    use std::os::unix::fs::PermissionsExt;

    let work = std::env::temp_dir().join(format!("nanomgg-git-lfs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| io("temporary folder", e))?;
    let result = (|| {
        let archive = work.join("git-lfs.zip");
        std::fs::write(&archive, zip).map_err(|e| io("temporary file", e))?;
        let out_dir = work.join("x");
        let mut cmd = super::command(Path::new("/usr/bin/unzip"));
        cmd.arg("-q")
            .arg("-o")
            .arg(&archive)
            .arg("-d")
            .arg(&out_dir);
        let out = super::run_cancellable(cmd, cancel, Duration::from_secs(120))
            .map_err(|e| io("unzip", e))?;
        if out.cancelled {
            return Err(Fail::Cancelled);
        }
        if out.exit_code != Some(0) {
            return Err(Fail::Other(format!("unzip failed: {}", out.text.trim())));
        }
        let bin = find_binary(&out_dir)
            .ok_or_else(|| Fail::Other("git-lfs binary not found in the archive".into()))?;
        std::fs::create_dir_all(dest_dir).map_err(|e| io(&dest_dir.display().to_string(), e))?;
        let dest = dest_dir.join("git-lfs");
        let tmp = dest_dir.join(".git-lfs.new");
        std::fs::copy(&bin, &tmp).map_err(|e| io(&dest.display().to_string(), e))?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| io(&dest.display().to_string(), e))?;
        std::fs::rename(&tmp, &dest).map_err(|e| io(&dest.display().to_string(), e))?;
        Ok(dest)
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

#[cfg(not(unix))]
fn place(_zip: &[u8], _dest_dir: &Path, _cancel: &AtomicBool) -> Result<PathBuf, Fail> {
    Err(Fail::Other("only used on macOS".into()))
}

/// The whole install; never fails, the result says what happened.
pub(crate) async fn install(
    http: &reqwest::Client,
    mac_intel: bool,
    cancel: &AtomicBool,
    emit: &mut (dyn FnMut(SetupInstallProgress) + Send),
) -> SetupInstallResult {
    let tool = SetupToolId::GitLfs;
    let arch = if mac_intel { "amd64" } else { "arm64" };
    let mut tracker = ProgressTracker::new();
    let mut log: Vec<String> = Vec::new();
    let outcome = match run(http, arch, cancel, &mut tracker, emit, &mut log).await {
        Ok(()) => SetupInstallOutcome::Installed,
        Err(f) => {
            if !f.message().is_empty() {
                log.push(f.message().to_owned());
            }
            f.outcome()
        }
    };
    tracing::info!(?outcome, arch, "git-lfs release install finished");
    SetupInstallResult {
        tool,
        outcome,
        manager: Some(SetupPackageManager::GithubRelease),
        command: Some(format!(
            "download git-lfs-darwin-{arch} from {RELEASES_PAGE}"
        )),
        exit_code: None,
        output: nanomgg_logging::redact(&log.join("\n")).into_owned(),
    }
}

async fn run(
    http: &reqwest::Client,
    arch: &str,
    cancel: &AtomicBool,
    tracker: &mut ProgressTracker,
    emit: &mut (dyn FnMut(SetupInstallProgress) + Send),
    log: &mut Vec<String>,
) -> Result<(), Fail> {
    let (picked, zip) = fetch_verified(http, LATEST_API, arch, cancel, tracker, emit).await?;
    log.push(format!(
        "Downloaded {} ({} KB), checksum OK",
        picked.archive,
        zip.len() / 1024
    ));

    tracker.set_phase(SetupInstallPhase::Installing);
    let home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(PathBuf::from);
    let dest_dir = target_dir(Path::new("/usr/local/bin"), home.as_deref())
        .ok_or_else(|| Fail::Other("no folder to install into (HOME is not set)".into()))?;
    tracker.note(&format!("Installing to {}", dest_dir.display()));
    emit(ev(tracker));
    if cancel.load(Ordering::Relaxed) {
        return Err(Fail::Cancelled);
    }
    // Unpacking takes a second or two; not worth stopping half way.
    let dest = tauri::async_runtime::spawn_blocking(move || {
        place(&zip, &dest_dir, &AtomicBool::new(false))
    })
    .await
    .map_err(|e| Fail::Other(e.to_string()))??;
    log.push(format!(
        "Installed git-lfs {} to {}",
        picked.version,
        dest.display()
    ));

    tracker.set_phase(SetupInstallPhase::Finishing);
    tracker.note("git lfs install --skip-repo");
    emit(ev(tracker));
    // Not fatal: without git the filter is set up later, and the binary is in place.
    let mut cmd = super::command(&dest);
    cmd.args(["install", "--skip-repo"]);
    let ran = tauri::async_runtime::spawn_blocking(move || {
        super::run_cancellable(cmd, &AtomicBool::new(false), Duration::from_secs(60))
    })
    .await;
    match ran {
        Ok(Ok(out)) => log.push(format!("git lfs install --skip-repo: {}", out.text.trim())),
        Ok(Err(e)) => log.push(format!("git lfs install --skip-repo did not run: {e}")),
        Err(e) => log.push(format!("git lfs install --skip-repo did not run: {e}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, names: &[&str]) -> Release {
        Release {
            tag_name: tag.into(),
            assets: names
                .iter()
                .map(|n| Asset {
                    name: (*n).into(),
                    browser_download_url: format!("https://example.test/{n}"),
                })
                .collect(),
        }
    }

    #[test]
    fn picks_the_archive_for_the_mac_arch() {
        let r = release(
            "v3.7.1",
            &[
                "git-lfs-darwin-amd64-v3.7.1.zip",
                "git-lfs-darwin-arm64-v3.7.1.zip",
                "git-lfs-linux-amd64-v3.7.1.tar.gz",
                "sha256sums.asc",
            ],
        );
        let p = pick(&r, "amd64").unwrap();
        assert_eq!(p.version, "3.7.1");
        assert_eq!(p.archive, "git-lfs-darwin-amd64-v3.7.1.zip");
        assert_eq!(p.sums_url, "https://example.test/sha256sums.asc");
        assert_eq!(
            pick(&r, "arm64").unwrap().archive,
            "git-lfs-darwin-arm64-v3.7.1.zip"
        );
        assert!(
            pick(
                &release("v3.7.1", &["git-lfs-darwin-amd64-v3.7.1.zip"]),
                "amd64"
            )
            .is_none()
        );
        assert!(pick(&r, "riscv").is_none());
    }

    #[test]
    fn reads_checksums_from_a_signed_list() {
        let h = "a".repeat(64);
        let sums = format!(
            "-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA256\n\n{}  git-lfs-darwin-arm64-v3.7.1.zip\n{h}  git-lfs-darwin-amd64-v3.7.1.zip\n-----BEGIN PGP SIGNATURE-----\n",
            "b".repeat(64)
        );
        assert_eq!(
            expected_sha(&sums, "git-lfs-darwin-amd64-v3.7.1.zip"),
            Some(h)
        );
        assert_eq!(expected_sha(&sums, "missing.zip"), None);
        assert_eq!(expected_sha("short  x.zip", "x.zip"), None);
    }

    #[test]
    fn installs_into_usr_local_only_when_writable() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        assert_eq!(
            target_dir(tmp.path(), Some(&home)).as_deref(),
            Some(tmp.path())
        );
        let missing = tmp.path().join("nope");
        assert_eq!(
            target_dir(&missing, Some(&home)),
            Some(home.join(".local").join("bin"))
        );
        assert_eq!(target_dir(&missing, None), None);
    }

    #[test]
    fn finds_the_binary_in_the_unpacked_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let inner = tmp.path().join("git-lfs-3.7.1");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("install.sh"), "").unwrap();
        assert_eq!(find_binary(tmp.path()), None);
        std::fs::write(inner.join("git-lfs"), "bin").unwrap();
        assert_eq!(find_binary(tmp.path()), Some(inner.join("git-lfs")));
    }

    mod http {
        use super::*;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        async fn server(zip: &[u8], sha: &str) -> MockServer {
            let s = MockServer::start().await;
            let base = s.uri();
            let body = serde_json::json!({
                "tag_name": "v3.7.1",
                "assets": [
                    {"name": "git-lfs-darwin-amd64-v3.7.1.zip",
                     "browser_download_url": format!("{base}/dl/lfs.zip")},
                    {"name": "sha256sums.asc",
                     "browser_download_url": format!("{base}/dl/sums")}
                ]
            });
            Mock::given(method("GET"))
                .and(path("/latest"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&s)
                .await;
            Mock::given(method("GET"))
                .and(path("/dl/lfs.zip"))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(zip.to_vec()))
                .mount(&s)
                .await;
            Mock::given(method("GET"))
                .and(path("/dl/sums"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_string(format!("{sha}  git-lfs-darwin-amd64-v3.7.1.zip\n")),
                )
                .mount(&s)
                .await;
            s
        }

        async fn fetch(
            s: &MockServer,
            cancel: bool,
        ) -> (Result<(Picked, Vec<u8>), Fail>, Vec<SetupInstallProgress>) {
            let http = reqwest::Client::new();
            let mut tracker = ProgressTracker::new();
            let mut events = Vec::new();
            let r = fetch_verified(
                &http,
                &format!("{}/latest", s.uri()),
                "amd64",
                &AtomicBool::new(cancel),
                &mut tracker,
                &mut |p| events.push(p),
            )
            .await;
            (r, events)
        }

        #[tokio::test]
        async fn downloads_and_verifies_the_archive() {
            let zip = vec![7u8; 300_000];
            let s = server(&zip, &hex(&Sha256::digest(&zip))).await;
            let (r, events) = fetch(&s, false).await;
            let (picked, body) = r.unwrap();
            assert_eq!(picked.version, "3.7.1");
            assert_eq!(body, zip);
            let phases: Vec<_> = events.iter().map(|e| e.phase).collect();
            assert!(phases.contains(&SetupInstallPhase::Downloading));
            assert_eq!(phases.last(), Some(&SetupInstallPhase::Verifying));
            assert!(events.iter().any(|e| e.percent == Some(100)));
        }

        #[tokio::test]
        async fn a_wrong_checksum_is_refused() {
            let zip = vec![1u8; 1000];
            let s = server(&zip, &"0".repeat(64)).await;
            let (r, _) = fetch(&s, false).await;
            assert!(matches!(r, Err(Fail::Other(m)) if m.contains("checksum mismatch")));
        }

        #[tokio::test]
        async fn a_cancelled_install_downloads_nothing() {
            let zip = vec![1u8; 1000];
            let s = server(&zip, &hex(&Sha256::digest(&zip))).await;
            let (r, _) = fetch(&s, true).await;
            assert!(matches!(r, Err(Fail::Cancelled)));
        }
    }
}
