//! The built-in editor's access to the working tree: reading and writing files, listing and
//! searching the repository's files, creating / renaming / deleting files and folders, and
//! staging an edited version of a file straight into the index (stage one block).
//!
//! Files are only offered for editing when saving them back cannot lose anything: text in an
//! encoding the editor writes back byte for byte (UTF-8 with or without a BOM, UTF-16 with a
//! BOM, Windows-1252), not binary, not too large, and not a symlink (writing would follow it,
//! possibly out of the repository). Every read returns a `version` of the bytes on disk; a save
//! carries the version it was based on and is refused when the file changed in the meantime.

use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};

use crate::diff::check_path;
use crate::{AccountContext, GitError, GitRunner, blob};

/// Same limit as the diff editor: bigger files are not sent to the frontend.
const MAX_EDIT_BYTES: usize = 5 * 1024 * 1024;
const BINARY_SNIFF: usize = 8000;
/// Version of a file that does not exist.
const MISSING: &str = "missing";

/// How the bytes of a text file map to characters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum TextEncoding {
    #[default]
    Utf8,
    /// UTF-8 starting with the byte order mark `EF BB BF` (common in Unity / Visual Studio).
    Utf8Bom,
    /// UTF-16 little endian, with its byte order mark `FF FE`.
    Utf16Le,
    /// UTF-16 big endian, with its byte order mark `FE FF`.
    Utf16Be,
    /// Windows-1252 (Western European): every byte is one character, so any file decodes.
    Windows1252,
}

/// The line breaks of a text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum LineEnding {
    /// No line break at all.
    #[default]
    None,
    Lf,
    Crlf,
    /// More than one kind (LF and CRLF, or a lone CR).
    Mixed,
}

/// A working-tree file as the editor needs it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct WorktreeFile {
    /// Contents without the byte order mark, `None` unless the file exists and is editable
    /// text. Line breaks are as on disk.
    pub text: Option<String>,
    /// Encoding of `text` on disk (saving writes it back the same way).
    pub encoding: Option<TextEncoding>,
    /// Line breaks of `text`.
    pub eol: Option<LineEnding>,
    pub exists: bool,
    pub binary: bool,
    pub too_large: bool,
    /// Not text in the encoding asked for (or a broken UTF-16 file): shown read-only so saving
    /// cannot corrupt it. `encoding` is then the encoding asked for, if any.
    pub undecodable: bool,
    pub symlink: bool,
    /// Identifies the bytes on disk; passed back to [`write_file`].
    pub version: String,
}

/// A character the chosen encoding has no code for (1-based line, UTF-16 column).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Unencodable {
    pub character: String,
    pub line: u32,
    pub column: u32,
}

/// Result of [`write_file`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SaveOutcome {
    /// `false` when the file changed on disk since `expected` was read, or when the text has a
    /// character the encoding cannot hold; nothing was written.
    pub saved: bool,
    /// Version of the file on disk now.
    pub version: String,
    /// The first character the encoding cannot hold, when that is why nothing was written.
    pub unencodable: Option<Unencodable>,
}

fn version(bytes: &[u8]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}

fn invalid(what: &str) -> GitError {
    GitError::Failed {
        args: "edit".into(),
        status: "rejected".into(),
        stderr: what.to_owned(),
    }
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

/// `root/path`, refusing paths that leave the repository through `..` or a symlinked folder,
/// and anything inside `.git`.
fn resolve(root: &Path, path: &str) -> Result<PathBuf, GitError> {
    check_path(path)?;
    if path
        .split(['/', '\\'])
        .any(|c| c.eq_ignore_ascii_case(".git") || c.is_empty() || c == ".")
    {
        return Err(invalid(&format!("invalid path: {path:?}")));
    }
    let full = root.join(path);
    // The closest folder that exists must still be inside the repository.
    let mut existing = full.parent();
    while let Some(dir) = existing
        && !dir.exists()
    {
        existing = dir.parent();
    }
    if let Some(dir) = existing
        && let (Ok(dir), Ok(root)) = (dir.canonicalize(), root.canonicalize())
        && !dir.starts_with(&root)
    {
        return Err(invalid(&format!("{path} is outside the repository")));
    }
    Ok(full)
}

// ---------------------------------------------------------------------------------------------
// Encodings and line breaks.

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: &[u8] = &[0xFF, 0xFE];
const UTF16BE_BOM: &[u8] = &[0xFE, 0xFF];

/// Windows-1252 bytes `0x80..=0x9F` (WHATWG mapping). The five bytes Windows leaves undefined
/// map to the C1 control of the same value, so every byte decodes and encodes back unchanged.
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

/// Why bytes are not offered as editable text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotText {
    Binary,
    Undecodable,
}

fn decode_cp1252(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => CP1252_HIGH[usize::from(b - 0x80)],
            _ => char::from(b),
        })
        .collect()
}

fn encode_cp1252(c: char) -> Option<u8> {
    match u32::from(c) {
        n @ (0..=0x7F | 0xA0..=0xFF) => u8::try_from(n).ok(),
        _ => CP1252_HIGH
            .iter()
            .position(|&m| m == c)
            .and_then(|i| u8::try_from(0x80 + i).ok()),
    }
}

fn has_nul(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_SNIFF)].contains(&0)
}

/// Strict UTF-16: an odd number of bytes or an unpaired surrogate could not be written back.
fn decode_utf16(bytes: &[u8], little_endian: bool) -> Result<String, NotText> {
    let (pairs, rest) = bytes.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(NotText::Undecodable);
    }
    let units = pairs.iter().map(|&p| {
        if little_endian {
            u16::from_le_bytes(p)
        } else {
            u16::from_be_bytes(p)
        }
    });
    let text: String = char::decode_utf16(units)
        .collect::<Result<_, _>>()
        .map_err(|_| NotText::Undecodable)?;
    // NUL characters: a binary file that happens to start like UTF-16.
    if text.chars().take(BINARY_SNIFF).any(|c| c == '\0') {
        return Err(NotText::Binary);
    }
    Ok(text)
}

