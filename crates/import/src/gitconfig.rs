//! `~/.gitconfig` scanner: global identity, `includeIf` → per-directory / per-remote rules.
//!
//! Files are parsed by a built-in parser that follows git's `config.c` (sections, quoted
//! subsections, legacy `[a.b]`, quotes, escapes, line continuations, `#`/`;` comments, CRLF)
//! and records line numbers. When a git executable is available, `git config --file <f>
//! --list -z` is run as well and git's values are preferred whenever both parsers agree on the
//! sequence of keys (git stays the reference for quoting/escaping corner cases); a file git
//! rejects produces a warning.
//!
//! Includes follow git's rules: `include.path` is inlined where it appears, relative paths are
//! relative to the including file, `~/` is the home directory, missing files are ignored
//! (reported as warnings), nesting stops at depth 10.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::paths::{self, is_absolute_like, to_slash};
use crate::{ImportWarning, ScanEnv, SourceRef, SuggestionNote, WarningKind};

/// git's own limit (`MAX_INCLUDE_DEPTH`).
const MAX_INCLUDE_DEPTH: usize = 10;

/// One `name = value` of a config file, `name` as printed by `git config --list`
/// (`section[.subsection].key`, section and key lower-cased, subsection case kept).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEntry {
    pub name: String,
    /// `None` for a bare `key` (boolean true).
    pub value: Option<String>,
    /// 1-based line where the entry starts.
    pub line: u32,
}

impl ConfigEntry {
    /// Section (before the first dot).
    pub fn section(&self) -> &str {
        self.name.split('.').next().unwrap_or("")
    }

    /// Key (after the last dot).
    pub fn key(&self) -> &str {
        self.name.rsplit('.').next().unwrap_or("")
    }

    /// Subsection (between the first and the last dot), if any.
    pub fn subsection(&self) -> Option<&str> {
        let first = self.name.find('.')?;
        let last = self.name.rfind('.')?;
        (last > first).then(|| &self.name[first + 1..last])
    }
}

/// Result of [`parse_config`]: entries plus `(line, message)` syntax errors (the parser
/// skips the offending line and continues, git would reject the whole file).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedConfig {
    pub entries: Vec<ConfigEntry>,
    pub errors: Vec<(u32, String)>,
}

struct Cursor<'a> {
    chars: Vec<char>,
    pos: usize,
    line: u32,
    _text: std::marker::PhantomData<&'a str>,
}

impl Cursor<'_> {
    /// Next char with `\r\n` folded into `\n`; `None` at EOF.
    fn next(&mut self) -> Option<char> {
        let c = *self.chars.get(self.pos)?;
        self.pos += 1;
        if c == '\r' && self.chars.get(self.pos) == Some(&'\n') {
            self.pos += 1;
            self.line += 1;
            return Some('\n');
        }
        if c == '\n' {
            self.line += 1;
        }
        Some(c)
    }

    fn peek(&self) -> Option<char> {
        let c = *self.chars.get(self.pos)?;
        if c == '\r' && self.chars.get(self.pos + 1) == Some(&'\n') {
            return Some('\n');
        }
        Some(c)
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.next() {
            if c == '\n' {
                break;
            }
        }
    }
}

fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

/// Parses git config text.
pub fn parse_config(text: &str) -> ParsedConfig {
    let mut cur = Cursor {
        chars: text.trim_start_matches('\u{feff}').chars().collect(),
        pos: 0,
        line: 1,
        _text: std::marker::PhantomData,
    };
    let mut out = ParsedConfig::default();
    let mut section: Option<String> = None;
    while let Some(c) = cur.peek() {
        if c.is_whitespace() {
            cur.next();
            continue;
        }
        if c == '#' || c == ';' {
            cur.skip_line();
            continue;
        }
        let line = cur.line;
        if c == '[' {
            cur.next();
            match parse_header(&mut cur) {
                Ok(name) => section = Some(name),
                Err(msg) => {
                    out.errors.push((line, msg));
                    section = None;
                    if cur.line == line {
                        cur.skip_line();
                    }
                }
            }
            continue;
        }
        if !c.is_ascii_alphabetic() {
            out.errors
                .push((line, format!("unexpected character {c:?}")));
            cur.skip_line();
            continue;
        }
        let mut key = String::new();
        while let Some(c) = cur.peek().filter(|c| is_key_char(*c)) {
            key.push(c.to_ascii_lowercase());
            cur.next();
        }
        while matches!(cur.peek(), Some(' ' | '\t')) {
            cur.next();
        }
        let value = match cur.peek() {
            None | Some('\n') => {
                cur.next();
                None
            }
            Some('=') => {
                cur.next();
                match parse_value(&mut cur) {
                    Ok(v) => Some(v),
                    Err(msg) => {
                        out.errors.push((line, msg));
                        continue;
                    }
                }
            }
            Some(other) => {
                out.errors
                    .push((line, format!("unexpected {other:?} after key `{key}`")));
                cur.skip_line();
                continue;
            }
        };
        match &section {
            Some(s) => out.entries.push(ConfigEntry {
                name: format!("{s}.{key}"),
                value,
                line,
            }),
            None => out
                .errors
                .push((line, format!("key `{key}` outside of a section"))),
        }
    }
    out
}

