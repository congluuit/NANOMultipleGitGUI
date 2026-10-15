//! Tracing setup with secret redaction for NANOMultipleGitGUI logs.
//!
//! Every formatted log line passes through [`redact`] before it reaches a file or the console,
//! so a token that slips into an error message or a git stderr dump is never persisted.

mod redact;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub use redact::{REDACTED, redact};

/// Environment variable overriding the log filter, e.g. `NANOMGG_LOG=debug,nanomgg=trace`.
pub const LOG_ENV: &str = "NANOMGG_LOG";
const DEFAULT_FILTER: &str = "info,nanomgg=debug,nanomgg_lib=debug";

/// Wraps a writer so each write is redacted before being forwarded.
///
/// `tracing-subscriber`'s fmt layer formats a whole event into one buffer and writes it
/// with a single `write_all`, so per-write redaction sees complete lines.
pub struct RedactingWriter<W> {
    inner: W,
}

impl<W: Write> Write for RedactingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match std::str::from_utf8(buf) {
            Ok(text) => self.inner.write_all(redact(text).as_bytes())?,
            Err(_) => self
                .inner
                .write_all(redact(&String::from_utf8_lossy(buf)).as_bytes())?,
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// [`MakeWriter`] adapter producing [`RedactingWriter`]s.
#[derive(Clone)]
pub struct Redacting<M>(pub M);

impl<'a, M: MakeWriter<'a>> MakeWriter<'a> for Redacting<M> {
    type Writer = RedactingWriter<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        RedactingWriter {
            inner: self.0.make_writer(),
        }
    }
}

/// Log files are `<prefix>.<yyyy-mm-dd>.log`.
pub const LOG_FILE_PREFIX: &str = "nanomgg";
/// Lines kept in memory while the log folder is being moved.
const PAUSE_BUFFER_LIMIT: usize = 1024 * 1024;

enum Sink {
    File(NonBlocking),
    /// The folder is being moved: lines wait in memory and are written once it is reopened.
    Paused(Arc<Mutex<Vec<u8>>>),
}

struct Shared {
    sink: RwLock<Sink>,
    guard: Mutex<Option<WorkerGuard>>,
    dir: Mutex<PathBuf>,
}

/// The log file writer, which can be pointed at another folder while the app runs
/// (Settings → Storage). Keep it alive until exit: dropping it stops file logging.
#[derive(Clone)]
pub struct LogHandle(Arc<Shared>);

/// Writer handed to the fmt layer for each event.
pub enum SinkWriter {
    File(NonBlocking),
    Paused(Arc<Mutex<Vec<u8>>>),
}

impl Write for SinkWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            SinkWriter::File(w) => w.write(buf),
            SinkWriter::Paused(b) => {
                let mut b = b.lock().unwrap_or_else(|e| e.into_inner());
                if b.len() + buf.len() <= PAUSE_BUFFER_LIMIT {
                    b.extend_from_slice(buf);
                }
                Ok(buf.len())
            }
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            SinkWriter::File(w) => w.flush(),
            SinkWriter::Paused(_) => Ok(()),
        }
    }
}

impl<'a> MakeWriter<'a> for LogHandle {
    type Writer = SinkWriter;

    fn make_writer(&'a self) -> Self::Writer {
        match &*self.0.sink.read().unwrap_or_else(|e| e.into_inner()) {
            Sink::File(w) => SinkWriter::File(w.clone()),
            Sink::Paused(b) => SinkWriter::Paused(b.clone()),
        }
    }
}

fn open_dir(dir: &Path) -> io::Result<(NonBlocking, WorkerGuard)> {
    std::fs::create_dir_all(dir)?;
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix("log")
        .max_log_files(14)
        .build(dir)
        .map_err(io::Error::other)?;
    Ok(tracing_appender::non_blocking(appender))
}

impl LogHandle {
    /// Opens the daily-rotated file in `dir` (not installed as a subscriber).
    pub fn open(dir: &Path) -> io::Result<Self> {
        let (writer, guard) = open_dir(dir)?;
        Ok(Self(Arc::new(Shared {
            sink: RwLock::new(Sink::File(writer)),
            guard: Mutex::new(Some(guard)),
            dir: Mutex::new(dir.to_owned()),
        })))
    }

