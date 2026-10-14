//! `~/.ssh/config` scanner: `Host` blocks → SSH host ↔ key suggestions.
//!
//! Follows `ssh_config(5)`: case-insensitive keywords, `Keyword value` or `Keyword=value`,
//! quoted arguments, first obtained value wins (except `IdentityFile`, which accumulates),
//! `Host` patterns with `*`/`?`/`!`, `Include` (globs, relative to `~/.ssh`, allowed inside a
//! `Host` block where it applies conditionally, max depth 16). `Match` blocks are not
//! evaluated (reported as unsupported). Only `<key>.pub` files are read to fingerprint keys;
//! private keys are never opened.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::paths::{self, to_slash, wildcard_match};
use crate::{ImportWarning, ProviderKind, SourceRef, SuggestionNote, WarningKind};

/// ssh's `READCONF_MAX_DEPTH`.
const MAX_DEPTH: usize = 16;

/// An `IdentityFile` of a host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SuggestedKeyFile {
    /// Expanded path (`~`, `%d`, `%h`, `%r`, `%%`), forward slashes.
    pub path: String,
    pub exists: bool,
    /// From `<path>.pub` when present.
    pub fingerprint: Option<String>,
    pub public_key: Option<String>,
    pub algorithm: Option<String>,
    pub comment: Option<String>,
    pub source: SourceRef,
}

/// One host alias (or wildcard default) of the ssh config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SuggestedSshHost {
    /// The `Host` pattern (`github.com-work`; `*` / `*.corp` for wildcard defaults).
    pub alias: String,
    /// Effective `HostName` (`%h` expanded) or the alias itself.
    pub host_name: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub identity_files: Vec<SuggestedKeyFile>,
    pub identities_only: Option<bool>,
    pub provider: ProviderKind,
    /// `HostName` differs from the alias (`github.com-work` → `github.com`).
    pub is_alias: bool,
    pub wildcard: bool,
    pub notes: Vec<SuggestionNote>,
    /// The `Host` line.
    pub source: SourceRef,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SshConfigScan {
    pub hosts: Vec<SuggestedSshHost>,
    pub warnings: Vec<ImportWarning>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone)]
struct Opt {
    keyword: String,
    args: Vec<String>,
    source: SourceRef,
}

#[derive(Debug, Clone)]
struct Block {
    patterns: Vec<String>,
    is_match: bool,
    /// Options before the first `Host` line.
    implicit: bool,
    source: SourceRef,
    options: Vec<Opt>,
}

struct Parser<'a> {
    home: &'a Path,
    blocks: Vec<Block>,
    warnings: Vec<ImportWarning>,
    files: Vec<String>,
}

/// Splits `Keyword args…` / `Keyword=args…` into the lower-cased keyword and its arguments.
fn split_line(line: &str) -> Option<(String, Vec<String>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let end = line
        .find(|c: char| c.is_whitespace() || c == '=')
        .unwrap_or(line.len());
    let keyword = line[..end].to_ascii_lowercase();
    let mut rest = line[end..].trim_start();
    if let Some(r) = rest.strip_prefix('=') {
        rest = r.trim_start();
    }
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    for c in rest.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_word = true;
            }
            None if c.is_whitespace() => {
                if in_word {
                    args.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            None if c == '#' && !in_word => break,
            None => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        args.push(cur);
    }
    Some((keyword, args))
}