fn decode_utf8(body: &[u8], encoding: TextEncoding) -> Result<(TextEncoding, String), NotText> {
    if has_nul(body) {
        return Err(NotText::Binary);
    }
    std::str::from_utf8(body)
        .map(|s| (encoding, s.to_owned()))
        .map_err(|_| NotText::Undecodable)
}

/// Decodes a file's bytes, guessing the encoding when `wanted` is `None`: a byte order mark
/// first, then UTF-8, and Windows-1252 for anything else. The text never contains the BOM.
fn decode(bytes: &[u8], wanted: Option<TextEncoding>) -> Result<(TextEncoding, String), NotText> {
    use TextEncoding as E;
    match wanted {
        None => {
            if let Some(rest) = bytes.strip_prefix(UTF16LE_BOM) {
                return decode_utf16(rest, true).map(|t| (E::Utf16Le, t));
            }
            if let Some(rest) = bytes.strip_prefix(UTF16BE_BOM) {
                return decode_utf16(rest, false).map(|t| (E::Utf16Be, t));
            }
            if has_nul(bytes) {
                return Err(NotText::Binary);
            }
            if let Some(rest) = bytes.strip_prefix(UTF8_BOM)
                && let Ok(text) = std::str::from_utf8(rest)
            {
                return Ok((E::Utf8Bom, text.to_owned()));
            }
            match std::str::from_utf8(bytes) {
                Ok(text) => Ok((E::Utf8, text.to_owned())),
                Err(_) => Ok((E::Windows1252, decode_cp1252(bytes))),
            }
        }
        // Read as UTF-8: a BOM, when there is one, is written back.
        Some(E::Utf8 | E::Utf8Bom) => match bytes.strip_prefix(UTF8_BOM) {
            Some(rest) => decode_utf8(rest, E::Utf8Bom),
            None => decode_utf8(bytes, E::Utf8),
        },
        // UTF-16 without its BOM is not offered: saving would add one.
        Some(E::Utf16Le) => match bytes.strip_prefix(UTF16LE_BOM) {
            Some(rest) => decode_utf16(rest, true).map(|t| (E::Utf16Le, t)),
            None if has_nul(bytes) => Err(NotText::Binary),
            None => Err(NotText::Undecodable),
        },
        Some(E::Utf16Be) => match bytes.strip_prefix(UTF16BE_BOM) {
            Some(rest) => decode_utf16(rest, false).map(|t| (E::Utf16Be, t)),
            None if has_nul(bytes) => Err(NotText::Binary),
            None => Err(NotText::Undecodable),
        },
        Some(E::Windows1252) if has_nul(bytes) => Err(NotText::Binary),
        Some(E::Windows1252) => Ok((E::Windows1252, decode_cp1252(bytes))),
    }
}

/// Where in `text` the character `c` (at byte `at`) is.
fn unencodable_at(text: &str, at: usize, c: char) -> Unencodable {
    let before = &text[..at];
    let line_start = before.rfind(['\n', '\r']).map_or(0, |i| i + 1);
    let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    Unencodable {
        character: c.to_string(),
        line: to_u32(before.matches('\n').count() + 1),
        column: to_u32(before[line_start..].encode_utf16().count() + 1),
    }
}

/// The bytes of `text` in `encoding`, with its BOM. `Err` names the first character the
/// encoding has no code for.
fn encode(text: &str, encoding: TextEncoding) -> Result<Vec<u8>, Unencodable> {
    use TextEncoding as E;
    let mut out = Vec::with_capacity(text.len() + 3);
    match encoding {
        E::Utf8 => out.extend_from_slice(text.as_bytes()),
        E::Utf8Bom => {
            out.extend_from_slice(UTF8_BOM);
            out.extend_from_slice(text.as_bytes());
        }
        E::Utf16Le => {
            out.extend_from_slice(UTF16LE_BOM);
            out.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        }
        E::Utf16Be => {
            out.extend_from_slice(UTF16BE_BOM);
            out.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        }
        E::Windows1252 => {
            for (at, c) in text.char_indices() {
                out.push(encode_cp1252(c).ok_or_else(|| unencodable_at(text, at, c))?);
            }
        }
    }
    Ok(out)
}

/// The kind of line breaks in `text`.
pub fn detect_eol(text: &str) -> LineEnding {
    let (mut lf, mut crlf, mut cr) = (false, false, false);
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\n' if i > 0 && bytes[i - 1] == b'\r' => crlf = true,
            b'\n' => lf = true,
            b'\r' if bytes.get(i + 1) != Some(&b'\n') => cr = true,
            _ => {}
        }
    }
    match (lf, crlf, cr) {
        (false, false, false) => LineEnding::None,
        (true, false, false) => LineEnding::Lf,
        (false, true, false) => LineEnding::Crlf,
        _ => LineEnding::Mixed,
    }
}

/// `text` with every line break (LF, CRLF or a lone CR) turned into `eol`. `None` and `Mixed`
/// leave the text as it is.
pub fn convert_eol(text: &str, eol: LineEnding) -> std::borrow::Cow<'_, str> {
    let to = match eol {
        LineEnding::Lf => "\n",
        LineEnding::Crlf => "\r\n",
        LineEnding::None | LineEnding::Mixed => return text.into(),
    };
    let found = detect_eol(text);
    if found == eol || found == LineEnding::None {
        return text.into();
    }
    let mut out = String::with_capacity(text.len() + text.len() / 16);
    let mut rest = text;
    while let Some(i) = rest.find(['\r', '\n']) {
        out.push_str(&rest[..i]);
        out.push_str(to);
        let skip = if rest[i..].starts_with("\r\n") { 2 } else { 1 };
        rest = &rest[i + skip..];
    }
    out.push_str(rest);
    out.into()
}

