//! CLI account sync against a fake `gh` (script in a temp folder, never the real CLI) and a
//! wiremock GitHub Enterprise.

use std::path::{Path, PathBuf};
use std::time::Duration;

use nanomgg_accounts::{AccountSecret, AuthMethod, ProviderKind};
use nanomgg_storage::Storage;
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::accounts::commands::{AccountInput, account_save_impl, account_secret};

fn state() -> AppState {
    AppState::new(Storage::open_in_memory().unwrap())
}

fn runtime() -> AuthRuntime {
    AuthRuntime::new().unwrap()
}

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// Fake gh ≥ 2.40: `auth token --hostname <h> --user <u>` prints `gho_<u>`, except for `erin`
/// (signed out).
fn fake_gh(dir: &Path) -> PathBuf {
    if cfg!(windows) {
        let p = dir.join("gh.cmd");
        write(
            &p,
            "@echo off\r\n\
             if not \"%1 %2 %3\"==\"auth token --hostname\" exit /b 2\r\n\
             if not \"%5\"==\"--user\" (echo gho_alice& exit /b 0)\r\n\
             if \"%6\"==\"erin\" (echo no oauth token found for %4 account erin 1>&2& exit /b 1)\r\n\
             echo gho_%6\r\n",
        );
        p
    } else {
        let p = dir.join("gh");
        write(
            &p,
            "#!/bin/sh\n\
             [ \"$1 $2 $3\" = \"auth token --hostname\" ] || exit 2\n\
             [ \"$5\" = \"--user\" ] || { echo gho_alice; exit 0; }\n\
             [ \"$6\" = erin ] && { echo \"no oauth token found for $4 account erin\" >&2; exit 1; }\n\
             echo \"gho_$6\"\n",
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        p
    }
}

async fn mount_user(server: &MockServer, token: &str, login: &str, scopes: &str) {
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .and(header("authorization", format!("Bearer {token}").as_str()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-oauth-scopes", scopes)
                .set_body_json(json!({
                    "id": 7, "login": login, "name": null,
                    "avatar_url": null, "html_url": "https://ghe.example/x", "email": null
                })),
        )
        .mount(server)
        .await;
}

struct Fixture {
    _tmp: tempfile::TempDir,
    st: AppState,
    env: CliEnv,
    host: String,
    hosts_file: PathBuf,
    provider_id: String,
}

fn ghe(st: &AppState, server: &MockServer) -> Provider {
    provider_save_impl(
        st,
        ProviderInput {
            id: None,
            kind: ProviderKind::GithubEnterprise,
            name: "GHE test".into(),
            base_url: server.uri(),
            api_url: Some(format!("{}/api/v3", server.uri())),
            oauth_client_id: None,
            large_file_warn_mb: None,
            large_file_block_mb: None,
        },
    )
    .unwrap()
}

fn account(st: &AppState, provider_id: &str, user: &str) -> String {
    account_save_impl(
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
            label: None,
            default_clone_dir: None,
        },
    )
    .unwrap()
    .id
}

fn hosts_yml(host: &str, users: &[&str]) -> String {
    let mut s = format!("\"{host}\":\n    users:\n");
    for u in users {
        s.push_str(&format!("        {u}:\n"));
    }
    s.push_str("    git_protocol: https\n    user: alice\n");
    s
}

fn fixture(server: &MockServer, users: &[&str]) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let st = state();
    let p = ghe(&st, server);
    let host = p.host().unwrap();
    let gh_dir = tmp.path().join("gh-config");
    let hosts_file = gh_dir.join("hosts.yml");
    write(&hosts_file, &hosts_yml(&host, users));
    let bin = tmp.path().join("bin");
    let env = CliEnv {
        home: tmp.path().join("home"),
        appdata: None,
        scan: ScanEnv {
            gh_config_dir: Some(gh_dir),
            ..ScanEnv::default()
        },
        gh: Some(fake_gh(&bin)),
        glab: None,
        timeout: Duration::from_secs(20),
    };
    Fixture {
        _tmp: tmp,
        st,
        env,
        host,
        hosts_file,
        provider_id: p.id,
    }
}

fn r(host: &str, user: &str) -> CliAccountRef {
    CliAccountRef {
        tool: CliTool::Gh,
        host: host.into(),
        username: Some(user.into()),
    }
}

