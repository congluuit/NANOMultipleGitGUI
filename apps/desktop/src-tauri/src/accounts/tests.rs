//! End-to-end tests of the resolver against a local Git + LFS server with two accounts on the
//! same host: contexts built by [`context_for`] push to each owner's repo through the real
//! `nanomgg-credential` sidecar (built by build.rs), the wrong account gets HTTP 403, tokens never
//! reach `.git/config`, and LFS objects are uploaded by the bound account.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use nanomgg_accounts::{AccountSecret, AuthMethod, ContextSource, ProviderKind, SshKey, Transport};
use nanomgg_credentials::{
    CredentialServer, KeychainSource, MemorySecretStore, SecretKey, SecretStore,
};
use nanomgg_git_engine::{AccountContext, GitRunner, repo};
use nanomgg_storage::Storage;
use nanomgg_test_git_server::{EventKind, TestGitServer};
use zeroize::Zeroizing;

use super::commands::*;
use super::*;
use crate::state::AppState;

const ALICE_TOKEN: &str = "alice-secret-token-0123";
const BOB_TOKEN: &str = "bob-secret-token-4567";

struct Harness {
    tmp: tempfile::TempDir,
    server: TestGitServer,
    st: AppState,
    secrets: Arc<MemorySecretStore>,
    alice: String,
    bob: String,
}