/// After `[`: `section]`, `section "sub"]` or legacy `section.sub]` (all lower-cased).
fn parse_header(cur: &mut Cursor) -> Result<String, String> {
    let mut name = String::new();
    loop {
        match cur.peek() {
            None | Some('\n') => return Err("unterminated section header".into()),
            Some(']') => {
                cur.next();
                if name.is_empty() {
                    return Err("empty section name".into());
                }
                return Ok(name);
            }
            Some(c) if c == ' ' || c == '\t' => {
                break;
            }
            Some(c) if is_key_char(c) || c == '.' => {
                name.push(c.to_ascii_lowercase());
                cur.next();
            }
            Some(c) => return Err(format!("invalid character {c:?} in section name")),
        }
    }
    // Extended form: `[section "subsection"]`.
    while matches!(cur.peek(), Some(' ' | '\t')) {
        cur.next();
    }
    if cur.next() != Some('"') {
        return Err("expected `\"` in section header".into());
    }
    let mut sub = String::new();
    loop {
        match cur.next() {
            None | Some('\n') => return Err("unterminated subsection".into()),
            Some('"') => break,
            Some('\\') => match cur.next() {
                None | Some('\n') => return Err("unterminated subsection".into()),
                Some(c) => sub.push(c),
            },
            Some(c) => sub.push(c),
        }
    }
    if cur.next() != Some(']') {
        return Err("expected `]` after subsection".into());
    }
    Ok(format!("{name}.{sub}"))
}

/// After `=`: git's `parse_value`.
fn parse_value(cur: &mut Cursor) -> Result<String, String> {
    let mut value = String::new();
    let mut quote = false;
    let mut comment = false;
    let mut spaces = 0usize;
    loop {
        let Some(c) = cur.next() else {
            if quote {
                return Err("unterminated quoted value".into());
            }
            return Ok(value);
        };
        if c == '\n' {
            if quote {
                return Err("unterminated quoted value".into());
            }
            return Ok(value);
        }
        if comment {
            continue;
        }
        if c.is_whitespace() && !quote {
            if !value.is_empty() {
                spaces += 1;
            }
            continue;
        }
        if !quote && (c == ';' || c == '#') {
            comment = true;
            continue;
        }
        for _ in 0..spaces {
            value.push(' ');
        }
        spaces = 0;
        match c {
            '\\' => match cur.next() {
                Some('\n') => {}
                Some('t') => value.push('\t'),
                Some('b') => value.push('\u{8}'),
                Some('n') => value.push('\n'),
                Some(c @ ('\\' | '"')) => value.push(c),
                other => {
                    cur.skip_line();
                    return Err(format!("invalid escape {other:?}"));
                }
            },
            '"' => quote = !quote,
            c => value.push(c),
        }
    }
}

