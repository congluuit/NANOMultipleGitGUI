//! Raw file contents for previews: blobs of a commit / the index / a conflict stage or a working
//! tree file, with Git LFS pointers resolved to the real object from the local LFS store, sent
//! to the UI as base64 (images are shown with `<img src="data:…">`, never parsed by us).

use std::io::Read;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::commit::is_valid_oid;
use crate::{AccountContext, GitError, GitRunner};

/// Largest content sent to the UI as a preview (base64 grows it by a third).
pub const MAX_PREVIEW_BYTES: u64 = 16 * 1024 * 1024;
/// LFS pointer files are tiny; anything bigger is real content.
const MAX_POINTER_BYTES: u64 = 1024;
const LFS_VERSION_LINE: &[u8] = b"version https://git-lfs.github.com/spec/v1\n";

/// Where a file's bytes come from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BlobSource {
    /// The file as of a commit.
    Commit { oid: String },
    /// The index: stage 0 normally, 1 (base) / 2 (ours) / 3 (theirs) during a conflict.
    Index { stage: u8 },
    /// The file on disk.
    Worktree,
}

/// A Git LFS pointer (`oid sha256:…`, `size …`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LfsPointer {
    /// SHA-256 of the real content (64 hex digits).
    pub oid: String,
    /// Size of the real content in bytes.
    pub size: u64,
}

/// Preview of one side of a file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct BlobPreview {
    /// Size of the real content (of the LFS object for a pointer).
    pub size: u64,
    /// MIME type guessed from the file extension.
    pub mime: String,
    /// Base64 content; `None` when too large or the LFS object is not downloaded.
    pub data: Option<String>,
    /// Set when the stored file is an LFS pointer.
    pub lfs: Option<LfsPointer>,
    /// LFS pointer whose object is not in the local LFS store (needs a download).
    pub lfs_missing: bool,
    pub too_large: bool,
    /// Contains NUL bytes in its first 8000 bytes (git's heuristic).
    pub binary: bool,
}

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Image file extensions the UI can preview.
const IMAGE_TYPES: [(&str, &str); 9] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("bmp", "image/bmp"),
    ("ico", "image/x-icon"),
    ("svg", "image/svg+xml"),
    ("avif", "image/avif"),
];

/// MIME type of an image path, `None` for anything else.
pub fn image_mime(path: &str) -> Option<&'static str> {
    let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
    IMAGE_TYPES.iter().find(|(e, _)| *e == ext).map(|(_, m)| *m)
}

pub fn is_image(path: &str) -> bool {
    image_mime(path).is_some()
}

/// Parses a Git LFS pointer file.
pub fn parse_lfs_pointer(bytes: &[u8]) -> Option<LfsPointer> {
    if bytes.len() as u64 >= MAX_POINTER_BYTES || !bytes.starts_with(LFS_VERSION_LINE) {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let mut oid = None;
    let mut size = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("oid sha256:") {
            let v = v.trim();
            if v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit()) {
                oid = Some(v.to_ascii_lowercase());
            }
        } else if let Some(v) = line.strip_prefix("size ") {
            size = v.trim().parse().ok();
        }
    }
    Some(LfsPointer {
        oid: oid?,
        size: size?,
    })
}

pub(crate) fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(8000)].contains(&0)
}

pub(crate) fn invalid(what: &str) -> GitError {
    GitError::Failed {
        args: "blob".into(),
        status: "rejected".into(),
        stderr: what.to_owned(),
    }
}

/// Rejects paths that could be read as options or escape the repository.
pub fn check_path(path: &str) -> Result<(), GitError> {
    let bad = path.is_empty()
        || path.starts_with('-')
        || path.contains('\0')
        || path.chars().any(|c| c == '\n' || c == '\r')
        || Path::new(path).is_absolute()
        || path.split(['/', '\\']).any(|c| c == "..");
    if bad {
        Err(invalid(&format!("invalid path: {path:?}")))
    } else {
        Ok(())
    }
}

/// `<rev>:<path>` / `:<n>:<path>` object name for a source, or `None` for the working tree.
fn object_name(source: &BlobSource, path: &str) -> Result<Option<String>, GitError> {
    check_path(path)?;
    Ok(match source {
        BlobSource::Commit { oid } => {
            if !is_valid_oid(oid) {
                return Err(invalid(&format!("invalid commit id: {oid:?}")));
            }
            Some(format!("{oid}:{path}"))
        }
        BlobSource::Index { stage } => {
            if *stage > 3 {
                return Err(invalid("invalid index stage"));
            }
            Some(format!(":{stage}:{path}"))
        }
        BlobSource::Worktree => None,
    })
}

/// Size of an object (`git cat-file -s`), `None` if it does not exist.
pub(crate) fn object_size(git: &GitRunner, root: &Path, name: &str) -> Option<u64> {
    git.run_text(root, &AccountContext::default(), ["cat-file", "-s", name])
        .ok()?
        .parse()
        .ok()
}

