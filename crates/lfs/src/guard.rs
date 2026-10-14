//! Large File Guard (PLAN §6.2): before a commit, find files that are too big for the
//! provider, binary files that belong in LFS and probable build output.
//!
//! **Which size counts.** The guard judges what the commit will actually contain:
//! - a path in the index is judged by its *staged blob* (`git cat-file --batch-check` on
//!   `:<path>`), so an LFS file staged as a pointer is ~130 bytes and passes, while a file that
//!   was staged before its pattern was tracked is caught (`needs_renormalize`);
//! - a path not in the index (new, unstaged) is judged by its *size on disk*, unless it is
//!   already LFS-tracked, in which case `git add` will store a pointer.
//!
//! Both sizes are reported so the UI can show the real file size next to the verdict.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;

use nanomgg_git_engine::{AccountContext, GitRunner};
use serde::{Deserialize, Serialize};

use crate::LfsError;
use crate::setup::normalize_path;

pub const MIB: u64 = 1024 * 1024;

/// Binary files at least this big get a [`GuardLevel::Suggest`] even below the warn threshold
/// (small icons and fonts are not worth LFS).
pub const SUGGEST_MIN_BYTES: u64 = MIB;

/// git's own binary heuristic looks for a NUL in the first 8000 bytes.
pub const SNIFF_BYTES: usize = 8000;

/// LFS pointer files are always smaller than this (git-lfs spec).
pub const MAX_POINTER_BYTES: u64 = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum GuardProvider {
    Github,
    Gitlab,
    /// Any other host: warn like GitLab, never block.
    Other,
}

/// Size limits in bytes: warn above `warn_bytes`, block above `block_bytes` (if any).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GuardThresholds {
    pub warn_bytes: u64,
    pub block_bytes: Option<u64>,
}

impl GuardThresholds {
    /// GitHub rejects pushes with files over 100 MiB and warns over 50 MiB.
    pub const GITHUB: Self = Self {
        warn_bytes: 50 * MIB,
        block_bytes: Some(100 * MIB),
    };
    /// GitLab limits vary by instance: warn only (PLAN §6.2).
    pub const GITLAB: Self = Self {
        warn_bytes: 50 * MIB,
        block_bytes: None,
    };

    pub fn for_provider(provider: GuardProvider) -> Self {
        match provider {
            GuardProvider::Github => Self::GITHUB,
            GuardProvider::Gitlab | GuardProvider::Other => Self::GITLAB,
        }
    }
}

