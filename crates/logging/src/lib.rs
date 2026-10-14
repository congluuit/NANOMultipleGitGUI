//! Tracing setup with secret redaction for NANOMultipleGitGUI logs.
//!
//! Every formatted log line passes through [`redact`] before it reaches a file or the console,
//! so a token that slips into an error message or a git stderr dump is never persisted.

mod redact;

use std::io::{self, Write};
use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
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

/// Installs the global subscriber: daily-rotated `nanomgg.log` in `log_dir`
/// (plus stderr in debug builds). Keep the returned guard alive until exit to flush logs.
pub fn init(log_dir: &Path) -> io::Result<WorkerGuard> {
    std::fs::create_dir_all(log_dir)?;
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("nanomgg")
        .filename_suffix("log")
        .max_log_files(14)
        .build(log_dir)
        .map_err(io::Error::other)?;
    let (file_writer, guard) = tracing_appender::non_blocking(appender);

    let filter =
        EnvFilter::try_from_env(LOG_ENV).unwrap_or_else(|_| EnvFilter::new(DEFAULT_FILTER));
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(Redacting(file_writer));
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
    Ok(guard)
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