/// `git config --file <file> --list -z` → `(name, value)` pairs.
fn git_list(git: &Path, file: &Path) -> Result<Vec<(String, Option<String>)>, String> {
    let mut cmd = Command::new(git);
    cmd.arg("config")
        .arg("--file")
        .arg(file)
        .args(["--list", "-z"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("cannot run git: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Ok(text
        .split('\0')
        .filter(|r| !r.is_empty())
        .map(|r| match r.split_once('\n') {
            Some((k, v)) => (k.to_string(), Some(v.to_string())),
            None => (r.to_string(), None),
        })
        .collect())
}

// ---------------------------------------------------------------------------------------
// Suggestions

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SigningFormat {
    /// `gpg.format = openpgp` (git's default).
    Gpg,
    Ssh,
    X509,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SigningSuggestion {
    pub format: SigningFormat,
    /// `user.signingkey` as written (GPG key id, `~/.ssh/id.pub`, `key::ssh-ed25519 …`).
    pub key: Option<String>,
    /// For SSH signing with a key file path: the expanded path (forward slashes).
    pub key_path: Option<String>,
    /// `commit.gpgsign`.
    pub sign_commits: bool,
    /// `tag.gpgsign`.
    pub sign_tags: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum IdentityScope {
    /// From the global config (applies everywhere unless a rule matches).
    Global,
    /// From a file included by `includeIf` — see the rule pointing at it.
    Conditional,
}

/// A commit identity found in the config (PLAN.md 4.1 `Identity`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SuggestedIdentity {
    pub name: Option<String>,
    pub email: Option<String>,
    pub signing: Option<SigningSuggestion>,
    /// `core.sshCommand` of that scope (common multi-account trick).
    pub ssh_command: Option<String>,
    /// Key passed with `-i` / `IdentityFile=` in `core.sshCommand`, expanded.
    pub ssh_key_path: Option<String>,
    pub scope: IdentityScope,
    /// Raw `includeIf` condition for conditional identities.
    pub condition: Option<String>,
    /// Where `user.email` (else `user.name`, else the first value) is defined.
    pub source: SourceRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum RuleKind {
    /// `gitdir:` / `gitdir/i:` → repository path glob.
    PathGlob,
    /// `hasconfig:remote.*.url:` → remote URL pattern.
    RemoteUrl,
}

/// A rule suggestion (PLAN.md 4.1 `Rule`): "repos matching X use identity Y".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SuggestedRule {
    pub kind: RuleKind,
    /// Raw condition, e.g. `gitdir/i:~/work/`.
    pub condition: String,
    /// Glob on the working tree path, `~` expanded, forward slashes: `C:/Users/an/work/**`.
    pub path_glob: Option<String>,
    /// The pattern git matches against the `.git` directory (after git's own expansion).
    pub gitdir_pattern: Option<String>,
    /// `gitdir/i:` (case-insensitive match).
    pub case_insensitive: bool,
    /// `hasconfig:remote.*.url:` glob as written.
    pub remote_url_glob: Option<String>,
    /// Anchored regex equivalent of `remote_url_glob`.
    pub remote_regex: Option<String>,
    /// Host / first path segment when the URL glob pins them (`github.com`, `acme`).
    pub host: Option<String>,
    pub owner_or_group: Option<String>,
    /// Index into [`crate::ImportReport::identities`] (or [`GitConfigScan::identities`]).
    pub identity_index: Option<u32>,
    /// The included file that carries the identity.
    pub include_file: String,
    pub notes: Vec<SuggestionNote>,
    /// The `path = …` line of the `includeIf` block.
    pub source: SourceRef,
}

/// Result of the git config scan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GitConfigScan {
    pub identities: Vec<SuggestedIdentity>,
    pub rules: Vec<SuggestedRule>,
    pub warnings: Vec<ImportWarning>,
    pub files: Vec<String>,
}

/// Global config files in git's reading order (later wins): XDG `git/config`, then
/// `~/.gitconfig`; only `GIT_CONFIG_GLOBAL` when set.
pub fn global_config_files(home: &Path, env: &ScanEnv) -> Vec<PathBuf> {
    if let Some(g) = &env.git_config_global {
        return vec![g.clone()];
    }
    let xdg = env
        .xdg_config_home
        .clone()
        .unwrap_or_else(|| home.join(".config"))
        .join("git")
        .join("config");
    vec![xdg, home.join(".gitconfig")]
}

/// Scans the global git config of `home`.
pub fn scan_gitconfig(home: &Path, env: &ScanEnv) -> GitConfigScan {
    scan_gitconfig_files(&global_config_files(home, env), home, env.git.as_deref())
}

#[derive(Debug, Clone)]
struct FlatEntry {
    entry: ConfigEntry,
    file: PathBuf,
}

#[derive(Debug, Clone)]
struct CondInclude {
    condition: String,
    target: PathBuf,
    /// Directory of the including file (for `./` in `gitdir:`).
    base_dir: PathBuf,
    source: SourceRef,
    depth: usize,
    nested: bool,
}

struct Walker<'a> {
    home: &'a Path,
    git: Option<&'a Path>,
    cache: HashMap<PathBuf, Option<Vec<ConfigEntry>>>,
    warnings: Vec<ImportWarning>,
    files: Vec<String>,
}

impl Walker<'_> {
    fn warn(&mut self, kind: WarningKind, file: &Path, line: Option<u32>, message: String) {
        self.warnings.push(ImportWarning {
            kind,
            source: SourceRef::new(file, line),
            message,
        });
    }

    /// Parsed entries of a file (`None` if missing/unreadable), cached.
    fn load(&mut self, file: &Path) -> Option<Vec<ConfigEntry>> {
        if let Some(cached) = self.cache.get(file) {
            return cached.clone();
        }
        let result = if file.is_file() {
            match paths::read_text(file) {
                Ok(text) => {
                    self.files.push(file.to_string_lossy().into_owned());
                    let parsed = parse_config(&text);
                    for (line, msg) in &parsed.errors {
                        self.warn(WarningKind::Parse, file, Some(*line), msg.clone());
                    }
                    let mut entries = parsed.entries;
                    if let Some(git) = self.git {
                        match git_list(git, file) {
                            Ok(list) => {
                                let same = list.len() == entries.len()
                                    && list.iter().zip(&entries).all(|((k, _), e)| *k == e.name);
                                if same {
                                    for ((_, v), e) in list.into_iter().zip(entries.iter_mut()) {
                                        e.value = v;
                                    }
                                } else if parsed.errors.is_empty() {
                                    self.warn(
                                        WarningKind::Parse,
                                        file,
                                        None,
                                        "built-in parser and git disagree; using the built-in result"
                                            .into(),
                                    );
                                }
                            }
                            Err(msg) if !msg.starts_with("cannot run git") => {
                                self.warn(WarningKind::Parse, file, None, msg);
                            }
                            Err(_) => {}
                        }
                    }
                    Some(entries)
                }
                Err(msg) => {
                    self.warn(WarningKind::Unreadable, file, None, msg);
                    None
                }
            }
        } else {
            None
        };
        self.cache.insert(file.to_path_buf(), result.clone());
        result
    }

    /// Entries of `file` with unconditional includes inlined; conditional includes collected.
    fn flatten(
        &mut self,
        file: &Path,
        depth: usize,
        chain: &mut Vec<PathBuf>,
        nested: bool,
        out: &mut Vec<FlatEntry>,
        conds: &mut Vec<CondInclude>,
    ) {
        let Some(entries) = self.load(file) else {
            return;
        };
        chain.push(file.to_path_buf());
        let base_dir = file.parent().unwrap_or(Path::new("")).to_path_buf();
        for entry in entries {
            let is_path = entry.key() == "path";
            let section = entry.section().to_string();
            if is_path && section == "include" && entry.subsection().is_none() {
                let Some(raw) = entry.value.clone().filter(|v| !v.is_empty()) else {
                    continue;
                };
                let target = paths::normalize(&paths::resolve(&raw, self.home, &base_dir));
                if !self.check_include(file, &entry, &target, depth, chain) {
                    continue;
                }
                self.flatten(&target, depth + 1, chain, nested, out, conds);
            } else if is_path && section == "includeif" {
                let (Some(condition), Some(raw)) = (
                    entry.subsection().map(str::to_string),
                    entry.value.clone().filter(|v| !v.is_empty()),
                ) else {
                    continue;
                };
                let target = paths::normalize(&paths::resolve(&raw, self.home, &base_dir));
                if !self.check_include(file, &entry, &target, depth, chain) {
                    continue;
                }
                conds.push(CondInclude {
                    condition,
                    target,
                    base_dir: base_dir.clone(),
                    source: SourceRef::new(file, Some(entry.line)),
                    depth: depth + 1,
                    nested,
                });
            } else {
                out.push(FlatEntry {
                    entry,
                    file: file.to_path_buf(),
                });
            }
        }
        chain.pop();
    }

    fn check_include(
        &mut self,
        file: &Path,
        entry: &ConfigEntry,
        target: &Path,
        depth: usize,
        chain: &[PathBuf],
    ) -> bool {
        if depth + 1 > MAX_INCLUDE_DEPTH || chain.iter().any(|p| p == target) {
            self.warn(
                WarningKind::IncludeLoop,
                file,
                Some(entry.line),
                format!(
                    "include of {} skipped (cycle or too deep)",
                    target.display()
                ),
            );
            return false;
        }
        if !target.is_file() {
            self.warn(
                WarningKind::MissingInclude,
                file,
                Some(entry.line),
                format!("included file {} does not exist", target.display()),
            );
            return false;
        }
        true
    }
}