/// Reads `path` (relative to the repository root) for the editor. With `encoding` the file is
/// read in that encoding instead of a detected one ("reopen with encoding").
pub fn read_file(
    root: &Path,
    path: &str,
    encoding: Option<TextEncoding>,
) -> Result<WorktreeFile, GitError> {
    let full = resolve(root, path)?;
    let mut out = WorktreeFile {
        version: MISSING.into(),
        ..WorktreeFile::default()
    };
    let meta = match std::fs::symlink_metadata(&full) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.into()),
    };
    out.exists = true;
    if meta.file_type().is_symlink() {
        out.symlink = true;
        return Ok(out);
    }
    if meta.is_dir() {
        return Err(invalid(&format!("{path} is a folder")));
    }
    if meta.len() > MAX_EDIT_BYTES as u64 {
        out.too_large = true;
        return Ok(out);
    }
    let bytes = std::fs::read(&full)?;
    out.version = version(&bytes);
    match decode(&bytes, encoding) {
        Ok((enc, text)) => {
            out.eol = Some(detect_eol(&text));
            out.encoding = Some(enc);
            out.text = Some(text);
        }
        Err(NotText::Binary) => out.binary = true,
        Err(NotText::Undecodable) => {
            out.undecodable = true;
            out.encoding = encoding;
        }
    }
    Ok(out)
}

/// The staged copy of `path` read in `encoding`, so the editor can mark the changes of a file
/// that is not UTF-8. `None` when it is not in the index or not text in that encoding.
pub fn read_index_text(
    git: &GitRunner,
    root: &Path,
    path: &str,
    encoding: TextEncoding,
) -> Result<Option<String>, GitError> {
    check_path(path)?;
    let listed = git.run_text(
        root,
        &AccountContext::default(),
        ["ls-files", "-s", "--", path],
    )?;
    if listed.trim().is_empty() {
        return Ok(None);
    }
    let Some(bytes) = blob::object_bytes(git, root, &format!(":{path}")) else {
        return Ok(None);
    };
    Ok(decode(&bytes, Some(encoding)).ok().map(|(_, text)| text))
}

fn disk_version(full: &Path) -> Result<String, GitError> {
    match std::fs::read(full) {
        Ok(bytes) => Ok(version(&bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(MISSING.into()),
        Err(e) => Err(e.into()),
    }
}

/// Writes `text` to `path` in `encoding` (with its BOM), every line break turned into `eol`
/// (`Mixed` and `None` keep them as they are). With `expected`, nothing is written when the
/// file on disk is no longer that version (changed by another program); the outcome then
/// reports `saved: false`, as it does when the text has a character the encoding cannot hold.
pub fn write_file(
    root: &Path,
    path: &str,
    text: &str,
    encoding: TextEncoding,
    eol: LineEnding,
    expected: Option<&str>,
) -> Result<SaveOutcome, GitError> {
    let full = resolve(root, path)?;
    if is_symlink(&full) {
        return Err(invalid(&format!("{path} is a symlink")));
    }
    if let Some(expected) = expected {
        let current = disk_version(&full)?;
        if current != expected {
            return Ok(SaveOutcome {
                saved: false,
                version: current,
                unencodable: None,
            });
        }
    }
    let bytes = match encode(&convert_eol(text, eol), encoding) {
        Ok(bytes) => bytes,
        Err(e) => {
            return Ok(SaveOutcome {
                saved: false,
                version: disk_version(&full)?,
                unencodable: Some(e),
            });
        }
    };
    std::fs::write(&full, &bytes)?;
    Ok(SaveOutcome {
        saved: true,
        version: version(&bytes),
        unencodable: None,
    })
}

// ---------------------------------------------------------------------------------------------
// Listing and searching the repository's files.

/// Files listed for quick open and the file tree are capped (huge monorepos).
const MAX_LIST: usize = 200_000;

/// Every file of the working tree that is not ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RepoFiles {
    /// Tracked files present on disk, sorted.
    pub tracked: Vec<String>,
    /// Untracked files that are not ignored, sorted.
    pub untracked: Vec<String>,
    /// Folders with no listed file inside (empty folders, nested repositories), sorted.
    pub folders: Vec<String>,
    pub truncated: bool,
}

fn ls_files(git: &GitRunner, root: &Path, args: &[&str]) -> Result<Vec<String>, GitError> {
    let mut all = vec!["ls-files", "-z"];
    all.extend_from_slice(args);
    let out = git.run(root, &AccountContext::default(), &all)?;
    let mut paths: Vec<String> = out
        .stdout
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub fn list_files(git: &GitRunner, root: &Path) -> Result<RepoFiles, GitError> {
    let deleted: HashSet<String> = ls_files(git, root, &["--deleted"])?.into_iter().collect();
    let mut tracked: Vec<String> = ls_files(git, root, &["--cached"])?
        .into_iter()
        .filter(|p| !deleted.contains(p))
        .collect();
    // `--others` lists a nested repository as `dir/`: a folder, not a file.
    let (mut folders, mut untracked): (Vec<String>, Vec<String>) =
        ls_files(git, root, &["--others", "--exclude-standard"])?
            .into_iter()
            .partition(|p| p.ends_with('/'));
    // `--directory` collapses untracked folders to `dir/`, which also reveals empty ones.
    for dir in ls_files(
        git,
        root,
        &["--others", "--exclude-standard", "--directory"],
    )? {
        let Some(name) = dir.strip_suffix('/') else {
            continue;
        };
        let at = untracked.partition_point(|u| u.as_str() < dir.as_str());
        let has_files = untracked.get(at).is_some_and(|u| u.starts_with(&dir));
        if !has_files && !folders.contains(&dir) {
            folders.push(name.to_owned());
        }
    }
    for f in &mut folders {
        if let Some(name) = f.strip_suffix('/') {
            *f = name.to_owned();
        }
    }
    folders.sort();
    folders.dedup();
    let truncated = tracked.len() + untracked.len() > MAX_LIST;
    tracked.truncate(MAX_LIST);
    untracked.truncate(MAX_LIST - tracked.len());
    Ok(RepoFiles {
        tracked,
        untracked,
        folders,
        truncated,
    })
}

/// How [`search`] matches the query.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    pub case_sensitive: bool,
    pub whole_word: bool,
    /// Extended regular expression instead of plain text.
    pub regex: bool,
    /// Also search untracked files (never ignored ones).
    pub untracked: bool,
}

/// One matching line.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SearchMatch {
    pub path: String,
    /// 1-based line number.
    pub line: u32,
    /// 1-based column of the first match, in UTF-16 units (what the editor counts).
    pub column: u32,
    /// The line, or the part of a long line around the match.
    pub text: String,
    /// Column (0-based, UTF-16) of `text`'s first character in the line.
    pub text_start: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub matches: Vec<SearchMatch>,
    /// More lines matched than `limit`.
    pub truncated: bool,
}