/// Verdict for one path, from least to most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum GuardLevel {
    None,
    /// Below the thresholds but looks binary and is not in LFS: suggest tracking it.
    Suggest,
    Warn,
    /// Over the provider's hard limit: the push would be rejected.
    Block,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GuardFinding {
    /// Repository-relative path with `/` separators.
    pub path: String,
    /// Bytes the commit would store for this path (see module docs); `None` when the path is
    /// neither in the index nor a file on disk (deleted).
    pub size: Option<u64>,
    /// Size of the file in the working tree.
    pub disk_size: Option<u64>,
    /// Size of the staged blob (`git cat-file -s :<path>`), when the path is in the index.
    pub staged_size: Option<u64>,
    pub level: GuardLevel,
    /// `.gitattributes` routes this path through LFS (`filter=lfs`).
    pub already_lfs: bool,
    /// The staged blob is an LFS pointer.
    pub staged_pointer: bool,
    /// LFS-tracked but staged as full content (staged before tracking): needs
    /// `git add --renormalize`.
    pub needs_renormalize: bool,
    /// Known binary/media/archive extension, or a NUL byte in the first 8000 bytes.
    pub looks_binary: bool,
    /// Under `build/`, `dist/`, `out/`, `bin/`, `obj/`, `target/`, or a `.zip` / `.exe`.
    pub looks_like_build_output: bool,
    /// `*.<ext>` pattern for "track by extension"; `None` without an extension.
    pub pattern_by_extension: Option<String>,
    /// Anchored pattern matching exactly this path, for "track by path".
    pub pattern_by_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GuardReport {
    pub thresholds: GuardThresholds,
    /// One finding per distinct input path, in input order.
    pub findings: Vec<GuardFinding>,
    pub suggest_count: u32,
    pub warn_count: u32,
    pub block_count: u32,
}

impl GuardReport {
    /// Some file exceeds the hard limit ("commit anyway" should be disabled).
    pub fn blocked(&self) -> bool {
        self.block_count > 0
    }
}

/// Extensions (lower case) of files that are binary in practice: media, design, 3D/game
/// assets, archives, executables/libraries, office documents, fonts, data/model files.
pub const BINARY_EXTENSIONS: &[&str] = &[
    // images & design
    "psd",
    "psb",
    "ai",
    "eps",
    "tif",
    "tiff",
    "png",
    "jpg",
    "jpeg",
    "gif",
    "bmp",
    "ico",
    "webp",
    "heic",
    "avif",
    "exr",
    "hdr",
    "tga",
    "dds",
    "raw",
    "cr2",
    "nef",
    "kra",
    "xcf",
    "sketch",
    "fig",
    "xd",
    "indd", // video
    "mp4",
    "mov",
    "avi",
    "mkv",
    "wmv",
    "webm",
    "flv",
    "m4v",
    "mpg",
    "mpeg",
    "3gp",
    // audio
    "wav",
    "mp3",
    "ogg",
    "flac",
    "aac",
    "m4a",
    "aif",
    "aiff",
    "wma",
    "opus",
    "mid",
    // 3D & game engines
    "fbx",
    "blend",
    "max",
    "ma",
    "mb",
    "3ds",
    "glb",
    "usdz",
    "abc",
    "unitypackage",
    "unity3d",
    "uasset",
    "umap",
    "pak",
    "bank", // archives & packages
    "zip",
    "7z",
    "rar",
    "tar",
    "gz",
    "tgz",
    "bz2",
    "xz",
    "zst",
    "lz4",
    "iso",
    "dmg",
    "jar",
    "war",
    "apk",
    "aab",
    "ipa",
    "nupkg",
    "deb",
    "rpm",
    "msi",
    "cab",
    "vsix",
    // executables & libraries
    "exe",
    "dll",
    "so",
    "dylib",
    "lib",
    "a",
    "o",
    "pdb",
    "bin",
    "wasm",
    "class",
    "pyd",
    // documents
    "pdf",
    "doc",
    "docx",
    "xls",
    "xlsx",
    "ppt",
    "pptx",
    "key",
    "odt",
    "ods",
    "odp",
    // fonts
    "ttf",
    "otf",
    "woff",
    "woff2",
    "eot", // data & ML models
    "sqlite",
    "sqlite3",
    "db",
    "mdb",
    "h5",
    "hdf5",
    "npy",
    "npz",
    "pt",
    "pth",
    "onnx",
    "ckpt",
    "safetensors",
    "pkl",
    "parquet",
    "tfrecord",
    "gguf",
];

/// Directory names that usually hold generated output.
pub const BUILD_DIRS: &[&str] = &["build", "dist", "out", "bin", "obj", "target"];

/// Extension of the file name (after the last `.`), unless the name is a dotfile like
/// `.gitignore`.
pub fn extension(path: &str) -> Option<&str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty() && !ext.is_empty()).then_some(ext)
}

pub fn has_binary_extension(path: &str) -> bool {
    extension(path).is_some_and(|e| {
        let e = e.to_ascii_lowercase();
        BINARY_EXTENSIONS.contains(&e.as_str())
    })
}

/// git's heuristic: a NUL byte in the first [`SNIFF_BYTES`] bytes.
pub fn sniff_binary(head: &[u8]) -> bool {
    head[..head.len().min(SNIFF_BYTES)].contains(&0)
}

pub fn looks_like_build_output(path: &str) -> bool {
    let mut parts: Vec<&str> = path.split('/').collect();
    parts.pop(); // file name
    let in_build_dir = parts
        .iter()
        .any(|d| BUILD_DIRS.iter().any(|b| d.eq_ignore_ascii_case(b)));
    in_build_dir
        || extension(path)
            .is_some_and(|e| e.eq_ignore_ascii_case("zip") || e.eq_ignore_ascii_case("exe"))
}

/// Escapes glob metacharacters with bracket expressions (`[*]`), which work the same on
/// every platform (a backslash could be taken for a path separator on Windows).
fn escape_glob(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '*' | '?' | '[' | '\\' => {
                out.push('[');
                out.push(c);
                out.push(']');
            }
            _ => out.push(c),
        }
    }
    out
}

/// `*.<ext>` keeping the extension's case (attribute matching is case-sensitive unless
/// `core.ignorecase` is set).
pub fn pattern_by_extension(path: &str) -> Option<String> {
    extension(path).map(|e| format!("*.{}", escape_glob(e)))
}

/// `/<path>` with glob characters escaped: the leading slash anchors the pattern to the
/// repository root, so a same-named file elsewhere is not tracked by accident.
pub fn pattern_by_path(path: &str) -> String {
    format!("/{}", escape_glob(path))
}