#[tokio::test]
async fn scan_marks_existing_accounts_and_honours_dismissal() {
    let server = MockServer::start().await;
    let f = fixture(&server, &["alice", "Frank"]);
    account(&f.st, &f.provider_id, "frank");

    let scan = scan_impl(&f.st, &f.env).unwrap();
    assert_eq!(scan.accounts.len(), 2);
    let alice = scan
        .accounts
        .iter()
        .find(|a| a.username.as_deref() == Some("alice"))
        .unwrap();
    assert!(alice.existing_account_id.is_none() && alice.active);
    assert_eq!(alice.provider_id.as_deref(), Some(f.provider_id.as_str()));
    assert_eq!(alice.key, format!("gh|{}|alice", f.host));
    let frank = scan
        .accounts
        .iter()
        .find(|a| a.username.as_deref() == Some("Frank"))
        .unwrap();
    assert!(
        frank.existing_account_id.is_some(),
        "case-insensitive match"
    );
    assert_eq!(scan.new_count, 1);
    assert!(scan.should_notify);
    let gh = scan.tools.iter().find(|t| t.tool == CliTool::Gh).unwrap();
    assert!(gh.installed && gh.config_file.is_some());
    assert!(
        !scan
            .tools
            .iter()
            .find(|t| t.tool == CliTool::Glab)
            .unwrap()
            .installed
    );

    dismiss_impl(&f.st, vec![alice.key.clone()]).unwrap();
    let scan = scan_impl(&f.st, &f.env).unwrap();
    assert!(!scan.should_notify, "dismissed set unchanged");
    assert_eq!(scan.new_count, 1);

    // A new CLI account brings the notice back.
    write(
        &f.hosts_file,
        &hosts_yml(&f.host, &["alice", "Frank", "bob"]),
    );
    let scan = scan_impl(&f.st, &f.env).unwrap();
    assert!(scan.should_notify);
    assert_eq!(scan.new_count, 2);
}

