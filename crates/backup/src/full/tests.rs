use super::*;
use nanomgg_accounts::{Account, AuthMethod, SshKey};
use nanomgg_credentials::MemorySecretStore;

use crate::{BackupKind, KdfCost, crypto};

const PW: &str = "backup-password";

fn account(id: &str, user: &str) -> Account {
    Account {
        id: id.into(),
        provider_id: "github".into(),
        username: user.into(),
        display_name: None,
        avatar_url: None,
        auth_method: AuthMethod::Pat,
        scopes: vec![],
        token_expires_at: None,
        color: "#336699".into(),
        label: None,
        default_clone_dir: None,
        secrets: vec![],
        sort_order: 0,
        created_at: 0,
        updated_at: 0,
    }
}

/// A data folder with a database (settings, notes, themes, a repo, an account with a token)
/// and an SSH key in the app key store.
fn source(root: &Path) -> (PathBuf, Storage, MemorySecretStore) {
    let data = root.join("Roaming").join("app");
    let storage = Storage::open(&data.join(DB_FILE)).unwrap();
    storage.set_setting("language", "vi").unwrap();
    storage
        .set_setting(
            "notes.items",
            &serde_json::json!([{ "id": 1 }, { "id": 2 }]),
        )
        .unwrap();
    storage
        .set_setting("theme.custom", &serde_json::json!([{ "id": "t1" }]))
        .unwrap();
    storage
        .touch_repo("D:/Work/api", "d:/work/api", "api", 1)
        .unwrap();
    storage.insert_account(&account("acc_1", "an-dev")).unwrap();
    storage
        .set_account_secrets("acc_1", &[AccountSecret::Token])
        .unwrap();
    let store = KeyStore::new(&data);
    let key = nanomgg_ssh_keys::generate_ed25519("an@x", None).unwrap();
    let stored = store.save_generated("key_1", &key).unwrap();
    storage
        .insert_ssh_key(&SshKey {
            id: "key_1".into(),
            account_id: Some("acc_1".into()),
            key_type: "ed25519".into(),
            fingerprint: key.info.fingerprint.clone(),
            public_key: key.info.public_key.clone(),
            private_key_path: Some(stored.private_key_path),
            has_passphrase: true,
            uploaded_to_provider: false,
            provider_key_id: None,
            comment: None,
            created_at: 0,
        })
        .unwrap();
    let secrets = MemorySecretStore::default();
    secrets
        .set(&SecretKey::account("acc_1", SecretKind::Token), "ghp_one")
        .unwrap();
    secrets
        .set(&SecretKey::ssh_passphrase("key_1"), "key pass")
        .unwrap();
    secrets
        .set(&SecretKey::new("ai_openai", SecretKind::Token), "sk-ai")
        .unwrap();
    (data, storage, secrets)
}

/// Payload → encrypted file → payload, like the app does.
fn through_file(payload: &FullPayload) -> (BackupInfo, FullPayload) {
    let json = Zeroizing::new(serde_json::to_vec(payload).unwrap());
    let file = crypto::seal(BackupKind::Full, &json, PW, "1.0.0", KdfCost::TEST).unwrap();
    let (info, plain) = crypto::open(&file, PW, Some(BackupKind::Full)).unwrap();
    (info, serde_json::from_slice(&plain).unwrap())
}

/// The new machine: another data folder that already has a (different) database.
fn target(root: &Path) -> PathBuf {
    let data = root.join("Other").join("data");
    let storage = Storage::open(&data.join(DB_FILE)).unwrap();
    storage.set_setting("language", "en").unwrap();
    storage
        .insert_account(&account("acc_new", "someone"))
        .unwrap();
    data
}

