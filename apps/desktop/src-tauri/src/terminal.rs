//! Integrated terminal: pseudo-terminals (ConPTY on Windows, openpty elsewhere) running the
//! user's shell in a repository, with that repository's account context exported so `git push`
//! typed in the terminal uses the same account as the app (PLAN.md §4.4 "Terminal tích hợp").
//!
//! Each session owns three threads: a reader (blocking PTY reads → chunks), a pump (batches the
//! chunks for ~16 ms, decodes UTF-8 across read boundaries and sends them to the UI) and a writer
//! (keystrokes, in order, never blocking the IPC thread). A waiter thread reaps the child and
//! reports its exit code. Dropping a [`Session`] kills the child and closes the PTY.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use nanomgg_git_engine::{AccountContext, GitRunner};
use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use serde::Serialize;
use specta::Type;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::repos::repo_root;
use crate::state::AppState;

/// Upper bound on concurrently open terminals (all repositories together).
pub const MAX_SESSIONS: usize = 16;
/// Output is coalesced for this long before being sent to the UI (one frame at 60 Hz).
const BATCH_WINDOW: Duration = Duration::from_millis(16);
/// A batch is flushed early once this large, so `cat big-file` stays responsive.
const MAX_BATCH_BYTES: usize = 256 * 1024;
const READ_BUFFER: usize = 16 * 1024;
/// How long the waiter waits for the remaining output after the child exited.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Streamed to the UI through a [`Channel`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TerminalEvent {
    /// Terminal output (UTF-8; invalid sequences are replaced by U+FFFD).
    Output { data: String },
    /// The shell exited; the session is gone. `code` is `null` when it could not be read.
    Exit { code: Option<u32> },
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: u32,
    /// Display name of the shell ("Git Bash", "PowerShell", "zsh", …).
    pub shell: String,
}

// ---------------------------------------------------------------------------------------------
// Shell detection
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    pub program: PathBuf,
    pub args: Vec<String>,
    /// Shell-specific variables (e.g. keep Git Bash in the start directory).
    pub env: Vec<(String, String)>,
    pub name: String,
}

impl Shell {
    fn new(program: PathBuf, args: &[&str], name: &str) -> Self {
        Self {
            program,
            args: args.iter().map(|a| (*a).to_owned()).collect(),
            env: Vec::new(),
            name: name.to_owned(),
        }
    }
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

/// `<root>/bin/bash.exe` of a Git for Windows installation found among the ancestors of `path`
/// (`<root>/mingw64/libexec/git-core`, `<root>/cmd/git.exe`, …). The `usr/bin` check makes sure
/// it is Git's MSYS2 layout and not some unrelated `bin/bash.exe` (or WSL's `bash.exe`).
fn git_bash_near(path: &Path) -> Option<PathBuf> {
    path.ancestors().take(5).find_map(|root| {
        let bash = root.join("bin").join("bash.exe");
        (bash.is_file() && root.join("usr").join("bin").is_dir()).then_some(bash)
    })
}

/// Git Bash from, in order: `git --exec-path`, `git.exe` on `PATH`, the usual install folders.
fn find_git_bash(
    exec_path: Option<&Path>,
    git_exe: Option<&Path>,
    install_roots: &[PathBuf],
) -> Option<PathBuf> {
    exec_path
        .and_then(git_bash_near)
        .or_else(|| git_exe.and_then(git_bash_near))
        .or_else(|| install_roots.iter().find_map(|r| git_bash_near(r)))
}

fn git_exec_path(git: &GitRunner) -> Option<PathBuf> {
    let out = git
        .command(Path::new("."), &AccountContext::default(), ["--exec-path"])
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && !path.is_empty()).then(|| PathBuf::from(path))
}