/// Scans explicit global config files (in git order) and everything they include.
pub fn scan_gitconfig_files(files: &[PathBuf], home: &Path, git: Option<&Path>) -> GitConfigScan {
    let mut walker = Walker {
        home,
        git,
        cache: HashMap::new(),
        warnings: Vec::new(),
        files: Vec::new(),
    };
    let mut global = Vec::new();
    let mut conds = Vec::new();
    for file in files {
        walker.flatten(file, 0, &mut Vec::new(), false, &mut global, &mut conds);
    }

    let mut scan = GitConfigScan::default();
    if let Some(identity) = identity_from(&global, home, IdentityScope::Global, None) {
        scan.identities.push(identity);
    }

    let mut seen = std::collections::HashSet::new();
    let mut i = 0;
    while i < conds.len() {
        let cond = conds[i].clone();
        i += 1;
        // A file included by several conditions yields its nested includes once.
        if !seen.insert((cond.source.clone(), cond.condition.clone())) {
            continue;
        }
        let Some(mut rule) = rule_for(&cond, home) else {
            let file = PathBuf::from(&cond.source.file);
            walker.warn(
                WarningKind::Unsupported,
                &file,
                cond.source.line,
                format!("includeIf condition `{}` is not imported", cond.condition),
            );
            continue;
        };
        let mut entries = Vec::new();
        let mut nested_conds = Vec::new();
        walker.flatten(
            &cond.target,
            cond.depth,
            &mut Vec::new(),
            true,
            &mut entries,
            &mut nested_conds,
        );
        conds.extend(nested_conds);
        if let Some(identity) = identity_from(
            &entries,
            home,
            IdentityScope::Conditional,
            Some(cond.condition.clone()),
        ) {
            rule.identity_index = Some(u32::try_from(scan.identities.len()).unwrap_or(u32::MAX));
            scan.identities.push(identity);
        }
        scan.rules.push(rule);
    }
    scan.warnings = walker.warnings;
    scan.files = walker.files;
    scan
}

