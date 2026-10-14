//! Local multi-user Git smart-HTTP + Git LFS server for integration tests (dev only).
//!
//! - Repositories live at `<root>/<owner>/<name>.git`; only the owner may read or write them,
//!   so a push made with the wrong account fails with HTTP 403 exactly like on a real host.
//! - Git traffic is served by `git http-backend` (CGI); Git LFS uses the Batch API with the
//!   `basic` transfer adapter, streaming objects to `<root>/lfs/<owner>/<name>/<oid>`.
//! - Every request is recorded as an [`Event`] so tests can assert *who* did *what*.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    GitRead,
    GitWrite,
    LfsBatch { operation: String },
    LfsUpload { oid: String, size: u64 },
    LfsDownload { oid: String },
    Unauthorized,
    Forbidden,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Authenticated user, if any.
    pub user: Option<String>,
    /// `owner/name` of the repository addressed.
    pub repo: String,
    pub kind: EventKind,
}

struct Shared {
    root: PathBuf,
    users: HashMap<String, String>,
    base_url: String,
    events: Mutex<Vec<Event>>,
    empty_config: PathBuf,
}

impl Shared {
    fn record(&self, user: Option<&str>, repo: &str, kind: EventKind) {
        self.events.lock().unwrap().push(Event {
            user: user.map(str::to_owned),
            repo: repo.to_owned(),
            kind,
        });
    }

    fn lfs_dir(&self, owner: &str, repo: &str) -> PathBuf {
        self.root.join("lfs").join(owner).join(repo)
    }
}

pub struct TestGitServer {
    shared: Arc<Shared>,
    port: u16,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl TestGitServer {
    /// Starts the server on `127.0.0.1:<random port>`. `users` are `(username, token)` pairs.
    pub fn start(root: &Path, users: &[(&str, &str)]) -> io::Result<Self> {
        std::fs::create_dir_all(root)?;
        let empty_config = root.join(".empty-gitconfig");
        std::fs::write(&empty_config, "")?;

        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let root = root.to_owned();
        let users: HashMap<_, _> = users
            .iter()
            .map(|(u, t)| ((*u).to_owned(), (*t).to_owned()))
            .collect();

        let thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(4)
                .enable_all()
                .build()
                .expect("tokio runtime");
            rt.block_on(async move {
                let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
                    Ok(l) => l,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let port = listener.local_addr().unwrap().port();
                let shared = Arc::new(Shared {
                    root,
                    users,
                    base_url: format!("http://127.0.0.1:{port}"),
                    events: Mutex::new(Vec::new()),
                    empty_config,
                });
                let app = Router::new().fallback(handle).with_state(shared.clone());
                let _ = ready_tx.send(Ok((port, shared)));
                let _ = axum::serve(listener, app)
                    .with_graceful_shutdown(async {
                        let _ = shutdown_rx.await;
                    })
                    .await;
            });
        });

        let (port, shared) = ready_rx.recv().map_err(io::Error::other)??;
        Ok(Self {
            shared,
            port,
            shutdown: Some(shutdown_tx),
            thread: Some(thread),
        })
    }

    /// `127.0.0.1:<port>` — the `host` git puts in credential requests.
    pub fn host(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    pub fn url(&self, owner: &str, name: &str) -> String {
        format!("{}/{owner}/{name}.git", self.shared.base_url)
    }

    /// Creates an empty bare repository owned by `owner`.
    pub fn create_repo(&self, owner: &str, name: &str) -> io::Result<PathBuf> {
        let path = self.shared.root.join(owner).join(format!("{name}.git"));
        std::fs::create_dir_all(&path)?;
        let status = std::process::Command::new("git")
            .args(["init", "--bare", "-q", "-b", "main"])
            .arg(&path)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", &self.shared.empty_config)
            .status()?;
        if !status.success() {
            return Err(io::Error::other("git init --bare failed"));
        }
        Ok(path)
    }

    pub fn events(&self) -> Vec<Event> {
        self.shared.events.lock().unwrap().clone()
    }

    pub fn clear_events(&self) {
        self.shared.events.lock().unwrap().clear();
    }

    pub fn lfs_object_path(&self, owner: &str, name: &str, oid: &str) -> PathBuf {
        self.shared.lfs_dir(owner, name).join(oid)
    }
}

impl Drop for TestGitServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

// ---------------------------------------------------------------------------------------------

enum Route {
    Git {
        owner: String,
        repo: String,
        rest: String,
    },
    LfsObject {
        owner: String,
        repo: String,
        oid: String,
    },
}