/// Whether `content` is a Git LFS pointer file.
pub fn is_lfs_pointer(content: &[u8]) -> bool {
    content.len() < MAX_POINTER_BYTES as usize
        && content.starts_with(b"version https://git-lfs.github.com/spec/v1\n")
        && content.windows(11).any(|w| w == b"\noid sha256")
        && content.windows(6).any(|w| w == b"\nsize ")
}

/// Bytes the commit would store: the staged blob if staged, else the disk file unless LFS
/// turns it into a pointer on `git add`.
pub fn committed_size(
    staged_size: Option<u64>,
    staged_pointer: bool,
    disk_size: Option<u64>,
    already_lfs: bool,
) -> Option<u64> {
    match staged_size {
        Some(_) if staged_pointer => Some(0),
        Some(s) => Some(s),
        None if already_lfs => disk_size.map(|_| 0),
        None => disk_size,
    }
}

pub fn classify(
    size: Option<u64>,
    thresholds: &GuardThresholds,
    looks_binary: bool,
    already_lfs: bool,
) -> GuardLevel {
    let Some(size) = size else {
        return GuardLevel::None;
    };
    if thresholds.block_bytes.is_some_and(|b| size > b) {
        GuardLevel::Block
    } else if size > thresholds.warn_bytes {
        GuardLevel::Warn
    } else if looks_binary && !already_lfs && size >= SUGGEST_MIN_BYTES {
        GuardLevel::Suggest
    } else {
        GuardLevel::None
    }
}

/// Parses `git check-attr -z <attr>` output (`path\0attr\0value\0` triples) into
/// `path -> value`.
pub fn parse_check_attr(raw: &[u8]) -> HashMap<String, String> {
    let text = String::from_utf8_lossy(raw);
    let fields: Vec<&str> = text.split('\0').collect();
    let (triples, _) = fields.as_chunks::<3>();
    triples
        .iter()
        .map(|[path, _attr, value]| ((*path).to_owned(), (*value).to_owned()))
        .collect()
}

/// Parses `git cat-file --batch-check='%(objectname) %(objecttype) %(objectsize)'` output,
/// one entry per input line: `Some((oid, size))` for blobs, `None` for missing/other.
pub fn parse_batch_check(raw: &str) -> Vec<Option<(String, u64)>> {
    raw.lines()
        .map(|line| {
            let mut f = line.split(' ');
            match (f.next(), f.next(), f.next(), f.next()) {
                (Some(oid), Some("blob"), Some(size), None) => {
                    size.parse().ok().map(|s| (oid.to_owned(), s))
                }
                _ => None,
            }
        })
        .collect()
}

/// Parses `git cat-file --batch` output into `oid -> content`.
pub fn parse_batch(raw: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut out = HashMap::new();
    let mut rest = raw;
    while let Some(nl) = rest.iter().position(|&b| b == b'\n') {
        let header = String::from_utf8_lossy(&rest[..nl]).into_owned();
        rest = &rest[nl + 1..];
        let f: Vec<&str> = header.split(' ').collect();
        let [oid, _kind, size] = f[..] else {
            continue; // `<oid> missing`
        };
        let Ok(size) = size.parse::<usize>() else {
            break;
        };
        if rest.len() < size {
            break;
        }
        out.insert(oid.to_owned(), rest[..size].to_vec());
        rest = rest.get(size + 1..).unwrap_or_default(); // content is followed by LF
    }
    out
}

fn read_head(path: &Path) -> Option<Vec<u8>> {
    let mut buf = Vec::with_capacity(SNIFF_BYTES);
    std::fs::File::open(path)
        .ok()?
        .take(SNIFF_BYTES as u64)
        .read_to_end(&mut buf)
        .ok()?;
    Some(buf)
}

