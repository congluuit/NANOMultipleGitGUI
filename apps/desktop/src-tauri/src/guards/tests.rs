//! Guard report builders: pure cases, then end-to-end against a temp repo + wiremock provider
//! (emails, permission), with an in-memory database and keychain and an isolated git config.

use std::path::{Path, PathBuf};

use nanomgg_accounts::{AccountSecret, AuthMethod, ContextSource, ProviderKind, Rule};
use nanomgg_git_engine::guard::UnpushedReport;
use nanomgg_git_engine::{GitRunner, repo};
use nanomgg_storage::Storage;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
use zeroize::Zeroizing;

use super::*;
use crate::accounts::commands::{
    AccountInput, BindingInput, IdentityInput, ProviderInput, RuleInput, account_save_impl,
    account_save_token_impl, identity_save_impl, provider_save_impl, repo_binding_set_impl,
    rule_save_impl,
};

const MIB_USIZE: usize = 1024 * 1024;

// ----- pure builders -------------------------------------------------------------------------

fn account(id: &str, provider: &str, username: &str) -> Account {
    Account {
        id: id.into(),
        provider_id: provider.into(),
        username: username.into(),
        display_name: None,
        avatar_url: None,
        auth_method: AuthMethod::Pat,
        scopes: vec![],
        token_expires_at: None,
        color: "#1c7ed6".into(),
        label: Some(id.to_uppercase()),
        default_clone_dir: None,
        secrets: vec![],
        sort_order: 0,
        created_at: 0,
        updated_at: 0,
    }
}

fn identity(id: &str, account: &str, email: &str) -> Identity {
    Identity {
        id: id.into(),
        account_id: account.into(),
        name: id.into(),
        email: email.into(),
        signing: SigningMethod::None,
        signing_key_ref: None,
        is_default: true,
        created_at: 0,
    }
}

fn resolved(account: &str, rule: Option<&str>) -> Resolved {
    Resolved {
        account_id: account.into(),
        identity_id: None,
        source: if rule.is_some() {
            ContextSource::Rule
        } else {
            ContextSource::Manual
        },
        rule_id: rule.map(str::to_owned),
        binding_remote: None,
        from_binding: rule.is_none(),
    }
}

fn dir() -> Directory {
    Directory {
        accounts: vec![
            account("work", "github", "hoang-acme"),
            account("personal", "github", "hoangnv"),
        ],
        providers: vec![Provider::github_com()],
        identities: vec![
            identity("idw", "work", "hoang@acme.dev"),
            identity("idp", "personal", "hoangnv@gmail.com"),
        ],
    }
}

fn gid(email: &str, account: Option<&str>) -> GuardIdentity {
    GuardIdentity {
        name: "Hoàng".into(),
        email: email.into(),
        source: if account.is_some() {
            IdentitySource::Account
        } else {
            IdentitySource::GitConfig
        },
        account_id: account.map(str::to_owned),
        signing: SigningMethod::None,
    }
}

fn facts(bound: Option<Resolved>, rule: Option<Resolved>, email: &str) -> CommitFacts {
    CommitFacts {
        repo_id: 1,
        dir: dir(),
        identity: Some(gid(email, bound.as_ref().map(|b| b.account_id.as_str()))),
        bound,
        rule,
        remote: RemoteUrl::parse("https://github.com/acme-corp/game.git"),
        gpg_sign: false,
        signing_key: false,
        api: None,
    }
}

fn pemail(email: &str, verified: bool) -> ProviderEmail {
    ProviderEmail {
        email: email.into(),
        verified,
        primary: true,
        commit_email: false,
    }
}

