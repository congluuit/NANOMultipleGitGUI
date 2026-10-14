//! Minimal glob → regex translation for rule matching.
//!
//! - `**` matches any number of path segments (including none: `a/**/b` matches `a/b`, and a
//!   trailing `/**` also matches the directory itself),
//! - `*` matches within one segment, `?` one character other than `/`,
//! - everything else is literal; `\` in patterns is treated as a path separator.
//!
//! Translating to the `regex` crate keeps matching linear-time whatever the pattern.

use regex::{Regex, RegexBuilder};

/// Patterns and the texts they are matched against are short; this bounds compiled size.
const SIZE_LIMIT: usize = 1 << 20;

/// Whether `s` contains glob metacharacters.
pub fn has_wildcards(s: &str) -> bool {
    s.contains(['*', '?'])
}

/// Regex source (unanchored) for a glob pattern.
pub fn to_regex_source(pattern: &str) -> String {
    let chars: Vec<char> = pattern.replace('\\', "/").chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' if chars.get(i + 1) == Some(&'*') => {
                let after = i + 2;
                let at_segment_start = i == 0 || chars[i - 1] == '/';
                if at_segment_start && chars.get(after) == Some(&'/') {
                    // `**/` → zero or more whole segments.
                    out.push_str("(?:[^/]*/)*");
                    i = after + 1;
                } else if at_segment_start && after == chars.len() && i > 0 {
                    // Trailing `/**` → the directory itself or anything below it.
                    out.pop();
                    out.push_str("(?:/.*)?");
                    i = after;
                } else {
                    out.push_str(".*");
                    i = after;
                }
                continue;
            }
            '*' => out.push_str("[^/]*"),
            '?' => out.push_str("[^/]"),
            c => out.push_str(&regex::escape(c.encode_utf8(&mut [0; 4]))),
        }
        i += 1;
    }
    out
}

/// Builds an anchored regex from a regex source.
pub fn build(source: &str, case_insensitive: bool) -> Result<Regex, regex::Error> {
    RegexBuilder::new(&format!("^(?:{source})$"))
        .case_insensitive(case_insensitive)
        .size_limit(SIZE_LIMIT)
        .build()
}

/// Compiles a glob into an anchored regex.
pub fn compile(pattern: &str, case_insensitive: bool) -> Result<Regex, regex::Error> {
    build(&to_regex_source(pattern), case_insensitive)
}

/// One-shot match, mostly for tests and previews.
pub fn matches(pattern: &str, text: &str, case_insensitive: bool) -> bool {
    compile(pattern, case_insensitive).is_ok_and(|re| re.is_match(&text.replace('\\', "/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_star_spans_segments() {
        assert!(matches("D:/Work/**", "D:/Work/acme/api", false));
        assert!(matches("D:/Work/**", "D:/Work", false));
        assert!(!matches("D:/Work/**", "D:/Workshop/x", false));
        assert!(matches("a/**/b", "a/b", false));
        assert!(matches("a/**/b", "a/x/y/b", false));
        assert!(!matches("a/**/b", "a/xb", false));
        assert!(matches("**/repo", "/home/me/repo", false));
        assert!(matches("x**", "x/y/z", false));
    }

    #[test]
    fn single_star_and_question_mark_stay_in_segment() {
        assert!(matches("acme-*", "acme-corp", false));
        assert!(!matches("acme-*", "acme-corp/x", false));
        assert!(matches("repo?", "repo1", false));
        assert!(!matches("repo?", "repo/", false));
        assert!(matches("*.acme.dev", "git.eu.acme.dev", false));
    }

    #[test]
    fn literals_are_escaped_and_case_is_optional() {
        assert!(matches(
            "C:/Users/me (work)/+x",
            "C:/Users/me (work)/+x",
            false
        ));
        assert!(!matches("a.b", "axb", false));
        assert!(matches("D:/WORK/**", "d:/work/a", true));
        assert!(!matches("D:/WORK/**", "d:/work/a", false));
        assert!(matches("D:\\Work\\**", "D:\\Work\\a", false));
    }
}
