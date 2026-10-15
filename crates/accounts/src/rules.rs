//! Rule engine: which account a repo belongs to, judged from its remote URL and folder.
//!
//! Each [`Rule`] condition is optional and all given conditions must hold:
//! - `host`: glob on the remote host (`github.com`, `*.acme.dev`); with `:port` the
//!   non-default port must match too,
//! - `owner_or_group`: segment-wise prefix of the remote path, glob per segment, so `acme`
//!   matches `acme/api` and `acme/platform/api` (GitLab subgroups) but not `acme-labs/x`,
//! - `path_glob`: glob on the repo folder; a pattern without wildcards means "this folder and
//!   everything below it"; `~/` expands to the home folder,
//! - `remote_regex`: regex searched in the remote URL as configured (password removed).
//!
//! Rules are tried by ascending priority (then creation time, then id) and the first enabled
//! match wins. Host, owner and path comparisons ignore case where the platform does
//! (folders on Windows/macOS; hosts and owners always).

use regex::Regex;

use crate::glob;
use crate::model::{Rule, ValidationError};
use crate::url::{RemoteUrl, redact_url};

/// Platform knobs for path matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchOptions {
    /// Compare folders case-insensitively (Windows and macOS file systems).
    pub case_insensitive_paths: bool,
    /// Expansion for a leading `~/` in path globs.
    pub home_dir: Option<String>,
}

// On Linux the `cfg!` below is `false`, so clippy sees an all-default impl there; a derive
// would silently make paths case-sensitive on Windows and macOS.
#[allow(
    clippy::derivable_impls,
    reason = "the default depends on the target platform"
)]
impl Default for MatchOptions {
    fn default() -> Self {
        Self {
            case_insensitive_paths: cfg!(any(windows, target_os = "macos")),
            home_dir: None,
        }
    }
}

/// What a rule is matched against.
#[derive(Debug, Clone, Copy, Default)]
pub struct MatchInput<'a> {
    /// Repository root folder.
    pub repo_path: Option<&'a str>,
    /// Remote URL exactly as configured in git.
    pub remote_url: Option<&'a str>,
}

/// A rule with its patterns compiled.
#[derive(Debug, Clone)]
pub struct CompiledRule {
    pub rule: Rule,
    host: Option<Regex>,
    owner: Option<Regex>,
    path: Option<Regex>,
    remote: Option<Regex>,
}

fn normalize_path(p: &str) -> String {
    let p = p.trim().replace('\\', "/");
    let trimmed = p.trim_end_matches('/');
    if trimmed.is_empty() {
        p
    } else {
        trimmed.to_owned()
    }
}

impl CompiledRule {
    pub fn compile(rule: &Rule, opts: &MatchOptions) -> Result<Self, ValidationError> {
        let err =
            |field: &'static str| move |e: regex::Error| ValidationError::new(field, e.to_string());
        let host = rule
            .host
            .as_deref()
            .map(|h| glob::compile(h.trim(), true).map_err(err("host")))
            .transpose()?;
        let owner = rule
            .owner_or_group
            .as_deref()
            .map(|o| {
                let o = o.trim().trim_matches('/');
                glob::build(&format!("{}(?:/.*)?", glob::to_regex_source(o)), true)
                    .map_err(err("ownerOrGroup"))
            })
            .transpose()?;
        let path = rule
            .path_glob
            .as_deref()
            .map(|p| {
                let mut p = normalize_path(p);
                if let (Some(rest), Some(home)) = (p.strip_prefix("~/"), &opts.home_dir) {
                    p = format!("{}/{rest}", normalize_path(home));
                } else if p == "~"
                    && let Some(home) = &opts.home_dir
                {
                    p = normalize_path(home);
                }
                if !glob::has_wildcards(&p) {
                    p.push_str("/**");
                }
                glob::compile(&p, opts.case_insensitive_paths).map_err(err("pathGlob"))
            })
            .transpose()?;
        let remote = rule
            .remote_regex
            .as_deref()
            .map(|r| {
                regex::RegexBuilder::new(r)
                    .size_limit(1 << 20)
                    .build()
                    .map_err(err("remoteRegex"))
            })
            .transpose()?;
        Ok(Self {
            rule: rule.clone(),
            host,
            owner,
            path,
            remote,
        })
    }