#[test]
fn rule_conflict_suggests_the_rule_account() {
    let f = facts(
        Some(resolved("personal", None)),
        Some(resolved("work", Some("r1"))),
        "hoangnv@gmail.com",
    );
    let report = build_commit_report(&f, None);
    assert_eq!(report.account_id.as_deref(), Some("personal"));
    assert!(!report.emails_checked);
    let [
        CommitFinding::EmailMismatch {
            reason,
            expected_email,
            suggestions,
            ..
        },
    ] = report.findings.as_slice()
    else {
        panic!("{:?}", report.findings);
    };
    assert_eq!(*reason, MismatchReason::RuleAccount);
    assert_eq!(expected_email.as_deref(), Some("hoang@acme.dev"));
    assert_eq!(suggestions[0].account_id, "work");
    assert_eq!(suggestions[0].reason, SuggestionReason::MatchedRule);
    assert_eq!(suggestions[0].email.as_deref(), Some("hoang@acme.dev"));
    assert!(suggestions.iter().all(|s| s.account_id != "personal"));

    // Bound through the rule itself: nothing to say.
    let ok = facts(
        Some(resolved("work", Some("r1"))),
        Some(resolved("work", Some("r1"))),
        "hoang@acme.dev",
    );
    assert!(build_commit_report(&ok, None).findings.is_empty());
}

#[test]
fn webmail_on_org_repo_and_foreign_email() {
    // No rule, bound to personal: gmail on acme-corp while "work" has @acme.dev.
    let f = facts(Some(resolved("personal", None)), None, "hoangnv@gmail.com");
    let report = build_commit_report(&f, None);
    let [
        CommitFinding::EmailMismatch {
            reason,
            suggestions,
            ..
        },
    ] = report.findings.as_slice()
    else {
        panic!("{:?}", report.findings);
    };
    assert_eq!(*reason, MismatchReason::PersonalEmailOnOrgRepo);
    assert_eq!(suggestions[0].account_id, "work");
    assert_eq!(suggestions[0].reason, SuggestionReason::OrgDomain);

    // The account has no identity: git config email that is not the account's.
    let mut f = facts(Some(resolved("work", None)), None, "someone@else.org");
    f.identity = Some(gid("someone@else.org", None));
    f.dir.identities.retain(|i| i.account_id != "work");
    let report = build_commit_report(&f, None);
    assert!(matches!(
        report.findings.as_slice(),
        [CommitFinding::EmailMismatch {
            reason: MismatchReason::NotAccountEmail,
            ..
        }]
    ));
    // ...unless the provider lists it.
    let list = [pemail("someone@else.org", true)];
    assert!(build_commit_report(&f, Some(&list)).findings.is_empty());
}

#[test]
fn unbound_unverified_and_signing() {
    let f = facts(None, None, "x@y.dev");
    let report = build_commit_report(&f, None);
    let [CommitFinding::Unbound { suggestions }] = report.findings.as_slice() else {
        panic!("{:?}", report.findings);
    };
    // Both accounts are on github.com; neither owns `acme-corp` → work via org domain first.
    assert_eq!(suggestions[0].account_id, "work");
    assert_eq!(suggestions.len(), 2);

    let f = facts(
        Some(resolved("work", Some("r"))),
        Some(resolved("work", Some("r"))),
        "hoang@acme.dev",
    );
    let unverified = [
        pemail("hoang@acme.dev", false),
        pemail("other@acme.dev", true),
    ];
    let report = build_commit_report(&f, Some(&unverified));
    assert!(report.emails_checked);
    assert!(matches!(
        report.findings.as_slice(),
        [CommitFinding::EmailNotVerified { account_id, .. }] if account_id == "work"
    ));
    let verified = [pemail("HOANG@acme.dev", true)];
    assert!(build_commit_report(&f, Some(&verified)).findings.is_empty());

    let mut f = facts(
        Some(resolved("work", Some("r"))),
        Some(resolved("work", Some("r"))),
        "1+hoang@users.noreply.github.com",
    );
    f.dir.identities[0].email = "1+hoang@users.noreply.github.com".into();
    f.gpg_sign = true;
    let report = build_commit_report(&f, Some(&verified));
    assert!(matches!(
        report.findings.as_slice(),
        [CommitFinding::SigningMissing { gpg_sign: true, .. }]
    ));
    f.signing_key = true;
    assert!(build_commit_report(&f, Some(&verified)).findings.is_empty());
}

