//! `GIT_LFS_PROGRESS`: git-lfs appends one line per progress update to the file named by this
//! variable:
//!
//! ```text
//! upload 1/3 32768/31457280 a.bin
//! download 2/5 8192/-1 sub dir/b.bin
//! ```
//!
//! i.e. `<direction> <file index>/<file count> <bytes so far>/<file size> <name>`. The byte
//! counts are for **that file**, not the whole transfer; the size is `-1` when git-lfs does not
//! know it (downloads). Files transferred in parallel interleave their lines, so the file index
//! (unique within one command) identifies a file. [`ProgressTracker`] folds the lines into
//! totals, speed and ETA; [`ProgressFile`] reads new lines while git-lfs is writing.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::LfsObject;

/// Direction of an LFS transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum LfsDirection {
    Upload,
    Download,
    /// Writing downloaded objects into the working tree (smudge during `git lfs pull`).
    Checkout,
}

/// One line of the progress file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressLine {
    pub direction: LfsDirection,
    /// 1-based index of the file within this command.
    pub index: u32,
    pub count: u32,
    pub bytes_done: u64,
    /// `None` when git-lfs printed `-1`.
    pub bytes_total: Option<u64>,
    pub name: String,
}

/// Parses one progress-file line; `None` for anything else.
pub fn parse_progress_line(line: &str) -> Option<ProgressLine> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut parts = line.splitn(4, ' ');
    let direction = match parts.next()? {
        "upload" => LfsDirection::Upload,
        "download" => LfsDirection::Download,
        "checkout" => LfsDirection::Checkout,
        _ => return None,
    };
    let (index, count) = parts.next()?.split_once('/')?;
    let (done, total) = parts.next()?.split_once('/')?;
    let name = parts.next()?.to_owned();
    let total: i64 = total.parse().ok()?;
    Some(ProgressLine {
        direction,
        index: index.parse().ok()?,
        count: count.parse().ok()?,
        bytes_done: done.parse().ok()?,
        bytes_total: u64::try_from(total).ok(),
        name,
    })
}

/// Progress of one file being transferred.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LfsFileProgress {
    pub name: String,
    pub bytes_done: u64,
    /// `None` when neither git-lfs nor the estimate knows the size.
    pub bytes_total: Option<u64>,
}

/// Aggregated progress of one LFS command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LfsTransferStats {
    pub direction: LfsDirection,
    pub files_done: u32,
    pub files_total: u32,
    pub bytes_done: u64,
    /// Best known total: the estimate, or the sizes git-lfs reported so far.
    pub bytes_total: u64,
    /// File most recently reported.
    pub current_file: Option<String>,
    /// Average over the last few seconds.
    pub bytes_per_sec: u64,
    /// `None` until the speed is known.
    pub eta_secs: Option<u64>,
    /// Files currently in flight (at most `lfs.concurrenttransfers`, capped).
    pub active: Vec<LfsFileProgress>,
    pub done: bool,
}

#[derive(Debug, Clone)]
struct FileState {
    name: String,
    done: u64,
    total: Option<u64>,
}

impl FileState {
    fn finished(&self) -> bool {
        self.total.is_some_and(|t| self.done >= t)
    }
}

/// Window used for the transfer speed.
const SPEED_WINDOW: Duration = Duration::from_secs(5);
/// Most files listed in [`LfsTransferStats::active`].
const MAX_ACTIVE: usize = 16;

/// Folds progress lines into [`LfsTransferStats`].
#[derive(Debug)]
pub struct ProgressTracker {
    direction: LfsDirection,
    /// Sizes by path from the estimate (download lines carry `-1`).
    known_sizes: HashMap<String, u64>,
    expected_files: u32,
    expected_bytes: u64,
    files: BTreeMap<u32, FileState>,
    count: u32,
    current: Option<String>,
    samples: VecDeque<(Instant, u64)>,
}

