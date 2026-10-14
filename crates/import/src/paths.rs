//! Path helpers shared by the scanners (tilde expansion, Windows paths, simple globs).

use std::path::{Component, Path, PathBuf};

/// Expands a leading `~` / `~/` / `~\` to `home`; other `~user` forms are left as-is.
pub fn expand_tilde(s: &str, home: &Path) -> PathBuf {
    if s == "~" {
        return home.to_path_buf();
    }
    match s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        Some(rest) => home.join(rest),
        None => PathBuf::from(s),
    }
}

/// `C:\Users\a` → `C:/Users/a`.
pub fn to_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// `/x`, `\\server\share`, `C:/x`, `C:\x`.
pub fn is_absolute_like(s: &str) -> bool {
    let b = s.as_bytes();
    s.starts_with('/')
        || s.starts_with('\\')
        || (b.len() >= 2 && b[0].is_ascii_alphabetic() && b[1] == b':')
}

/// Resolves `s` like git/ssh do: `~` → home, absolute stays, otherwise relative to `base`.
pub fn resolve(s: &str, home: &Path, base: &Path) -> PathBuf {
    if s == "~" || s.starts_with("~/") || s.starts_with("~\\") {
        expand_tilde(s, home)
    } else if is_absolute_like(s) {
        PathBuf::from(s)
    } else {
        base.join(s)
    }
}

/// Lexically removes `.` and `a/..` (no file system access; keeps symlinks unresolved).
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Reads a UTF-8 text file, dropping a BOM.
pub fn read_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let bytes = bytes
        .strip_prefix(&[0xEF, 0xBB, 0xBF][..])
        .unwrap_or(&bytes);
    String::from_utf8(bytes.to_vec()).map_err(|_| "file is not valid UTF-8".to_string())
}

/// `*` / `?` wildcard match (ssh_config / known_hosts style, `*` crosses everything).
pub fn wildcard_match(pattern: &str, text: &str, case_insensitive: bool) -> bool {
    let fold = |s: &str| -> Vec<char> {
        if case_insensitive {
            s.to_lowercase().chars().collect()
        } else {
            s.chars().collect()
        }
    };
    let (p, t) = (fold(pattern), fold(text));
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

pub fn has_wildcard(s: &str) -> bool {
    s.contains(['*', '?'])
}

/// Expands `*`/`?` in any component of `pattern` against the file system (sorted, like
/// glob(3) used by ssh `Include`). A pattern without wildcards is returned as-is.
pub fn glob_paths(pattern: &Path) -> Vec<PathBuf> {
    let s = pattern.to_string_lossy();
    if !has_wildcard(&s) {
        return vec![pattern.to_path_buf()];
    }
    let mut current = vec![PathBuf::new()];
    for comp in pattern.components() {
        let part = comp.as_os_str().to_string_lossy();
        let mut next = Vec::new();
        for base in &current {
            if matches!(comp, Component::Normal(_)) && has_wildcard(&part) {
                let dir = if base.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    base.as_path()
                };
                let Ok(entries) = std::fs::read_dir(dir) else {
                    continue;
                };
                let mut names: Vec<String> = entries
                    .filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| !n.starts_with('.') || part.starts_with('.'))
                    .filter(|n| wildcard_match(&part, n, cfg!(windows)))
                    .collect();
                names.sort();
                next.extend(names.into_iter().map(|n| base.join(n)));
            } else {
                next.push(base.join(comp.as_os_str()));
            }
        }
        current = next;
    }
    current
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tilde_and_resolution() {
        let home = Path::new("/home/an");
        assert_eq!(expand_tilde("~/.ssh/x", home), Path::new("/home/an/.ssh/x"));
        assert_eq!(expand_tilde("~\\work", home), Path::new("/home/an/work"));
        assert_eq!(expand_tilde("~bob/x", home), Path::new("~bob/x"));
        assert!(is_absolute_like("C:\\Work"));
        assert!(is_absolute_like("d:/x"));
        assert!(!is_absolute_like("work/x"));
        assert_eq!(
            resolve("inc/a.conf", home, Path::new("/etc/git")),
            Path::new("/etc/git/inc/a.conf")
        );
        assert_eq!(normalize(Path::new("/a/b/../c/./d")), Path::new("/a/c/d"));
    }

    #[test]
    fn wildcards() {
        assert!(wildcard_match("github.com-*", "github.com-work", false));
        assert!(wildcard_match("*.Corp", "git.corp", true));
        assert!(!wildcard_match("*.Corp", "git.corp", false));
        assert!(wildcard_match("h?st", "host", false));
    }

    #[test]
    fn globbing() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["b.conf", "a.conf", "c.txt", ".hidden.conf"] {
            std::fs::write(dir.path().join(n), "").unwrap();
        }
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("x.conf"), "").unwrap();
        let found = glob_paths(&dir.path().join("*.conf"));
        let names: Vec<_> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.conf", "b.conf"]);
        let nested = glob_paths(&dir.path().join("s*").join("*.conf"));
        assert_eq!(nested, [dir.path().join("sub").join("x.conf")]);
    }
}