fn git_bool(value: Option<&str>) -> bool {
    match value {
        None => true,
        Some(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "true" | "yes" | "on" | "1"
        ),
    }
}

fn identity_from(
    entries: &[FlatEntry],
    home: &Path,
    scope: IdentityScope,
    condition: Option<String>,
) -> Option<SuggestedIdentity> {
    let last = |name: &str| entries.iter().rev().find(|e| e.entry.name == name);
    let value = |name: &str| last(name).and_then(|e| e.entry.value.clone());
    let name = value("user.name").filter(|v| !v.is_empty());
    let email = value("user.email").filter(|v| !v.is_empty());
    let signing_key = value("user.signingkey").filter(|v| !v.is_empty());
    let format = value("gpg.format");
    let sign_commits = last("commit.gpgsign").map(|e| git_bool(e.entry.value.as_deref()));
    let sign_tags = last("tag.gpgsign").map(|e| git_bool(e.entry.value.as_deref()));
    let ssh_command = value("core.sshcommand").filter(|v| !v.is_empty());

    let signing =
        (signing_key.is_some() || sign_commits == Some(true) || format.is_some()).then(|| {
            let format = match format.as_deref().map(str::to_ascii_lowercase).as_deref() {
                Some("ssh") => SigningFormat::Ssh,
                Some("x509") => SigningFormat::X509,
                _ => SigningFormat::Gpg,
            };
            let key_path = signing_key
                .as_deref()
                .filter(|k| format == SigningFormat::Ssh && !k.starts_with("key::"))
                .map(|k| to_slash(&paths::expand_tilde(k, home)));
            SigningSuggestion {
                format,
                key: signing_key.clone(),
                key_path,
                sign_commits: sign_commits.unwrap_or(false),
                sign_tags: sign_tags.unwrap_or(false),
            }
        });
    if name.is_none() && email.is_none() && signing.is_none() && ssh_command.is_none() {
        return None;
    }
    let source = [
        "user.email",
        "user.name",
        "user.signingkey",
        "core.sshcommand",
    ]
    .iter()
    .find_map(|n| last(n))
    .or_else(|| entries.first())
    .map(|e| SourceRef::new(&e.file, Some(e.entry.line)))?;
    let ssh_key_path = ssh_command
        .as_deref()
        .and_then(ssh_key_from_command)
        .map(|k| to_slash(&paths::expand_tilde(&k, home)));
    Some(SuggestedIdentity {
        name,
        email,
        signing,
        ssh_command,
        ssh_key_path,
        scope,
        condition,
        source,
    })
}