fn git_lfs_available() -> bool {
    let ok = Command::new("git")
        .args(["lfs", "version"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("git-lfs not installed; skipping");
    }
    ok
}

impl Harness {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let server = TestGitServer::start(
            &tmp.path().join("server"),
            &[("alice", ALICE_TOKEN), ("bob", BOB_TOKEN)],
        )
        .unwrap();
        server.create_repo("alice", "app").unwrap();
        server.create_repo("bob", "site").unwrap();

        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let secrets = Arc::new(MemorySecretStore::default());
        let source =
            KeychainSource::new(Arc::new(StorageDirectory(storage.clone())), secrets.clone());
        let creds = CredentialServer::start(Arc::new(source)).unwrap();
        let sidecar = sidecar_path()
            .expect("nanomgg-credential must be next to the test binary (build.rs + externalBin)");
        let rt = AccountsRuntime::new(Some(creds), Some(sidecar), secrets.clone());
        let mut st = AppState::with_accounts(storage, rt);

        // Isolated "global" config with a decoy helper returning a wrong password.
        let global = tmp.path().join("global.gitconfig");
        std::fs::write(
            &global,
            "[credential]\n\thelper = \"!f() { echo username=alice; echo password=WRONG; }; f\"\n\
             [init]\n\tdefaultBranch = main\n[core]\n\tautocrlf = false\n",
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
                name: "Local test server".into(),
                base_url: format!("http://{}/", server.host()),
                api_url: None,
                oauth_client_id: None,
                large_file_warn_mb: None,
                large_file_block_mb: None,
            },
        )
        .unwrap();
        assert_eq!(provider.large_file_block_mb, Some(100));
        let mut ids = Vec::new();
        for (user, token, email) in [
            ("alice", ALICE_TOKEN, "alice@acme.dev"),
            ("bob", BOB_TOKEN, "bob@personal.dev"),
        ] {
            let account = account_save_impl(
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
                    label: Some(user.to_uppercase()),
                    default_clone_dir: None,
                },
            )
            .unwrap();
            identity_save_impl(
                &st,
                IdentityInput {
                    id: None,
                    account_id: account.id.clone(),
                    name: user.to_uppercase(),
                    email: email.into(),
                    signing: Default::default(),
                    signing_key_ref: None,
                    is_default: false,
                },
            )
            .unwrap();
            let saved = account_save_token_impl(
                &st,
                &account.id,
                AccountSecret::Token,
                Zeroizing::new(format!("  {token}\n")),
            )
            .unwrap();
            assert_eq!(saved.secrets, [AccountSecret::Token]);
            rule_save_impl(
                &st,
                RuleInput {
                    id: None,
                    priority: None,
                    host: Some(server.host().split(':').next().unwrap().into()),
                    owner_or_group: Some(user.into()),
                    path_glob: None,
                    remote_regex: None,
                    account_id: account.id.clone(),
                    identity_id: None,
                    enabled: true,
                },
            )
            .unwrap();
            ids.push(account.id);
        }
        // Distinct default colors from the palette.
        let accounts = st.storage.accounts().unwrap();
        assert_ne!(accounts[0].color, accounts[1].color);

        let bob = ids.pop().unwrap();
        let alice = ids.pop().unwrap();
        Self {
            tmp,
            server,
            st,
            secrets,
            alice,
            bob,
        }
    }

    fn none() -> AccountContext {
        AccountContext::default()
    }

    /// `git init` + origin, registered as an open repo.
    fn repo(&self, name: &str, origin: &str) -> (PathBuf, i64) {
        let dir = self.tmp.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let g = |args: &[&str]| self.st.git.run(&dir, &Self::none(), args).unwrap();
        g(&["init", "-q", "-b", "main"]);
        g(&["config", "core.autocrlf", "false"]);
        g(&["remote", "add", "origin", origin]);
        let info = repo::discover(&self.st.git, &dir).unwrap();
        let path = info.root.to_string_lossy().into_owned();
        let rec = self
            .st
            .storage
            .touch_repo(&path, &crate::repos::path_key(&info.root), name, 1)
            .unwrap();
        self.st.workspace().repos.insert(rec.id, info.clone());
        (info.root, rec.id)
    }

    fn commit_file(&self, dir: &Path, id: i64, file: &str, contents: &[u8]) {
        std::fs::write(dir.join(file), contents).unwrap();
        let ctx = context_for(&self.st, id, None).unwrap();
        self.st.git.run(dir, &ctx, ["add", "."]).unwrap();
        self.st
            .git
            .run(dir, &ctx, ["commit", "-q", "-m", file])
            .unwrap();
    }

    fn push(
        &self,
        dir: &Path,
        id: i64,
    ) -> Result<std::process::Output, nanomgg_git_engine::GitError> {
        let ctx = context_for(&self.st, id, Some("origin")).unwrap();
        self.st
            .git
            .run(dir, &ctx, ["push", "-q", "-u", "origin", "main"])
    }

    fn assert_no_secret_in_config(&self, dir: &Path) {
        let config = std::fs::read_to_string(dir.join(".git/config")).unwrap();
        let listed = self
            .st
            .git
            .run_text(dir, &Self::none(), ["config", "--list", "--show-origin"])
            .unwrap();
        for token in [ALICE_TOKEN, BOB_TOKEN] {
            assert!(!config.contains(token), "token in .git/config: {config}");
            assert!(!listed.contains(token), "token in git config: {listed}");
        }
        assert!(
            !config.contains("nanomgg-credential"),
            "helper persisted: {config}"
        );
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

#[test]
fn resolver_contexts_push_each_owner_repo_with_lfs() {
    if !git_lfs_available() {
        return;
    }
    let h = Harness::new();
    for (owner, repo_name, account, seed) in [
        ("alice", "app", h.alice.clone(), 1u64),
        ("bob", "site", h.bob.clone(), 2u64),
    ] {
        let (dir, id) = h.repo(&format!("work-{owner}"), &h.server.url(owner, repo_name));

        // Rule engine picked the owner's account.
        let pill = repo_account_context_impl(&h.st, id).unwrap();
        assert_eq!(pill.source, ContextSource::Rule);
        assert_eq!(pill.account.as_ref().unwrap().id, account);
        assert_eq!(pill.matched_rule.as_ref().unwrap().account_id, account);
        assert_eq!(pill.per_remote.len(), 1);
        assert_eq!(
            pill.per_remote[0].host.as_deref(),
            Some(h.server.host().as_str())
        );
        assert_eq!(pill.per_remote[0].transport, Some(Transport::Https));
        let email = pill.identity.as_ref().unwrap().email.clone();

        let ctx = context_for(&h.st, id, None).unwrap();
        h.st.git
            .run(&dir, &ctx, ["lfs", "install", "--local"])
            .unwrap();
        h.st.git.run(&dir, &ctx, ["lfs", "track", "*.bin"]).unwrap();
        h.commit_file(&dir, id, "asset.bin", &pseudo_random(256 << 10, seed));
        let oid =
            h.st.git
                .run_text(&dir, &ctx, ["lfs", "ls-files", "-l"])
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .to_owned();
        h.push(&dir, id).unwrap();

        let author =
            h.st.git
                .run_text(&dir, &Harness::none(), ["log", "-1", "--format=%ae"])
                .unwrap();
        assert_eq!(author, email);
        let full = format!("{owner}/{repo_name}");
        let events = h.server.events();
        assert!(events.iter().any(|e| e.repo == full
            && e.user.as_deref() == Some(owner)
            && e.kind == EventKind::GitWrite));
        assert!(
            events.iter().any(|e| e.repo == full
                && e.user.as_deref() == Some(owner)
                && e.kind
                    == EventKind::LfsUpload {
                        oid: oid.clone(),
                        size: 256 << 10
                    }),
            "LFS object not uploaded by {owner}: {events:#?}"
        );
        h.assert_no_secret_in_config(&dir);
    }
    assert!(
        !h.server
            .events()
            .iter()
            .any(|e| e.kind == EventKind::Forbidden)
    );
}

#[test]
fn locked_binding_beats_rules_and_the_wrong_account_gets_403() {
    let h = Harness::new();
    let (dir, id) = h.repo("wrong", &h.server.url("alice", "app"));
    h.commit_file(&dir, id, "a.txt", b"x");

    let pill = repo_binding_set_impl(
        &h.st,
        id,
        BindingInput {
            remote: None,
            account_id: h.bob.clone(),
            identity_id: None,
            transport: None,
            locked: true,
            source: None,
        },
    )
    .unwrap();
    // Repo-level lock applies to the same-host remote too.
    assert_eq!(pill.source, ContextSource::Locked);
    assert_eq!(pill.per_remote[0].account.as_ref().unwrap().id, h.bob);
    assert!(pill.per_remote[0].inherited);
    assert_eq!(pill.bindings.len(), 1);

    let err = h.push(&dir, id).unwrap_err();
    assert!(err.to_string().contains("403"), "{err}");
    let events = h.server.events();
    assert!(events.iter().any(|e| e.repo == "alice/app"
        && e.user.as_deref() == Some("bob")
        && e.kind == EventKind::Forbidden));
    assert!(!events.iter().any(|e| e.kind == EventKind::GitWrite));
    assert!(
        !events.iter().any(|e| e.user.as_deref() == Some("alice")),
        "decoy/global helper must never be consulted"
    );

    // Binding written into .git/config as identity only (flag on by default).
    let local_email =
        h.st.git
            .run_text(&dir, &Harness::none(), ["config", "--local", "user.email"])
            .unwrap();
    assert_eq!(local_email, "bob@personal.dev");
    h.assert_no_secret_in_config(&dir);

    // Clearing the binding returns to the rule → alice can push.
    let pill = repo_binding_clear_impl(&h.st, id, None).unwrap();
    assert_eq!(pill.source, ContextSource::Rule);
    h.push(&dir, id).unwrap();
    assert!(h.server.events().iter().any(|e| e.repo == "alice/app"
        && e.user.as_deref() == Some("alice")
        && e.kind == EventKind::GitWrite));
    let local_email =
        h.st.git
            .run_text(&dir, &Harness::none(), ["config", "--local", "user.email"])
            .unwrap();
    assert_eq!(local_email, "alice@acme.dev");
}

#[test]
fn per_remote_binding_and_unbound_repos() {
    let h = Harness::new();
    let (dir, id) = h.repo("multi", "https://example.invalid/team/x.git");
    // Unbound: git's own config, nothing injected, banner in the UI.
    assert_eq!(
        context_for(&h.st, id, None).unwrap(),
        AccountContext::default()
    );
    let pill = repo_account_context_impl(&h.st, id).unwrap();
    assert!(pill.account.is_none());
    assert_eq!(pill.source, ContextSource::None);
    assert!(!sync_repo_config(&h.st, id).unwrap());

    // `upstream` on the test server follows the rules; `origin` stays unbound.
    h.st.git
        .run(
            &dir,
            &Harness::none(),
            ["remote", "add", "upstream", &h.server.url("bob", "site")],
        )
        .unwrap();
    let up = context_for(&h.st, id, Some("upstream")).unwrap();
    assert_eq!(up.identity.unwrap().email, "bob@personal.dev");
    assert!(
        up.https
            .unwrap()
            .env
            .iter()
            .any(|(k, v)| k == "NANOMGG_ACCOUNT" && v == &h.bob)
    );
    assert_eq!(
        context_for(&h.st, id, Some("origin")).unwrap(),
        AccountContext::default()
    );
    // Repo default follows `origin` (unbound), not the first matching remote.
    assert_eq!(
        context_for(&h.st, id, None).unwrap(),
        AccountContext::default()
    );

    // A manual binding for `origin` only.
    repo_binding_set_impl(
        &h.st,
        id,
        BindingInput {
            remote: Some("origin".into()),
            account_id: h.alice.clone(),
            identity_id: None,
            transport: Some(Transport::Https),
            locked: false,
            source: None,
        },
    )
    .unwrap();
    let pill = repo_account_context_impl(&h.st, id).unwrap();
    let origin = pill
        .per_remote
        .iter()
        .find(|r| r.remote == "origin")
        .unwrap();
    assert_eq!(origin.source, ContextSource::Manual);
    assert!(!origin.inherited);
    assert_eq!(pill.account.unwrap().id, h.alice);

    // One-off context and identity ownership check.
    let bob_identity = h.st.storage.identities(Some(&h.bob)).unwrap()[0].id.clone();
    assert!(context_for_account(&h.st, &h.alice, Some(&bob_identity)).is_err());
    assert_eq!(
        context_for_account(&h.st, &h.bob, Some(&bob_identity))
            .unwrap()
            .identity
            .unwrap()
            .email,
        "bob@personal.dev"
    );
}

#[test]
fn rule_preview_and_settings_flags() {
    let h = Harness::new();
    let res = rule_test_impl(&h.st, Some(h.server.url("alice", "anything")), None).unwrap();
    assert_eq!(res.account.unwrap().id, h.alice);
    assert_eq!(res.all_matches.len(), 1);
    assert_eq!(res.parsed.unwrap().owner(), Some("alice"));
    let none = rule_test_impl(&h.st, Some("git@github.com:alice/x.git".into()), None).unwrap();
    assert!(none.matched_rule.is_none());

    assert_eq!(
        account_settings_impl(&h.st).unwrap(),
        AccountSettings {
            identity_write_to_repo_config: true,
            ssh_write_core_ssh_command: false
        }
    );
    h.st.storage
        .set_setting(KEY_WRITE_IDENTITY, &false)
        .unwrap();
    let (dir, id) = h.repo("noconfig", &h.server.url("alice", "app"));
    assert!(!sync_repo_config(&h.st, id).unwrap());
    assert!(
        h.st.git
            .run(&dir, &Harness::none(), ["config", "--local", "user.email"])
            .is_err()
    );
}

#[test]
fn ssh_remote_gets_key_command_askpass_and_optional_core_ssh_command() {
    let h = Harness::new();
    let key_path = h.tmp.path().join("keys").join("key_alice");
    std::fs::create_dir_all(key_path.parent().unwrap()).unwrap();
    std::fs::write(&key_path, "not a real key").unwrap();
    h.st.storage
        .insert_ssh_key(&SshKey {
            id: "key_alice".into(),
            account_id: Some(h.alice.clone()),
            key_type: "ed25519".into(),
            fingerprint: "SHA256:test".into(),
            public_key: "ssh-ed25519 AAAA alice".into(),
            private_key_path: Some(key_path.to_string_lossy().into_owned()),
            has_passphrase: true,
            uploaded_to_provider: false,
            provider_key_id: None,
            comment: None,
            created_at: 0,
        })
        .unwrap();
    h.secrets
        .set(&SecretKey::ssh_passphrase("key_alice"), "correct horse")
        .unwrap();

    let host = h.server.host();
    let hostname = host.split(':').next().unwrap();
    let (dir, id) = h.repo("ssh", &format!("git@{hostname}:alice/app.git"));
    let ctx = context_for(&h.st, id, Some("origin")).unwrap();
    let ssh = ctx
        .ssh
        .clone()
        .expect("ssh auth for an account with a key file");
    let command = ssh.git_ssh_command();
    assert!(command.contains("IdentityFile="), "{command}");
    assert!(command.contains("key_alice"), "{command}");
    assert!(command.contains("IdentityAgent=none"), "{command}");
    let env = ctx.env();
    let get = |k: &str| env.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
    assert_eq!(get("SSH_ASKPASS_REQUIRE").as_deref(), Some("force"));

    // The sidecar, run exactly as ssh would run SSH_ASKPASS, answers the passphrase prompt
    // for this account's key — and nothing else.
    let askpass = |prompt: &str| {
        Command::new(get("SSH_ASKPASS").unwrap())
            .arg(prompt)
            .envs(env.iter().map(|(k, v)| (k, v)))
            .output()
            .unwrap()
    };
    let prompt = format!(
        "Enter passphrase for key '{}': ",
        key_path.to_string_lossy().replace('\\', "/")
    );
    let ok = askpass(&prompt);
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&ok.stdout).trim_end(),
        "correct horse"
    );
    for denied in [
        "Enter passphrase for key 'C:/somebody/else': ".to_owned(),
        "Are you sure you want to continue connecting (yes/no/[fingerprint])? ".to_owned(),
        format!("Username for 'http://{host}': "),
    ] {
        let out = askpass(&denied);
        assert!(!out.status.success(), "{denied}");
        assert!(out.stdout.is_empty(), "{denied}");
    }

    // Opt-in core.sshCommand (no askpass/known_hosts in the test runtime) + identity.
    assert!(sync_repo_config(&h.st, id).unwrap());
    let local = |key: &str| {
        h.st.git
            .run_text(&dir, &Harness::none(), ["config", "--local", "--get", key])
    };
    assert!(local("core.sshCommand").is_err(), "off by default");
    h.st.storage
        .set_setting(KEY_WRITE_SSH_COMMAND, &true)
        .unwrap();
    assert!(sync_repo_config(&h.st, id).unwrap());
    assert_eq!(local("core.sshCommand").unwrap(), command);
    assert_eq!(local("user.email").unwrap(), "alice@acme.dev");
    assert!(!sync_repo_config(&h.st, id).unwrap(), "idempotent");
}

