//! Account-context resolution for a repository (PLAN §4.3).
//!
//! For each remote, the first candidate that applies wins:
//! 1. a **locked** binding for that remote, then a locked repo-level binding;
//! 2. an unlocked **manual** or **clone** binding for that remote, then at repo level;
//! 3. the **rule engine** (priority order) on the remote URL + repo folder;
//! 4. a remembered `auto_rule` binding (remote, then repo level);
//! 5. nothing: the repo is "unbound" — the UI shows a banner and git uses its own config.
//!
//! Repo-level bindings and rules *without* a host condition are generic: they are skipped for a
//! remote whose host differs from the account's provider host (an `upstream` on github.com must
//! not get the company GitLab account just because the repo default says so). Bindings made for
//! that exact remote and rules naming a host always apply.
//!
//! The repo **default** (commit identity, commands not tied to a remote) is: repo-level locked
//! binding → repo-level manual/clone binding → the primary remote's resolution (`origin`, else
//! the first remote) → rules that only need the folder → repo-level `auto_rule` binding.

use serde::Serialize;

use crate::model::{Account, BindingSource, Identity, Provider, RepoBinding};
use crate::rules::{MatchInput, RuleSet};
use crate::url::{RemoteUrl, hostname_of};

/// Where a resolved account came from, for the account pill and banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ContextSource {
    Locked,
    Manual,
    Clone,
    Rule,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    pub account_id: String,
    pub identity_id: Option<String>,
    pub source: ContextSource,
    /// Rule that matched when `source` is `rule` and it came from the engine.
    pub rule_id: Option<String>,
    /// The binding used, if any: its remote (`None` = repo-level binding).
    pub binding_remote: Option<String>,
    /// Whether a binding was used (distinguishes a repo-level binding from a rule).
    pub from_binding: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteInfo {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RemoteResolution {
    pub remote: String,
    pub url: String,
    pub parsed: Option<RemoteUrl>,
    pub resolved: Option<Resolved>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Resolution {
    pub default: Option<Resolved>,
    pub remotes: Vec<RemoteResolution>,
}

impl Resolution {
    /// The account for a command on `remote` (`None` or an unknown remote → repo default).
    pub fn for_remote(&self, remote: Option<&str>) -> Option<&Resolved> {
        match remote.and_then(|name| self.remotes.iter().find(|r| r.remote == name)) {
            Some(r) => r.resolved.as_ref(),
            None => self.default.as_ref(),
        }
    }
}

/// Accounts, providers and identities known to the app.
#[derive(Debug, Clone, Copy)]
pub struct AccountDirectory<'a> {
    pub accounts: &'a [Account],
    pub providers: &'a [Provider],
    pub identities: &'a [Identity],
}

impl<'a> AccountDirectory<'a> {
    pub fn account(&self, id: &str) -> Option<&'a Account> {
        self.accounts.iter().find(|a| a.id == id)
    }

    pub fn provider_of(&self, account_id: &str) -> Option<&'a Provider> {
        let account = self.account(account_id)?;
        self.providers.iter().find(|p| p.id == account.provider_id)
    }

    /// `host[:port]` of the account's provider.
    pub fn account_host(&self, account_id: &str) -> Option<String> {
        self.provider_of(account_id)?.host()
    }

    /// `preferred` if it belongs to the account, else its default identity, else its oldest.
    pub fn identity_for(&self, account_id: &str, preferred: Option<&str>) -> Option<&'a Identity> {
        let mine = || {
            self.identities
                .iter()
                .filter(move |i| i.account_id == account_id)
        };
        preferred
            .and_then(|id| mine().find(|i| i.id == id))
            .or_else(|| mine().find(|i| i.is_default))
            .or_else(|| mine().min_by(|a, b| (a.created_at, &a.id).cmp(&(b.created_at, &b.id))))
    }
}

struct Ctx<'a> {
    repo_path: &'a str,
    bindings: &'a [RepoBinding],
    rules: &'a RuleSet,
    dir: &'a AccountDirectory<'a>,
}