#[test]
fn backup_restores_database_secrets_and_keys_into_another_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let (src_data, src, src_secrets) = source(tmp.path());
    let payload = create(&src, &src_secrets, &src_data, true).unwrap();
    assert_eq!(payload.secrets.len(), 3, "token, passphrase, AI key");
    assert_eq!(payload.files.len(), 2, "private + public key");
    let (info, payload) = through_file(&payload);

    let preview = inspect(&payload).unwrap();
    assert_eq!((preview.accounts, preview.repos), (1, 1));
    assert_eq!((preview.notes, preview.custom_themes), (Some(2), Some(1)));
    assert_eq!((preview.secrets, preview.ssh_key_files), (3, 1));
    assert!(preview.includes_secrets);

    let data = target(tmp.path());
    let secrets = MemorySecretStore::default();
    let staged = stage_restore(&payload, &info, &data, &secrets).unwrap();
    assert_eq!((staged.secrets, staged.ssh_key_files), (3, 1));
    assert!(has_pending_restore(&data));
    // The live database is untouched until the next start.
    let live = Storage::open(&data.join(DB_FILE)).unwrap();
    assert_eq!(
        live.get_setting::<String>("language").unwrap().as_deref(),
        Some("en")
    );
    drop(live);

    let applied = apply_pending_restore(&data).unwrap().unwrap();
    assert_eq!(applied.created_at, Some(info.created_at));
    assert_eq!(applied.app_version.as_deref(), Some("1.0.0"));
    let previous = applied.previous_copy.unwrap();
    assert!(previous.starts_with(data.join(BACKUPS_DIR)));
    let old = Storage::open(&previous).unwrap();
    assert_eq!(
        old.accounts().unwrap()[0].id,
        "acc_new",
        "previous database kept"
    );
    drop(old);
    assert!(!has_pending_restore(&data));
    assert!(apply_pending_restore(&data).is_none(), "applied once");

    let db = Storage::open(&data.join(DB_FILE)).unwrap();
    assert_eq!(
        db.get_setting::<String>("language").unwrap().as_deref(),
        Some("vi")
    );
    assert_eq!(db.recent_repos(10).unwrap().len(), 1);
    let accounts = db.accounts().unwrap();
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "acc_1");
    assert_eq!(accounts[0].secrets, [AccountSecret::Token]);
    let key = db.ssh_key("key_1").unwrap().unwrap();
    let store = KeyStore::new(&data);
    let path = store.private_key_path("key_1").unwrap();
    assert_eq!(
        key.private_key_path.as_deref(),
        Some(path.to_str().unwrap())
    );
    assert!(path.is_file());
    assert!(store.check_permissions("key_1").unwrap().restricted);
    for (key, value) in [
        (SecretKey::account("acc_1", SecretKind::Token), "ghp_one"),
        (SecretKey::ssh_passphrase("key_1"), "key pass"),
        (SecretKey::new("ai_openai", SecretKind::Token), "sk-ai"),
    ] {
        assert_eq!(secrets.get(&key).unwrap().unwrap().as_str(), value);
    }
}

#[test]
fn restore_without_secrets_asks_to_sign_in_again() {
    let tmp = tempfile::tempdir().unwrap();
    let (src_data, src, src_secrets) = source(tmp.path());
    let payload = create(&src, &src_secrets, &src_data, false).unwrap();
    assert!(payload.secrets.is_empty() && payload.files.is_empty());
    let text = serde_json::to_string(&payload).unwrap();
    assert!(!text.contains("ghp_one") && !text.contains("PRIVATE KEY"));
    let (info, payload) = through_file(&payload);
    assert!(!inspect(&payload).unwrap().includes_secrets);

    let data = target(tmp.path());
    let secrets = MemorySecretStore::default();
    stage_restore(&payload, &info, &data, &secrets).unwrap();
    apply_pending_restore(&data).unwrap().unwrap();
    let db = Storage::open(&data.join(DB_FILE)).unwrap();
    assert!(db.account("acc_1").unwrap().unwrap().secrets.is_empty());
    assert!(secrets.is_empty());
}

fn with_user_version(payload: &FullPayload, version: u32) -> FullPayload {
    let mut db = B64.decode(&payload.db).unwrap();
    db[60..64].copy_from_slice(&version.to_be_bytes());
    FullPayload {
        db: B64.encode(db),
        ..payload.clone()
    }
}