#[test]
fn deleting_an_account_removes_its_keychain_entries_and_bindings() {
    let h = Harness::new();
    let (_dir, id) = h.repo("del", &h.server.url("alice", "app"));
    repo_binding_set_impl(
        &h.st,
        id,
        BindingInput {
            remote: None,
            account_id: h.alice.clone(),
            identity_id: None,
            transport: None,
            locked: true,
            source: None,
        },
    )
    .unwrap();
    assert_eq!(h.secrets.len(), 2);
    assert!(account_delete_impl(&h.st, &h.alice).unwrap());
    assert_eq!(h.secrets.len(), 1);
    assert!(h.st.storage.repo_bindings(id).unwrap().is_empty());
    let a = account_store_tokens(
        &h.st,
        &h.bob,
        Zeroizing::new("new-access".into()),
        Some(Zeroizing::new("refresh".into())),
        Some(1_900_000_000_000),
        vec!["api".into()],
    )
    .unwrap();
    assert_eq!(
        a.secrets,
        [AccountSecret::RefreshToken, AccountSecret::Token]
    );
    assert_eq!(a.token_expires_at, Some(1_900_000_000_000));
    assert_eq!(a.scopes, ["api"]);
    assert_eq!(
        account_secret(&h.st, &h.bob, AccountSecret::Token)
            .unwrap()
            .unwrap()
            .as_str(),
        "new-access"
    );
    let a = account_store_tokens(
        &h.st,
        &h.bob,
        Zeroizing::new("again".into()),
        None,
        None,
        vec![],
    )
    .unwrap();
    assert_eq!(a.secrets, [AccountSecret::Token]);
    let a = account_delete_token_impl(&h.st, &h.bob, AccountSecret::Token).unwrap();
    assert!(a.secrets.is_empty());
    assert!(h.secrets.is_empty());
    assert!(
        account_save_token_impl(
            &h.st,
            &h.bob,
            AccountSecret::Token,
            Zeroizing::new("  ".into())
        )
        .is_err()
    );
}

