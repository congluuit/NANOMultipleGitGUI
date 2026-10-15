use super::*;
use nanomgg_credentials::MemorySecretStore;

struct Machine {
    storage: Storage,
    secrets: MemorySecretStore,
    store: KeyStore,
    // Last: the folder is removed after the database is closed.
    _dir: tempfile::TempDir,
}

fn machine() -> Machine {
    let dir = tempfile::tempdir().unwrap();
    let store = KeyStore::new(&dir.path().join("data"));
    Machine {
        storage: Storage::open(&dir.path().join("data").join("nanomgg.db")).unwrap(),
        secrets: MemorySecretStore::default(),
        store,
        _dir: dir,
    }
}

fn account(id: &str, provider: &str, user: &str, method: AuthMethod) -> Account {
    Account {
        id: id.into(),
        provider_id: provider.into(),
        username: user.into(),
        display_name: Some(format!("{user} name")),
        avatar_url: None,
        auth_method: method,
        scopes: vec!["repo".into()],
        token_expires_at: None,
        color: "#336699".into(),
        label: Some("Công ty".into()),
        default_clone_dir: Some("D:/Work".into()),
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
        name: "An".into(),
        email: email.into(),
        signing: SigningMethod::None,
        signing_key_ref: None,
        is_default: false,
        created_at: 0,
    }
}

fn rule(id: &str, account: &str, identity: Option<&str>, host: &str) -> Rule {
    Rule {
        id: id.into(),
        priority: 0,
        host: Some(host.into()),
        owner_or_group: Some("acme".into()),
        path_glob: None,
        remote_regex: None,
        account_id: account.into(),
        identity_id: identity.map(Into::into),
        enabled: true,
        created_at: 0,
    }
}

/// Source machine: a GitHub PAT account and a self-hosted GitLab OAuth account with an SSH
/// key used for signing, tokens in the keychain.
fn source() -> Machine {
    let m = machine();
    let s = &m.storage;
    s.insert_provider(&Provider {
        id: "prv_acme".into(),
        kind: ProviderKind::GitlabSelfHosted,
        name: "ACME GitLab".into(),
        base_url: "https://gitlab.acme.dev".into(),
        api_url: "https://gitlab.acme.dev/api/v4".into(),
        oauth_client_id: Some("client-123".into()),
        large_file_warn_mb: 40,
        large_file_block_mb: None,
        builtin: false,
    })
    .unwrap();
    s.set_setting("auth.loopbackPort.prv_acme", &47011).unwrap();
    s.insert_account(&account("acc_gh", "github", "an-dev", AuthMethod::Pat))
        .unwrap();
    s.insert_account(&account(
        "acc_gl",
        "prv_acme",
        "an.nguyen",
        AuthMethod::Oauth,
    ))
    .unwrap();
    s.insert_identity(&identity("idn_gh", "acc_gh", "an@personal.vn"))
        .unwrap();

    let key = nanomgg_ssh_keys::generate_ed25519("an@acme", None).unwrap();
    let stored = m.store.save_generated("key_src", &key).unwrap();
    s.insert_ssh_key(&SshKey {
        id: "key_src".into(),
        account_id: Some("acc_gl".into()),
        key_type: "ed25519".into(),
        fingerprint: key.info.fingerprint.clone(),
        public_key: key.info.public_key.clone(),
        private_key_path: Some(stored.private_key_path.clone()),
        has_passphrase: true,
        uploaded_to_provider: true,
        provider_key_id: Some("77".into()),
        comment: Some("an@acme".into()),
        created_at: 5,
    })
    .unwrap();
    s.set_setting("ssh.defaultKey.acc_gl", "key_src").unwrap();
    m.secrets
        .set(&SecretKey::ssh_passphrase("key_src"), "key pass")
        .unwrap();

    let mut work = identity("idn_work", "acc_gl", "an@acme.dev");
    work.signing = SigningMethod::Ssh;
    work.signing_key_ref = Some(stored.public_key_path.clone());
    s.insert_identity(&work).unwrap();
    s.insert_identity(&identity("idn_alt", "acc_gl", "an.alt@acme.dev"))
        .unwrap();
    s.insert_rule(&rule(
        "rul_1",
        "acc_gl",
        Some("idn_work"),
        "gitlab.acme.dev",
    ))
    .unwrap();
    let mut by_path = rule("rul_2", "acc_gl", None, "gitlab.acme.dev");
    by_path.priority = 5;
    by_path.owner_or_group = Some("acme/platform".into());
    s.insert_rule(&by_path).unwrap();
    s.insert_rule(&rule("rul_gh", "acc_gh", None, "github.com"))
        .unwrap();

    for (acc, token) in [("acc_gh", "ghp_secret"), ("acc_gl", "glo_secret")] {
        m.secrets
            .set(&SecretKey::account(acc, SecretKind::Token), token)
            .unwrap();
    }
    m.secrets
        .set(
            &SecretKey::account("acc_gl", SecretKind::RefreshToken),
            "glr_refresh",
        )
        .unwrap();
    s.set_account_secrets("acc_gh", &[AccountSecret::Token])
        .unwrap();
    s.set_account_secrets(
        "acc_gl",
        &[AccountSecret::Token, AccountSecret::RefreshToken],
    )
    .unwrap();
    s.set_setting(
        "auth.status.acc_gl",
        &serde_json::json!({ "tokenKind": "gitlabOauth", "scopesKnown": true }),
    )
    .unwrap();
    m
}