#[cfg(windows)]
fn detect_shell(git: &GitRunner) -> Shell {
    let mut roots = Vec::new();
    for var in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(dir) = std::env::var_os(var) {
            roots.push(PathBuf::from(dir).join("Git"));
        }
    }
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(dir).join("Programs").join("Git"));
    }
    let exec_path = git_exec_path(git);
    let git_exe = find_in_path("git.exe");
    if let Some(bash) = find_git_bash(exec_path.as_deref(), git_exe.as_deref(), &roots) {
        let mut shell = Shell::new(bash, &["--login", "-i"], "Git Bash");
        // MSYS2's /etc/profile would otherwise `cd ~` in login shells.
        shell.env.push(("CHERE_INVOKING".into(), "1".into()));
        return shell;
    }
    if let Some(pwsh) = find_in_path("pwsh.exe") {
        return Shell::new(pwsh, &["-NoLogo"], "PowerShell");
    }
    if let Some(ps) = find_in_path("powershell.exe") {
        return Shell::new(ps, &["-NoLogo"], "Windows PowerShell");
    }
    let cmd = std::env::var_os("COMSPEC").map_or_else(|| PathBuf::from("cmd.exe"), PathBuf::from);
    Shell::new(cmd, &[], "cmd")
}

#[cfg(not(windows))]
fn detect_shell(_git: &GitRunner) -> Shell {
    let program = std::env::var_os("SHELL")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| {
            ["/bin/zsh", "/bin/bash", "/bin/sh"]
                .iter()
                .map(PathBuf::from)
                .find(|p| p.is_file())
        })
        .unwrap_or_else(|| PathBuf::from("/bin/sh"));
    let name = program
        .file_name()
        .map_or_else(|| "sh".to_owned(), |n| n.to_string_lossy().into_owned());
    Shell::new(program, &["-l"], &name)
}

/// The detected shell, resolved once per run (it may spawn `git --exec-path`).
fn shell(git: &GitRunner) -> &'static Shell {
    static SHELL: OnceLock<Shell> = OnceLock::new();
    SHELL.get_or_init(|| detect_shell(git))
}

// ---------------------------------------------------------------------------------------------
// Environment
// ---------------------------------------------------------------------------------------------

/// Variables added on top of the parent environment: the repository's account context (identity,
/// credential helper, `NANOMGG_*`, `GIT_SSH_COMMAND`), `NANOMGG_REPO` and the terminal type.
///
/// `GIT_TERMINAL_PROMPT=0` is left out: it is meant for the app's background git processes, while
/// in an interactive terminal a username/passphrase prompt is legitimate.
pub fn session_env(ctx: &AccountContext, root: &Path) -> Vec<(String, String)> {
    let mut env: Vec<_> = ctx
        .env()
        .into_iter()
        .filter(|(k, _)| k != "GIT_TERMINAL_PROMPT")
        .collect();
    env.push(("NANOMGG_REPO".into(), root.to_string_lossy().into_owned()));
    env.push(("TERM".into(), "xterm-256color".into()));
    env.push(("COLORTERM".into(), "truecolor".into()));
    env.push(("TERM_PROGRAM".into(), "NANOMultipleGitGUI".into()));
    env
}

/// Account context exported into a repository's terminal: the repo default (`None`), i.e. the
/// identity used for commits and the account of `origin`. Unbound repos get nothing extra.
fn context(st: &AppState, repo_id: i64) -> AppResult<AccountContext> {
    crate::accounts::context_for(st, repo_id, None)
}

// ---------------------------------------------------------------------------------------------
// UTF-8 stream decoding and output batching
// ---------------------------------------------------------------------------------------------

/// Decodes a byte stream whose chunks may split multi-byte UTF-8 sequences.
#[derive(Debug, Default)]
pub struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    /// Decodes `input`; an incomplete sequence at the end is kept for the next call.
    pub fn decode(&mut self, input: &[u8]) -> String {
        let mut bytes = std::mem::take(&mut self.pending);
        bytes.extend_from_slice(input);
        let mut out = String::with_capacity(bytes.len());
        let mut rest = bytes.as_slice();
        loop {
            match std::str::from_utf8(rest) {
                Ok(s) => {
                    out.push_str(s);
                    break;
                }
                Err(e) => {
                    let (valid, after) = rest.split_at(e.valid_up_to());
                    out.push_str(&String::from_utf8_lossy(valid));
                    match e.error_len() {
                        Some(len) => {
                            out.push(char::REPLACEMENT_CHARACTER);
                            rest = &after[len..];
                        }
                        None => {
                            self.pending = after.to_vec();
                            break;
                        }
                    }
                }
            }
        }
        out
    }

    /// End of stream: a dangling partial sequence becomes U+FFFD.
    pub fn finish(&mut self) -> String {
        if std::mem::take(&mut self.pending).is_empty() {
            String::new()
        } else {
            char::REPLACEMENT_CHARACTER.to_string()
        }
    }
}

