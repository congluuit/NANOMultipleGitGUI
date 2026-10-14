//! Clone commands: destination template, URL guess, per-account list errors, and a real clone
//! as an account through the `nanomgg-credential` sidecar against the local test server.

use std::path::Path;
use std::sync::Arc;

use nanomgg_accounts::{AccountSecret, AuthMethod, BindingSource, ProviderKind, Transport};
use nanomgg_credentials::{CredentialServer, KeychainSource, MemorySecretStore};
use nanomgg_git_engine::progress::CancelToken;
use nanomgg_git_engine::{AccountContext, GitRunner, repo};
use nanomgg_storage::Storage;
use nanomgg_test_git_server::TestGitServer;
use zeroize::Zeroizing;

use super::*;
use crate::accounts::commands::{
    AccountInput, IdentityInput, ProviderInput, RuleInput, account_save_impl,
    account_save_token_impl, identity_save_impl, provider_save_impl, rule_save_impl,
};
use crate::accounts::{AccountsRuntime, StorageDirectory, sidecar_path};

const ALICE_TOKEN: &str = "alice-clone-token-0123";
const BOB_TOKEN: &str = "bob-clone-token-4567";

fn vars(owner: &str, repo: &str) -> TemplateVars {
    TemplateVars {
        clone_dir: "D:/Work/".into(),
        owner: owner.into(),
        repo: repo.into(),
        host: "gitlab.acme.dev:8443".into(),
        account: "alice".into(),
    }
}

fn native(p: &str) -> PathBuf {
    PathBuf::from(p.replace('/', std::path::MAIN_SEPARATOR_STR))
}

#[test]
fn template_renders_owner_repo_under_the_clone_dir() {
    let v = vars("acme/platform", "api");
    assert_eq!(
        render_template(DEFAULT_TEMPLATE, &v),
        native("D:/Work/acme/platform/api")
    );
    assert_eq!(
        render_template("{clone_dir}/{owner}/{repo}", &vars("", "dotfiles")),
        native("D:/Work/dotfiles")
    );
    assert_eq!(
        render_template("{host}/{account}/{repo}", &v),
        native("D:/Work/gitlab.acme.dev_8443/alice/api")
    );
    // Values cannot escape the folder or use characters Windows rejects.
    assert_eq!(
        render_template(DEFAULT_TEMPLATE, &vars("../..", "a:b*?.")),
        native("D:/Work/_/_/a_b__")
    );
    if cfg!(windows) {
        assert_eq!(
            render_template("E:/Clones/{repo}", &v),
            PathBuf::from(r"E:\Clones\api")
        );
    }
}

/// App state over an in-memory database (no credential server).
fn state() -> AppState {
    AppState::new(Storage::open_in_memory().unwrap())
}

fn provider(st: &AppState, kind: ProviderKind, base_url: &str) -> String {
    provider_save_impl(
        st,
        ProviderInput {
            id: None,
            kind,
            name: base_url.into(),
            base_url: base_url.into(),
            api_url: None,
            oauth_client_id: None,
            large_file_warn_mb: None,
            large_file_block_mb: None,
        },
    )
    .unwrap()
    .id
}

fn account(st: &AppState, provider_id: &str, user: &str, clone_dir: Option<&str>) -> String {
    let id = account_save_impl(
        st,
        AccountInput {
            id: None,
            provider_id: provider_id.into(),
            username: user.into(),
            display_name: None,
            avatar_url: None,
            auth_method: AuthMethod::Pat,
            scopes: vec![],
            token_expires_at: None,
            color: None,
            label: Some(user.to_uppercase()),
            default_clone_dir: clone_dir.map(str::to_owned),
        },
    )
    .unwrap()
    .id;
    identity_save_impl(
        st,
        IdentityInput {
            id: None,
            account_id: id.clone(),
            name: user.to_uppercase(),
            email: format!("{user}@acme.dev"),
            signing: Default::default(),
            signing_key_ref: None,
            is_default: false,
        },
    )
    .unwrap();
    id
}

fn rule(st: &AppState, host: &str, owner: &str, account_id: &str) {
    rule_save_impl(
        st,
        RuleInput {
            id: None,
            priority: None,
            host: Some(host.into()),
            owner_or_group: Some(owner.into()),
            path_glob: None,
            remote_regex: None,
            account_id: account_id.into(),
            identity_id: None,
            enabled: true,
        },
    )
    .unwrap();
}