fn commit(oid: &str, email: &str) -> UnpushedCommit {
    UnpushedCommit {
        oid: oid.into(),
        short_oid: oid[..2].into(),
        summary: oid.into(),
        author_name: "A".into(),
        author_email: email.into(),
        committer_name: "A".into(),
        committer_email: email.into(),
        time: 0,
        shared: false,
    }
}

fn blob(path: &str, mib: u64, shared: bool) -> HeavyBlob {
    HeavyBlob {
        oid: format!("{path}-oid"),
        path: path.into(),
        size: mib * MIB,
        commits: vec!["c1".into()],
        in_shared_commit: shared,
    }
}

fn push_facts_with(commits: Vec<UnpushedCommit>, heavy: Vec<HeavyBlob>) -> PushFacts {
    PushFacts {
        repo_id: 1,
        identity: Some(gid("me@acme.dev", Some("work"))),
        unpushed: UnpushedReport {
            remote: "origin".into(),
            branch: "main".into(),
            upstream: Some("refs/remotes/origin/main".into()),
            total: commits.len() as u32,
            commits,
            rewritable: true,
        },
        heavy,
        thresholds: GuardThresholds::GITHUB,
        is_current: true,
        remote_url: None,
        api: None,
        bound: true,
    }
}

#[test]
fn push_report_levels_mismatch_and_blocking() {
    let allowed = PushPermission {
        status: PermissionStatus::Allowed,
        level: Some(AccessLevel::Write),
        account_id: Some("work".into()),
        account_label: Some("WORK".into()),
        reason: None,
    };
    let f = push_facts_with(
        vec![
            commit("c3", "ME@acme.dev"),
            commit("c2", "me@gmail.com"),
            commit("c1", "me@acme.dev"),
        ],
        vec![
            blob("huge.psd", 120, false),
            blob("big.wav", 60, false),
            blob("old.zip", 70, true),
        ],
    );
    let r = build_push_report(f, "origin", "main", allowed.clone());
    assert_eq!(r.author_mismatch.len(), 1);
    assert_eq!(r.author_mismatch[0].oid, "c2");
    assert_eq!(r.heavy_blobs[0].level, GuardLevel::Block);
    assert_eq!(r.heavy_blobs[1].level, GuardLevel::Warn);
    assert!(r.blocked);
    assert_eq!(
        r.findings,
        vec![
            PushFinding::AuthorMismatch {
                count: 1,
                email: "me@acme.dev".into()
            },
            PushFinding::HeavyBlobs { block: 1, warn: 1 },
            PushFinding::HeavyBlobsShared { count: 1 },
        ]
    );

    // GitLab limits: warn only. Permission denied blocks.
    let mut f = push_facts_with(
        vec![commit("c1", "me@acme.dev")],
        vec![blob("huge.psd", 120, false)],
    );
    f.thresholds = GuardThresholds::GITLAB;
    let denied = PushPermission {
        status: PermissionStatus::Denied,
        level: Some(AccessLevel::Read),
        ..allowed.clone()
    };
    let r = build_push_report(f, "origin", "main", denied);
    assert_eq!(r.heavy_blobs[0].level, GuardLevel::Warn);
    assert!(r.blocked);
    assert!(matches!(
        r.findings[0],
        PushFinding::PermissionDenied {
            level: Some(AccessLevel::Read),
            ..
        }
    ));

    // Unknown permission + clean commits: nothing.
    let f = push_facts_with(vec![commit("c1", "me@acme.dev")], vec![]);
    let r = build_push_report(f, "origin", "main", PushPermission::unknown("network"));
    assert!(r.findings.is_empty() && !r.blocked);
}