/// Forwards chunks from `rx` to `emit` in batches of at most `window`, until the sender is gone.
/// Returns `false` as soon as `emit` reports the receiver is gone.
fn pump(rx: &Receiver<Vec<u8>>, window: Duration, mut emit: impl FnMut(String) -> bool) -> bool {
    let mut utf8 = Utf8Stream::default();
    while let Ok(mut batch) = rx.recv() {
        let deadline = Instant::now() + window;
        let mut closed = false;
        while batch.len() < MAX_BATCH_BYTES {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match rx.recv_timeout(deadline - now) {
                Ok(chunk) => batch.extend_from_slice(&chunk),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    closed = true;
                    break;
                }
            }
        }
        let text = utf8.decode(&batch);
        if !text.is_empty() && !emit(text) {
            return false;
        }
        if closed {
            break;
        }
    }
    let tail = utf8.finish();
    tail.is_empty() || emit(tail)
}

// ---------------------------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------------------------

/// What to run, where and with which extra environment.
#[derive(Debug, Clone)]
pub struct SpawnRequest {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
}

fn pty_size(cols: u16, rows: u16) -> PtySize {
    // ConPTY rejects a zero size; xterm.js can report 0 while its host is hidden.
    PtySize {
        cols: cols.clamp(2, 1000),
        rows: rows.clamp(1, 500),
        pixel_width: 0,
        pixel_height: 0,
    }
}

struct Session {
    master: Box<dyn MasterPty + Send>,
    /// Keystrokes for the writer thread; dropping it ends that thread.
    input: Sender<Vec<u8>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // Kill first, then `master` is dropped, which closes the PTY (ClosePseudoConsole on
        // Windows also terminates every process still attached to it).
        let _ = self.killer.kill();
    }
}

#[derive(Default)]
struct Registry {
    sessions: Mutex<HashMap<u32, Session>>,
    next_id: AtomicU32,
}

/// All open terminals. Cheap to clone; managed by Tauri (`app.manage`).
#[derive(Clone)]
pub struct Terminals {
    inner: Arc<Registry>,
    limit: usize,
}

impl Default for Terminals {
    fn default() -> Self {
        Self::with_limit(MAX_SESSIONS)
    }
}

type Sink = Arc<dyn Fn(TerminalEvent) -> bool + Send + Sync>;

impl Terminals {
    pub fn with_limit(limit: usize) -> Self {
        Self {
            inner: Arc::default(),
            limit,
        }
    }

    fn sessions(&self) -> MutexGuard<'_, HashMap<u32, Session>> {
        self.inner
            .sessions
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    pub fn len(&self) -> usize {
        self.sessions().len()
    }

    /// Starts `req` in a new PTY; `sink` receives output and the final exit event and returns
    /// `false` once nobody listens anymore (the session is then closed).
    pub fn spawn(
        &self,
        req: SpawnRequest,
        sink: impl Fn(TerminalEvent) -> bool + Send + Sync + 'static,
    ) -> Result<u32, String> {
        if self.len() >= self.limit {
            return Err(format!("too many terminals open (at most {})", self.limit));
        }
        let pair = native_pty_system()
            .openpty(pty_size(req.cols, req.rows))
            .map_err(|e| format!("cannot open a pseudo-terminal: {e}"))?;
        let mut cmd = CommandBuilder::new(&req.program);
        cmd.args(&req.args);
        cmd.cwd(&req.cwd);
        for (k, v) in &req.env {
            cmd.env(k, v);
        }
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("cannot start {}: {e}", req.program.display()))?;
        // Only the child may keep the slave side open, so the reader sees EOF when it exits.
        drop(pair.slave);
        let master = pair.master;
        let reader = master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = master.take_writer().map_err(|e| e.to_string())?;
        let killer = child.clone_killer();
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let sink: Sink = Arc::new(sink);