    pub fn matches(&self, input: &MatchInput<'_>) -> bool {
        if !self.rule.enabled {
            return false;
        }
        let parsed = input
            .remote_url
            .and_then(RemoteUrl::parse)
            .filter(|u| !u.is_local());
        if let Some(re) = &self.host {
            let Some(u) = &parsed else { return false };
            let candidate = if self.rule.host.as_deref().is_some_and(|h| h.contains(':')) {
                u.host_port()
            } else {
                u.host.clone()
            };
            if !re.is_match(&candidate) {
                return false;
            }
        }
        if let Some(re) = &self.owner {
            let Some(u) = &parsed else { return false };
            if !re.is_match(&u.path) {
                return false;
            }
        }
        if let Some(re) = &self.path {
            let Some(path) = input.repo_path else {
                return false;
            };
            if !re.is_match(&normalize_path(path)) {
                return false;
            }
        }
        if let Some(re) = &self.remote {
            let Some(url) = input.remote_url else {
                return false;
            };
            if !re.is_match(&redact_url(url.trim())) {
                return false;
            }
        }
        true
    }
}

/// All rules, compiled and sorted by priority. Rules that fail to compile are kept aside in
/// [`invalid`](Self::invalid) instead of failing the whole set.
#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    rules: Vec<CompiledRule>,
    invalid: Vec<(String, ValidationError)>,
}

impl RuleSet {
    pub fn new(rules: impl IntoIterator<Item = Rule>, opts: &MatchOptions) -> Self {
        let mut rules: Vec<Rule> = rules.into_iter().collect();
        rules.sort_by(|a, b| {
            (a.priority, a.created_at, &a.id).cmp(&(b.priority, b.created_at, &b.id))
        });
        let mut set = Self::default();
        for rule in rules {
            match CompiledRule::compile(&rule, opts) {
                Ok(c) => set.rules.push(c),
                Err(e) => set.invalid.push((rule.id, e)),
            }
        }
        set
    }

    pub fn rules(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter().map(|c| &c.rule)
    }

    pub fn invalid(&self) -> &[(String, ValidationError)] {
        &self.invalid
    }

    /// Every matching rule, best first.
    pub fn matching<'s>(&'s self, input: &MatchInput<'_>) -> Vec<&'s Rule> {
        self.rules
            .iter()
            .filter(|c| c.matches(input))
            .map(|c| &c.rule)
            .collect()
    }

    pub fn first_match(&self, input: &MatchInput<'_>) -> Option<&Rule> {
        self.rules
            .iter()
            .find(|c| c.matches(input))
            .map(|c| &c.rule)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, priority: i64) -> Rule {
        Rule {
            id: id.into(),
            priority,
            host: None,
            owner_or_group: None,
            path_glob: None,
            remote_regex: None,
            account_id: format!("acc_{id}"),
            identity_id: None,
            enabled: true,
            created_at: 0,
        }
    }

    fn opts() -> MatchOptions {
        MatchOptions {
            case_insensitive_paths: true,
            home_dir: Some("C:\\Users\\me".into()),
        }
    }