impl Ctx<'_> {
    fn resolved_from(&self, b: &RepoBinding) -> Resolved {
        Resolved {
            account_id: b.account_id.clone(),
            identity_id: self
                .dir
                .identity_for(&b.account_id, b.identity_id.as_deref())
                .map(|i| i.id.clone()),
            source: if b.locked {
                ContextSource::Locked
            } else {
                match b.source {
                    BindingSource::Manual => ContextSource::Manual,
                    BindingSource::Clone => ContextSource::Clone,
                    BindingSource::AutoRule => ContextSource::Rule,
                }
            },
            rule_id: None,
            binding_remote: b.remote_name.clone(),
            from_binding: true,
        }
    }

    /// Bindings of one level (`remote` = `None` for repo level) passing `keep`.
    fn binding(
        &self,
        remote: Option<&str>,
        keep: impl Fn(&RepoBinding) -> bool,
    ) -> Option<&RepoBinding> {
        self.bindings.iter().find(|b| {
            b.remote_name.as_deref() == remote
                && self.dir.account(&b.account_id).is_some()
                && keep(b)
        })
    }

    /// Whether a generic candidate (repo-level binding, host-less rule) may serve `url`.
    fn host_compatible(&self, url: Option<&RemoteUrl>, account_id: &str) -> bool {
        match (
            url.filter(|u| !u.is_local()),
            self.dir.account_host(account_id),
        ) {
            (Some(u), Some(provider_host)) => hostname_of(&provider_host) == u.host,
            _ => true,
        }
    }

    fn rule_match(&self, input: MatchInput<'_>, url: Option<&RemoteUrl>) -> Option<Resolved> {
        self.rules
            .matching(&input)
            .into_iter()
            .find(|r| {
                self.dir.account(&r.account_id).is_some()
                    && (r.host.is_some() || self.host_compatible(url, &r.account_id))
            })
            .map(|r| Resolved {
                account_id: r.account_id.clone(),
                identity_id: self
                    .dir
                    .identity_for(&r.account_id, r.identity_id.as_deref())
                    .map(|i| i.id.clone()),
                source: ContextSource::Rule,
                rule_id: Some(r.id.clone()),
                binding_remote: None,
                from_binding: false,
            })
    }

    fn remote(&self, remote: &RemoteInfo) -> Option<Resolved> {
        let url = RemoteUrl::parse(&remote.url);
        let url = url.as_ref();
        let name = Some(remote.name.as_str());
        let generic_ok = |b: &RepoBinding| self.host_compatible(url, &b.account_id);
        let explicit = |b: &RepoBinding| b.source != BindingSource::AutoRule;

        self.binding(name, |b| b.locked)
            .or_else(|| self.binding(None, |b| b.locked && generic_ok(b)))
            .or_else(|| self.binding(name, explicit))
            .or_else(|| self.binding(None, |b| explicit(b) && generic_ok(b)))
            .map(|b| self.resolved_from(b))
            .or_else(|| {
                self.rule_match(
                    MatchInput {
                        repo_path: Some(self.repo_path),
                        remote_url: Some(&remote.url),
                    },
                    url,
                )
            })
            .or_else(|| {
                self.binding(name, |_| true)
                    .or_else(|| self.binding(None, generic_ok))
                    .map(|b| self.resolved_from(b))
            })
    }
}