#[test]
fn guess_lists_rule_matches_then_accounts_on_the_host() {
    let st = state();
    let ghe = provider(&st, ProviderKind::GithubEnterprise, "https://git.acme.dev/");
    let gh = st
        .storage
        .providers()
        .unwrap()
        .into_iter()
        .find(|p| p.kind == ProviderKind::Github)
        .expect("built-in github.com provider")
        .id;
    let alice = account(&st, &ghe, "alice", None);
    let bob = account(&st, &ghe, "bob", None);
    let carol = account(&st, &gh, "carol", None);
    rule(&st, "git.acme.dev", "acme", &bob);

    let g = clone_guess_impl(&st, "https://git.acme.dev/acme/api.git").unwrap();
    assert!(g.valid && !g.local);
    assert_eq!(g.transport, Some(Transport::Https));
    assert_eq!(g.owner.as_deref(), Some("acme"));
    assert_eq!(g.repo.as_deref(), Some("api"));
    let ids: Vec<_> = g
        .candidates
        .iter()
        .map(|c| (c.account_id.as_str(), c.reason))
        .collect();
    assert_eq!(
        ids,
        [
            (bob.as_str(), GuessReason::Rule),
            (alice.as_str(), GuessReason::Host)
        ]
    );
    assert_eq!(g.ssh_url.as_deref(), Some("git@git.acme.dev:acme/api.git"));

    // SSH URL, no rule: every account on the host, HTTPS form from the provider base URL.
    let g = clone_guess_impl(&st, "git@git.acme.dev:other/tool.git").unwrap();
    assert_eq!(g.transport, Some(Transport::Ssh));
    let ids: Vec<_> = g.candidates.iter().map(|c| c.account_id.clone()).collect();
    assert_eq!(ids, [alice.clone(), bob.clone()]);
    assert_eq!(
        g.https_url.as_deref(),
        Some("https://git.acme.dev/other/tool.git")
    );

    // The account named like the URL's owner comes first among host matches.
    let g = clone_guess_impl(&st, "https://git.acme.dev/bob/tool.git").unwrap();
    let ids: Vec<_> = g.candidates.iter().map(|c| c.account_id.clone()).collect();
    assert_eq!(ids, [bob.clone(), alice.clone()]);

    let g = clone_guess_impl(&st, "https://github.com/rust-lang/rust").unwrap();
    assert_eq!(g.candidates.len(), 1);
    assert_eq!(g.candidates[0].account_id, carol);
    assert!(
        clone_guess_impl(&st, "https://unknown.example/x/y.git")
            .unwrap()
            .candidates
            .is_empty()
    );
    for bad in ["", "-x", "ext::sh -c id"] {
        assert!(!clone_guess_impl(&st, bad).unwrap().valid, "{bad:?}");
    }
    let local = clone_guess_impl(&st, "C:/repos/app.git").unwrap();
    assert!(local.valid && local.local && local.candidates.is_empty());
}

#[test]
fn destination_follows_the_account_folder_template_and_cache() {
    let tmp = tempfile::tempdir().unwrap();
    let st = state();
    let gl = provider(
        &st,
        ProviderKind::GitlabSelfHosted,
        "https://gitlab.acme.dev/",
    );
    let work = tmp.path().join("work");
    let alice = account(&st, &gl, "alice", Some(&work.to_string_lossy()));

    let url = "https://gitlab.acme.dev/acme/platform/api.git";
    let d = clone_destination_impl(&st, Some(&alice), url, None).unwrap();
    assert_eq!(
        PathBuf::from(&d.path),
        work.join("acme").join("platform").join("api")
    );
    assert_eq!(d.template, DEFAULT_TEMPLATE);
    assert_eq!(
        (d.owner.as_str(), d.repo.as_str()),
        ("acme/platform", "api")
    );
    assert!(d.usable && !d.exists);
    assert!(d.free_bytes.is_some_and(|f| f > 0));
    assert_eq!(d.size_bytes, None);

    // Sizes come from the cached list.
    store(
        &alice,
        Ok((
            vec![CloneRepo {
                account_id: alice.clone(),
                id: "1".into(),
                full_name: "acme/platform/api".into(),
                owner: "acme/platform".into(),
                name: "api".into(),
                description: None,
                private: true,
                fork: false,
                parent_full_name: None,
                archived: false,
                default_branch: Some("main".into()),
                clone_https: url.into(),
                clone_ssh: "git@gitlab.acme.dev:acme/platform/api.git".into(),
                web_url: "https://gitlab.acme.dev/acme/platform/api".into(),
                updated_at: Some(1),
                stars: 0,
                size_bytes: Some(1234),
                lfs_enabled: Some(true),
                lfs_size_bytes: Some(4_200_000_000),
                can_push: true,
            }],
            false,
        )),
    );
    let d = clone_destination_impl(
        &st,
        Some(&alice),
        "git@gitlab.acme.dev:ACME/platform/api",
        None,
    )
    .unwrap();
    assert_eq!(
        (d.size_bytes, d.lfs_size_bytes),
        (Some(1234), Some(4_200_000_000))
    );

    // Template setting; an explicit path is only checked.
    st.storage
        .set_setting(KEY_PATH_TEMPLATE, "{clone_dir}/{account}/{repo}")
        .unwrap();
    let d = clone_destination_impl(&st, Some(&alice), url, None).unwrap();
    assert_eq!(PathBuf::from(&d.path), work.join("alice").join("api"));
    std::fs::create_dir_all(work.join("busy")).unwrap();
    std::fs::write(work.join("busy/x"), "x").unwrap();
    let d = clone_destination_impl(
        &st,
        Some(&alice),
        url,
        Some(&work.join("busy").to_string_lossy()),
    )
    .unwrap();
    assert!(d.exists && !d.usable);

    // Without an account the root is `~/Source` (home injected: the real one is never used).
    assert_eq!(
        clone_root_under(Some(tmp.path().into())),
        tmp.path().join("Source")
    );
}