impl ProgressTracker {
    /// `estimate`: the objects expected to move, if known (sizes for downloads, totals).
    pub fn new(direction: LfsDirection, estimate: Option<&[LfsObject]>) -> Self {
        let objects = estimate.unwrap_or_default();
        Self {
            direction,
            known_sizes: objects
                .iter()
                .filter(|o| o.size > 0)
                .map(|o| (o.path.clone(), o.size))
                .collect(),
            expected_files: objects.len() as u32,
            expected_bytes: objects.iter().map(|o| o.size).sum(),
            files: BTreeMap::new(),
            count: 0,
            current: None,
            samples: VecDeque::new(),
        }
    }

    pub fn update(&mut self, line: &ProgressLine, now: Instant) {
        // `git lfs pull` reports downloads and then checkouts of the same files; the
        // checkout pass would double the byte counts, so only the main direction counts.
        if line.direction != self.direction {
            return;
        }
        self.count = self.count.max(line.count);
        let total = line
            .bytes_total
            .or_else(|| self.known_sizes.get(&line.name).copied());
        let state = self.files.entry(line.index).or_insert_with(|| FileState {
            name: line.name.clone(),
            done: 0,
            total,
        });
        state.done = state.done.max(line.bytes_done);
        if state.total.is_none() {
            state.total = total;
        }
        self.current = Some(line.name.clone());
        let bytes = self.bytes_done();
        self.samples.push_back((now, bytes));
        while let Some(&(at, _)) = self.samples.front() {
            if now.duration_since(at) > SPEED_WINDOW && self.samples.len() > 2 {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    fn bytes_done(&self) -> u64 {
        self.files
            .values()
            .map(|f| f.total.map_or(f.done, |t| f.done.min(t)))
            .sum()
    }

    fn speed(&self, now: Instant) -> u64 {
        let (Some(&(first_at, first)), Some(&(_, last))) =
            (self.samples.front(), self.samples.back())
        else {
            return 0;
        };
        let secs = now.duration_since(first_at).as_secs_f64();
        if secs < 0.5 {
            return 0;
        }
        (last.saturating_sub(first) as f64 / secs) as u64
    }

    /// Current totals; `done` marks the end of a successful command (everything counted).
    pub fn snapshot(&self, now: Instant, done: bool) -> LfsTransferStats {
        let seen_bytes: u64 = self.files.values().filter_map(|f| f.total).sum();
        let bytes_total = self.expected_bytes.max(seen_bytes);
        let files_total = self.expected_files.max(self.count);
        let mut bytes_done = self.bytes_done();
        let mut files_done = self.files.values().filter(|f| f.finished()).count() as u32;
        if done {
            bytes_done = bytes_total.max(bytes_done);
            files_done = files_total.max(files_done);
        }
        let bytes_per_sec = if done { 0 } else { self.speed(now) };
        let eta_secs = (!done && bytes_per_sec > 0 && bytes_total >= bytes_done)
            .then(|| (bytes_total - bytes_done).div_ceil(bytes_per_sec));
        let active = if done {
            Vec::new()
        } else {
            self.files
                .values()
                .filter(|f| !f.finished())
                .take(MAX_ACTIVE)
                .map(|f| LfsFileProgress {
                    name: f.name.clone(),
                    bytes_done: f.done,
                    bytes_total: f.total,
                })
                .collect()
        };
        LfsTransferStats {
            direction: self.direction,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
            current_file: self.current.clone(),
            bytes_per_sec,
            eta_secs,
            active,
            done,
        }
    }

    /// Names of the files that went through completely.
    pub fn finished_files(&self) -> Vec<String> {
        self.files
            .values()
            .filter(|f| f.finished())
            .map(|f| f.name.clone())
            .collect()
    }
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A temporary progress file that git-lfs appends to and we read incrementally.
/// The file is removed on drop.
#[derive(Debug)]
pub struct ProgressFile {
    path: PathBuf,
    file: Option<File>,
    pending: Vec<u8>,
}

impl ProgressFile {
    /// Creates an empty, uniquely named file in the system temp directory.
    pub fn create() -> std::io::Result<Self> {
        Self::create_in(&std::env::temp_dir())
    }

    pub fn create_in(dir: &Path) -> std::io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(
            "nanomgg-lfs-{}-{nanos:x}-{n}.progress",
            std::process::id()
        ));
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self {
            path,
            file: None,
            pending: Vec::new(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Complete lines appended since the last call.
    pub fn read_lines(&mut self) -> Vec<String> {
        if self.file.is_none() {
            self.file = File::open(&self.path).ok();
        }
        let Some(file) = self.file.as_mut() else {
            return Vec::new();
        };
        let mut buf = Vec::new();
        if file.read_to_end(&mut buf).is_err() || buf.is_empty() {
            return Vec::new();
        }
        self.pending.extend_from_slice(&buf);
        let mut lines = Vec::new();
        while let Some(pos) = self.pending.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = self.pending.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&raw).trim_end().to_owned();
            if !line.is_empty() {
                lines.push(line);
            }
        }
        lines
    }
}

impl Drop for ProgressFile {
    fn drop(&mut self) {
        self.file = None;
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Captured from `git lfs push` (git-lfs 3.5.1) against the local test server.
    const UPLOAD: &str = "upload 1/3 32768/31457280 a.bin
upload 1/3 31457280/31457280 a.bin
upload 2/3 32768/20971520 sub dir/b.bin
upload 2/3 20971520/20971520 sub dir/b.bin
upload 3/3 1000/1000 c.bin
";

    /// Captured from `git lfs pull --include="sub dir/b.bin"`.
    const DOWNLOAD: &str = "download 1/1 8192/-1 sub dir/b.bin
download 1/1 20971520/-1 sub dir/b.bin
";

    fn obj(path: &str, size: u64) -> LfsObject {
        LfsObject {
            oid: format!("{:0>64}", size),
            size,
            path: path.into(),
        }
    }

    #[test]
    fn parses_upload_and_download_lines() {
        let l = parse_progress_line("upload 2/3 32768/20971520 sub dir/b.bin").unwrap();
        assert_eq!(l.direction, LfsDirection::Upload);
        assert_eq!((l.index, l.count), (2, 3));
        assert_eq!((l.bytes_done, l.bytes_total), (32768, Some(20971520)));
        assert_eq!(l.name, "sub dir/b.bin");

        let l = parse_progress_line("download 1/1 8192/-1 sub dir/b.bin\r\n").unwrap();
        assert_eq!(l.direction, LfsDirection::Download);
        assert_eq!(l.bytes_total, None);

        let l = parse_progress_line("checkout 1/2 10/10 Ảnh/ảnh đẹp.psd").unwrap();
        assert_eq!(l.direction, LfsDirection::Checkout);
        assert_eq!(l.name, "Ảnh/ảnh đẹp.psd");
    }

    #[test]
    fn rejects_other_lines() {
        for line in [
            "",
            "Uploading LFS objects: 100% (3/3), 52 MB | 13 MB/s, done.",
            "upload 1/3 32768 a.bin",
            "upload x/3 1/2 a.bin",
            "upload 1/3 1/2",
            "push abc => a.bin",
        ] {
            assert_eq!(parse_progress_line(line), None, "{line:?}");
        }
    }

    #[test]
    fn tracks_totals_per_file() {
        let t0 = Instant::now();
        let estimate = [
            obj("a.bin", 31457280),
            obj("sub dir/b.bin", 20971520),
            obj("c.bin", 1000),
        ];
        let mut tr = ProgressTracker::new(LfsDirection::Upload, Some(&estimate));
        let lines: Vec<_> = UPLOAD.lines().filter_map(parse_progress_line).collect();
        tr.update(&lines[0], t0);
        let s = tr.snapshot(t0, false);
        assert_eq!((s.files_done, s.files_total), (0, 3));
        assert_eq!(s.bytes_done, 32768);
        assert_eq!(s.bytes_total, 31457280 + 20971520 + 1000);
        assert_eq!(s.current_file.as_deref(), Some("a.bin"));
        assert_eq!(s.active.len(), 1);

        tr.update(&lines[1], t0 + Duration::from_secs(1));
        tr.update(&lines[2], t0 + Duration::from_secs(2));
        let s = tr.snapshot(t0 + Duration::from_secs(2), false);
        assert_eq!(s.files_done, 1);
        assert_eq!(s.bytes_done, 31457280 + 32768);
        assert_eq!(s.current_file.as_deref(), Some("sub dir/b.bin"));
        // (31457280 + 32768 - 32768) bytes in 2 s.
        assert_eq!(s.bytes_per_sec, 31457280 / 2);
        let left = s.bytes_total - s.bytes_done;
        assert_eq!(s.eta_secs, Some(left.div_ceil(s.bytes_per_sec)));
        assert_eq!(tr.finished_files(), vec!["a.bin".to_owned()]);

        for l in &lines[3..] {
            tr.update(l, t0 + Duration::from_secs(3));
        }
        let s = tr.snapshot(t0 + Duration::from_secs(3), true);
        assert_eq!((s.files_done, s.files_total), (3, 3));
        assert_eq!(s.bytes_done, s.bytes_total);
        assert!(s.done && s.active.is_empty() && s.eta_secs.is_none());
    }

    #[test]
    fn interleaved_parallel_lines_are_kept_apart() {
        let t0 = Instant::now();
        let mut tr = ProgressTracker::new(LfsDirection::Upload, None);
        for line in [
            "upload 1/2 100/1000 same-name.bin",
            "upload 2/2 50/500 same-name.bin",
            "upload 1/2 400/1000 same-name.bin",
            "upload 2/2 500/500 same-name.bin",
        ] {
            tr.update(&parse_progress_line(line).unwrap(), t0);
        }
        let s = tr.snapshot(t0, false);
        assert_eq!(s.bytes_done, 900);
        assert_eq!(s.bytes_total, 1500, "sizes reported by git-lfs");
        assert_eq!(s.files_done, 1);
        assert_eq!(s.files_total, 2);
        assert_eq!(s.active.len(), 1);
        assert_eq!(s.active[0].bytes_done, 400);
    }

    #[test]
    fn download_sizes_come_from_the_estimate() {
        let t0 = Instant::now();
        let estimate = [obj("sub dir/b.bin", 20971520)];
        let mut tr = ProgressTracker::new(LfsDirection::Download, Some(&estimate));
        let lines: Vec<_> = DOWNLOAD.lines().filter_map(parse_progress_line).collect();
        tr.update(&lines[0], t0);
        let s = tr.snapshot(t0, false);
        assert_eq!(s.active[0].bytes_total, Some(20971520));
        tr.update(&lines[1], t0);
        let s = tr.snapshot(t0, false);
        assert_eq!((s.files_done, s.bytes_done), (1, 20971520));

        // Unknown size and no estimate: counted, never "finished" until the end.
        let mut tr = ProgressTracker::new(LfsDirection::Download, None);
        tr.update(&lines[1], t0);
        let s = tr.snapshot(t0, false);
        assert_eq!(
            (s.files_done, s.bytes_done, s.bytes_total),
            (0, 20971520, 0)
        );
        assert_eq!(tr.snapshot(t0, true).files_done, 1);
    }

    #[test]
    fn checkout_pass_does_not_double_count() {
        let t0 = Instant::now();
        let mut tr = ProgressTracker::new(LfsDirection::Download, None);
        tr.update(
            &parse_progress_line("download 1/1 10/10 a.bin").unwrap(),
            t0,
        );
        tr.update(
            &parse_progress_line("checkout 1/1 10/10 a.bin").unwrap(),
            t0,
        );
        assert_eq!(tr.snapshot(t0, false).bytes_done, 10);
    }

    #[test]
    fn progress_file_yields_only_complete_lines() {
        let dir = tempfile::tempdir().unwrap();
        let mut pf = ProgressFile::create_in(dir.path()).unwrap();
        let path = pf.path().to_owned();
        assert!(pf.read_lines().is_empty());
        let mut w = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        use std::io::Write;
        w.write_all(b"upload 1/1 1/2 a.bin\nupload 1/1 2/").unwrap();
        w.flush().unwrap();
        assert_eq!(pf.read_lines(), vec!["upload 1/1 1/2 a.bin".to_owned()]);
        w.write_all(b"2 a.bin\n").unwrap();
        w.flush().unwrap();
        assert_eq!(pf.read_lines(), vec!["upload 1/1 2/2 a.bin".to_owned()]);
        drop(w);
        drop(pf);
        assert!(!path.exists(), "removed on drop");
    }
}