/// Resolves the account for the repo at `repo_path` and each of its `remotes`.
pub fn resolve(
    repo_path: &str,
    remotes: &[RemoteInfo],
    bindings: &[RepoBinding],
    rules: &RuleSet,
    dir: &AccountDirectory<'_>,
) -> Resolution {
    let ctx = Ctx {
        repo_path,
        bindings,
        rules,
        dir,
    };
    let remotes: Vec<RemoteResolution> = remotes
        .iter()
        .map(|r| RemoteResolution {
            remote: r.name.clone(),
            url: crate::url::redact_url(&r.url),
            parsed: RemoteUrl::parse(&r.url),
            resolved: ctx.remote(r),
        })
        .collect();
    let primary = remotes
        .iter()
        .find(|r| r.remote == "origin")
        .or_else(|| remotes.first());

    let default = ctx
        .binding(None, |b| b.locked)
        .or_else(|| ctx.binding(None, |b| b.source != BindingSource::AutoRule))
        .map(|b| ctx.resolved_from(b))
        .or_else(|| primary.and_then(|p| p.resolved.clone()))
        .or_else(|| {
            ctx.rule_match(
                MatchInput {
                    repo_path: Some(repo_path),
                    remote_url: None,
                },
                None,
            )
        })
        .or_else(|| ctx.binding(None, |_| true).map(|b| ctx.resolved_from(b)));

    Resolution { default, remotes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AuthMethod, ProviderKind, Rule, SigningMethod};
    use crate::rules::MatchOptions;

    struct World {
        accounts: Vec<Account>,
        providers: Vec<Provider>,
        identities: Vec<Identity>,
    }

    fn account(id: &str, provider: &str) -> Account {
        Account {
            id: id.into(),
            provider_id: provider.into(),
            username: id.into(),
            display_name: None,
            avatar_url: None,
            auth_method: AuthMethod::Pat,
            scopes: vec![],
            token_expires_at: None,
            color: "#1c7ed6".into(),
            label: None,
            default_clone_dir: None,
            secrets: vec![],
            sort_order: 0,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn identity(id: &str, account: &str, default: bool, created_at: i64) -> Identity {
        Identity {
            id: id.into(),
            account_id: account.into(),
            name: id.into(),
            email: format!("{id}@x.dev"),
            signing: SigningMethod::None,
            signing_key_ref: None,
            is_default: default,
            created_at,
        }
    }

    fn world() -> World {
        let mut acme_gitlab = Provider::gitlab_com();
        acme_gitlab.id = "acme".into();
        acme_gitlab.kind = ProviderKind::GitlabSelfHosted;
        acme_gitlab.base_url = "https://gitlab.acme.dev".into();
        acme_gitlab.builtin = false;
        World {
            accounts: vec![
                account("work", "acme"),
                account("personal", "github"),
                account("oss", "github"),
            ],
            providers: vec![Provider::github_com(), Provider::gitlab_com(), acme_gitlab],
            identities: vec![
                identity("work-main", "work", true, 1),
                identity("work-alt", "work", false, 2),
                identity("personal-old", "personal", false, 1),
                identity("personal-new", "personal", false, 2),
            ],
        }
    }

    impl World {
        fn dir(&self) -> AccountDirectory<'_> {
            AccountDirectory {
                accounts: &self.accounts,
                providers: &self.providers,
                identities: &self.identities,
            }
        }
    }

    fn binding(
        remote: Option<&str>,
        account: &str,
        source: BindingSource,
        locked: bool,
    ) -> RepoBinding {
        RepoBinding {
            repo_id: 1,
            remote_name: remote.map(Into::into),
            account_id: account.into(),
            identity_id: None,
            transport: None,
            source,
            locked,
            updated_at: 0,
        }
    }

    fn rule(id: &str, priority: i64, account: &str) -> Rule {
        Rule {
            id: id.into(),
            priority,
            host: None,
            owner_or_group: None,
            path_glob: None,
            remote_regex: None,
            account_id: account.into(),
            identity_id: None,
            enabled: true,
            created_at: 0,
        }
    }

    fn remotes(list: &[(&str, &str)]) -> Vec<RemoteInfo> {
        list.iter()
            .map(|(n, u)| RemoteInfo {
                name: (*n).into(),
                url: (*u).into(),
            })
            .collect()
    }

    fn rules(list: Vec<Rule>) -> RuleSet {
        RuleSet::new(list, &MatchOptions::default())
    }

    const ORIGIN: (&str, &str) = ("origin", "git@gitlab.acme.dev:platform/api.git");
    const UPSTREAM: (&str, &str) = ("upstream", "https://github.com/someone/api.git");

    #[test]
    fn unbound_repo() {
        let w = world();
        let res = resolve(
            "D:/x",
            &remotes(&[ORIGIN]),
            &[],
            &RuleSet::default(),
            &w.dir(),
        );
        assert_eq!(res.default, None);
        assert_eq!(res.remotes[0].resolved, None);
        assert_eq!(res.for_remote(Some("origin")), None);
    }

    #[test]
    fn rules_resolve_per_remote_with_identity_fallbacks() {
        let w = world();
        let mut work = rule("r-work", 0, "work");
        work.host = Some("gitlab.acme.dev".into());
        let mut personal = rule("r-personal", 1, "personal");
        personal.host = Some("github.com".into());
        let res = resolve(
            "D:/Work/api",
            &remotes(&[UPSTREAM, ORIGIN]),
            &[],
            &rules(vec![work, personal]),
            &w.dir(),
        );
        let origin = res.for_remote(Some("origin")).unwrap();
        assert_eq!(origin.account_id, "work");
        assert_eq!(origin.source, ContextSource::Rule);
        assert_eq!(origin.rule_id.as_deref(), Some("r-work"));
        assert_eq!(origin.identity_id.as_deref(), Some("work-main"));
        let upstream = res.for_remote(Some("upstream")).unwrap();
        assert_eq!(upstream.account_id, "personal");
        // No default identity: the oldest one.
        assert_eq!(upstream.identity_id.as_deref(), Some("personal-old"));
        // Default follows `origin`, not the first remote.
        assert_eq!(res.default.as_ref().unwrap().account_id, "work");
        // Unknown remote name falls back to the default.
        assert_eq!(res.for_remote(Some("fork")).unwrap().account_id, "work");
    }

    #[test]
    fn locked_beats_clone_beats_rules() {
        let w = world();
        let mut r = rule("r", 0, "work");
        r.host = Some("gitlab.acme.dev".into());
        let rs = rules(vec![r]);
        let rm = remotes(&[ORIGIN]);

        let clone = binding(Some("origin"), "personal", BindingSource::Clone, false);
        let res = resolve("D:/a", &rm, std::slice::from_ref(&clone), &rs, &w.dir());
        let got = res.for_remote(Some("origin")).unwrap();
        assert_eq!(
            (got.account_id.as_str(), got.source),
            ("personal", ContextSource::Clone)
        );

        let locked = binding(None, "oss", BindingSource::Manual, true);
        // Repo-level lock on a github account does not apply to a gitlab.acme.dev remote…
        let res = resolve("D:/a", &rm, &[clone.clone(), locked.clone()], &rs, &w.dir());
        assert_eq!(
            res.for_remote(Some("origin")).unwrap().account_id,
            "personal"
        );
        // …but it is the repo default.
        let d = res.default.as_ref().unwrap();
        assert_eq!(
            (d.account_id.as_str(), d.source),
            ("oss", ContextSource::Locked)
        );

        let locked_remote = binding(Some("origin"), "oss", BindingSource::Manual, true);
        let res = resolve("D:/a", &rm, &[clone, locked_remote], &rs, &w.dir());
        let got = res.for_remote(Some("origin")).unwrap();
        assert_eq!(
            (got.account_id.as_str(), got.source),
            ("oss", ContextSource::Locked)
        );
        assert_eq!(got.binding_remote.as_deref(), Some("origin"));
    }

    #[test]
    fn repo_level_binding_is_inherited_by_compatible_remotes_only() {
        let w = world();
        let manual = RepoBinding {
            identity_id: Some("work-alt".into()),
            ..binding(None, "work", BindingSource::Manual, false)
        };
        let res = resolve(
            "D:/a",
            &remotes(&[ORIGIN, UPSTREAM]),
            &[manual],
            &RuleSet::default(),
            &w.dir(),
        );
        let origin = res.for_remote(Some("origin")).unwrap();
        assert_eq!(origin.account_id, "work");
        assert_eq!(origin.identity_id.as_deref(), Some("work-alt"));
        assert_eq!(origin.binding_remote, None);
        assert!(origin.from_binding);
        assert_eq!(res.for_remote(Some("upstream")), None);
        assert_eq!(res.default.as_ref().unwrap().source, ContextSource::Manual);
    }

    #[test]
    fn hostless_rules_respect_provider_host_but_host_rules_do_not_need_to() {
        let w = world();
        let mut folder = rule("folder", 0, "work");
        folder.path_glob = Some("D:/Work/**".into());
        let res = resolve(
            "D:/Work/api",
            &remotes(&[ORIGIN, UPSTREAM]),
            &[],
            &rules(vec![folder]),
            &w.dir(),
        );
        assert_eq!(res.for_remote(Some("origin")).unwrap().account_id, "work");
        assert_eq!(res.for_remote(Some("upstream")), None);

        // A repo without remotes still gets folder rules as its default.
        let mut folder = rule("folder", 0, "work");
        folder.path_glob = Some("D:/Work/**".into());
        let mut host_only = rule("host", 1, "personal");
        host_only.host = Some("github.com".into());
        let res = resolve(
            "D:/Work/new",
            &[],
            &[],
            &rules(vec![host_only, folder]),
            &w.dir(),
        );
        let d = res.default.unwrap();
        assert_eq!(
            (d.account_id.as_str(), d.rule_id.as_deref()),
            ("work", Some("folder"))
        );
    }

    #[test]
    fn auto_rule_binding_is_a_fallback_and_missing_accounts_are_ignored() {
        let w = world();
        let remembered = binding(Some("origin"), "work", BindingSource::AutoRule, false);
        let ghost = binding(None, "deleted-account", BindingSource::Manual, true);
        let res = resolve(
            "D:/a",
            &remotes(&[ORIGIN]),
            &[ghost, remembered.clone()],
            &RuleSet::default(),
            &w.dir(),
        );
        let got = res.for_remote(Some("origin")).unwrap();
        assert_eq!(
            (got.account_id.as_str(), got.source),
            ("work", ContextSource::Rule)
        );
        assert_eq!(got.rule_id, None);

        let mut r = rule("r", 0, "personal");
        r.owner_or_group = Some("platform".into());
        r.host = Some("gitlab.acme.dev".into());
        let res = resolve(
            "D:/a",
            &remotes(&[ORIGIN]),
            &[remembered],
            &rules(vec![r]),
            &w.dir(),
        );
        assert_eq!(
            res.for_remote(Some("origin")).unwrap().account_id,
            "personal"
        );
    }

    #[test]
    fn local_remotes_accept_generic_candidates_and_urls_are_redacted() {
        let w = world();
        let manual = binding(None, "personal", BindingSource::Manual, false);
        let res = resolve(
            "D:/a",
            &remotes(&[
                ("origin", "D:/mirrors/api.git"),
                ("tok", "https://u:secret@github.com/a/b"),
            ]),
            &[manual],
            &RuleSet::default(),
            &w.dir(),
        );
        assert_eq!(
            res.for_remote(Some("origin")).unwrap().account_id,
            "personal"
        );
        assert_eq!(res.remotes[1].url, "https://u@github.com/a/b");
    }
}