/// Runs the guard over `paths` (repository-relative, typically staged and new files).
pub fn check(
    git: &GitRunner,
    repo: &Path,
    paths: &[String],
    thresholds: GuardThresholds,
) -> Result<GuardReport, LfsError> {
    let ctx = AccountContext::default();
    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for p in paths {
        let p = normalize_path(p)?;
        if seen.insert(p.clone()) {
            unique.push(p);
        }
    }

    let mut lfs_attr = HashMap::new();
    let mut staged: HashMap<&str, (String, u64)> = HashMap::new();
    let mut pointers = HashMap::new();
    if !unique.is_empty() {
        // Attributes, batched over stdin.
        let mut input = Vec::new();
        for p in &unique {
            input.extend_from_slice(p.as_bytes());
            input.push(0);
        }
        let out = git.run_with_input(
            repo,
            &ctx,
            ["check-attr", "-z", "--stdin", "filter"],
            &input,
        )?;
        lfs_attr = parse_check_attr(&out.stdout);

        // Staged blobs. The batch protocol is line-based, so a path containing a newline is
        // treated as not staged.
        let lookup: Vec<&str> = unique
            .iter()
            .map(String::as_str)
            .filter(|p| !p.contains('\n'))
            .collect();
        let input: String = lookup.iter().map(|p| format!(":{p}\n")).collect();
        let out = git.run_with_input(
            repo,
            &ctx,
            [
                "cat-file",
                "--batch-check=%(objectname) %(objecttype) %(objectsize)",
            ],
            input.as_bytes(),
        )?;
        for (p, entry) in lookup
            .iter()
            .zip(parse_batch_check(&String::from_utf8_lossy(&out.stdout)))
        {
            if let Some(e) = entry {
                staged.insert(p, e);
            }
        }

        // Pointer check: only small blobs can be pointers.
        let small: Vec<&str> = staged
            .values()
            .filter(|(_, size)| *size < MAX_POINTER_BYTES)
            .map(|(oid, _)| oid.as_str())
            .collect();
        if !small.is_empty() {
            let input: String = small.iter().map(|o| format!("{o}\n")).collect();
            let out = git.run_with_input(repo, &ctx, ["cat-file", "--batch"], input.as_bytes())?;
            pointers = parse_batch(&out.stdout);
        }
    }

    let mut report = GuardReport {
        thresholds,
        findings: Vec::with_capacity(unique.len()),
        suggest_count: 0,
        warn_count: 0,
        block_count: 0,
    };
    for path in &unique {
        let disk = repo.join(path);
        let disk_size = std::fs::metadata(&disk)
            .ok()
            .filter(|m| m.is_file())
            .map(|m| m.len());
        let staged_entry = staged.get(path.as_str());
        let staged_size = staged_entry.map(|(_, s)| *s);
        let staged_pointer = staged_entry
            .and_then(|(oid, _)| pointers.get(oid))
            .is_some_and(|c| is_lfs_pointer(c));
        let already_lfs = lfs_attr.get(path).is_some_and(|v| v == "lfs");
        let looks_binary = has_binary_extension(path)
            || disk_size.is_some() && read_head(&disk).is_some_and(|h| sniff_binary(&h));
        let size = committed_size(staged_size, staged_pointer, disk_size, already_lfs);
        let level = classify(size, &report.thresholds, looks_binary, already_lfs);
        match level {
            GuardLevel::None => {}
            GuardLevel::Suggest => report.suggest_count += 1,
            GuardLevel::Warn => report.warn_count += 1,
            GuardLevel::Block => report.block_count += 1,
        }
        report.findings.push(GuardFinding {
            path: path.clone(),
            size,
            disk_size,
            staged_size,
            level,
            already_lfs,
            staged_pointer,
            needs_renormalize: already_lfs && !staged_pointer && staged_size.is_some_and(|s| s > 0),
            looks_binary,
            looks_like_build_output: looks_like_build_output(path),
            pattern_by_extension: pattern_by_extension(path),
            pattern_by_path: pattern_by_path(path),
        });
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_defaults() {
        let gh = GuardThresholds::for_provider(GuardProvider::Github);
        assert_eq!(gh.warn_bytes, 50 * MIB);
        assert_eq!(gh.block_bytes, Some(100 * MIB));
        let gl = GuardThresholds::for_provider(GuardProvider::Gitlab);
        assert_eq!((gl.warn_bytes, gl.block_bytes), (50 * MIB, None));
        assert_eq!(GuardThresholds::for_provider(GuardProvider::Other), gl);
    }

    #[test]
    fn classification() {
        let gh = GuardThresholds::GITHUB;
        let gl = GuardThresholds::GITLAB;
        assert_eq!(classify(None, &gh, true, false), GuardLevel::None);
        assert_eq!(classify(Some(10), &gh, false, false), GuardLevel::None);
        assert_eq!(
            classify(Some(50 * MIB), &gh, false, false),
            GuardLevel::None,
            "not above"
        );
        assert_eq!(
            classify(Some(50 * MIB + 1), &gh, false, false),
            GuardLevel::Warn
        );
        assert_eq!(
            classify(Some(100 * MIB + 1), &gh, false, false),
            GuardLevel::Block
        );
        assert_eq!(
            classify(Some(300 * MIB), &gl, false, false),
            GuardLevel::Warn,
            "GitLab never blocks"
        );
        assert_eq!(
            classify(Some(2 * MIB), &gh, true, false),
            GuardLevel::Suggest
        );
        assert_eq!(
            classify(Some(2 * MIB), &gh, true, true),
            GuardLevel::None,
            "already LFS"
        );
        assert_eq!(
            classify(Some(MIB - 1), &gh, true, false),
            GuardLevel::None,
            "tiny binary"
        );
        assert!(GuardLevel::Block > GuardLevel::Warn && GuardLevel::Warn > GuardLevel::Suggest);
    }

    #[test]
    fn committed_size_rules() {
        // Staged pointer: nothing heavy goes into the commit.
        assert_eq!(
            committed_size(Some(130), true, Some(900 * MIB), true),
            Some(0)
        );
        // Staged full content wins over the disk size, even if LFS-tracked now.
        assert_eq!(
            committed_size(Some(80 * MIB), false, Some(1), true),
            Some(80 * MIB)
        );
        // Not staged: LFS-tracked files become pointers, others go in whole.
        assert_eq!(committed_size(None, false, Some(80 * MIB), true), Some(0));
        assert_eq!(
            committed_size(None, false, Some(80 * MIB), false),
            Some(80 * MIB)
        );
        assert_eq!(committed_size(None, false, None, false), None, "deleted");
    }

    #[test]
    fn binary_detection() {
        for p in [
            "Design/home.PSD",
            "a/b.zip",
            "intro.mp4",
            "x.fbx",
            "pkg.unitypackage",
            "lib.dll",
        ] {
            assert!(has_binary_extension(p), "{p}");
        }
        for p in [
            "src/main.rs",
            "README",
            ".gitignore",
            "dir.psd/notes.txt",
            "file.",
        ] {
            assert!(!has_binary_extension(p), "{p}");
        }
        assert!(sniff_binary(b"abc\0def"));
        assert!(!sniff_binary("tiếng Việt\n".as_bytes()));
        let mut late = vec![b'a'; SNIFF_BYTES];
        late.push(0);
        assert!(!sniff_binary(&late), "NUL after the sniff window");
    }

    #[test]
    fn build_output_detection() {
        for p in [
            "build/app.zip",
            "web/dist/main.js",
            "Bin/Release/x.dll",
            "obj/a.o",
            "target/debug/x",
            "release.zip",
            "setup.EXE",
        ] {
            assert!(looks_like_build_output(p), "{p}");
        }
        for p in [
            "src/build.rs",
            "docs/output.md",
            "binary.txt",
            "dist",
            "a/targets/x",
        ] {
            assert!(!looks_like_build_output(p), "{p}");
        }
    }

    #[test]
    fn suggested_patterns() {
        assert_eq!(extension("Assets/intro.MP4"), Some("MP4"));
        assert_eq!(extension(".env"), None);
        assert_eq!(
            pattern_by_extension("Assets/intro.mp4").as_deref(),
            Some("*.mp4")
        );
        assert_eq!(pattern_by_extension("Makefile"), None);
        assert_eq!(pattern_by_path("Assets/intro.mp4"), "/Assets/intro.mp4");
        assert_eq!(pattern_by_path("big.bin"), "/big.bin");
        assert_eq!(
            pattern_by_path("art/[wip] a*b?.psd"),
            "/art/[[]wip] a[*]b[?].psd"
        );
    }

    #[test]
    fn pointer_detection() {
        let ptr = b"version https://git-lfs.github.com/spec/v1\noid sha256:4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393\nsize 12345\n";
        assert!(is_lfs_pointer(ptr));
        assert!(!is_lfs_pointer(
            b"version https://git-lfs.github.com/spec/v1\n"
        ));
        assert!(!is_lfs_pointer(b"hello"));
    }

    #[test]
    fn parses_git_outputs() {
        let attrs = parse_check_attr(b"a.bin\0filter\0lfs\0b c.txt\0filter\0unspecified\0");
        assert_eq!(attrs["a.bin"], "lfs");
        assert_eq!(attrs["b c.txt"], "unspecified");

        let checks = parse_batch_check("abc blob 42\n:nope missing\ndef tree 10\n:x y missing\n");
        assert_eq!(checks, vec![Some(("abc".into(), 42)), None, None, None]);

        let batch = parse_batch(b"aa blob 3\nxyz\nbb missing\ncc blob 0\n\ndd blob 2\n\n\n\n");
        assert_eq!(batch["aa"], b"xyz");
        assert_eq!(batch["cc"], b"");
        assert_eq!(batch["dd"], b"\n\n");
        assert!(!batch.contains_key("bb"));
    }
}