impl Parser<'_> {
    fn warn(&mut self, kind: WarningKind, source: SourceRef, message: String) {
        self.warnings.push(ImportWarning {
            kind,
            source,
            message,
        });
    }

    fn parse_file(&mut self, file: &Path, depth: usize, current: usize) {
        let text = match paths::read_text(file) {
            Ok(t) => t,
            Err(msg) => {
                self.warn(WarningKind::Unreadable, SourceRef::new(file, None), msg);
                return;
            }
        };
        self.files.push(file.to_string_lossy().into_owned());
        let mut current = current;
        for (idx, raw) in text.lines().enumerate() {
            let line_no = u32::try_from(idx + 1).unwrap_or(u32::MAX);
            let Some((keyword, args)) = split_line(raw) else {
                continue;
            };
            let source = SourceRef::new(file, Some(line_no));
            match keyword.as_str() {
                "host" => {
                    self.blocks.push(Block {
                        patterns: args,
                        is_match: false,
                        implicit: false,
                        source,
                        options: Vec::new(),
                    });
                    current = self.blocks.len() - 1;
                }
                "match" => {
                    self.warn(
                        WarningKind::Unsupported,
                        source.clone(),
                        "`Match` blocks are not evaluated".into(),
                    );
                    self.blocks.push(Block {
                        patterns: args,
                        is_match: true,
                        implicit: false,
                        source,
                        options: Vec::new(),
                    });
                    current = self.blocks.len() - 1;
                }
                "include" => {
                    for arg in args {
                        let path = if arg.starts_with('~') || paths::is_absolute_like(&arg) {
                            paths::expand_tilde(&arg, self.home)
                        } else {
                            self.home.join(".ssh").join(&arg)
                        };
                        let found = paths::glob_paths(&path);
                        let found: Vec<PathBuf> =
                            found.into_iter().filter(|p| p.is_file()).collect();
                        if found.is_empty() && !paths::has_wildcard(&arg) {
                            self.warn(
                                WarningKind::MissingInclude,
                                source.clone(),
                                format!("included file {} does not exist", path.display()),
                            );
                        }
                        for inc in found {
                            if depth + 1 > MAX_DEPTH {
                                self.warn(
                                    WarningKind::IncludeLoop,
                                    source.clone(),
                                    format!("include of {} skipped (too deep)", inc.display()),
                                );
                                continue;
                            }
                            // Lines of the included file belong to the current block until
                            // it opens its own `Host`; afterwards we continue here.
                            self.parse_file(&inc, depth + 1, current);
                        }
                    }
                }
                _ => self.blocks[current].options.push(Opt {
                    keyword,
                    args,
                    source,
                }),
            }
        }
    }
}

fn host_matches(patterns: &[String], host: &str) -> bool {
    let mut matched = false;
    for p in patterns {
        if let Some(neg) = p.strip_prefix('!') {
            if wildcard_match(neg, host, true) {
                return false;
            }
        } else if wildcard_match(p, host, true) {
            matched = true;
        }
    }
    matched
}

#[derive(Default)]
struct Effective {
    host_name: Option<String>,
    user: Option<String>,
    port: Option<String>,
    identities_only: Option<String>,
    identity_files: Vec<(String, SourceRef)>,
}

impl Effective {
    fn apply(&mut self, opt: &Opt) {
        let first = || opt.args.first().cloned();
        match opt.keyword.as_str() {
            "hostname" => {
                self.host_name = self.host_name.take().or_else(first);
            }
            "user" => self.user = self.user.take().or_else(first),
            "port" => self.port = self.port.take().or_else(first),
            "identitiesonly" => {
                self.identities_only = self.identities_only.take().or_else(first);
            }
            "identityfile" => {
                if let Some(f) = first() {
                    // `IdentityFile none` disables keys.
                    if !f.eq_ignore_ascii_case("none") {
                        self.identity_files.push((f, opt.source.clone()));
                    }
                }
            }
            _ => {}
        }
    }
}

/// Expands ssh tokens we can know: `%d` home, `%h` host name, `%r` remote user, `%n` alias,
/// `%%`; `~` at the start.
fn expand_tokens(s: &str, home: &Path, host: &str, user: Option<&str>, alias: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('d') => out.push_str(&to_slash(home)),
            Some('h') => out.push_str(host),
            Some('n') => out.push_str(alias),
            Some('r') if user.is_some() => out.push_str(user.unwrap_or_default()),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    to_slash(&paths::expand_tilde(&out, home))
}

fn key_file(path: String, source: SourceRef) -> SuggestedKeyFile {
    let exists = Path::new(&path).is_file();
    let public = std::fs::read_to_string(format!("{path}.pub"))
        .ok()
        .and_then(|t| nanomgg_ssh_keys::parse_public_key(t.lines().next().unwrap_or("")).ok());
    SuggestedKeyFile {
        exists,
        fingerprint: public.as_ref().map(|p| p.fingerprint.clone()),
        public_key: public.as_ref().map(|p| p.public_key.clone()),
        algorithm: public.as_ref().map(|p| p.algorithm.clone()),
        comment: public
            .as_ref()
            .map(|p| p.comment.clone())
            .filter(|c| !c.is_empty()),
        path,
        source,
    }
}

fn parse_bool(v: Option<&str>) -> Option<bool> {
    match v?.to_ascii_lowercase().as_str() {
        "yes" | "true" => Some(true),
        "no" | "false" => Some(false),
        _ => None,
    }
}