#[test]
fn one_failing_account_does_not_hide_the_others() {
    let st = state();
    // Nothing listens on port 9 (discard): the request fails fast with a network error.
    let ghe = provider(&st, ProviderKind::GithubEnterprise, "http://127.0.0.1:9/");
    let no_token = account(&st, &ghe, "alice", None);
    let unreachable = account(&st, &ghe, "bob", None);
    let fine = account(&st, &ghe, "carol", None);
    account_save_token_impl(
        &st,
        &unreachable,
        AccountSecret::Token,
        Zeroizing::new("tok".into()),
    )
    .unwrap();
    tauri::async_runtime::block_on(async {
        fetch_account(&st, &no_token).await;
        fetch_account(&st, &unreachable).await;
    });
    store(&fine, Ok((Vec::new(), true)));

    let ids = vec![no_token.clone(), unreachable.clone(), fine.clone()];
    let list = snapshot(&ids, &HashSet::new());
    assert_eq!(list.accounts.len(), 3);
    assert_eq!(list.accounts[0].error.as_ref().unwrap().code, "noToken");
    let code = &list.accounts[1].error.as_ref().unwrap().code;
    assert!(
        ["network", "timeout"].contains(&code.as_str()),
        "unexpected error code {code}"
    );
    assert!(list.accounts[2].error.is_none() && list.accounts[2].truncated);
    assert!(list.accounts.iter().all(|a| a.fetched_at.is_some()));

    // A failed refresh keeps the previous repositories.
    store(&fine, Err(ListError::new("rateLimited", "slow down")));
    let list = snapshot(std::slice::from_ref(&fine), &HashSet::new());
    assert!(list.accounts[0].truncated);
    assert_eq!(list.accounts[0].error.as_ref().unwrap().code, "rateLimited");

    // Account filter keeps the user's order and drops unknown ids.
    let wanted = vec![fine.clone(), "acc-missing".into(), no_token.clone()];
    assert_eq!(account_ids(&st, Some(wanted)).unwrap(), [no_token, fine]);
}

// ----- real clone through the sidecar --------------------------------------------------------