/// Bytes of an object, `None` if it does not exist.
pub(crate) fn object_bytes(git: &GitRunner, root: &Path, name: &str) -> Option<Vec<u8>> {
    git.run(root, &AccountContext::default(), ["cat-file", "blob", name])
        .ok()
        .map(|o| o.stdout)
}

/// Location of the local LFS object store (`lfs.storage`, default `<common git dir>/lfs`).
fn lfs_store(git: &GitRunner, root: &Path) -> Option<PathBuf> {
    let ctx = AccountContext::default();
    let common = git
        .run_text(
            root,
            &ctx,
            ["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )
        .ok()?;
    let common = PathBuf::from(common);
    match git.run_text(root, &ctx, ["config", "--get", "lfs.storage"]) {
        Ok(s) if !s.is_empty() => {
            let p = PathBuf::from(s);
            Some(if p.is_absolute() { p } else { common.join(p) })
        }
        _ => Some(common.join("lfs")),
    }
}

/// Path of an LFS object in the local store if it is there with the expected size.
pub fn lfs_object_path(git: &GitRunner, root: &Path, pointer: &LfsPointer) -> Option<PathBuf> {
    let o = &pointer.oid;
    let path = lfs_store(git, root)?
        .join("objects")
        .join(&o[0..2])
        .join(&o[2..4])
        .join(o);
    let meta = std::fs::metadata(&path).ok()?;
    (meta.is_file() && meta.len() == pointer.size).then_some(path)
}

fn read_limited(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(limit + 1)
        .read_to_end(&mut buf)
        .ok()?;
    Some(buf)
}

/// Builds a preview from stored bytes (resolving an LFS pointer through the local store).
pub(crate) fn preview_of(git: &GitRunner, root: &Path, path: &str, bytes: Vec<u8>) -> BlobPreview {
    let mime = image_mime(path)
        .unwrap_or("application/octet-stream")
        .to_owned();
    if let Some(pointer) = parse_lfs_pointer(&bytes) {
        let mut out = BlobPreview {
            size: pointer.size,
            mime,
            lfs: Some(pointer.clone()),
            ..Default::default()
        };
        if pointer.size > MAX_PREVIEW_BYTES {
            out.too_large = true;
        } else if let Some(content) =
            lfs_object_path(git, root, &pointer).and_then(|p| read_limited(&p, MAX_PREVIEW_BYTES))
        {
            out.binary = looks_binary(&content);
            out.data = Some(base64(&content));
        } else {
            out.lfs_missing = true;
        }
        return out;
    }
    let size = bytes.len() as u64;
    BlobPreview {
        size,
        mime,
        binary: looks_binary(&bytes),
        too_large: size > MAX_PREVIEW_BYTES,
        data: (size <= MAX_PREVIEW_BYTES).then(|| base64(&bytes)),
        ..Default::default()
    }
}

/// Size-only preview for content too big to load.
fn too_large(path: &str, size: u64) -> BlobPreview {
    BlobPreview {
        size,
        mime: image_mime(path)
            .unwrap_or("application/octet-stream")
            .to_owned(),
        too_large: true,
        ..Default::default()
    }
}

/// Preview of `path` as stored in an object (`<rev>:<path>`), `None` if it does not exist.
pub(crate) fn preview_object(
    git: &GitRunner,
    root: &Path,
    name: &str,
    path: &str,
) -> Option<BlobPreview> {
    let size = object_size(git, root, name)?;
    if size > MAX_PREVIEW_BYTES {
        return Some(too_large(path, size));
    }
    Some(preview_of(git, root, path, object_bytes(git, root, name)?))
}

/// Preview of a working tree file (may be an un-smudged LFS pointer), `None` if missing.
pub(crate) fn preview_worktree(git: &GitRunner, root: &Path, path: &str) -> Option<BlobPreview> {
    let full = root.join(path);
    let meta = std::fs::metadata(&full).ok().filter(|m| m.is_file())?;
    if meta.len() > MAX_PREVIEW_BYTES {
        return Some(too_large(path, meta.len()));
    }
    Some(preview_of(git, root, path, std::fs::read(full).ok()?))
}

/// Preview of `path` from `source`; `None` when the file does not exist there.
pub fn blob_preview(
    git: &GitRunner,
    root: &Path,
    source: &BlobSource,
    path: &str,
) -> Result<Option<BlobPreview>, GitError> {
    Ok(match object_name(source, path)? {
        Some(name) => preview_object(git, root, &name, path),
        None => preview_worktree(git, root, path),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::Identity;

    #[test]
    fn base64_matches_rfc4648_vectors() {
        for (input, want) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(input.as_bytes()), want);
        }
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }

    #[test]
    fn parses_lfs_pointers_and_image_types() {
        let oid = "a".repeat(64);
        let ptr =
            format!("version https://git-lfs.github.com/spec/v1\noid sha256:{oid}\nsize 1234\n");
        assert_eq!(
            parse_lfs_pointer(ptr.as_bytes()),
            Some(LfsPointer {
                oid: oid.clone(),
                size: 1234
            })
        );
        assert!(parse_lfs_pointer(b"hello").is_none());
        assert!(
            parse_lfs_pointer(b"version https://git-lfs.github.com/spec/v1\nsize 1\n").is_none()
        );
        assert_eq!(image_mime("a/b.PNG"), Some("image/png"));
        assert_eq!(image_mime("x.svg"), Some("image/svg+xml"));
        assert_eq!(image_mime("x.txt"), None);
        assert_eq!(image_mime("noext"), None);
    }

    #[test]
    fn rejects_bad_sources() {
        let git = GitRunner::default();
        let dir = tempfile::tempdir().unwrap();
        let bad = BlobSource::Commit {
            oid: "--output=x".into(),
        };
        assert!(blob_preview(&git, dir.path(), &bad, "a.png").is_err());
        assert!(blob_preview(&git, dir.path(), &BlobSource::Index { stage: 9 }, "a").is_err());
        assert!(blob_preview(&git, dir.path(), &BlobSource::Worktree, "../x").is_err());
        assert!(blob_preview(&git, dir.path(), &BlobSource::Worktree, "-x").is_err());
    }

    pub(crate) struct Repo {
        pub dir: tempfile::TempDir,
        pub git: GitRunner,
        pub ctx: AccountContext,
    }

    impl Repo {
        pub fn new() -> Self {
            let r = Self {
                dir: tempfile::tempdir().unwrap(),
                git: GitRunner::default().with_env("LC_ALL", "C"),
                ctx: AccountContext {
                    identity: Some(Identity {
                        name: "T".into(),
                        email: "t@x.dev".into(),
                    }),
                    ..Default::default()
                },
            };
            r.run(&["init", "-q", "-b", "main"]);
            r.run(&["config", "core.autocrlf", "false"]);
            r.run(&["config", "commit.gpgsign", "false"]);
            r
        }
        pub fn p(&self) -> &Path {
            self.dir.path()
        }
        pub fn run(&self, args: &[&str]) -> String {
            self.git.run_text(self.p(), &self.ctx, args).unwrap()
        }
        pub fn try_run(&self, args: &[&str]) -> Result<String, GitError> {
            self.git.run_text(self.p(), &self.ctx, args)
        }
        pub fn write(&self, name: &str, data: impl AsRef<[u8]>) {
            let p = self.p().join(name);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(p, data).unwrap();
        }
        pub fn read(&self, name: &str) -> Vec<u8> {
            std::fs::read(self.p().join(name)).unwrap()
        }
        pub fn commit_all(&self, msg: &str) -> String {
            self.run(&["add", "-A"]);
            self.run(&["commit", "-q", "-m", msg]);
            self.run(&["rev-parse", "HEAD"])
        }
    }

    #[test]
    fn previews_commit_index_worktree_and_lfs_objects() {
        let r = Repo::new();
        let png = [0x89u8, b'P', b'N', b'G', 0, 1, 2, 3];
        r.write("img/a.png", png);
        let c = r.commit_all("png");
        let p = blob_preview(
            &r.git,
            r.p(),
            &BlobSource::Commit { oid: c.clone() },
            "img/a.png",
        )
        .unwrap()
        .unwrap();
        assert_eq!(p.size, 8);
        assert_eq!(p.mime, "image/png");
        assert!(p.binary && p.lfs.is_none());
        assert_eq!(p.data.as_deref(), Some(base64(&png).as_str()));
        assert!(
            blob_preview(&r.git, r.p(), &BlobSource::Commit { oid: c }, "nope.png")
                .unwrap()
                .is_none()
        );
        assert!(
            blob_preview(&r.git, r.p(), &BlobSource::Index { stage: 0 }, "img/a.png")
                .unwrap()
                .is_some()
        );

        // A pointer whose object is in the local store resolves to the real content; an unknown
        // one is reported as missing.
        let content = b"real image bytes";
        let oid = "0123456789abcdef".repeat(4);
        let store = r
            .p()
            .join(".git/lfs/objects")
            .join(&oid[0..2])
            .join(&oid[2..4]);
        std::fs::create_dir_all(&store).unwrap();
        std::fs::write(store.join(&oid), content).unwrap();
        r.write(
            "b.png",
            format!(
                "version https://git-lfs.github.com/spec/v1\noid sha256:{oid}\nsize {}\n",
                content.len()
            ),
        );
        let p = blob_preview(&r.git, r.p(), &BlobSource::Worktree, "b.png")
            .unwrap()
            .unwrap();
        assert_eq!(p.lfs.as_ref().map(|l| l.size), Some(content.len() as u64));
        assert!(!p.lfs_missing);
        assert_eq!(p.data.as_deref(), Some(base64(content).as_str()));
        r.write(
            "c.png",
            format!(
                "version https://git-lfs.github.com/spec/v1\noid sha256:{}\nsize 10\n",
                "f".repeat(64)
            ),
        );
        let p = blob_preview(&r.git, r.p(), &BlobSource::Worktree, "c.png")
            .unwrap()
            .unwrap();
        assert!(p.lfs_missing && p.data.is_none());
        assert_eq!(p.size, 10);
    }
}