/// Characters of a long line kept before / after the match.
const PREVIEW_BEFORE: usize = 60;
const PREVIEW_LEN: usize = 240;

/// Parses one `git grep -z -n --column` line: `path\0line\0column\0text`.
fn parse_grep_line(raw: &[u8]) -> Option<SearchMatch> {
    let raw = raw.strip_suffix(b"\n").unwrap_or(raw);
    let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
    let mut parts = raw.splitn(4, |&b| b == 0);
    let path = String::from_utf8_lossy(parts.next()?).into_owned();
    let line: u32 = std::str::from_utf8(parts.next()?).ok()?.parse().ok()?;
    let byte_col: usize = std::str::from_utf8(parts.next()?).ok()?.parse().ok()?;
    let content = parts.next()?;
    let before = &content[..byte_col.saturating_sub(1).min(content.len())];
    let before_chars: Vec<char> = String::from_utf8_lossy(before).chars().collect();
    let text: Vec<char> = String::from_utf8_lossy(content).chars().collect();
    let start = before_chars.len().saturating_sub(PREVIEW_BEFORE);
    let end = (start + PREVIEW_LEN).min(text.len());
    let utf16 = |chars: &[char]| chars.iter().map(|c| c.len_utf16()).sum::<usize>() as u32;
    Some(SearchMatch {
        path,
        line,
        column: utf16(&before_chars) + 1,
        text: text[start.min(end)..end].iter().collect(),
        text_start: utf16(&text[..start.min(text.len())]),
    })
}

/// `git grep` over the working tree (tracked files, and untracked ones on request), stopping
/// after `limit` matching lines.
pub fn search(
    git: &GitRunner,
    root: &Path,
    query: &str,
    opts: SearchOptions,
    limit: usize,
) -> Result<SearchResults, GitError> {
    Ok(search_stoppable(git, root, query, opts, limit, &|| false)?.results)
}

/// How often a running search checks whether it should stop.
const STOP_POLL: std::time::Duration = std::time::Duration::from_millis(15);

/// Result of [`search_stoppable`].
#[derive(Debug, Default)]
pub(crate) struct SearchRun {
    pub results: SearchResults,
    /// `should_stop` answered true: git was killed, `results` holds what was found until then.
    pub stopped: bool,
}