struct Harness {
    tmp: tempfile::TempDir,
    server: TestGitServer,
    st: AppState,
    alice: String,
    bob: String,
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
        let storage = Arc::new(Storage::open_in_memory().unwrap());
        let secrets = Arc::new(MemorySecretStore::default());
        let source =
            KeychainSource::new(Arc::new(StorageDirectory(storage.clone())), secrets.clone());
        let creds = CredentialServer::start(Arc::new(source)).unwrap();
        let sidecar = sidecar_path().expect("nanomgg-credential next to the test binary");
        let mut st = AppState::with_accounts(
            storage,
            AccountsRuntime::new(Some(creds), Some(sidecar), secrets),
        );
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
        let ghe = provider(
            &st,
            ProviderKind::GithubEnterprise,
            &format!("http://{}/", server.host()),
        );
        let alice = account(&st, &ghe, "alice", None);
        let bob = account(&st, &ghe, "bob", None);
        for (id, token) in [(&alice, ALICE_TOKEN), (&bob, BOB_TOKEN)] {
            account_save_token_impl(&st, id, AccountSecret::Token, Zeroizing::new(token.into()))
                .unwrap();
        }
        Self {
            tmp,
            server,
            st,
            alice,
            bob,
        }
    }

    /// Pushes one commit to alice/app as alice.
    fn seed(&self) {
        let work = self.tmp.path().join("seed");
        std::fs::create_dir_all(&work).unwrap();
        let ctx = context_for_account(&self.st, &self.alice, None).unwrap();
        let g = |args: &[&str]| self.st.git.run(&work, &ctx, args).unwrap();
        g(&["init", "-q", "-b", "main"]);
        std::fs::write(work.join("README.md"), "app\n").unwrap();
        g(&["add", "."]);
        g(&["commit", "-q", "-m", "seed"]);
        g(&["push", "-q", &self.server.url("alice", "app"), "main"]);
    }

    fn request(&self, account: &str, dest: &str) -> CloneRequest {
        CloneRequest {
            url: self.server.url("alice", "app"),
            account_id: Some(account.to_owned()),
            identity_id: None,
            dest: self.tmp.path().join(dest).to_string_lossy().into_owned(),
            branch: None,
            depth: Some(1),
            recurse_submodules: false,
            lfs: LfsMode::Later,
            write_identity: true,
        }
    }

    /// Registers the clone as an open repo (what `repos::register` does without a window).
    fn open(&self, root: &Path) -> i64 {
        let info = repo::discover(&self.st.git, root).unwrap();
        let path = info.root.to_string_lossy().into_owned();
        let rec = self
            .st
            .storage
            .touch_repo(&path, &crate::repos::path_key(&info.root), "app", 1)
            .unwrap();
        self.st.workspace().repos.insert(rec.id, info);
        rec.id
    }
}

#[test]
fn clone_as_an_account_binds_the_repo_and_writes_its_identity() {
    let h = Harness::new();
    h.seed();

    // Bob may not read alice's repository: refused, nothing left behind.
    let req = h.request(&h.bob, "denied");
    let err = clone_blocking(&h.st, &req, &CancelToken::new(), |_| {}).unwrap_err();
    assert!(!matches!(err, OpError::Cancelled), "{err}");
    assert!(!h.tmp.path().join("denied").exists());

    let req = h.request(&h.alice, "clones/app");
    let out = clone_blocking(&h.st, &req, &CancelToken::new(), |_| {}).unwrap();
    assert_eq!(
        std::fs::read_to_string(out.path.join("README.md")).unwrap(),
        "app\n"
    );
    assert!(!out.uses_lfs && !out.lfs_pending);

    let repo_id = h.open(&out.path);
    assert!(bind_cloned(&h.st, repo_id, &out.path, &req).unwrap());
    let binding = h.st.storage.repo_binding(repo_id, None).unwrap().unwrap();
    assert_eq!(binding.account_id, h.alice);
    assert_eq!(binding.source, BindingSource::Clone);
    assert_eq!(binding.transport, Some(Transport::Https));
    assert!(binding.identity_id.is_some() && !binding.locked);

    let none = AccountContext::default();
    let email =
        h.st.git
            .run_text(&out.path, &none, ["config", "--local", "user.email"])
            .unwrap();
    assert_eq!(email, "alice@acme.dev");
    let config = std::fs::read_to_string(out.path.join(".git/config")).unwrap();
    for secret in [ALICE_TOKEN, "nanomgg-credential", "helper"] {
        assert!(!config.contains(secret), "{secret} in .git/config");
    }
    // The resolver now picks alice through the clone binding.
    let ctx = crate::accounts::context_for(&h.st, repo_id, Some("origin")).unwrap();
    assert_eq!(ctx.identity.unwrap().email, "alice@acme.dev");

    // Without "write identity" the config is left alone.
    let mut req = h.request(&h.alice, "clones/plain");
    req.write_identity = false;
    req.depth = None;
    let out = clone_blocking(&h.st, &req, &CancelToken::new(), |_| {}).unwrap();
    let repo_id = h.open(&out.path);
    assert!(!bind_cloned(&h.st, repo_id, &out.path, &req).unwrap());
    assert!(
        h.st.git
            .run_text(&out.path, &none, ["config", "--local", "user.email"])
            .is_err()
    );
}