#[tokio::test]
async fn import_reports_each_outcome_and_never_touches_the_cli_config() {
    let server = MockServer::start().await;
    mount_user(
        &server,
        "gho_alice",
        "alice",
        "repo, read:user, user:email, workflow",
    )
    .await;
    mount_user(&server, "gho_bob", "bob", "repo, gist, read:org").await;
    mount_user(
        &server,
        "gho_carol",
        "mallory",
        "repo, user:email, workflow",
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user"))
        .and(header("authorization", "Bearer gho_dave"))
        .respond_with(
            ResponseTemplate::new(401).set_body_json(json!({"message": "Bad credentials"})),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let f = fixture(&server, &["alice", "bob", "carol", "dave", "erin", "frank"]);
    let frank = account(&f.st, &f.provider_id, "frank");
    let before = std::fs::read(&f.hosts_file).unwrap();
    let rt = runtime();

    let refs = ["alice", "bob", "carol", "dave", "erin", "frank", "ghost"]
        .into_iter()
        .map(|u| r(&f.host, u))
        .collect();
    let items = import_impl(&f.st, &rt, &f.env, refs).await.unwrap();
    let statuses: Vec<_> = items.iter().map(|i| i.status).collect();
    assert_eq!(
        statuses,
        [
            CliImportStatus::Added,
            CliImportStatus::MissingScopes,
            CliImportStatus::WrongUser,
            CliImportStatus::InvalidToken,
            CliImportStatus::NoToken,
            CliImportStatus::AlreadyExists,
            CliImportStatus::NotFound,
        ],
        "{items:#?}"
    );
    assert_eq!(items[1].missing_scopes, ["user:email", "workflow"]);
    assert_eq!(
        items[1].fix_command.as_deref(),
        Some(
            format!(
                "gh auth switch -h {h} -u bob && gh auth refresh -h {h} -s user:email,workflow",
                h = f.host
            )
            .as_str()
        ),
        "bob is not the active gh account"
    );
    assert_eq!(items[2].actual_username.as_deref(), Some("mallory"));
    assert!(items[4].message.as_deref().unwrap().contains("erin"));
    assert_eq!(items[5].account_id.as_deref(), Some(frank.as_str()));
    let json = serde_json::to_string(&items).unwrap();
    assert!(!json.contains("gho_"), "no token in the results: {json}");

    // Only alice and bob were added, as imported CLI accounts with their own token.
    let accounts = f.st.storage.accounts().unwrap();
    let mut names: Vec<_> = accounts.iter().map(|a| a.username.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["alice", "bob", "frank"]);
    let alice = accounts.iter().find(|a| a.username == "alice").unwrap();
    assert_eq!(alice.auth_method, AuthMethod::ImportedCli);
    assert_eq!(items[0].account_id.as_deref(), Some(alice.id.as_str()));
    assert_eq!(
        account_secret(&f.st, &alice.id, AccountSecret::Token)
            .unwrap()
            .unwrap()
            .as_str(),
        "gho_alice"
    );
    let bob = accounts.iter().find(|a| a.username == "bob").unwrap();
    assert_eq!(
        account_secret(&f.st, &bob.id, AccountSecret::Token)
            .unwrap()
            .unwrap()
            .as_str(),
        "gho_bob"
    );
    // Health treats the imported token like a PAT.
    let health = service::health(&f.st, &bob.id).unwrap();
    assert!(health.has_token && !health.needs_sign_in && !health.auto_refresh);
    assert!(
        health
            .missing_scopes
            .iter()
            .any(|m| m.feature == Feature::ReadEmails)
    );
    assert_eq!(
        std::fs::read(&f.hosts_file).unwrap(),
        before,
        "gh config untouched"
    );

    // Scanning again: alice and bob are now known, as updatable imported accounts.
    let scan = scan_impl(&f.st, &f.env).unwrap();
    assert_eq!(scan.new_count, 3, "carol, dave, erin");
    let entry = |u: &str| {
        scan.accounts
            .iter()
            .find(|a| a.username.as_deref() == Some(u))
            .unwrap()
    };
    assert!(entry("bob").can_update);
    assert!(entry("frank").existing_account_id.is_some() && !entry("frank").can_update);

    // After `gh auth refresh -s user:email,workflow`, importing bob again copies the new token.
    server.reset().await;
    mount_user(
        &server,
        "gho_bob",
        "bob",
        "repo, read:user, user:email, workflow",
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/v3/user/emails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let again = import_impl(
        &f.st,
        &rt,
        &f.env,
        vec![r(&f.host, "BOB"), r(&f.host, "frank")],
    )
    .await
    .unwrap();
    assert_eq!(again[0].status, CliImportStatus::Added, "{again:#?}");
    assert_eq!(again[0].account_id.as_deref(), Some(bob.id.as_str()));
    assert_eq!(again[1].status, CliImportStatus::AlreadyExists);
    let bob_now = f.st.storage.account(&bob.id).unwrap().unwrap();
    assert!(bob_now.scopes.iter().any(|s| s == "workflow"));
    assert_eq!(f.st.storage.accounts().unwrap().len(), 3);
}

#[tokio::test]
async fn missing_cli_is_reported_without_running_anything() {
    let server = MockServer::start().await;
    let mut f = fixture(&server, &["alice"]);
    f.env.gh = None;
    let rt = runtime();
    let items = import_impl(&f.st, &rt, &f.env, vec![r(&f.host, "alice")])
        .await
        .unwrap();
    assert_eq!(items[0].status, CliImportStatus::CliMissing);
    assert!(f.st.storage.accounts().unwrap().is_empty());
    let scan = scan_impl(&f.st, &f.env).unwrap();
    assert!(!scan.tools[0].installed);
}

#[test]
fn new_hosts_get_an_enterprise_or_self_hosted_provider_once() {
    let st = state();
    let gh = CliAccountEntry {
        key: "gh|ghe.acme.vn|an".into(),
        tool: CliTool::Gh,
        provider: ImportProviderKind::GithubEnterprise,
        host: "ghe.acme.vn".into(),
        api_host: None,
        username: Some("an".into()),
        active: true,
        token_storage: TokenStorage::Keyring,
        existing_account_id: None,
        can_update: false,
        provider_id: None,
    };
    let (draft, input) = target_provider(&st, &gh).unwrap();
    assert_eq!(draft.kind, ProviderKind::GithubEnterprise);
    assert_eq!(draft.api_url, "https://ghe.acme.vn/api/v3");
    let saved = provider_save_impl(&st, input.unwrap()).unwrap();
    assert_eq!(saved.base_url, "https://ghe.acme.vn");
    // A second account on the same host reuses the row created for the first one.
    let (p, input) = target_provider(
        &st,
        &CliAccountEntry {
            key: "gh|ghe.acme.vn|bo".into(),
            username: Some("bo".into()),
            ..gh.clone()
        },
    )
    .unwrap();
    assert!(input.is_none());
    assert_eq!(p.id, saved.id);

    let glab = CliAccountEntry {
        key: "glab|gitlab.acme.vn|an".into(),
        tool: CliTool::Glab,
        provider: ImportProviderKind::GitlabSelfHosted,
        host: "gitlab.acme.vn".into(),
        api_host: Some("gitlab-api.acme.vn".into()),
        ..gh
    };
    let (draft, _) = target_provider(&st, &glab).unwrap();
    assert_eq!(draft.kind, ProviderKind::GitlabSelfHosted);
    assert_eq!(draft.api_url, "https://gitlab-api.acme.vn/api/v4");
    // github.com / gitlab.com map to the built-in providers.
    let builtin = CliAccountEntry {
        host: "GitHub.com".into(),
        ..glab
    };
    let (p, input) = target_provider(&st, &builtin).unwrap();
    assert!(input.is_none());
    assert_eq!(p.id, "github");
}