// ----- end to end ----------------------------------------------------------------------------

struct World {
    tmp: tempfile::TempDir,
    st: AppState,
    server: MockServer,
    work: String,
    personal: String,
    work_identity: String,
}

impl World {
    async fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let server = MockServer::start().await;
        let mut st = AppState::new(Storage::open_in_memory().unwrap());
        let global = tmp.path().join("global.gitconfig");
        std::fs::write(
            &global,
            "[init]\n\tdefaultBranch = main\n[core]\n\tautocrlf = false\n[user]\n\tname = Global\n\temail = global@example.com\n",
        )
        .unwrap();
        st.git = GitRunner::default()
            .with_env("GIT_CONFIG_NOSYSTEM", "1")
            .with_env("GIT_CONFIG_GLOBAL", global.to_string_lossy());
        let provider = provider_save_impl(
            &st,
            ProviderInput {
                id: None,
                kind: ProviderKind::GithubEnterprise,
                name: "GHE test".into(),
                base_url: server.uri(),
                api_url: Some(format!("{}/api/v3", server.uri())),
                oauth_client_id: None,
                large_file_warn_mb: Some(1),
                large_file_block_mb: Some(2),
            },
        )
        .unwrap();
        let mut ids = Vec::new();
        for (user, email) in [
            ("hoang-acme", "hoang@acme.dev"),
            ("hoangnv", "hoangnv@gmail.com"),
        ] {
            let acc = account_save_impl(
                &st,
                AccountInput {
                    id: None,
                    provider_id: provider.id.clone(),
                    username: user.into(),
                    display_name: None,
                    avatar_url: None,
                    auth_method: AuthMethod::Pat,
                    scopes: vec!["repo".into()],
                    token_expires_at: None,
                    color: None,
                    label: None,
                    default_clone_dir: None,
                },
            )
            .unwrap();
            let idn = identity_save_impl(
                &st,
                IdentityInput {
                    id: None,
                    account_id: acc.id.clone(),
                    name: user.into(),
                    email: email.into(),
                    signing: SigningMethod::None,
                    signing_key_ref: None,
                    is_default: true,
                },
            )
            .unwrap();
            account_save_token_impl(
                &st,
                &acc.id,
                AccountSecret::Token,
                Zeroizing::new(format!("token-{user}")),
            )
            .unwrap();
            ids.push((acc.id, idn.id));
        }
        let (personal, _) = ids.pop().unwrap();
        let (work, work_identity) = ids.pop().unwrap();
        let rule: Rule = rule_save_impl(
            &st,
            RuleInput {
                id: None,
                priority: None,
                host: None,
                owner_or_group: Some("acme-corp".into()),
                path_glob: None,
                remote_regex: None,
                account_id: work.clone(),
                identity_id: None,
                enabled: true,
            },
        )
        .unwrap();
        assert_eq!(rule.account_id, work);
        Self {
            tmp,
            st,
            server,
            work,
            personal,
            work_identity,
        }
    }

    fn git(&self, dir: &Path, args: &[&str]) -> String {
        self.st
            .git
            .run_text(dir, &AccountContext::default(), args)
            .unwrap()
    }

    /// Repo with origin on the mock host, one "pushed" commit (remote-tracking ref set by
    /// hand: there is no git server), registered as open.
    fn repo(&self) -> (PathBuf, i64) {
        let dir = self.tmp.path().join("game");
        std::fs::create_dir_all(&dir).unwrap();
        self.git(&dir, &["init", "-q", "-b", "main"]);
        let origin = format!("{}/acme-corp/game.git", self.server.uri());
        self.git(&dir, &["remote", "add", "origin", &origin]);
        std::fs::write(dir.join("README.md"), "hi\n").unwrap();
        self.git(&dir, &["add", "."]);
        self.git(&dir, &["commit", "-q", "-m", "init"]);
        self.git(&dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        let info = repo::discover(&self.st.git, &dir).unwrap();
        let rec = self
            .st
            .storage
            .touch_repo(
                &info.root.to_string_lossy(),
                &crate::repos::path_key(&info.root),
                "game",
                1,
            )
            .unwrap();
        self.st.workspace().repos.insert(rec.id, info.clone());
        (info.root, rec.id)
    }
}