#[test]
fn a_backup_from_a_newer_app_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let (src_data, src, src_secrets) = source(tmp.path());
    let payload = create(&src, &src_secrets, &src_data, false).unwrap();
    let newer = with_user_version(&payload, nanomgg_storage::LATEST_VERSION + 1);
    assert!(matches!(
        inspect(&newer),
        Err(BackupError::SchemaTooNew { .. })
    ));
    let data = target(tmp.path());
    let info = BackupInfo {
        kind: BackupKind::Full,
        created_at: 1,
        app_version: "9.0.0".into(),
    };
    let err = stage_restore(&newer, &info, &data, &MemorySecretStore::default()).unwrap_err();
    assert!(matches!(err, BackupError::SchemaTooNew { .. }), "{err}");
    assert!(!has_pending_restore(&data));
    assert!(!data.join(format!("{PENDING_FILE}.tmp")).exists());

    let junk = FullPayload {
        db: B64.encode(b"not a database"),
        ..payload
    };
    assert!(matches!(inspect(&junk), Err(BackupError::Corrupt(_))));
}

#[test]
fn a_broken_staged_database_keeps_the_current_one() {
    let tmp = tempfile::tempdir().unwrap();
    let data = target(tmp.path());
    std::fs::write(data.join(PENDING_FILE), b"SQLite format 3\0 but broken").unwrap();
    std::fs::write(data.join(PENDING_META), b"{}").unwrap();
    let err = apply_pending_restore(&data).unwrap().unwrap_err();
    assert!(err.contains("cannot be read"), "{err}");
    assert!(!has_pending_restore(&data), "not retried on every start");
    assert!(data.join(FAILED_FILE).is_file());
    assert!(!data.join(PENDING_META).exists());
    let db = Storage::open(&data.join(DB_FILE)).unwrap();
    assert_eq!(
        db.get_setting::<String>("language").unwrap().as_deref(),
        Some("en")
    );
    assert!(!data.join(BACKUPS_DIR).exists(), "nothing copied");
}

#[test]
fn only_a_few_pre_restore_copies_are_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(BACKUPS_DIR);
    std::fs::create_dir_all(&dir).unwrap();
    for name in [
        "pre-restore-20260101-000000.db",
        "pre-restore-20260102-000000.db",
        "pre-restore-20260103-000000.db",
        "pre-restore-20260104-000000.db",
        "other.db",
    ] {
        std::fs::write(dir.join(name), b"x").unwrap();
    }
    prune_previous(&dir);
    let mut left: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            "other.db",
            "pre-restore-20260102-000000.db",
            "pre-restore-20260103-000000.db",
            "pre-restore-20260104-000000.db"
        ]
    );
}

#[test]
fn key_paths_follow_the_data_folder() {
    let to = Path::new("D:/Data/app");
    let moved = |v: &str, from: &str| rebase_path(v, from, to).map(|s| s.replace('\\', "/"));
    assert_eq!(
        moved(
            r"C:\Users\An\AppData\Roaming\app\ssh\key_1",
            r"C:\Users\An\AppData\Roaming\app"
        )
        .as_deref(),
        Some("D:/Data/app/ssh/key_1")
    );
    assert_eq!(
        moved(
            "c:/users/an/appdata/roaming/APP/ssh/key_1.pub",
            r"C:\Users\An\AppData\Roaming\app\"
        )
        .as_deref(),
        Some("D:/Data/app/ssh/key_1.pub")
    );
    assert_eq!(
        moved(
            "/Users/an/Library/Application Support/app/ssh/k",
            "/Users/an/Library/Application Support/app"
        )
        .as_deref(),
        Some("D:/Data/app/ssh/k")
    );
    assert_eq!(
        moved("C:/Users/An/.ssh/id_ed25519", "C:/Users/An/AppData"),
        None
    );
    assert_eq!(moved("C:/app-other/ssh/k", "C:/app"), None);
    assert_eq!(moved("C:/app", "C:/app"), None);
}

#[test]
fn utc_stamps() {
    assert_eq!(utc_stamp(0), "19700101-000000");
    assert_eq!(utc_stamp(1_760_000_000_000), "20251009-085320");
    assert_eq!(utc_stamp((951_782_400 + 3661) * 1000), "20000229-010101");
}