fn as_account(account_id: &str) -> AccountOverride {
    AccountOverride {
        account_id: account_id.to_owned(),
        identity_id: None,
    }
}

#[test]
fn commit_create_uses_the_bound_identity_and_a_one_shot_override() {
    use nanomgg_git_engine::commit_write::CommitOptions;

    let h = Harness::new();
    let (dir, id) = h.repo("commit", &h.server.url("alice", "app"));
    let stage = |file: &str| {
        std::fs::write(dir.join(file), file).unwrap();
        h.st.git.run(&dir, &Harness::none(), ["add", file]).unwrap();
    };
    let last = |format: &str| {
        h.st.git
            .run_text(&dir, &Harness::none(), ["log", "-1", format])
            .unwrap()
    };
    let local_email = || {
        h.st.git
            .run_text(&dir, &Harness::none(), ["config", "--local", "user.email"])
            .ok()
    };
    let opts = CommitOptions::default();

    // Reading the pill never writes the repo config…
    repo_account_context_impl(&h.st, id).unwrap();
    assert_eq!(local_email(), None);

    // …the commit does (identity flag on by default), and records the rule's account.
    stage("a.txt");
    crate::worktree::commit_create_impl(&h.st, id, "as alice", &opts, None).unwrap();
    assert_eq!(last("--format=%ae %ce"), "alice@acme.dev alice@acme.dev");
    assert_eq!(last("--format=%an"), "ALICE");
    assert_eq!(local_email().as_deref(), Some("alice@acme.dev"));

    // "Commit as…" another account: its default identity, binding and repo config untouched.
    stage("b.txt");
    crate::worktree::commit_create_impl(&h.st, id, "as bob", &opts, Some(&as_account(&h.bob)))
        .unwrap();
    assert_eq!(
        last("--format=%ae %ce"),
        "bob@personal.dev bob@personal.dev"
    );
    assert_eq!(local_email().as_deref(), Some("alice@acme.dev"));
    assert!(h.st.storage.repo_bindings(id).unwrap().is_empty());

    // An explicit second identity of the same account.
    let work = identity_save_impl(
        &h.st,
        IdentityInput {
            id: None,
            account_id: h.bob.clone(),
            name: "Bob at Work".into(),
            email: "bob@work.dev".into(),
            signing: Default::default(),
            signing_key_ref: None,
            is_default: false,
        },
    )
    .unwrap();
    stage("c.txt");
    let over = AccountOverride {
        account_id: h.bob.clone(),
        identity_id: Some(work.id.clone()),
    };
    crate::worktree::commit_create_impl(&h.st, id, "as bob@work", &opts, Some(&over)).unwrap();
    assert_eq!(last("--format=%an <%ae>"), "Bob at Work <bob@work.dev>");

    // Unknown account / foreign identity are refused before git runs.
    stage("d.txt");
    assert!(
        crate::worktree::commit_create_impl(&h.st, id, "x", &opts, Some(&as_account("nope")))
            .is_err()
    );
    let foreign = AccountOverride {
        account_id: h.alice.clone(),
        identity_id: Some(work.id),
    };
    assert!(crate::worktree::commit_create_impl(&h.st, id, "x", &opts, Some(&foreign)).is_err());
    assert_eq!(last("--format=%s"), "as bob@work");
}