fn pseudo_random(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

#[tokio::test]
async fn commit_guard_end_to_end_with_provider_emails() {
    let w = World::new().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"email": "hoangnv@gmail.com", "primary": true, "verified": false, "visibility": null}
        ])))
        .expect(1)
        .mount(&w.server)
        .await;
    let (_root, id) = w.repo();
    let http =
        nanomgg_providers::build_http_client(&nanomgg_providers::HttpConfig::default()).unwrap();

    // Rule → work account: clean. (The emails mock answers for any token and expects a single
    // call, kept for the personal account below, so this run skips the API.)
    let mut facts = commit_facts(&w.st, id).unwrap();
    assert_eq!(facts.bound.as_ref().unwrap().account_id, w.work);
    assert_eq!(facts.rule.as_ref().unwrap().account_id, w.work);
    assert!(facts.api.is_some());
    facts.api = None;
    assert!(
        commit_guard_impl(facts, Some(&http))
            .await
            .findings
            .is_empty()
    );

    // Bound by hand to the personal account (gmail) while the rule says work.
    repo_binding_set_impl(
        &w.st,
        id,
        BindingInput {
            remote: None,
            account_id: w.personal.clone(),
            identity_id: None,
            transport: None,
            locked: false,
            source: None,
        },
    )
    .unwrap();
    let report = commit_guard_impl(commit_facts(&w.st, id).unwrap(), Some(&http)).await;
    assert_eq!(report.identity.as_ref().unwrap().email, "hoangnv@gmail.com");
    assert!(report.emails_checked);
    assert_eq!(report.findings.len(), 2, "{:?}", report.findings);
    let CommitFinding::EmailMismatch {
        reason,
        suggestions,
        ..
    } = &report.findings[0]
    else {
        panic!("{:?}", report.findings);
    };
    assert_eq!(*reason, MismatchReason::RuleAccount);
    assert_eq!(suggestions[0].account_id, w.work);
    assert_eq!(
        suggestions[0].identity_id.as_deref(),
        Some(w.work_identity.as_str())
    );
    assert!(
        matches!(&report.findings[1], CommitFinding::EmailNotVerified { account_id, .. } if *account_id == w.personal)
    );

    // Second run: email list served from the 10-minute cache (mock expects one call).
    let again = commit_guard_impl(commit_facts(&w.st, id).unwrap(), Some(&http)).await;
    assert_eq!(again.findings, report.findings);

    // Offline: no http client → emails skipped, mismatch still reported.
    let offline = commit_guard_impl(commit_facts(&w.st, id).unwrap(), None).await;
    assert!(!offline.emails_checked);
    assert_eq!(offline.findings.len(), 1);
}