fn export_all(m: &Machine, with_secrets: bool) -> AccountsPayload {
    let payload = export(&m.storage, &m.secrets, &m.store, &[], with_secrets).unwrap();
    // Through JSON like the real file.
    let json = serde_json::to_vec(&payload).unwrap();
    serde_json::from_slice(&json).unwrap()
}

fn all_keys(p: &AccountsPayload) -> Vec<String> {
    p.accounts.iter().map(|a| a.account.id.clone()).collect()
}

fn by_user(m: &Machine, user: &str) -> Account {
    m.storage
        .accounts()
        .unwrap()
        .into_iter()
        .find(|a| a.username == user)
        .unwrap()
}

#[test]
fn export_without_secrets_holds_no_secret() {
    let src = source();
    let payload = export_all(&src, false);
    let text = serde_json::to_string(&payload).unwrap();
    for secret in [
        "ghp_secret",
        "glo_secret",
        "glr_refresh",
        "key pass",
        "PRIVATE KEY",
    ] {
        assert!(!text.contains(secret), "{secret} leaked");
    }
    assert_eq!(payload.accounts.len(), 2);
    assert_eq!(payload.providers.len(), 2, "github + acme");
    let gl = &payload.accounts[1];
    assert_eq!(gl.identities.len(), 2);
    assert_eq!(gl.rules.len(), 2);
    assert_eq!(gl.ssh_keys.len(), 1, "metadata only");
    assert_eq!(gl.default_ssh_key.as_deref(), Some("key_src"));
    // Only the selected accounts.
    let one = export(
        &src.storage,
        &src.secrets,
        &src.store,
        &["acc_gh".into()],
        false,
    )
    .unwrap();
    assert_eq!(one.accounts.len(), 1);
    assert_eq!(one.providers.len(), 1);
    assert!(matches!(
        export(
            &src.storage,
            &src.secrets,
            &src.store,
            &["nope".into()],
            false
        ),
        Err(BackupError::Invalid(_))
    ));
}