    fn remote(url: &str) -> MatchInput<'_> {
        MatchInput {
            repo_path: None,
            remote_url: Some(url),
        }
    }

    fn one(r: Rule) -> CompiledRule {
        CompiledRule::compile(&r, &opts()).unwrap()
    }

    #[test]
    fn host_and_owner() {
        let mut r = rule("work", 0);
        r.host = Some("github.com".into());
        r.owner_or_group = Some("acme-corp".into());
        let c = one(r);
        assert!(c.matches(&remote("https://github.com/acme-corp/api.git")));
        assert!(c.matches(&remote("git@github.com:Acme-Corp/api.git")));
        assert!(c.matches(&remote("ssh://git@GITHUB.com:22/acme-corp/x")));
        assert!(!c.matches(&remote("https://github.com/acme-corporate/api.git")));
        assert!(!c.matches(&remote("https://gitlab.com/acme-corp/api.git")));
        assert!(!c.matches(&remote("https://github.com.evil.io/acme-corp/api.git")));
        assert!(!c.matches(&MatchInput::default()));
        assert!(!c.matches(&remote("C:/mirrors/acme-corp/api.git")));
    }

    #[test]
    fn gitlab_subgroups_and_globs() {
        let mut r = rule("g", 0);
        r.owner_or_group = Some("acme/platform".into());
        let c = one(r);
        assert!(c.matches(&remote("git@gitlab.com:acme/platform/backend/api.git")));
        assert!(c.matches(&remote("https://gitlab.com/acme/platform")));
        assert!(!c.matches(&remote("https://gitlab.com/acme/platform-x/api")));
        assert!(!c.matches(&remote("https://gitlab.com/acme/api")));

        let mut r = rule("g2", 0);
        r.owner_or_group = Some("acme-*".into());
        r.host = Some("*.acme.dev".into());
        let c = one(r);
        assert!(c.matches(&remote("https://git.acme.dev/acme-labs/x.git")));
        assert!(!c.matches(&remote("https://acme.dev/acme-labs/x.git")));
    }

    #[test]
    fn host_with_port() {
        let mut r = rule("p", 0);
        r.host = Some("gitlab.acme.dev:8443".into());
        let c = one(r);
        assert!(c.matches(&remote("https://gitlab.acme.dev:8443/a/b.git")));
        assert!(!c.matches(&remote("https://gitlab.acme.dev/a/b.git")));
        let mut r = rule("q", 0);
        r.host = Some("gitlab.acme.dev".into());
        let c = one(r);
        assert!(c.matches(&remote("ssh://git@gitlab.acme.dev:2222/a/b.git")));
    }

    #[test]
    fn path_globs() {
        let mut r = rule("d", 0);
        r.path_glob = Some("D:/Work/**".into());
        let c = one(r);
        fn at(p: &str) -> MatchInput<'_> {
            MatchInput {
                repo_path: Some(p),
                remote_url: None,
            }
        }
        assert!(c.matches(&at("D:\\Work\\acme\\api")));
        assert!(c.matches(&at("d:/work/x")));
        assert!(!c.matches(&at("D:/Workshop/x")));

        let mut r = rule("plain", 0);
        r.path_glob = Some("D:\\Work\\".into());
        let c = one(r);
        assert!(c.matches(&at("D:/Work")));
        assert!(c.matches(&at("D:/Work/a/b")));
        assert!(!c.matches(&at("D:/Work2")));

        let mut r = rule("home", 0);
        r.path_glob = Some("~/oss/*".into());
        let c = one(r);
        assert!(c.matches(&at("C:/Users/me/oss/lib")));
        assert!(!c.matches(&at("C:/Users/me/oss/lib/sub")));

        let mut r = rule("cs", 0);
        r.path_glob = Some("/home/Me/**".into());
        let c = CompiledRule::compile(
            &r,
            &MatchOptions {
                case_insensitive_paths: false,
                home_dir: None,
            },
        )
        .unwrap();
        assert!(!c.matches(&at("/home/me/x")));
        assert!(c.matches(&at("/home/Me/x")));
    }

    #[test]
    fn remote_regex_never_sees_passwords() {
        let mut r = rule("re", 0);
        r.remote_regex = Some(r"github\.com[:/]acme/".into());
        let c = one(r);
        assert!(c.matches(&remote("git@github.com:acme/api.git")));
        assert!(c.matches(&remote("https://github.com/acme/api.git")));
        assert!(!c.matches(&remote("https://github.com/other/api.git")));

        let mut r = rule("leak", 0);
        r.remote_regex = Some("ghp_".into());
        let c = one(r);
        assert!(!c.matches(&remote("https://me:ghp_abc@github.com/a/b.git")));
    }

    #[test]
    fn all_conditions_must_hold_and_disabled_rules_are_skipped() {
        let mut r = rule("both", 0);
        r.host = Some("github.com".into());
        r.path_glob = Some("D:/Work".into());
        let c = one(r.clone());
        let input = |p, u| MatchInput {
            repo_path: Some(p),
            remote_url: Some(u),
        };
        assert!(c.matches(&input("D:/Work/a", "https://github.com/x/y")));
        assert!(!c.matches(&input("E:/Other", "https://github.com/x/y")));
        assert!(!c.matches(&input("D:/Work/a", "https://gitlab.com/x/y")));
        r.enabled = false;
        assert!(!one(r).matches(&input("D:/Work/a", "https://github.com/x/y")));
    }

    #[test]
    fn rule_set_orders_by_priority_and_keeps_invalid_aside() {
        let mut a = rule("a", 5);
        a.host = Some("github.com".into());
        let mut b = rule("b", 1);
        b.owner_or_group = Some("acme".into());
        let mut c = rule("c", 1);
        c.created_at = -1;
        c.host = Some("github.com".into());
        let mut bad = rule("bad", 0);
        bad.remote_regex = Some("(".into());
        let set = RuleSet::new([a, b, c, bad], &opts());
        assert_eq!(
            set.rules().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["c", "b", "a"]
        );
        assert_eq!(set.invalid().len(), 1);
        let input = remote("https://github.com/acme/api");
        assert_eq!(set.first_match(&input).unwrap().id, "c");
        assert_eq!(
            set.matching(&input)
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            ["c", "b", "a"]
        );
        assert!(set.first_match(&remote("https://gitlab.com/x/y")).is_none());
    }
}