#[test]
fn push_with_a_one_shot_override_authenticates_as_the_other_account() {
    let h = Harness::new();
    // bob's repo, but locked to alice: the default push is refused…
    let (dir, id) = h.repo("override", &h.server.url("bob", "site"));
    repo_binding_set_impl(
        &h.st,
        id,
        BindingInput {
            remote: None,
            account_id: h.alice.clone(),
            identity_id: None,
            transport: None,
            locked: true,
            source: None,
        },
    )
    .unwrap();
    h.commit_file(&dir, id, "a.txt", b"a");
    let err = h.push(&dir, id).unwrap_err();
    assert!(err.to_string().contains("403"), "{err}");

    // …"Push as bob" goes through as bob, without changing the binding.
    let ctx = context_with(&h.st, id, Some("origin"), Some(&as_account(&h.bob))).unwrap();
    h.st.git
        .run(&dir, &ctx, ["push", "-q", "-u", "origin", "main"])
        .unwrap();
    let events = h.server.events();
    assert!(events.iter().any(|e| e.repo == "bob/site"
        && e.user.as_deref() == Some("bob")
        && e.kind == EventKind::GitWrite));
    assert!(events.iter().any(|e| e.repo == "bob/site"
        && e.user.as_deref() == Some("alice")
        && e.kind == EventKind::Forbidden));
    assert_eq!(
        repo_account_context_impl(&h.st, id).unwrap().source,
        ContextSource::Locked
    );
    h.assert_no_secret_in_config(&dir);
}