/// Splits a command line like `sh` would (quotes and backslashes; no expansion).
pub fn shell_words(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                for c in chars.by_ref() {
                    if c == '\'' {
                        break;
                    }
                    cur.push(c);
                }
            }
            '"' => {
                in_word = true;
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' if matches!(chars.peek(), Some('"' | '\\' | '$' | '`')) => {
                            cur.push(chars.next().unwrap_or('\\'));
                        }
                        c => cur.push(c),
                    }
                }
            }
            // A backslash before a non-special char is kept: Windows paths (`C:\Users`) in
            // `core.sshCommand` are common and git passes them to `sh` unchanged anyway.
            '\\' if matches!(chars.peek(), Some(' ' | '"' | '\'' | '\\')) => {
                in_word = true;
                cur.push(chars.next().unwrap_or('\\'));
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    words
}

/// Key path from `ssh -i <key>` / `-i<key>` / `-o IdentityFile=<key>`.
pub fn ssh_key_from_command(command: &str) -> Option<String> {
    let words = shell_words(command);
    let mut it = words.iter().skip(1);
    while let Some(w) = it.next() {
        if w == "-i" {
            return it.next().cloned();
        }
        if let Some(rest) = w.strip_prefix("-i") {
            return Some(rest.to_string());
        }
        let opt = if w == "-o" {
            it.next().map(String::as_str)
        } else {
            w.strip_prefix("-o")
        };
        if let Some(opt) = opt {
            let (k, v) = opt
                .split_once('=')
                .or_else(|| opt.split_once(' '))
                .unwrap_or((opt, ""));
            if k.trim().eq_ignore_ascii_case("identityfile") {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

fn rule_for(cond: &CondInclude, home: &Path) -> Option<SuggestedRule> {
    let mut rule = SuggestedRule {
        kind: RuleKind::PathGlob,
        condition: cond.condition.clone(),
        path_glob: None,
        gitdir_pattern: None,
        case_insensitive: false,
        remote_url_glob: None,
        remote_regex: None,
        host: None,
        owner_or_group: None,
        identity_index: None,
        include_file: cond.target.to_string_lossy().into_owned(),
        notes: if cond.nested {
            vec![SuggestionNote::NestedCondition]
        } else {
            Vec::new()
        },
        source: cond.source.clone(),
    };
    let (kind, pattern) = cond.condition.split_once(':')?;
    match kind {
        "gitdir" | "gitdir/i" => {
            let gitdir = gitdir_pattern(pattern, home, &cond.base_dir);
            rule.case_insensitive = kind == "gitdir/i";
            rule.path_glob = Some(worktree_glob(&gitdir));
            rule.gitdir_pattern = Some(gitdir);
        }
        "hasconfig" => {
            let glob = pattern.strip_prefix("remote.*.url:")?;
            rule.kind = RuleKind::RemoteUrl;
            rule.remote_regex = Some(glob_to_regex(glob));
            let (host, owner) = host_owner_from_url_glob(glob);
            rule.host = host;
            rule.owner_or_group = owner;
            rule.remote_url_glob = Some(glob.to_string());
        }
        _ => return None,
    }
    Some(rule)
}

/// The pattern git really matches for `gitdir:` (git's `include_by_gitdir`): `~/` → home,
/// `./` → directory of the including file, relative patterns get `**/`, a trailing `/` gets
/// `**`. Backslashes become `/`.
pub fn gitdir_pattern(pattern: &str, home: &Path, base_dir: &Path) -> String {
    let p = pattern.replace('\\', "/");
    let mut p = if let Some(rest) = p.strip_prefix("~/") {
        format!("{}/{rest}", to_slash(home).trim_end_matches('/'))
    } else if let Some(rest) = p.strip_prefix("./") {
        format!("{}/{rest}", to_slash(base_dir).trim_end_matches('/'))
    } else {
        p
    };
    if !is_absolute_like(&p) && !p.starts_with("**/") {
        p = format!("**/{p}");
    }
    if p.ends_with('/') {
        p.push_str("**");
    }
    p
}

/// Working-tree glob for a gitdir pattern: `…/proj/.git` → `…/proj`.
fn worktree_glob(gitdir: &str) -> String {
    gitdir
        .strip_suffix("/.git")
        .or_else(|| gitdir.strip_suffix("/.git/"))
        .unwrap_or(gitdir)
        .to_string()
}

/// wildmatch (with `WM_PATHNAME`, as used by `hasconfig:remote.*.url:`) → anchored regex.
pub fn glob_to_regex(glob: &str) -> String {
    let mut re = String::from("^");
    let chars: Vec<char> = glob.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '*' if chars.get(i + 1) == Some(&'*') => {
                i += 1;
                if chars.get(i + 1) == Some(&'/') {
                    i += 1;
                    re.push_str("(?:.*/)?");
                } else {
                    re.push_str(".*");
                }
            }
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            c if "\\.+()|[]{}^$#&-~".contains(c) => {
                re.push('\\');
                re.push(c);
            }
            c => re.push(c),
        }
        i += 1;
    }
    re.push('$');
    re
}

/// Host and owner pinned by a URL glob (`https://github.com/acme/**`,
/// `git@gitlab.com:group/**`, `ssh://git@host:2222/g/**`).
pub fn host_owner_from_url_glob(glob: &str) -> (Option<String>, Option<String>) {
    let (rest, has_scheme) = match glob.find("://") {
        Some(i) => (&glob[i + 3..], true),
        None => (glob, false),
    };
    let rest = match rest.find('@') {
        Some(i) if !rest[..i].contains('/') => &rest[i + 1..],
        _ => rest,
    };
    let end = rest.find([':', '/']).unwrap_or(rest.len());
    let host = &rest[..end];
    let mut path = &rest[end..];
    if has_scheme && let Some(after) = path.strip_prefix(':') {
        // `host:port/path`
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        path = &after[digits..];
    }
    let path = path.trim_start_matches([':', '/']);
    let host = (!host.is_empty() && !paths::has_wildcard(host) && !host.contains('['))
        .then(|| host.to_ascii_lowercase());
    let owner = path
        .split('/')
        .next()
        .filter(|s| !s.is_empty() && !paths::has_wildcard(s) && !s.contains('['))
        .filter(|_| path.contains('/') && host.is_some())
        .map(str::to_string);
    (host, owner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(p: &ParsedConfig) -> Vec<(String, Option<String>, u32)> {
        p.entries
            .iter()
            .map(|e| (e.name.clone(), e.value.clone(), e.line))
            .collect()
    }

    #[test]
    fn parses_git_syntax() {
        let text = "\u{feff}# comment\r\n\
            [user]\r\n\
            \tname = \"Nguyễn Văn An\"  ; trailing comment\r\n\
            \temail=an@acme.vn\r\n\
            [core] sshCommand = ssh -i \"C:\\\\Users\\\\An\\\\.ssh\\\\id work\" \r\n\
            [includeIf \"gitdir/i:~/Work/\"]\n\
            \tpath = .gitconfig-work\n\
            [Remote.Origin]\n\
            \turl = a\\\n   b\n\
            [x]\n\
            \tflag\n\
            \tspaced = a   b\t c  \n\
            \tquoted = \" x # y \"\n\
            \tesc = a\\tb\\nc\n";
        let p = parse_config(text);
        assert!(p.errors.is_empty(), "{:?}", p.errors);
        assert_eq!(
            names(&p),
            vec![
                ("user.name".into(), Some("Nguyễn Văn An".into()), 3),
                ("user.email".into(), Some("an@acme.vn".into()), 4),
                // git quotes are syntax (removed), `\\` is an escaped backslash.
                (
                    "core.sshcommand".into(),
                    Some("ssh -i C:\\Users\\An\\.ssh\\id work".into()),
                    5
                ),
                (
                    "includeif.gitdir/i:~/Work/.path".into(),
                    Some(".gitconfig-work".into()),
                    7
                ),
                ("remote.origin.url".into(), Some("a   b".into()), 9),
                ("x.flag".into(), None, 12),
                ("x.spaced".into(), Some("a   b  c".into()), 13),
                ("x.quoted".into(), Some(" x # y ".into()), 14),
                ("x.esc".into(), Some("a\tb\nc".into()), 15),
            ]
        );
        let e = &p.entries[3];
        assert_eq!(e.section(), "includeif");
        assert_eq!(e.subsection(), Some("gitdir/i:~/Work/"));
        assert_eq!(e.key(), "path");
    }

    #[test]
    fn reports_errors_and_continues() {
        let p = parse_config("[user\nname = x\n[ok]\nk = \"open\nz = 1\nbad\\x\n");
        assert_eq!(names(&p), vec![("ok.z".into(), Some("1".into()), 5)]);
        let lines: Vec<u32> = p.errors.iter().map(|(l, _)| *l).collect();
        assert_eq!(lines, [1, 2, 4, 6]);
    }

    #[test]
    fn gitdir_patterns() {
        let home = Path::new("C:\\Users\\an");
        let base = Path::new("C:\\Users\\an\\cfg");
        assert_eq!(gitdir_pattern("~/work/", home, base), "C:/Users/an/work/**");
        assert_eq!(
            gitdir_pattern("./repos/", home, base),
            "C:/Users/an/cfg/repos/**"
        );
        assert_eq!(gitdir_pattern("D:\\Work\\", home, base), "D:/Work/**");
        assert_eq!(gitdir_pattern("oss/", home, base), "**/oss/**");
        assert_eq!(gitdir_pattern("**/oss/", home, base), "**/oss/**");
        assert_eq!(gitdir_pattern("/srv/x/.git", home, base), "/srv/x/.git");
        assert_eq!(worktree_glob("/srv/x/.git"), "/srv/x");
    }

    #[test]
    fn url_globs() {
        assert_eq!(
            host_owner_from_url_glob("https://github.com/acme/**"),
            (Some("github.com".into()), Some("acme".into()))
        );
        assert_eq!(
            host_owner_from_url_glob("git@GitLab.com:group/**"),
            (Some("gitlab.com".into()), Some("group".into()))
        );
        assert_eq!(
            host_owner_from_url_glob("ssh://git@git.acme.vn:2222/team/**"),
            (Some("git.acme.vn".into()), Some("team".into()))
        );
        assert_eq!(
            host_owner_from_url_glob("*://github.com/**"),
            (Some("github.com".into()), None)
        );
        assert_eq!(host_owner_from_url_glob("**/acme/**"), (None, None));
        assert_eq!(
            glob_to_regex("https://github.com/acme/**"),
            "^https://github\\.com/acme/.*$"
        );
        assert_eq!(
            glob_to_regex("git@*:a?/**/x"),
            "^git@[^/]*:a[^/]/(?:.*/)?x$"
        );
    }

    #[test]
    fn ssh_command_keys() {
        assert_eq!(
            ssh_key_from_command("ssh -i ~/.ssh/id_work -o IdentitiesOnly=yes").as_deref(),
            Some("~/.ssh/id_work")
        );
        assert_eq!(
            ssh_key_from_command("ssh -i \"C:\\Users\\An\\.ssh\\id work\"").as_deref(),
            Some("C:\\Users\\An\\.ssh\\id work")
        );
        assert_eq!(
            ssh_key_from_command("ssh -o 'IdentityFile=/k/a b' -F /dev/null").as_deref(),
            Some("/k/a b")
        );
        assert_eq!(ssh_key_from_command("ssh -i/k/x").as_deref(), Some("/k/x"));
        assert_eq!(ssh_key_from_command("ssh -v"), None);
    }
}