#[tokio::test]
async fn push_guard_end_to_end_permission_blobs_and_fix() {
    let w = World::new().await;
    Mock::given(method("GET"))
        .and(path("/api/v3/repos/acme-corp/game"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": 1, "name": "game", "full_name": "acme-corp/game",
            "owner": {"login": "acme-corp"}, "private": true,
            "permissions": {"admin": false, "push": false, "pull": true}
        })))
        .mount(&w.server)
        .await;
    let (root, id) = w.repo();
    let http =
        nanomgg_providers::build_http_client(&nanomgg_providers::HttpConfig::default()).unwrap();

    // Two unpushed commits: one as the personal identity, one adding a 3 MiB blob.
    let personal = AccountContext {
        identity: Some(nanomgg_git_engine::Identity {
            name: "Me".into(),
            email: "hoangnv@gmail.com".into(),
        }),
        ..Default::default()
    };
    std::fs::write(root.join("a.txt"), "a\n").unwrap();
    w.git(&root, &["add", "."]);
    w.st.git
        .run(&root, &personal, ["commit", "-q", "-m", "personal"])
        .unwrap();
    std::fs::write(root.join("big.bin"), pseudo_random(3 * MIB_USIZE, 5)).unwrap();
    w.git(&root, &["add", "."]);
    let work_ctx = crate::accounts::context_for(&w.st, id, None).unwrap();
    w.st.git
        .run(&root, &work_ctx, ["commit", "-q", "-m", "big"])
        .unwrap();

    let facts = push_facts(&w.st, id, "origin", "main").unwrap().unwrap();
    assert_eq!(facts.identity.as_ref().unwrap().email, "hoang@acme.dev");
    assert_eq!(facts.thresholds.warn_bytes, MIB);
    let permission = check_permission(
        &http,
        facts.api.as_ref(),
        facts.remote_url.as_deref(),
        facts.bound,
    )
    .await;
    assert_eq!(permission.status, PermissionStatus::Denied);
    assert_eq!(permission.account_id.as_deref(), Some(w.work.as_str()));
    let report = build_push_report(facts, "origin", "main", permission);
    assert_eq!(report.total_unpushed, 2);
    assert_eq!(report.author_mismatch.len(), 1);
    assert_eq!(report.author_mismatch[0].summary, "personal");
    assert_eq!(report.heavy_blobs.len(), 1);
    assert_eq!(report.heavy_blobs[0].path, "big.bin");
    assert_eq!(report.heavy_blobs[0].level, GuardLevel::Block);
    assert!(report.blocked && report.is_current && report.rewritable);

    // Network failure → unknown, not blocking.
    let facts = push_facts(&w.st, id, "origin", "main").unwrap().unwrap();
    let dead = ApiAccess {
        account_id: "x".into(),
        account_label: "X".into(),
        endpoint: ProviderEndpoint::custom(
            nanomgg_providers::ProviderKind::GithubEnterprise,
            "http://127.0.0.1:9",
            "http://127.0.0.1:9/api/v3",
        )
        .unwrap(),
        token: SecretToken::new("t"),
    };
    let unknown = check_permission(
        &http,
        Some(&dead),
        Some("http://127.0.0.1:9/acme-corp/game.git"),
        true,
    )
    .await;
    assert_eq!(unknown.status, PermissionStatus::Unknown);
    let report = build_push_report(facts, "origin", "main", unknown);
    assert!(
        !report
            .findings
            .iter()
            .any(|f| matches!(f, PushFinding::PermissionDenied { .. }))
    );

    // Fix authors on every unpushed commit → no more mismatch; the backup can undo it.
    let before = w.git(&root, &["rev-parse", "HEAD"]);
    let out = fix_authors_impl(&w.st, id, "origin", "main", FixAuthorMode::RewriteUnpushed)
        .unwrap()
        .unwrap();
    assert_eq!(out.rewritten, 2);
    let facts = push_facts(&w.st, id, "origin", "main").unwrap().unwrap();
    let report = build_push_report(facts, "origin", "main", PushPermission::unknown("offline"));
    assert!(report.author_mismatch.is_empty());
    let backups = guard::list_backups(&w.st.git, &root).unwrap();
    assert_eq!(backups[0].name, out.backup_ref);
    guard::restore_backup(&w.st.git, &root, &out.backup_ref, None).unwrap();
    assert_eq!(w.git(&root, &["rev-parse", "HEAD"]), before);

    // Invalid names come back as a rejection, not an error.
    assert!(push_facts(&w.st, id, "origin", "nope").unwrap().is_err());
    assert!(push_facts(&w.st, id, "-x", "main").unwrap().is_err());
}