#[test]
fn import_without_secrets_creates_new_records_that_need_sign_in() {
    let src = source();
    let payload = export_all(&src, false);
    // The other machine: the source's key files do not exist there.
    drop(src);
    let dst = machine();
    let preview = preview(&dst.storage, &payload).unwrap();
    assert!(!preview.has_secrets);
    assert!(
        preview
            .accounts
            .iter()
            .all(|a| a.existing_account_id.is_none())
    );
    assert_eq!(
        preview
            .accounts
            .iter()
            .map(|a| a.new_provider)
            .collect::<Vec<_>>(),
        [false, true]
    );

    let report = import(
        &dst.storage,
        &dst.secrets,
        &dst.store,
        &payload,
        &all_keys(&payload),
        false,
    )
    .unwrap();
    assert_eq!((report.added, report.skipped, report.tokens), (2, 0, 0));
    assert_eq!(report.providers_added, 1);
    assert_eq!(report.need_sign_in.len(), 2);
    assert_eq!(
        report.signing_disabled, 1,
        "the signing key file is not here"
    );
    assert!(dst.secrets.is_empty());

    let gl = by_user(&dst, "an.nguyen");
    assert_ne!(gl.id, "acc_gl", "new id");
    assert!(gl.secrets.is_empty());
    assert_eq!(gl.label.as_deref(), Some("Công ty"));
    assert_eq!(gl.default_clone_dir.as_deref(), Some("D:/Work"));
    assert_eq!(gl.auth_method, AuthMethod::Oauth);
    let provider = dst.storage.provider(&gl.provider_id).unwrap().unwrap();
    assert_eq!(provider.base_url, "https://gitlab.acme.dev");
    assert_eq!(provider.oauth_client_id.as_deref(), Some("client-123"));
    assert_eq!(
        dst.storage
            .get_setting::<u16>(&format!("auth.loopbackPort.{}", provider.id))
            .unwrap(),
        Some(47011)
    );
    let identities = dst.storage.identities(Some(&gl.id)).unwrap();
    assert_eq!(identities.len(), 2);
    let work = identities
        .iter()
        .find(|i| i.email == "an@acme.dev")
        .unwrap();
    assert_eq!(work.signing, SigningMethod::None);
    assert!(work.is_default, "default kept");
    let rules: Vec<Rule> = dst
        .storage
        .rules()
        .unwrap()
        .into_iter()
        .filter(|r| r.account_id == gl.id)
        .collect();
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].identity_id.as_deref(), Some(work.id.as_str()));
    assert_eq!(rules[1].owner_or_group.as_deref(), Some("acme/platform"));
    assert!(dst.storage.ssh_keys(None).unwrap().is_empty());
    assert_eq!(
        by_user(&dst, "an-dev").provider_id,
        "github",
        "built-in reused"
    );
}

#[test]
fn import_with_secrets_brings_tokens_and_ssh_keys() {
    let src = source();
    let payload = export_all(&src, true);
    let dst = machine();
    assert!(preview(&dst.storage, &payload).unwrap().has_secrets);
    let report = import(
        &dst.storage,
        &dst.secrets,
        &dst.store,
        &payload,
        &all_keys(&payload),
        false,
    )
    .unwrap();
    assert_eq!((report.added, report.tokens, report.ssh_keys), (2, 2, 1));
    assert!(report.need_sign_in.is_empty());
    assert_eq!(report.signing_disabled, 0);

    let gl = by_user(&dst, "an.nguyen");
    assert!(gl.has_secret(AccountSecret::Token) && gl.has_secret(AccountSecret::RefreshToken));
    let token = dst
        .secrets
        .get(&SecretKey::account(&gl.id, SecretKind::Token))
        .unwrap()
        .unwrap();
    assert_eq!(token.as_str(), "glo_secret");
    let status: serde_json::Value = dst
        .storage
        .get_setting(&format!("auth.status.{}", gl.id))
        .unwrap()
        .unwrap();
    assert_eq!(status["tokenKind"], "gitlabOauth");

    let keys = dst.storage.ssh_keys(Some(&gl.id)).unwrap();
    assert_eq!(keys.len(), 1);
    let key = &keys[0];
    assert_ne!(key.id, "key_src");
    let path = dst.store.private_key_path(&key.id).unwrap();
    assert_eq!(
        key.private_key_path.as_deref(),
        Some(path.to_str().unwrap())
    );
    assert!(path.is_file());
    assert!(dst.store.check_permissions(&key.id).unwrap().restricted);
    assert!(key.uploaded_to_provider);
    assert_eq!(
        dst.secrets
            .get(&SecretKey::ssh_passphrase(&key.id))
            .unwrap()
            .unwrap()
            .as_str(),
        "key pass"
    );
    assert_eq!(
        dst.storage
            .get_setting::<String>(&format!("ssh.defaultKey.{}", gl.id))
            .unwrap()
            .as_deref(),
        Some(key.id.as_str())
    );
    let work = dst
        .storage
        .identities(Some(&gl.id))
        .unwrap()
        .into_iter()
        .find(|i| i.email == "an@acme.dev")
        .unwrap();
    assert_eq!(work.signing, SigningMethod::Ssh);
    assert_eq!(
        work.signing_key_ref.as_deref(),
        Some(format!("{}.pub", path.display()).as_str())
    );
}