fn suggestion(
    alias: &str,
    eff: &Effective,
    home: &Path,
    wildcard: bool,
    source: SourceRef,
) -> SuggestedSshHost {
    let host_name = eff
        .host_name
        .as_deref()
        .map(|h| h.replace("%h", alias).replace("%%", "%"))
        .unwrap_or_else(|| alias.to_string());
    let identity_files: Vec<SuggestedKeyFile> = eff
        .identity_files
        .iter()
        .map(|(f, src)| {
            key_file(
                expand_tokens(f, home, &host_name, eff.user.as_deref(), alias),
                src.clone(),
            )
        })
        .collect();
    let provider = if wildcard {
        ProviderKind::Unknown
    } else {
        ProviderKind::guess(&host_name)
    };
    let is_alias = !wildcard && !host_name.eq_ignore_ascii_case(alias);
    let mut notes = Vec::new();
    if is_alias && provider != ProviderKind::Unknown {
        notes.push(SuggestionNote::AliasUnnecessary);
    }
    if wildcard {
        notes.push(SuggestionNote::WildcardHost);
    }
    if identity_files.iter().any(|k| !k.exists) {
        notes.push(SuggestionNote::KeyFileMissing);
    }
    SuggestedSshHost {
        alias: alias.to_string(),
        host_name,
        user: eff.user.clone(),
        port: eff.port.as_deref().and_then(|p| p.parse().ok()),
        identity_files,
        identities_only: parse_bool(eff.identities_only.as_deref()),
        provider,
        is_alias,
        wildcard,
        notes,
        source,
    }
}

/// Scans `config` (default `<home>/.ssh/config`) and its includes.
pub fn scan_ssh_config(home: &Path, config: Option<&Path>) -> SshConfigScan {
    let default_path = home.join(".ssh").join("config");
    let path = config.unwrap_or(&default_path);
    let mut parser = Parser {
        home,
        blocks: vec![Block {
            patterns: vec!["*".into()],
            is_match: false,
            implicit: true,
            source: SourceRef::new(path, None),
            options: Vec::new(),
        }],
        warnings: Vec::new(),
        files: Vec::new(),
    };
    if path.is_file() {
        parser.parse_file(path, 0, 0);
    }

    let mut hosts = Vec::new();
    for block in parser.blocks.iter().filter(|b| !b.is_match) {
        let concrete: Vec<&String> = block
            .patterns
            .iter()
            .filter(|p| !p.starts_with('!') && !paths::has_wildcard(p))
            .collect();
        if concrete.is_empty() || block.implicit {
            // Wildcard defaults are only interesting when they name keys.
            let mut eff = Effective::default();
            block.options.iter().for_each(|o| eff.apply(o));
            if eff.identity_files.is_empty() {
                continue;
            }
            let source = if block.implicit {
                block.options[0].source.clone()
            } else {
                block.source.clone()
            };
            let alias = block
                .patterns
                .iter()
                .filter(|p| !p.starts_with('!'))
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            hosts.push(suggestion(&alias, &eff, home, true, source));
            continue;
        }
        for alias in concrete {
            let mut eff = Effective::default();
            for b in parser.blocks.iter().filter(|b| !b.is_match) {
                if host_matches(&b.patterns, alias) {
                    b.options.iter().for_each(|o| eff.apply(o));
                }
            }
            hosts.push(suggestion(alias, &eff, home, false, block.source.clone()));
        }
    }
    SshConfigScan {
        hosts,
        warnings: parser.warnings,
        files: parser.files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lines() {
        assert_eq!(
            split_line("  IdentityFile=\"C:\\Users\\An Nguyen\\.ssh\\id work\""),
            Some((
                "identityfile".into(),
                vec!["C:\\Users\\An Nguyen\\.ssh\\id work".into()]
            ))
        );
        assert_eq!(
            split_line("Host a b # comment"),
            Some(("host".into(), vec!["a".into(), "b".into()]))
        );
        assert_eq!(split_line("   # x"), None);
        assert_eq!(
            split_line("Port = 2222"),
            Some(("port".into(), vec!["2222".into()]))
        );
    }

    #[test]
    fn token_expansion() {
        let home = Path::new("/home/an");
        assert_eq!(
            expand_tokens("%d/.ssh/%h_%r_%x%%", home, "github.com", Some("git"), "gh"),
            "/home/an/.ssh/github.com_git_%x%"
        );
        assert_eq!(
            expand_tokens("~/.ssh/%n", home, "h", None, "alias"),
            "/home/an/.ssh/alias"
        );
    }

    #[test]
    fn negated_patterns() {
        let pats = vec!["*.corp".to_string(), "!bad.corp".to_string()];
        assert!(host_matches(&pats, "git.CORP"));
        assert!(!host_matches(&pats, "bad.corp"));
        assert!(!host_matches(&pats, "github.com"));
    }
}