        let (input, input_rx) = mpsc::channel::<Vec<u8>>();
        spawn_thread(format!("pty-{id}-in"), move || {
            write_loop(writer, &input_rx)
        });

        let (chunks, chunks_rx) = mpsc::channel::<Vec<u8>>();
        spawn_thread(format!("pty-{id}-read"), move || read_loop(reader, &chunks));

        let (drained, drained_rx) = mpsc::channel::<()>();
        let out_sink = sink.clone();
        let this = self.clone();
        spawn_thread(format!("pty-{id}-out"), move || {
            let alive = pump(&chunks_rx, BATCH_WINDOW, |data| {
                out_sink(TerminalEvent::Output { data })
            });
            if !alive {
                tracing::info!(id, "terminal UI gone, closing session");
                this.close(id);
            }
            let _ = drained.send(());
        });

        // Registered before the waiter starts, so a child that exits at once is still removed.
        self.sessions().insert(
            id,
            Session {
                master,
                input,
                killer,
            },
        );

        let this = self.clone();
        spawn_thread(format!("pty-{id}-wait"), move || {
            let code = child.wait().ok().map(|s| s.exit_code());
            // Closing the PTY makes the reader hit EOF (ConPTY keeps the pipe open otherwise).
            this.close(id);
            let _ = drained_rx.recv_timeout(DRAIN_TIMEOUT);
            tracing::info!(id, ?code, "terminal exited");
            sink(TerminalEvent::Exit { code });
        });
        Ok(id)
    }

    fn with_session<T>(&self, id: u32, f: impl FnOnce(&Session) -> T) -> Result<T, String> {
        self.sessions()
            .get(&id)
            .map(f)
            .ok_or_else(|| format!("terminal {id} is not open"))
    }

    /// Queues input for the shell (never blocks on a full PTY buffer).
    pub fn write(&self, id: u32, data: &[u8]) -> Result<(), String> {
        self.with_session(id, |s| s.input.send(data.to_vec()))?
            .map_err(|_| format!("terminal {id} is closing"))
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        self.with_session(id, |s| s.master.resize(pty_size(cols, rows)))?
            .map_err(|e| e.to_string())
    }

    #[cfg(test)]
    fn size(&self, id: u32) -> Result<(u16, u16), String> {
        let size = self
            .with_session(id, |s| s.master.get_size())?
            .map_err(|e| e.to_string())?;
        Ok((size.cols, size.rows))
    }

    /// Kills the shell and closes the PTY. Closing an unknown/finished session is a no-op.
    pub fn close(&self, id: u32) {
        let session = self.sessions().remove(&id);
        drop(session); // outside the lock: closing a PTY can take a moment
    }

    pub fn close_all(&self) {
        let sessions: Vec<_> = self.sessions().drain().map(|(_, s)| s).collect();
        if !sessions.is_empty() {
            tracing::info!(count = sessions.len(), "closing all terminals");
        }
        drop(sessions);
    }
}

fn spawn_thread(name: String, f: impl FnOnce() + Send + 'static) {
    if let Err(e) = thread::Builder::new().name(name).spawn(f) {
        tracing::error!(error = %e, "cannot spawn terminal thread");
    }
}

fn read_loop(mut reader: Box<dyn Read + Send>, out: &Sender<Vec<u8>>) {
    let mut buf = vec![0u8; READ_BUFFER];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if out.send(buf[..n].to_vec()).is_err() {
                    break;
                }
            }
        }
    }
}