#[test]
fn existing_accounts_are_skipped_or_only_relabelled() {
    let src = source();
    let payload = export_all(&src, true);
    let dst = machine();
    let mut mine = account("acc_mine", "github", "AN-DEV", AuthMethod::Pat);
    mine.label = Some("Cá nhân".into());
    mine.color = "#aa0000".into();
    mine.default_clone_dir = None;
    dst.storage.insert_account(&mine).unwrap();
    dst.secrets
        .set(
            &SecretKey::account("acc_mine", SecretKind::Token),
            "ghp_mine",
        )
        .unwrap();

    let p = preview(&dst.storage, &payload).unwrap();
    assert_eq!(
        p.accounts[0].existing_account_id.as_deref(),
        Some("acc_mine")
    );

    let report = import(
        &dst.storage,
        &dst.secrets,
        &dst.store,
        &payload,
        &["acc_gh".into()],
        false,
    )
    .unwrap();
    assert_eq!((report.added, report.skipped, report.updated), (0, 1, 0));
    assert_eq!(
        dst.storage
            .account("acc_mine")
            .unwrap()
            .unwrap()
            .label
            .as_deref(),
        Some("Cá nhân")
    );

    let report = import(
        &dst.storage,
        &dst.secrets,
        &dst.store,
        &payload,
        &["acc_gh".into()],
        true,
    )
    .unwrap();
    assert_eq!((report.added, report.updated), (0, 1));
    let updated = dst.storage.account("acc_mine").unwrap().unwrap();
    assert_eq!(updated.label.as_deref(), Some("Công ty"));
    assert_eq!(updated.color, "#336699");
    assert_eq!(updated.default_clone_dir, None, "only label and color");
    assert_eq!(
        dst.secrets
            .get(&SecretKey::account("acc_mine", SecretKind::Token))
            .unwrap()
            .unwrap()
            .as_str(),
        "ghp_mine",
        "token untouched"
    );
    assert_eq!(dst.storage.accounts().unwrap().len(), 1);
}

#[test]
fn a_failure_undoes_the_whole_import() {
    let src = source();
    let mut payload = export_all(&src, true);
    // The second account fails on its last rule.
    payload.accounts[1].rules[1].remote_regex = Some("(".into());
    let dst = machine();
    let err = import(
        &dst.storage,
        &dst.secrets,
        &dst.store,
        &payload,
        &all_keys(&payload),
        false,
    )
    .unwrap_err();
    assert!(matches!(err, BackupError::Storage(_)), "{err}");
    assert!(dst.storage.accounts().unwrap().is_empty());
    assert_eq!(dst.storage.providers().unwrap().len(), 2, "built-ins only");
    assert!(dst.storage.rules().unwrap().is_empty());
    assert!(dst.storage.ssh_keys(None).unwrap().is_empty());
    assert!(dst.store.list().unwrap().is_empty());
    assert!(dst.secrets.is_empty());
    assert!(
        dst.storage
            .settings()
            .unwrap()
            .keys()
            .all(|k| !k.starts_with("ssh.") && !k.starts_with("auth."))
    );
}

#[test]
fn payload_survives_the_encrypted_envelope() {
    let src = source();
    let payload = export(&src.storage, &src.secrets, &src.store, &[], true).unwrap();
    let json = Zeroizing::new(serde_json::to_vec(&payload).unwrap());
    let file = crate::crypto::seal(
        crate::BackupKind::Accounts,
        &json,
        "password-1",
        "1.0.0",
        crate::KdfCost::TEST,
    )
    .unwrap();
    assert!(!String::from_utf8_lossy(&file).contains("ghp_secret"));
    let (_, plain) =
        crate::crypto::open(&file, "password-1", Some(crate::BackupKind::Accounts)).unwrap();
    let back: AccountsPayload = serde_json::from_slice(&plain).unwrap();
    assert_eq!(back.accounts.len(), 2);
    assert_eq!(
        back.accounts[0]
            .secrets
            .as_ref()
            .unwrap()
            .token
            .as_deref()
            .map(String::as_str),
        Some("ghp_secret")
    );
}