#[test]
fn fetch_all_uses_each_remotes_own_account() {
    use nanomgg_git_engine::progress::CancelToken;
    use nanomgg_git_engine::remote::FetchOptions;

    let h = Harness::new();
    // Seed both server repos as their owners.
    for (owner, name) in [("alice", "app"), ("bob", "site")] {
        let (dir, id) = h.repo(&format!("seed-{owner}"), &h.server.url(owner, name));
        h.commit_file(&dir, id, "seed.txt", owner.as_bytes());
        h.push(&dir, id).unwrap();
    }
    let (dir, id) = h.repo("both", &h.server.url("alice", "app"));
    h.st.git
        .run(
            &dir,
            &Harness::none(),
            ["remote", "add", "upstream", &h.server.url("bob", "site")],
        )
        .unwrap();
    let before = h.server.events().len();

    let opts = FetchOptions::default();
    let summary =
        crate::branches::fetch_all(&h.st, id, &dir, &opts, &CancelToken::new(), &mut |_| {})
            .unwrap()
            .unwrap();
    assert!(summary.updated_refs >= 2, "{summary:?}");
    let events = h.server.events()[before..].to_vec();
    assert!(
        !events.iter().any(|e| e.kind == EventKind::Forbidden),
        "{events:#?}"
    );
    for (user, repo) in [("alice", "alice/app"), ("bob", "bob/site")] {
        assert!(
            events.iter().any(|e| e.repo == repo
                && e.user.as_deref() == Some(user)
                && e.kind == EventKind::GitRead),
            "{events:#?}"
        );
    }
    for r in ["origin/main", "upstream/main"] {
        h.st.git
            .run(&dir, &Harness::none(), ["rev-parse", "--verify", r])
            .unwrap();
    }
}