    /// Folder of the current log file.
    pub fn dir(&self) -> PathBuf {
        self.0.dir.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Flushes and closes the current file; later lines are kept in memory until
    /// [`LogHandle::resume`]. Used while the log folder is copied elsewhere.
    pub fn pause(&self) {
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let previous = std::mem::replace(
            &mut *self.0.sink.write().unwrap_or_else(|e| e.into_inner()),
            Sink::Paused(buffer),
        );
        drop(previous);
        // Dropping the guard flushes the queue and closes the file.
        let guard = self
            .0
            .guard
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        drop(guard);
    }

    /// Writes to a file in `dir` from now on (lines buffered by [`LogHandle::pause`] first).
    /// On error the handle keeps its previous state.
    pub fn resume(&self, dir: &Path) -> io::Result<()> {
        let (mut writer, guard) = open_dir(dir)?;
        let previous = {
            let mut sink = self.0.sink.write().unwrap_or_else(|e| e.into_inner());
            if let Sink::Paused(buffer) = &*sink {
                let buffered =
                    std::mem::take(&mut *buffer.lock().unwrap_or_else(|e| e.into_inner()));
                writer.write_all(&buffered)?;
            }
            std::mem::replace(&mut *sink, Sink::File(writer))
        };
        drop(previous);
        let old = self
            .0
            .guard
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace(guard);
        drop(old);
        *self.0.dir.lock().unwrap_or_else(|e| e.into_inner()) = dir.to_owned();
        Ok(())
    }
}

/// Installs the global subscriber: daily-rotated `nanomgg.<date>.log` in `log_dir`
/// (plus stderr in debug builds). Keep the returned handle alive until exit to flush logs.
pub fn init(log_dir: &Path) -> io::Result<LogHandle> {
    let handle = LogHandle::open(log_dir)?;

    let filter =
        EnvFilter::try_from_env(LOG_ENV).unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(Redacting(handle.clone()));
    let console_layer = cfg!(debug_assertions).then(|| {
        tracing_subscriber::fmt::layer()
            .with_writer(Redacting(io::stderr))
            .with_target(false)
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(console_layer)
        .try_init()
        .map_err(io::Error::other)?;
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);

    impl Write for Buf {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn log_text(dir: &Path) -> String {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| std::fs::read_to_string(e.path()).unwrap())
            .collect()
    }

    #[test]
    fn log_folder_can_be_switched_while_running() {
        let tmp = tempfile::tempdir().unwrap();
        let (first, second) = (tmp.path().join("a"), tmp.path().join("b"));
        let handle = LogHandle::open(&first).unwrap();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(Redacting(handle.clone()))
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!("line one");
            handle.pause();
            tracing::info!("line two ghp_1234567890abcdefghijABCDEFGHIJ123456");
            handle.resume(&second).unwrap();
            tracing::info!("line three");
        });
        assert_eq!(handle.dir(), second);
        // Flush the second file.
        handle.pause();
        let a = log_text(&first);
        let b = log_text(&second);
        assert!(a.contains("line one") && !a.contains("line two"), "{a}");
        assert!(
            b.contains("line two [REDACTED]") && b.contains("line three"),
            "{b}"
        );
        assert!(!b.contains("line one"), "{b}");
        let names: Vec<String> = std::fs::read_dir(&second)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names
                .iter()
                .all(|n| n.starts_with("nanomgg.") && n.ends_with(".log")),
            "{names:?}"
        );
    }

    #[test]
    fn subscriber_output_is_redacted() {
        let buf = Buf::default();
        let sink = buf.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(Redacting(move || sink.clone()))
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(
                remote = "https://bot:glpat-abcdefghijklmnopqrst@gitlab.com/x.git",
                "push failed for ghp_1234567890abcdefghijABCDEFGHIJ123456"
            );
        });
        let out = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        assert!(out.contains("push failed for [REDACTED]"), "{out}");
        assert!(
            out.contains("https://bot:[REDACTED]@gitlab.com/x.git"),
            "{out}"
        );
        assert!(!out.contains("ghp_"), "{out}");
        assert!(!out.contains("glpat-"), "{out}");
    }
}