/// [`search`] that kills `git grep` within a few milliseconds once `should_stop` returns true,
/// even while git is still looking and prints nothing.
pub(crate) fn search_stoppable(
    git: &GitRunner,
    root: &Path,
    query: &str,
    opts: SearchOptions,
    limit: usize,
    should_stop: &dyn Fn() -> bool,
) -> Result<SearchRun, GitError> {
    let mut out = SearchRun::default();
    if query.is_empty() {
        return Ok(out);
    }
    if should_stop() {
        out.stopped = true;
        return Ok(out);
    }
    let mut args = vec!["grep", "-z", "-n", "--column", "-I", "--no-color"];
    if !opts.case_sensitive {
        args.push("-i");
    }
    if opts.whole_word {
        args.push("-w");
    }
    args.push(if opts.regex { "-E" } else { "-F" });
    if opts.untracked {
        args.push("--untracked");
    }
    args.extend(["-e", query, "--"]);
    let mut child = git
        .command(root, &AccountContext::default(), &args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut stderr = child.stderr.take().expect("piped stderr");
    let errors = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    // Lines are read on their own thread so this one can watch `should_stop` while git is
    // silent (a big repository with few matches).
    let stdout = child.stdout.take().expect("piped stdout");
    let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(256);
    let lines = std::thread::spawn(move || -> std::io::Result<()> {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut buf = Vec::new();
            if reader.read_until(b'\n', &mut buf)? == 0 || tx.send(buf).is_err() {
                return Ok(());
            }
        }
    });
    let mut killed = false;
    loop {
        if should_stop() {
            out.stopped = true;
            killed = true;
            let _ = child.kill();
            break;
        }
        match rx.recv_timeout(STOP_POLL) {
            Ok(line) => {
                if out.results.matches.len() >= limit {
                    out.results.truncated = true;
                    killed = true;
                    let _ = child.kill();
                    break;
                }
                if let Some(m) = parse_grep_line(&line) {
                    out.results.matches.push(m);
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    // The reader stops at the end of the output, or at its next line once nobody listens.
    drop(rx);
    let status = child.wait()?;
    let read = lines.join().unwrap_or(Ok(()));
    let stderr = errors.join().unwrap_or_default();
    if killed {
        return Ok(out);
    }
    read?;
    // 1: nothing matched.
    if status.success() || (status.code() == Some(1) && stderr.trim().is_empty()) {
        return Ok(out);
    }
    Err(GitError::Failed {
        args: format!("grep {query}"),
        status: status.to_string(),
        stderr: stderr.trim().to_owned(),
    })
}

// ---------------------------------------------------------------------------------------------
// Creating, renaming and deleting files and folders.

fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// Creates an empty file (and its missing parent folders). Refused when it already exists.
pub fn create_file(root: &Path, path: &str) -> Result<(), GitError> {
    let full = resolve(root, path)?;
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&full)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => invalid(&format!("{path} already exists")),
            _ => e.into(),
        })?;
    Ok(())
}

/// Creates a folder (and its missing parents). Refused when something is already there.
pub fn create_dir(root: &Path, path: &str) -> Result<(), GitError> {
    let full = resolve(root, path)?;
    if exists(&full) {
        return Err(invalid(&format!("{path} already exists")));
    }
    std::fs::create_dir_all(&full)?;
    Ok(())
}

/// Renames / moves a file or folder inside the repository (a plain move: git sees the rename
/// once both sides are staged). Changing only the letter case is allowed on any file system.
pub fn rename_path(root: &Path, from: &str, to: &str) -> Result<(), GitError> {
    let src = resolve(root, from)?;
    let dst = resolve(root, to)?;
    if !exists(&src) {
        return Err(invalid(&format!("{from} does not exist")));
    }
    if exists(&dst) && !from.eq_ignore_ascii_case(to) {
        return Err(invalid(&format!("{to} already exists")));
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&src, &dst)?;
    Ok(())
}

/// Deletes a file, or a folder with everything in it. A symlink is removed, not followed.
pub fn delete_path(root: &Path, path: &str) -> Result<(), GitError> {
    let full = resolve(root, path)?;
    let meta = std::fs::symlink_metadata(&full)?;
    if meta.file_type().is_symlink() {
        // A link to a folder is a directory entry on Windows.
        std::fs::remove_file(&full).or_else(|_| std::fs::remove_dir(&full))?;
    } else if meta.is_dir() {
        std::fs::remove_dir_all(&full)?;
    } else {
        std::fs::remove_file(&full)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Staging edited text.

/// Puts `text` in the index as the staged version of `path` (the editor's "stage / unstage
/// this block": the frontend builds the whole new index text). `expected` is the index text the
/// edit was based on, `None` when the file was not in the index; when the index changed since,
/// nothing is written and `false` is returned. The working tree is not touched.
pub fn stage_text(
    git: &GitRunner,
    root: &Path,
    path: &str,
    text: &str,
    expected: Option<&str>,
) -> Result<bool, GitError> {
    check_path(path)?;
    let ctx = AccountContext::default();
    let entries = git.run_text(root, &ctx, ["ls-files", "-s", "--", path])?;
    let entries: Vec<&str> = entries.lines().filter(|l| !l.is_empty()).collect();
    if entries.len() > 1 {
        return Err(invalid(&format!("{path} has unresolved conflicts")));
    }
    let mode = entries
        .first()
        .and_then(|l| l.split_whitespace().next())
        .unwrap_or("100644")
        .to_owned();
    if mode != "100644" && mode != "100755" {
        return Err(invalid(&format!(
            "{path} is not a regular file in the index"
        )));
    }
    let current = if entries.is_empty() {
        None
    } else {
        let bytes = blob::object_bytes(git, root, &format!(":{path}")).unwrap_or_default();
        if blob::parse_lfs_pointer(&bytes).is_some() {
            return Err(invalid(&format!("{path} is stored in Git LFS")));
        }
        let Ok(text) = String::from_utf8(bytes) else {
            return Err(invalid(&format!("{path} is not UTF-8 text in the index")));
        };
        Some(text)
    };
    if current.as_deref() != expected {
        return Ok(false);
    }
    let out = git.run_with_input(
        root,
        &ctx,
        ["hash-object", "-w", "--stdin"],
        text.as_bytes(),
    )?;
    let oid = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    git.run(
        root,
        &ctx,
        [
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("{mode},{oid},{path}"),
        ],
    )?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_text_and_flags_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("a.txt"), "hello\r\nworld\n").unwrap();
        std::fs::write(root.join("bin.dat"), [1u8, 0, 2]).unwrap();
        std::fs::write(root.join("latin1.txt"), [b'c', 0xe9, b'\n']).unwrap();

        let a = read_file(root, "a.txt", None).unwrap();
        assert_eq!(a.text.as_deref(), Some("hello\r\nworld\n"));
        assert!(a.exists && !a.binary && !a.undecodable);
        assert_eq!(a.version, version(b"hello\r\nworld\n"));

        let bin = read_file(root, "bin.dat", None).unwrap();
        assert!(bin.binary && bin.text.is_none());

        // Not UTF-8: read as Windows-1252, editable.
        let latin = read_file(root, "latin1.txt", None).unwrap();
        assert_eq!(latin.text.as_deref(), Some("c\u{e9}\n"));
        assert_eq!(latin.encoding, Some(TextEncoding::Windows1252));
        assert!(!latin.undecodable);
        // Asked for UTF-8: it is not, so nothing to edit.
        let strict = read_file(root, "latin1.txt", Some(TextEncoding::Utf8)).unwrap();
        assert!(strict.undecodable && strict.text.is_none());
        assert_eq!(strict.encoding, Some(TextEncoding::Utf8));

        let gone = read_file(root, "gone.txt", None).unwrap();
        assert!(!gone.exists);
        assert_eq!(gone.version, MISSING);
    }

    #[test]
    fn refuses_paths_outside_the_repository() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_file(dir.path(), "../x.txt", None).is_err());
        assert!(
            write_file(
                dir.path(),
                "../x.txt",
                "",
                TextEncoding::Utf8,
                LineEnding::None,
                None
            )
            .is_err()
        );
        assert!(read_file(dir.path(), "/etc/passwd", None).is_err());
    }

    #[test]
    fn save_checks_the_version_it_was_based_on() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("f.txt"), "one\n").unwrap();
        let v1 = read_file(root, "f.txt", None).unwrap().version;

        let out = write_file(
            root,
            "f.txt",
            "two\n",
            TextEncoding::Utf8,
            LineEnding::None,
            Some(&v1),
        )
        .unwrap();
        assert!(out.saved);
        assert_eq!(
            std::fs::read_to_string(root.join("f.txt")).unwrap(),
            "two\n"
        );
        assert_eq!(out.version, read_file(root, "f.txt", None).unwrap().version);

        // Based on the old version: refused, the file keeps the newer contents.
        let stale = write_file(
            root,
            "f.txt",
            "three\n",
            TextEncoding::Utf8,
            LineEnding::None,
            Some(&v1),
        )
        .unwrap();
        assert!(!stale.saved);
        assert_eq!(stale.version, out.version);
        assert_eq!(
            std::fs::read_to_string(root.join("f.txt")).unwrap(),
            "two\n"
        );

        // Without a version (overwrite) it is written.
        assert!(
            write_file(
                root,
                "f.txt",
                "three\n",
                TextEncoding::Utf8,
                LineEnding::None,
                None
            )
            .unwrap()
            .saved
        );
        assert_eq!(
            std::fs::read_to_string(root.join("f.txt")).unwrap(),
            "three\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_read_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("target.txt"), "x").unwrap();
        std::os::unix::fs::symlink(root.join("target.txt"), root.join("link.txt")).unwrap();
        let f = read_file(root, "link.txt", None).unwrap();
        assert!(f.symlink && f.text.is_none());
        assert!(
            write_file(
                root,
                "link.txt",
                "y",
                TextEncoding::Utf8,
                LineEnding::None,
                None
            )
            .is_err()
        );
        assert_eq!(
            std::fs::read_to_string(root.join("target.txt")).unwrap(),
            "x"
        );
    }

    #[test]
    fn lists_tracked_untracked_and_empty_folders() {
        let r = crate::blob::tests::Repo::new();
        r.write("a.txt", "a");
        r.write("src/b.rs", "b");
        r.write("gone.txt", "x");
        r.commit_all("init");
        std::fs::remove_file(r.p().join("gone.txt")).unwrap();
        r.write("new/c.md", "c");
        r.write(".gitignore", "*.log");
        r.write("debug.log", "ignored");
        std::fs::create_dir_all(r.p().join("empty/inner")).unwrap();

        let files = list_files(&r.git, r.p()).unwrap();
        assert_eq!(files.tracked, ["a.txt", "src/b.rs"]);
        assert_eq!(files.untracked, [".gitignore", "new/c.md"]);
        assert_eq!(files.folders, ["empty"]);
        assert!(!files.truncated);
    }

    #[test]
    fn searches_with_columns_and_options() {
        let r = crate::blob::tests::Repo::new();
        r.write("a.txt", "hello world\nsay Hello\n");
        r.write("d/b.txt", "café hello\n");
        r.commit_all("init");
        r.write("u.txt", "hello untracked\n");

        let all = search(&r.git, r.p(), "hello", SearchOptions::default(), 100).unwrap();
        let found: Vec<_> = all
            .matches
            .iter()
            .map(|m| (m.path.as_str(), m.line, m.column))
            .collect();
        // Columns count UTF-16 units: "café " is 5 characters (6 bytes).
        assert_eq!(found, [("a.txt", 1, 1), ("a.txt", 2, 5), ("d/b.txt", 1, 6)]);

        let case = SearchOptions {
            case_sensitive: true,
            ..Default::default()
        };
        let exact = search(&r.git, r.p(), "Hello", case, 100).unwrap();
        assert_eq!(exact.matches.len(), 1);

        let untracked = SearchOptions {
            untracked: true,
            ..Default::default()
        };
        let with = search(&r.git, r.p(), "untracked", untracked, 100).unwrap();
        assert_eq!(with.matches.len(), 1);

        let limited = search(&r.git, r.p(), "hello", SearchOptions::default(), 1).unwrap();
        assert!(limited.truncated && limited.matches.len() == 1);

        let none = search(&r.git, r.p(), "nothing", SearchOptions::default(), 10).unwrap();
        assert!(none.matches.is_empty());
        let regex = SearchOptions {
            regex: true,
            ..Default::default()
        };
        assert!(search(&r.git, r.p(), "(", regex, 10).is_err());
    }

    #[test]
    fn creates_renames_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        create_file(root, "a/b/new.txt").unwrap();
        assert!(root.join("a/b/new.txt").is_file());
        assert!(create_file(root, "a/b/new.txt").is_err());
        create_dir(root, "folder/sub").unwrap();
        assert!(create_dir(root, "folder").is_err());

        rename_path(root, "a/b/new.txt", "moved/renamed.txt").unwrap();
        assert!(root.join("moved/renamed.txt").is_file());
        assert!(rename_path(root, "missing.txt", "x.txt").is_err());
        create_file(root, "taken.txt").unwrap();
        assert!(rename_path(root, "moved/renamed.txt", "taken.txt").is_err());
        // Only the letter case changes.
        rename_path(root, "taken.txt", "Taken.txt").unwrap();

        delete_path(root, "folder").unwrap();
        assert!(!root.join("folder").exists());
        delete_path(root, "moved/renamed.txt").unwrap();
        assert!(!root.join("moved/renamed.txt").exists());

        for bad in [".git/config", "a/.GIT/x", "../out.txt", "a//b"] {
            assert!(create_file(root, bad).is_err(), "{bad}");
            assert!(delete_path(root, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn stages_text_based_on_the_expected_index() {
        let r = crate::blob::tests::Repo::new();
        r.write("f.txt", "one\ntwo\n");
        r.commit_all("init");
        r.write("f.txt", "ONE\ntwo\nthree\n");

        // Stage only the first line's change; the working tree keeps everything.
        let staged = stage_text(&r.git, r.p(), "f.txt", "ONE\ntwo\n", Some("one\ntwo\n"));
        assert!(staged.unwrap());
        assert_eq!(r.run(&["show", ":f.txt"]), "ONE\ntwo");
        assert_eq!(r.read("f.txt"), b"ONE\ntwo\nthree\n");

        // Based on an outdated index: refused.
        let stale = stage_text(&r.git, r.p(), "f.txt", "x\n", Some("one\ntwo\n"));
        assert!(!stale.unwrap());
        assert_eq!(r.run(&["show", ":f.txt"]), "ONE\ntwo");

        // A file not in the index yet.
        r.write("n.txt", "new\n");
        assert!(stage_text(&r.git, r.p(), "n.txt", "new\n", None).unwrap());
        assert_eq!(r.run(&["show", ":n.txt"]), "new");
    }

    // -----------------------------------------------------------------------------------------
    // Encodings and line breaks.

    fn utf16(text: &str, little_endian: bool) -> Vec<u8> {
        let (bom, units): (&[u8], Vec<u8>) = if little_endian {
            (
                UTF16LE_BOM,
                text.encode_utf16().flat_map(u16::to_le_bytes).collect(),
            )
        } else {
            (
                UTF16BE_BOM,
                text.encode_utf16().flat_map(u16::to_be_bytes).collect(),
            )
        };
        [bom, &units].concat()
    }

    /// Every byte Windows-1252 can hold except NUL, with CRLF line breaks.
    fn cp1252_sample() -> Vec<u8> {
        let mut bytes: Vec<u8> = (1..=255u8).filter(|&b| b != b'\r' && b != b'\n').collect();
        bytes.extend_from_slice(b"\r\nline two\r\n");
        bytes
    }

    /// Reads the bytes as a file, saves the text unchanged with what was detected, and checks
    /// that the file is byte for byte the same.
    fn assert_round_trip(bytes: &[u8], encoding: TextEncoding, eol: LineEnding) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("f"), bytes).unwrap();
        let f = read_file(root, "f", None).unwrap();
        assert_eq!(f.encoding, Some(encoding), "{bytes:?}");
        assert_eq!(f.eol, Some(eol), "{bytes:?}");
        let text = f.text.expect("editable");
        assert!(
            !text.starts_with('\u{feff}'),
            "the BOM stays out of the text"
        );
        let out = write_file(root, "f", &text, encoding, eol, Some(&f.version)).unwrap();
        assert!(out.saved);
        assert_eq!(std::fs::read(root.join("f")).unwrap(), bytes);
        // Unchanged bytes, unchanged version: the editor sees no change on disk.
        assert_eq!(out.version, f.version);
    }

    #[test]
    fn load_and_save_is_byte_identical_in_every_encoding_and_line_ending() {
        use LineEnding as L;
        use TextEncoding as E;
        let text = "using UnityEngine;\r\n// Tiếng Việt, café, 日本語, \u{2070E}\r\nclass A {}\r\n";
        let lf = text.replace("\r\n", "\n");
        let mixed = "one\r\ntwo\nthree\rfour";

        assert_round_trip(lf.as_bytes(), E::Utf8, L::Lf);
        assert_round_trip(text.as_bytes(), E::Utf8, L::Crlf);
        assert_round_trip(mixed.as_bytes(), E::Utf8, L::Mixed);
        assert_round_trip(b"no line break", E::Utf8, L::None);
        assert_round_trip(b"", E::Utf8, L::None);
        for body in [text, lf.as_str(), mixed, ""] {
            let eol = detect_eol(body);
            assert_round_trip(&[UTF8_BOM, body.as_bytes()].concat(), E::Utf8Bom, eol);
            assert_round_trip(&utf16(body, true), E::Utf16Le, eol);
            assert_round_trip(&utf16(body, false), E::Utf16Be, eol);
        }
        assert_round_trip(&cp1252_sample(), E::Windows1252, L::Crlf);
        assert_round_trip(b"caf\xe9\nna\xefve\n", E::Windows1252, L::Lf);
        // A UTF-8 BOM followed by bytes that are not UTF-8 is Windows-1252 text from byte 0.
        assert_round_trip(b"\xef\xbb\xbfcaf\xe9", E::Windows1252, L::None);
    }

    #[test]
    fn windows_1252_maps_every_byte_both_ways() {
        let bytes: Vec<u8> = (1..=255u8).collect();
        let text = decode_cp1252(&bytes);
        assert_eq!(text.chars().count(), 255);
        assert_eq!(encode(&text, TextEncoding::Windows1252).unwrap(), bytes);
        assert_eq!(
            decode_cp1252(&[0x80, 0x99, 0x9f]),
            "\u{20ac}\u{2122}\u{178}"
        );
    }

    #[test]
    fn utf16_files_are_text_unless_broken() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Odd length and an unpaired surrogate cannot be written back unchanged.
        std::fs::write(root.join("odd"), [0xFF, 0xFE, b'a', 0, b'b']).unwrap();
        std::fs::write(root.join("lone"), [0xFF, 0xFE, 0x00, 0xD8, b'a', 0]).unwrap();
        // NUL characters: binary.
        std::fs::write(root.join("zeros"), [0xFF, 0xFE, 0, 0, b'a', 0]).unwrap();
        // UTF-16 without a BOM is binary, as before.
        std::fs::write(root.join("bare"), [b'a', 0, b'b', 0]).unwrap();
        for name in ["odd", "lone"] {
            let f = read_file(root, name, None).unwrap();
            assert!(f.undecodable && f.text.is_none() && !f.binary, "{name}");
        }
        assert!(read_file(root, "zeros", None).unwrap().binary);
        assert!(read_file(root, "bare", None).unwrap().binary);
        let asked = read_file(root, "bare", Some(TextEncoding::Utf16Le)).unwrap();
        assert!(asked.binary && asked.text.is_none());
    }

    #[test]
    fn reopens_with_another_encoding() {
        use TextEncoding as E;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("u.txt"), "caf\u{e9}\n").unwrap();
        std::fs::write(root.join("bom.txt"), [UTF8_BOM, b"x"].concat()).unwrap();

        // UTF-8 bytes read as Windows-1252: two characters for the accent, saved back as is.
        let w = read_file(root, "u.txt", Some(E::Windows1252)).unwrap();
        assert_eq!(w.text.as_deref(), Some("caf\u{c3}\u{a9}\n"));
        assert_eq!(w.encoding, Some(E::Windows1252));
        let text = w.text.unwrap();
        write_file(root, "u.txt", &text, E::Windows1252, LineEnding::Lf, None).unwrap();
        assert_eq!(
            std::fs::read(root.join("u.txt")).unwrap(),
            "caf\u{e9}\n".as_bytes()
        );

        // Asking for UTF-8 keeps a BOM that is there.
        let b = read_file(root, "bom.txt", Some(E::Utf8)).unwrap();
        assert_eq!(
            (b.text.as_deref(), b.encoding),
            (Some("x"), Some(E::Utf8Bom))
        );
        // UTF-16 needs its BOM.
        let no = read_file(root, "u.txt", Some(E::Utf16Le)).unwrap();
        assert!(no.undecodable && no.encoding == Some(E::Utf16Le));
    }

    #[test]
    fn saves_in_another_encoding_and_line_ending() {
        use LineEnding as L;
        use TextEncoding as E;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let save = |text: &str, enc, eol| {
            let out = write_file(root, "f.txt", text, enc, eol, None).unwrap();
            assert!(out.saved);
            std::fs::read(root.join("f.txt")).unwrap()
        };
        assert_eq!(save("a\nb\n", E::Utf8, L::Crlf), b"a\r\nb\r\n");
        assert_eq!(save("a\r\nb\rc\n", E::Utf8, L::Lf), b"a\nb\nc\n");
        assert_eq!(save("a\r\nb\n", E::Utf8, L::Mixed), b"a\r\nb\n");
        assert_eq!(
            save("\u{e9}\n", E::Utf8Bom, L::Lf),
            b"\xef\xbb\xbf\xc3\xa9\n"
        );
        assert_eq!(
            save("\u{e9}\n", E::Utf16Le, L::Crlf),
            b"\xff\xfe\xe9\0\r\0\n\0"
        );
        assert_eq!(save("\u{e9}\n", E::Utf16Be, L::Lf), b"\xfe\xff\0\xe9\0\n");
        assert_eq!(save("\u{20ac}\u{e9}", E::Windows1252, L::None), b"\x80\xe9");
    }

    #[test]
    fn refuses_characters_the_encoding_cannot_hold() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("f.txt"), b"caf\xe9\n").unwrap();
        let f = read_file(root, "f.txt", None).unwrap();
        let text = "caf\u{e9}\r\nTi\u{1ebf}ng Vi\u{1ec7}t\r\n";
        let out = write_file(
            root,
            "f.txt",
            text,
            TextEncoding::Windows1252,
            LineEnding::Crlf,
            Some(&f.version),
        )
        .unwrap();
        assert!(!out.saved);
        assert_eq!(out.version, f.version);
        assert_eq!(
            out.unencodable,
            Some(Unencodable {
                character: "\u{1ebf}".into(),
                line: 2,
                column: 3
            })
        );
        assert_eq!(std::fs::read(root.join("f.txt")).unwrap(), b"caf\xe9\n");
    }

    #[test]
    fn detects_and_converts_line_endings() {
        use LineEnding as L;
        assert_eq!(detect_eol(""), L::None);
        assert_eq!(detect_eol("a\nb"), L::Lf);
        assert_eq!(detect_eol("a\r\nb\r\n"), L::Crlf);
        assert_eq!(detect_eol("a\r\nb\n"), L::Mixed);
        assert_eq!(detect_eol("a\rb"), L::Mixed);
        assert_eq!(convert_eol("a\r\nb\nc\rd", L::Crlf), "a\r\nb\r\nc\r\nd");
        assert_eq!(convert_eol("a\r\nb\nc\rd", L::Lf), "a\nb\nc\nd");
        assert_eq!(convert_eol("a\r\nb\n", L::Mixed), "a\r\nb\n");
        assert!(matches!(
            convert_eol("a\nb", L::Lf),
            std::borrow::Cow::Borrowed(_)
        ));
    }

    #[test]
    fn reads_the_staged_copy_in_an_encoding() {
        let r = crate::blob::tests::Repo::new();
        std::fs::write(r.p().join("w.txt"), b"caf\xe9\n").unwrap();
        r.commit_all("init");
        let staged = read_index_text(&r.git, r.p(), "w.txt", TextEncoding::Windows1252);
        assert_eq!(staged.unwrap().as_deref(), Some("caf\u{e9}\n"));
        let utf8 = read_index_text(&r.git, r.p(), "w.txt", TextEncoding::Utf8).unwrap();
        assert_eq!(utf8, None);
        let missing = read_index_text(&r.git, r.p(), "nope.txt", TextEncoding::Utf8).unwrap();
        assert_eq!(missing, None);
    }
}