fn write_loop(mut writer: Box<dyn Write + Send>, input: &Receiver<Vec<u8>>) {
    for data in input {
        if writer
            .write_all(&data)
            .and_then(|()| writer.flush())
            .is_err()
        {
            break;
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

fn terminals(app: &AppHandle) -> Terminals {
    app.state::<Terminals>().inner().clone()
}

/// Opens a shell in the repository root with the repository's account context. Output and the
/// exit event arrive on `on_event`.
#[tauri::command]
#[specta::specta]
pub async fn terminal_open(
    app: AppHandle,
    repo_id: i64,
    cols: u16,
    rows: u16,
    on_event: Channel<TerminalEvent>,
) -> AppResult<TerminalInfo> {
    let root = repo_root(&app.state::<AppState>(), repo_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let ctx = context(&app.state::<AppState>(), repo_id)?;
        let shell = shell(&app.state::<AppState>().git);
        let mut env = shell.env.clone();
        env.extend(session_env(&ctx, &root));
        let req = SpawnRequest {
            program: shell.program.clone(),
            args: shell.args.clone(),
            cwd: root,
            env,
            cols,
            rows,
        };
        let id = terminals(&app)
            .spawn(req, move |event| on_event.send(event).is_ok())
            .map_err(AppError::Unsupported)?;
        tracing::info!(id, repo = repo_id, shell = %shell.name, "terminal opened");
        Ok(TerminalInfo {
            id,
            shell: shell.name.clone(),
        })
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_write(app: AppHandle, session_id: u32, data: String) -> AppResult<()> {
    terminals(&app)
        .write(session_id, data.as_bytes())
        .map_err(AppError::InvalidInput)
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_resize(
    app: AppHandle,
    session_id: u32,
    cols: u16,
    rows: u16,
) -> AppResult<()> {
    terminals(&app)
        .resize(session_id, cols, rows)
        .map_err(AppError::InvalidInput)
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_close(app: AppHandle, session_id: u32) -> AppResult<()> {
    let terms = terminals(&app);
    tauri::async_runtime::spawn_blocking(move || terms.close(session_id))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nanomgg_git_engine::context::Identity;

    // ---- UTF-8 / batching ----

    #[test]
    fn utf8_sequences_split_across_reads_are_joined() {
        let text = "Tiếng Việt ✓ 🚀";
        let bytes = text.as_bytes();
        // Every split point, including the middle of 2-, 3- and 4-byte sequences.
        for cut in 0..=bytes.len() {
            let mut s = Utf8Stream::default();
            let mut out = s.decode(&bytes[..cut]);
            out.push_str(&s.decode(&bytes[cut..]));
            out.push_str(&s.finish());
            assert_eq!(out, text, "split at {cut}");
        }
        // One byte at a time.
        let mut s = Utf8Stream::default();
        let out: String = bytes.iter().map(|b| s.decode(&[*b])).collect();
        assert_eq!(out, text);
    }

    #[test]
    fn invalid_utf8_is_replaced_and_dangling_bytes_flushed() {
        let mut s = Utf8Stream::default();
        assert_eq!(s.decode(b"a\xffb"), "a\u{FFFD}b");
        assert_eq!(s.decode(&"é".as_bytes()[..1]), "");
        assert_eq!(s.finish(), "\u{FFFD}");
        assert_eq!(s.finish(), "");
    }

    #[test]
    fn pump_batches_chunks_and_decodes_split_characters() {
        let (tx, rx) = mpsc::channel();
        let bytes = "xin chào ✓".as_bytes().to_vec();
        let (a, b) = bytes.split_at(bytes.len() - 2); // inside "✓"
        tx.send(a.to_vec()).unwrap();
        tx.send(b.to_vec()).unwrap();
        drop(tx);
        let mut batches = Vec::new();
        assert!(pump(&rx, Duration::from_millis(50), |s| {
            batches.push(s);
            true
        }));
        assert_eq!(batches, vec!["xin chào ✓".to_owned()]);
    }

    #[test]
    fn pump_stops_when_the_receiver_is_gone() {
        let (tx, rx) = mpsc::channel();
        tx.send(b"one".to_vec()).unwrap();
        let mut calls = 0;
        assert!(!pump(&rx, Duration::from_millis(1), |_| {
            calls += 1;
            false
        }));
        assert_eq!(calls, 1);
    }

    // ---- environment / shell ----

    fn work_ctx() -> AccountContext {
        AccountContext {
            identity: Some(Identity {
                name: "Alice".into(),
                email: "alice@acme.dev".into(),
            }),
            ..Default::default()
        }
    }

    #[test]
    fn session_env_exports_account_context_and_repo() {
        let env = session_env(&work_ctx(), Path::new("/work/repo"));
        let get = |k: &str| {
            env.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("GIT_CONFIG_COUNT"), Some("2"));
        assert_eq!(get("GIT_CONFIG_KEY_1"), Some("user.email"));
        assert_eq!(get("GIT_CONFIG_VALUE_1"), Some("alice@acme.dev"));
        assert_eq!(get("NANOMGG_REPO"), Some("/work/repo"));
        assert_eq!(get("TERM"), Some("xterm-256color"));
        assert_eq!(
            get("GIT_TERMINAL_PROMPT"),
            None,
            "interactive prompts stay allowed"
        );
    }

    #[test]
    fn git_bash_is_found_from_exec_path_or_git_exe() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Git");
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::create_dir_all(root.join("usr").join("bin")).unwrap();
        std::fs::create_dir_all(root.join("mingw64").join("libexec").join("git-core")).unwrap();
        std::fs::create_dir_all(root.join("cmd")).unwrap();
        std::fs::write(root.join("bin").join("bash.exe"), b"").unwrap();
        let bash = root.join("bin").join("bash.exe");

        let exec = root.join("mingw64").join("libexec").join("git-core");
        assert_eq!(find_git_bash(Some(&exec), None, &[]), Some(bash.clone()));
        let git_exe = root.join("cmd").join("git.exe");
        assert_eq!(find_git_bash(None, Some(&git_exe), &[]), Some(bash.clone()));
        assert_eq!(
            find_git_bash(None, None, std::slice::from_ref(&root)),
            Some(bash)
        );

        // A lone bin/bash.exe without Git's MSYS2 layout is not Git Bash.
        let other = dir.path().join("Other");
        std::fs::create_dir_all(other.join("bin")).unwrap();
        std::fs::write(other.join("bin").join("bash.exe"), b"").unwrap();
        assert_eq!(find_git_bash(Some(&other.join("bin")), None, &[]), None);
    }

    #[test]
    fn detected_shell_exists() {
        let shell = detect_shell(&GitRunner::default());
        assert!(
            shell.program.is_file() || find_in_path(&shell.program.to_string_lossy()).is_some(),
            "{shell:?}"
        );
        #[cfg(windows)]
        if shell.name == "Git Bash" {
            assert_eq!(shell.args, ["--login", "-i"]);
        }
    }

    // ---- real PTYs ----

    const TIMEOUT: Duration = Duration::from_secs(30);

    fn request(
        program: &str,
        args: &[&str],
        cwd: &Path,
        env: Vec<(String, String)>,
    ) -> SpawnRequest {
        SpawnRequest {
            program: PathBuf::from(program),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
            cwd: cwd.to_path_buf(),
            env,
            cols: 100,
            rows: 30,
        }
    }

    struct Running {
        terms: Terminals,
        id: u32,
        rx: Receiver<TerminalEvent>,
        output: String,
    }

    fn start(terms: &Terminals, req: SpawnRequest) -> Running {
        let (tx, rx) = mpsc::channel();
        let id = terms
            .spawn(req, move |e| tx.send(e).is_ok())
            .expect("spawn");
        Running {
            terms: terms.clone(),
            id,
            rx,
            output: String::new(),
        }
    }

    impl Running {
        /// Next event, answering ConPTY's cursor-position query like xterm.js does.
        fn next(&mut self, timeout: Duration) -> Option<TerminalEvent> {
            let event = self.rx.recv_timeout(timeout).ok()?;
            if let TerminalEvent::Output { data } = &event {
                if data.contains("\x1b[6n") {
                    let _ = self.terms.write(self.id, b"\x1b[1;1R");
                }
                self.output.push_str(data);
            }
            Some(event)
        }

        fn wait_exit(&mut self) -> Option<u32> {
            let deadline = Instant::now() + TIMEOUT;
            while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
                match self.next(remaining) {
                    Some(TerminalEvent::Exit { code }) => return code,
                    Some(_) => {}
                    None => break,
                }
            }
            panic!("no exit event; output so far: {:?}", self.output);
        }
    }

    #[test]
    fn runs_a_command_and_reports_exit() {
        let dir = tempfile::tempdir().unwrap();
        let terms = Terminals::default();
        let mut run = start(&terms, request("git", &["--version"], dir.path(), vec![]));
        assert_eq!(run.wait_exit(), Some(0));
        assert!(run.output.contains("git version"), "{:?}", run.output);
        assert_eq!(terms.len(), 0, "finished sessions are removed");
    }

    #[test]
    fn account_env_is_visible_inside_the_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let terms = Terminals::default();
        let env = session_env(&work_ctx(), dir.path());

        // git itself picks up the injected identity.
        let mut run = start(
            &terms,
            request(
                "git",
                &["config", "--get", "user.email"],
                dir.path(),
                env.clone(),
            ),
        );
        assert_eq!(run.wait_exit(), Some(0));
        assert!(run.output.contains("alice@acme.dev"), "{:?}", run.output);

        // And the shell sees the raw variables.
        #[cfg(windows)]
        let (program, args): (&str, &[&str]) =
            ("cmd.exe", &["/d", "/c", "echo", "count=%GIT_CONFIG_COUNT%"]);
        #[cfg(not(windows))]
        let (program, args): (&str, &[&str]) = ("/bin/sh", &["-c", "echo count=$GIT_CONFIG_COUNT"]);
        let mut run = start(&terms, request(program, args, dir.path(), env));
        assert_eq!(run.wait_exit(), Some(0));
        assert!(run.output.contains("count=2"), "{:?}", run.output);
    }

    /// The real interactive shell (Git Bash on Windows, `$SHELL -l` elsewhere) starts in the repo
    /// with the account environment, survives its login scripts and exits on `exit`.
    #[test]
    fn interactive_shell_starts_in_repo_with_account_env() {
        let shell = detect_shell(&GitRunner::default());
        if cfg!(windows) && shell.name != "Git Bash" {
            eprintln!("skipped: no Git Bash ({shell:?})");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let terms = Terminals::default();
        let mut env = isolated_shell_env(&shell, home.path());
        env.extend(session_env(&work_ctx(), dir.path()));
        let mut req = request("", &[], dir.path(), env);
        req.program = shell.program.clone();
        req.args = shell.args.clone();
        let mut run = start(&terms, req);
        terms
            .write(
                run.id,
                b"echo \"count=$GIT_CONFIG_COUNT email=$(git config user.email)\"; pwd; exit 7\r",
            )
            .unwrap();
        assert_eq!(run.wait_exit(), Some(7));
        assert!(
            run.output.contains("count=2 email=alice@acme.dev"),
            "{:?}",
            run.output
        );
        let name = dir.path().file_name().unwrap().to_string_lossy();
        assert!(run.output.contains(name.as_ref()), "cwd: {:?}", run.output);
    }

    /// Number of running `sleep <seconds>` processes (identified by their unusual duration).
    fn sleepers(seconds: u32) -> usize {
        #[cfg(windows)]
        let out = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "@(Get-CimInstance Win32_Process -Filter \"Name='sleep.exe'\" | \
                     Where-Object {{ $_.CommandLine -like '* {seconds}' }}).Count"
                ),
            ])
            .output()
            .unwrap();
        #[cfg(not(windows))]
        let out = std::process::Command::new("sh")
            .args(["-c", &format!("pgrep -f 'sleep {seconds}' | wc -l")])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout)
            .trim()
            .parse()
            .unwrap_or(0)
    }

    /// Environment for an interactive login shell in tests: a throw-away `HOME` so the user's
    /// own dotfiles (prompts, ssh-agent start-up…) neither slow down nor block the test, and
    /// nothing (`.bash_history`) is written to the real home directory.
    fn isolated_shell_env(shell: &Shell, home: &Path) -> Vec<(String, String)> {
        let mut env = shell.env.clone();
        env.push(("HOME".into(), home.to_string_lossy().into_owned()));
        env.push(("HISTFILE".into(), String::new()));
        env
    }

    /// Polls `f` while keeping the terminal's event stream (and ConPTY handshake) flowing.
    fn wait_until(run: &mut Running, what: &str, mut f: impl FnMut() -> bool) {
        let deadline = Instant::now() + TIMEOUT;
        while !f() {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}; output: {:?}",
                run.output
            );
            if run.next(Duration::from_millis(200)).is_none() {
                thread::sleep(Duration::from_millis(200));
            }
        }
    }

    /// Closing a terminal ends the whole tree: the shell and what it runs (Git Bash's
    /// `bin/bash.exe` is only a launcher for `usr/bin/bash.exe`).
    #[test]
    fn close_kills_the_shell_and_its_children() {
        let shell = detect_shell(&GitRunner::default());
        if cfg!(windows) && shell.name != "Git Bash" {
            eprintln!("skipped: no Git Bash ({shell:?})");
            return;
        }
        const SECONDS: u32 = 4242;
        let dir = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        let terms = Terminals::default();
        let mut req = request("", &[], dir.path(), isolated_shell_env(&shell, home.path()));
        req.program = shell.program.clone();
        req.args = shell.args.clone();
        let mut run = start(&terms, req);
        terms
            .write(run.id, format!("sleep {SECONDS}\r").as_bytes())
            .unwrap();
        wait_until(&mut run, "sleep to start", || sleepers(SECONDS) == 1);
        terms.close(run.id);
        run.wait_exit();
        wait_until(&mut run, "sleep to be killed", || sleepers(SECONDS) == 0);
    }

    /// Prints something at once, then runs for a minute.
    fn long_running(cwd: &Path) -> SpawnRequest {
        #[cfg(windows)]
        return request("ping.exe", &["-n", "60", "127.0.0.1"], cwd, vec![]);
        #[cfg(not(windows))]
        return request("/bin/sh", &["-c", "echo started; sleep 60"], cwd, vec![]);
    }

    #[test]
    fn resize_changes_the_pty_size() {
        let dir = tempfile::tempdir().unwrap();
        let terms = Terminals::default();
        let mut run = start(&terms, long_running(dir.path()));
        assert_eq!(terms.size(run.id), Ok((100, 30)));
        terms.resize(run.id, 132, 40).unwrap();
        assert_eq!(terms.size(run.id), Ok((132, 40)));
        terms.resize(run.id, 0, 0).unwrap(); // hidden xterm: clamped instead of failing
        assert_eq!(terms.size(run.id), Ok((2, 1)));
        terms.close(run.id);
        run.wait_exit();
    }

    #[test]
    fn close_kills_the_process_and_limit_is_enforced() {
        let dir = tempfile::tempdir().unwrap();
        let terms = Terminals::with_limit(1);
        let mut run = start(&terms, long_running(dir.path()));
        let err = terms
            .spawn(long_running(dir.path()), |_| true)
            .expect_err("limit");
        assert!(err.contains("at most 1"), "{err}");

        let started = Instant::now();
        terms.close(run.id);
        run.wait_exit();
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "killed promptly"
        );
        assert_eq!(terms.len(), 0);
        assert!(terms.write(run.id, b"x").is_err());
        terms.close(run.id); // idempotent
    }

    #[test]
    fn close_all_and_vanished_ui_end_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let terms = Terminals::default();
        let mut a = start(&terms, long_running(dir.path()));
        let mut b = start(&terms, long_running(dir.path()));
        assert_eq!(terms.len(), 2);
        terms.close_all();
        a.wait_exit();
        b.wait_exit();
        assert_eq!(terms.len(), 0);

        // A sink that reports "nobody listens" (webview reloaded) closes its session.
        let id = terms.spawn(long_running(dir.path()), |_| false).unwrap();
        let deadline = Instant::now() + TIMEOUT;
        while terms.with_session(id, |_| ()).is_ok() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(terms.len(), 0);
    }
}
