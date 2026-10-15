//! LFS files of the current checkout (PLAN §6.5, §6.7): `git lfs ls-files --json` with size,
//! whether the object is in the local store, whether the working-tree file is the real content
//! or still a pointer, and whether the path is `lockable`.

use std::collections::HashSet;
use std::path::Path;

use nanomgg_git_engine::{AccountContext, GitRunner};
use serde::{Deserialize, Serialize};

use crate::LfsError;

/// One LFS-tracked file of HEAD.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LfsFile {
    /// Repository-relative path.
    pub path: String,
    pub oid: String,
    /// Size of the real content in bytes.
    pub size: u64,
    /// The object is in the local LFS store.
    pub downloaded: bool,
    /// The working-tree file holds the real content (not the small pointer text).
    pub checked_out: bool,
    /// Matches a `lockable` pattern (read-only until locked).
    pub lockable: bool,
}

impl LfsFile {
    /// Only the pointer is present: "Tải file này" fetches and checks it out.
    pub fn is_pointer(&self) -> bool {
        !self.checked_out
    }
}

#[derive(Debug, Deserialize)]
struct LsFiles {
    files: Option<Vec<LsFile>>,
}

#[derive(Debug, Deserialize)]
struct LsFile {
    name: String,
    size: u64,
    oid: String,
    #[serde(default)]
    checkout: bool,
    #[serde(default)]
    downloaded: bool,
}

/// Parses `git lfs ls-files --json` (one entry per path; paths keep their order).
pub fn parse_ls_files(json: &str) -> Result<Vec<LfsFile>, serde_json::Error> {
    let parsed: LsFiles = serde_json::from_str(json)?;
    let mut seen = HashSet::new();
    Ok(parsed
        .files
        .unwrap_or_default()
        .into_iter()
        .filter(|f| seen.insert(f.name.clone()))
        .map(|f| LfsFile {
            path: f.name.replace('\\', "/"),
            oid: f.oid,
            size: f.size,
            downloaded: f.downloaded,
            checked_out: f.checkout,
            lockable: false,
        })
        .collect())
}

/// Paths with the `lockable` attribute set, from `git check-attr -z --stdin lockable`
/// (`<path>\0lockable\0<value>\0` triples).
pub fn parse_lockable_attrs(raw: &[u8]) -> HashSet<String> {
    let fields: Vec<String> = raw
        .split(|b| *b == 0)
        .map(|f| String::from_utf8_lossy(f).into_owned())
        .collect();
    fields
        .chunks(3)
        .filter(|c| c.len() == 3 && c[1] == "lockable" && c[2] != "unspecified" && c[2] != "unset")
        .map(|c| c[0].clone())
        .collect()
}

/// LFS files of HEAD with their download / checkout / lockable state.
pub fn list(git: &GitRunner, repo: &Path, ctx: &AccountContext) -> Result<Vec<LfsFile>, LfsError> {
    let out = git.run_text(repo, ctx, ["lfs", "ls-files", "--json"])?;
    let mut files = parse_ls_files(&out).map_err(|e| {
        LfsError::Io(std::io::Error::other(format!(
            "unexpected git lfs ls-files output: {e}"
        )))
    })?;
    if !files.is_empty() {
        let input: Vec<u8> = files
            .iter()
            .flat_map(|f| f.path.bytes().chain(std::iter::once(0)))
            .collect();
        let attrs = git.run_with_input(
            repo,
            &AccountContext::default(),
            ["check-attr", "-z", "--stdin", "lockable"],
            &input,
        )?;
        let lockable = parse_lockable_attrs(&attrs.stdout);
        for f in &mut files {
            f.lockable = lockable.contains(&f.path);
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ls_files_and_lockable_attributes() {
        // Captured from git-lfs 3.5.1.
        let json = r#"{
 "files": [
  {"name": "a.bin", "size": 1700, "checkout": true, "downloaded": true, "oid_type": "sha256",
   "oid": "4fef548847c0c200527049b6bf2886080ae01604c7f979fc3414a5cb0d4d8a04",
   "version": "https://git-lfs.github.com/spec/v1"},
  {"name": "sp ace.psd", "size": 2000, "checkout": false, "downloaded": false, "oid_type": "sha256",
   "oid": "aaa74713f2bc64ded8802831c7213e5c2e5df647c13317dbdb0cc357635a6794",
   "version": "https://git-lfs.github.com/spec/v1"}
 ]
}"#;
        let files = parse_ls_files(json).unwrap();
        assert_eq!(files.len(), 2);
        assert!(!files[0].is_pointer() && files[0].downloaded);
        assert!(files[1].is_pointer() && !files[1].downloaded);
        assert_eq!(files[1].size, 2000);
        assert!(parse_ls_files("{\"files\": null}").unwrap().is_empty());

        let raw =
            b"a.bin\0lockable\0unspecified\0sp ace.psd\0lockable\0set\0x.psd\0lockable\0unset\0";
        let l = parse_lockable_attrs(raw);
        assert_eq!(l.len(), 1);
        assert!(l.contains("sp ace.psd"));
    }
}