fn parse_route(path: &str) -> Option<Route> {
    let segs: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if segs.first() == Some(&"lfs-objects") && segs.len() == 4 {
        return Some(Route::LfsObject {
            owner: segs[1].to_owned(),
            repo: segs[2].to_owned(),
            oid: segs[3].to_owned(),
        });
    }
    if segs.len() >= 3 && segs[1].ends_with(".git") {
        return Some(Route::Git {
            owner: segs[0].to_owned(),
            repo: segs[1].trim_end_matches(".git").to_owned(),
            rest: segs[2..].join("/"),
        });
    }
    None
}

fn is_safe_segment(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && s != "."
        && s != ".."
}

fn is_oid(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn basic_user(shared: &Shared, headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let encoded = value.strip_prefix("Basic ")?;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let (user, token) = std::str::from_utf8(&decoded).ok()?.split_once(':')?;
    (shared.users.get(user)? == token).then(|| user.to_owned())
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Basic realm=\"nanomgg-test\"")],
        "authentication required\n",
    )
        .into_response()
}

async fn handle(State(shared): State<Arc<Shared>>, req: Request) -> Response {
    let Some(route) = parse_route(req.uri().path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (owner, repo) = match &route {
        Route::Git { owner, repo, .. } | Route::LfsObject { owner, repo, .. } => {
            (owner.clone(), repo.clone())
        }
    };
    if !is_safe_segment(&owner) || !is_safe_segment(&repo) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let full = format!("{owner}/{repo}");

    let Some(user) = basic_user(&shared, req.headers()) else {
        shared.record(None, &full, EventKind::Unauthorized);
        return unauthorized();
    };
    if user != owner {
        shared.record(Some(&user), &full, EventKind::Forbidden);
        return (
            StatusCode::FORBIDDEN,
            "you do not have access to this repository\n",
        )
            .into_response();
    }

    match route {
        Route::LfsObject { oid, .. } if is_oid(&oid) => match *req.method() {
            Method::PUT => lfs_upload(&shared, &user, &owner, &repo, &oid, req).await,
            Method::GET => lfs_download(&shared, &user, &owner, &repo, &oid).await,
            _ => StatusCode::METHOD_NOT_ALLOWED.into_response(),
        },
        Route::LfsObject { .. } => StatusCode::BAD_REQUEST.into_response(),
        Route::Git { rest, .. } => match (req.method().clone(), rest.as_str()) {
            (Method::POST, "info/lfs/objects/batch") => {
                lfs_batch(&shared, &user, &owner, &repo, req).await
            }
            (Method::POST, "info/lfs/locks/verify") => (
                [(header::CONTENT_TYPE, "application/vnd.git-lfs+json")],
                r#"{"ours":[],"theirs":[]}"#,
            )
                .into_response(),
            (Method::GET, "info/refs") | (Method::POST, "git-upload-pack" | "git-receive-pack") => {
                if rest == "git-upload-pack" {
                    shared.record(Some(&user), &full, EventKind::GitRead);
                } else if rest == "git-receive-pack" {
                    shared.record(Some(&user), &full, EventKind::GitWrite);
                }
                http_backend(&shared, &user, &owner, &repo, &rest, req).await
            }
            _ => StatusCode::NOT_FOUND.into_response(),
        },
    }
}

async fn http_backend(
    shared: &Shared,
    user: &str,
    owner: &str,
    repo: &str,
    rest: &str,
    req: Request,
) -> Response {
    let (parts, body) = req.into_parts();
    let body = match axum::body::to_bytes(body, usize::MAX).await {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let header_str = |name: header::HeaderName| {
        parts
            .headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned()
    };

    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("http-backend")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", &shared.empty_config)
        .env("GIT_PROJECT_ROOT", &shared.root)
        .env("GIT_HTTP_EXPORT_ALL", "1")
        .env("PATH_INFO", format!("/{owner}/{repo}.git/{rest}"))
        .env("REQUEST_METHOD", parts.method.as_str())
        .env("QUERY_STRING", parts.uri.query().unwrap_or(""))
        .env("REMOTE_USER", user)
        .env("REMOTE_ADDR", "127.0.0.1")
        .env("CONTENT_TYPE", header_str(header::CONTENT_TYPE))
        .env("CONTENT_LENGTH", body.len().to_string())
        .env(
            "HTTP_CONTENT_ENCODING",
            header_str(header::CONTENT_ENCODING),
        )
        .env(
            "HTTP_GIT_PROTOCOL",
            parts
                .headers
                .get("git-protocol")
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
        )
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let mut stdin = child.stdin.take().unwrap();
    let writer = tokio::spawn(async move {
        let _ = stdin.write_all(&body).await;
    });
    let output = match child.wait_with_output().await {
        Ok(o) => o,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let _ = writer.await;
    cgi_response(&output.stdout)
}

fn cgi_response(raw: &[u8]) -> Response {
    let (head_end, sep_len) = match find(raw, b"\r\n\r\n") {
        Some(i) => (i, 4),
        None => match find(raw, b"\n\n") {
            Some(i) => (i, 2),
            None => return (StatusCode::BAD_GATEWAY, "bad CGI response").into_response(),
        },
    };
    let head = String::from_utf8_lossy(&raw[..head_end]);
    let mut builder = Response::builder().status(StatusCode::OK);
    for line in head.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("status") {
            let code = value
                .split_whitespace()
                .next()
                .and_then(|c| c.parse().ok())
                .unwrap_or(500);
            builder = builder.status(code);
        } else if let Ok(v) = HeaderValue::from_str(value) {
            builder = builder.header(name, v);
        }
    }
    builder
        .body(Body::from(raw[head_end + sep_len..].to_vec()))
        .unwrap()
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[derive(Deserialize)]
struct BatchRequest {
    operation: String,
    objects: Vec<BatchObject>,
}

#[derive(Deserialize, Serialize, Clone)]
struct BatchObject {
    oid: String,
    size: u64,
}

async fn lfs_batch(shared: &Shared, user: &str, owner: &str, repo: &str, req: Request) -> Response {
    let auth = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let Ok(body) = axum::body::to_bytes(req.into_body(), 16 * 1024 * 1024).await else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(batch) = serde_json::from_slice::<BatchRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    shared.record(
        Some(user),
        &format!("{owner}/{repo}"),
        EventKind::LfsBatch {
            operation: batch.operation.clone(),
        },
    );

    let dir = shared.lfs_dir(owner, repo);
    let objects: Vec<serde_json::Value> = batch
        .objects
        .iter()
        .map(|o| {
            if !is_oid(&o.oid) {
                return serde_json::json!({
                    "oid": o.oid, "size": o.size,
                    "error": { "code": 422, "message": "invalid oid" }
                });
            }
            let path = dir.join(&o.oid);
            let existing = std::fs::metadata(&path).ok().map(|m| m.len());
            let href = format!("{}/lfs-objects/{owner}/{repo}/{}", shared.base_url, o.oid);
            let action = serde_json::json!({
                "href": href,
                "header": { "Authorization": auth },
                "expires_in": 3600
            });
            match (batch.operation.as_str(), existing) {
                ("upload", Some(size)) if size == o.size => {
                    serde_json::json!({ "oid": o.oid, "size": o.size })
                }
                ("upload", _) => serde_json::json!({
                    "oid": o.oid, "size": o.size, "authenticated": true,
                    "actions": { "upload": action }
                }),
                ("download", Some(_)) => serde_json::json!({
                    "oid": o.oid, "size": o.size, "authenticated": true,
                    "actions": { "download": action }
                }),
                _ => serde_json::json!({
                    "oid": o.oid, "size": o.size,
                    "error": { "code": 404, "message": "object not found" }
                }),
            }
        })
        .collect();

    (
        [(header::CONTENT_TYPE, "application/vnd.git-lfs+json")],
        serde_json::json!({ "transfer": "basic", "objects": objects, "hash_algo": "sha256" })
            .to_string(),
    )
        .into_response()
}

async fn lfs_upload(
    shared: &Shared,
    user: &str,
    owner: &str,
    repo: &str,
    oid: &str,
    req: Request,
) -> Response {
    let dir = shared.lfs_dir(owner, repo);
    if tokio::fs::create_dir_all(&dir).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let tmp = dir.join(format!("{oid}.part"));
    let Ok(mut file) = tokio::fs::File::create(&tmp).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut stream = req.into_body().into_data_stream();
    while let Some(chunk) = stream.next().await {
        let Ok(chunk) = chunk else {
            let _ = tokio::fs::remove_file(&tmp).await;
            return StatusCode::BAD_REQUEST.into_response();
        };
        hasher.update(&chunk);
        size += chunk.len() as u64;
        if file.write_all(&chunk).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    let _ = file.flush().await;
    drop(file);
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if digest != oid {
        let _ = tokio::fs::remove_file(&tmp).await;
        return (StatusCode::UNPROCESSABLE_ENTITY, "oid mismatch").into_response();
    }
    if tokio::fs::rename(&tmp, dir.join(oid)).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    shared.record(
        Some(user),
        &format!("{owner}/{repo}"),
        EventKind::LfsUpload {
            oid: oid.to_owned(),
            size,
        },
    );
    StatusCode::OK.into_response()
}

async fn lfs_download(shared: &Shared, user: &str, owner: &str, repo: &str, oid: &str) -> Response {
    let Ok(file) = tokio::fs::File::open(shared.lfs_dir(owner, repo).join(oid)).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    shared.record(
        Some(user),
        &format!("{owner}/{repo}"),
        EventKind::LfsDownload {
            oid: oid.to_owned(),
        },
    );
    Body::from_stream(tokio_util::io::ReaderStream::new(file)).into_response()
}
