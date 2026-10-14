//! Parsing `.gitattributes` for Git LFS patterns (`filter=lfs`, `lockable`).

use serde::Serialize;

/// A pattern routed through Git LFS by a `.gitattributes` line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct LfsPattern {
    /// Pattern as written (unquoted), relative to the directory of `file`.
    pub pattern: String,
    /// Repository-relative path of the attributes file (`.gitattributes`,
    /// `assets/.gitattributes`), or `$GIT_DIR/info/attributes`.
    pub file: String,
    /// 1-based line number in `file`.
    pub line: u32,
    /// Files matching the pattern are read-only until locked (`git lfs lock`).
    pub lockable: bool,
}

/// Undoes git's C-style quoting of a pattern (`"a\tb"`, `"\303\251"`).
fn unquote(s: &str) -> Option<(String, &str)> {
    let rest = s.strip_prefix('"')?;
    let mut bytes = Vec::new();
    let mut it = rest.char_indices();
    while let Some((i, c)) = it.next() {
        match c {
            '"' => {
                return Some((String::from_utf8_lossy(&bytes).into_owned(), &rest[i + 1..]));
            }
            '\\' => {
                let (_, e) = it.next()?;
                match e {
                    'n' => bytes.push(b'\n'),
                    't' => bytes.push(b'\t'),
                    'r' => bytes.push(b'\r'),
                    'a' => bytes.push(7),
                    'b' => bytes.push(8),
                    'f' => bytes.push(12),
                    'v' => bytes.push(11),
                    '0'..='7' => {
                        let mut v = e.to_digit(8)?;
                        for _ in 0..2 {
                            let (_, d) = it.next()?;
                            v = v * 8 + d.to_digit(8)?;
                        }
                        bytes.push(u8::try_from(v).ok()?);
                    }
                    other => {
                        let mut buf = [0; 4];
                        bytes.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
                    }
                }
            }
            other => {
                let mut buf = [0; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
    None
}

/// Splits a line into `(pattern, attributes)`, or `None` for blanks, comments and macros.
fn split_line(line: &str) -> Option<(String, &str)> {
    let line = line.trim_start();
    if line.is_empty() || line.starts_with('#') || line.starts_with("[attr]") {
        return None;
    }
    if line.starts_with('"') {
        return unquote(line);
    }
    let end = line.find([' ', '\t']).unwrap_or(line.len());
    Some((line[..end].to_owned(), &line[end..]))
}

/// LFS patterns declared by one attributes file. Within a line the last mention of an
/// attribute wins, as in git (`*.bin filter=lfs -filter` is not LFS).
pub fn parse_lfs_patterns(content: &str, file: &str) -> Vec<LfsPattern> {
    let mut out = Vec::new();
    for (i, raw) in content.lines().enumerate() {
        let Some((pattern, attrs)) = split_line(raw.trim_end_matches('\r')) else {
            continue;
        };
        let mut lfs = false;
        let mut lockable = false;
        for attr in attrs.split_whitespace() {
            let (unset, name) = match attr.strip_prefix(['-', '!']) {
                Some(n) => (true, n),
                None => (false, attr),
            };
            let (name, value) = match name.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (name, None),
            };
            match name {
                "filter" => lfs = !unset && value == Some("lfs"),
                "lockable" => lockable = !unset,
                _ => {}
            }
        }
        if lfs && !pattern.is_empty() {
            out.push(LfsPattern {
                pattern,
                file: file.to_owned(),
                line: i as u32 + 1,
                lockable,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lfs_lines_and_ignores_the_rest() {
        let content = "# comment\r\n\
            *.psd filter=lfs diff=lfs merge=lfs -text\r\n\
            *.txt text eol=lf\n\
            \n\
            [attr]binary -diff -merge -text\n\
            *.fbx filter=lfs diff=lfs merge=lfs -text lockable\n\
            a[[:space:]]b.mp4 filter=lfs diff=lfs merge=lfs -text\n\
            \"quoted name\\t\\303\\251.zip\" filter=lfs\n\
            *.tmp filter=lfs -filter\n\
            *.raw filter=other\n\
            *.uasset lockable filter=lfs -lockable\n\
              *.wav\tfilter=lfs\n";
        let p = parse_lfs_patterns(content, "assets/.gitattributes");
        let names: Vec<&str> = p.iter().map(|p| p.pattern.as_str()).collect();
        assert_eq!(
            names,
            [
                "*.psd",
                "*.fbx",
                "a[[:space:]]b.mp4",
                "quoted name\té.zip",
                "*.uasset",
                "*.wav"
            ]
        );
        assert_eq!(p[0].line, 2);
        assert_eq!(p[0].file, "assets/.gitattributes");
        assert!(!p[0].lockable);
        assert!(p[1].lockable);
        assert!(!p[4].lockable, "-lockable after lockable");
    }

    #[test]
    fn broken_quotes_are_skipped() {
        assert!(parse_lfs_patterns("\"unterminated filter=lfs\n", "x").is_empty());
        assert!(parse_lfs_patterns("\"bad\\39\" filter=lfs\n", "x").is_empty());
    }
}
