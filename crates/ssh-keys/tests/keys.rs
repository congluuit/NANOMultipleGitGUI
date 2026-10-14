//! Generation / import / storage round trips, checked against the real OpenSSH tools when they
//! are installed (Git for Windows ships `ssh`, `ssh-keygen`; Windows 10+ has Win32-OpenSSH).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use nanomgg_ssh_keys::{
    HostKeyPolicy, KeyKind, KeyStore, PublicFileCheck, SshCommandOptions, SshKeyError,
    check_permissions, generate_ed25519, git_ssh_command, import_private_key,
    import_private_key_file, inspect_private_key, verify_passphrase,
};

/// Every `ssh-keygen` we can find: PATH, Git for Windows, Win32-OpenSSH.
fn keygens() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut candidates = vec![PathBuf::from("ssh-keygen")];
    if cfg!(windows) {
        candidates.push(PathBuf::from(
            r"C:\Program Files\Git\usr\bin\ssh-keygen.exe",
        ));
        if let Some(root) = std::env::var_os("SystemRoot") {
            candidates.push(Path::new(&root).join(r"System32\OpenSSH\ssh-keygen.exe"));
        }
    }
    for c in candidates {
        // `ssh-keygen -l -f <missing>` fails but proves the binary runs.
        if Command::new(&c)
            .args(["-l", "-f", "nanomgg-missing-file"])
            .output()
            .is_ok()
        {
            found.push(c);
        }
    }
    if found.is_empty() {
        eprintln!("ssh-keygen not found, skipping interop checks");
    }
    found
}

fn run(program: &Path, args: &[&str]) -> Output {
    Command::new(program)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("{}: {e}", program.display()))
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// `<algorithm> <base64>` of a public key line (comment dropped).
fn key_part(line: &str) -> String {
    line.split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}

/// `ssh-keygen -lf` prints `256 SHA256:… comment (ED25519)`.
fn keygen_fingerprint(keygen: &Path, file: &Path) -> String {
    let out = run(
        keygen,
        &["-l", "-E", "sha256", "-f", file.to_str().unwrap()],
    );
    assert!(out.status.success(), "{}: {:?}", keygen.display(), out);
    stdout(&out).split_whitespace().nth(1).unwrap().to_string()
}

#[test]
fn generate_store_reload_without_passphrase() {
    let app = tempfile::tempdir().unwrap();
    let store = KeyStore::new(app.path());
    let key = generate_ed25519("Work <an@acme.vn>", None).unwrap();
    assert_eq!(key.info.kind, KeyKind::Ed25519);
    assert_eq!(key.info.algorithm, "ssh-ed25519");
    assert_eq!(key.info.bits, Some(256));
    assert!(!key.info.encrypted);
    assert!(key.info.fingerprint.starts_with("SHA256:"));
    assert!(key.info.public_key.ends_with(" Work <an@acme.vn>"));
    assert!(!format!("{key:?}").contains("OPENSSH PRIVATE KEY"));

    let stored = store.save_generated("acc1-ed25519", &key).unwrap();
    assert_eq!(stored.info, key.info);
    assert_eq!(
        Path::new(&stored.private_key_path),
        app.path().join("ssh").join("acc1-ed25519")
    );
    let public_text = std::fs::read_to_string(&stored.public_key_path).unwrap();
    assert_eq!(public_text, format!("{}\n", key.info.public_key));

    let loaded = store.load("acc1-ed25519").unwrap();
    assert_eq!(loaded.info, key.info);
    assert_eq!(store.list().unwrap(), ["acc1-ed25519"]);

    let imported = import_private_key_file(Path::new(&stored.private_key_path), None).unwrap();
    assert_eq!(imported.info, key.info);
    assert_eq!(imported.public_file, PublicFileCheck::Matches);

    for keygen in keygens() {
        let private = Path::new(&stored.private_key_path);
        assert_eq!(keygen_fingerprint(&keygen, private), key.info.fingerprint);
        assert_eq!(
            keygen_fingerprint(&keygen, Path::new(&stored.public_key_path)),
            key.info.fingerprint
        );
        // `-y` loads the private key, which also exercises OpenSSH's permission check.
        let out = run(&keygen, &["-y", "-f", private.to_str().unwrap()]);
        assert!(out.status.success(), "{}: {:?}", keygen.display(), out);
        assert_eq!(key_part(&stdout(&out)), key_part(&key.info.public_key));
    }

    assert!(store.remove("acc1-ed25519").unwrap());
    assert!(!store.remove("acc1-ed25519").unwrap());
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn generate_store_reload_with_passphrase() {
    let app = tempfile::tempdir().unwrap();
    let store = KeyStore::new(app.path());
    let key = generate_ed25519("an@acme.vn", Some("đúng mật khẩu")).unwrap();
    assert!(key.info.encrypted);
    assert!(key.private_key.contains("BEGIN OPENSSH PRIVATE KEY"));

    let info = inspect_private_key(&key.private_key).unwrap();
    assert!(info.encrypted);
    assert_eq!(info.fingerprint, key.info.fingerprint);

    assert!(matches!(
        import_private_key(&key.private_key, None),
        Err(SshKeyError::PassphraseRequired)
    ));
    assert!(matches!(
        import_private_key(&key.private_key, Some("sai")),
        Err(SshKeyError::WrongPassphrase)
    ));
    assert!(matches!(
        verify_passphrase(&key.private_key, ""),
        Err(SshKeyError::WrongPassphrase)
    ));
    verify_passphrase(&key.private_key, "đúng mật khẩu").unwrap();
    let imported = import_private_key(&key.private_key, Some("đúng mật khẩu")).unwrap();
    assert!(imported.info.encrypted);
    assert_eq!(imported.info.fingerprint, key.info.fingerprint);

    // Encrypted keys keep their comment in the encrypted section; it is still known here.
    assert_eq!(imported.info.comment, "an@acme.vn");
    let stored = store.save_generated("enc", &key).unwrap();
    assert_eq!(stored.info, key.info);
    let reloaded = store.load("enc").unwrap();
    assert_eq!(reloaded.info, key.info);
    assert!(matches!(
        store.save(
            "other",
            &key.private_key,
            Some(&generate_ed25519("x", None).unwrap().info.public_key)
        ),
        Err(SshKeyError::InvalidKey(_))
    ));
    verify_passphrase(&store.read_private_key("enc").unwrap(), "đúng mật khẩu").unwrap();

    for keygen in keygens() {
        let private = stored.private_key_path.as_str();
        // Public part is readable without the passphrase.
        assert_eq!(
            keygen_fingerprint(&keygen, Path::new(private)),
            key.info.fingerprint
        );
        let out = run(&keygen, &["-y", "-P", "đúng mật khẩu", "-f", private]);
        if !out.status.success() && cfg!(windows) {
            // Non-ASCII argv may be mangled by some Windows ssh builds; retry with ASCII.
            eprintln!(
                "{}: non-ASCII passphrase rejected via argv",
                keygen.display()
            );
            continue;
        }
        assert!(out.status.success(), "{}: {:?}", keygen.display(), out);
        assert_eq!(key_part(&stdout(&out)), key_part(&key.info.public_key));
        let wrong = run(&keygen, &["-y", "-P", "sai", "-f", private]);
        assert!(!wrong.status.success());
    }

    // ASCII passphrase through ssh-keygen as well.
    let ascii = generate_ed25519("ascii", Some("hunter2!")).unwrap();
    let stored = store.save_generated("ascii", &ascii).unwrap();
    for keygen in keygens() {
        let out = run(
            &keygen,
            &["-y", "-P", "hunter2!", "-f", &stored.private_key_path],
        );
        assert!(out.status.success(), "{}: {:?}", keygen.display(), out);
        assert_eq!(key_part(&stdout(&out)), key_part(&ascii.info.public_key));
    }
}

#[test]
fn never_overwrites_a_different_key() {
    let app = tempfile::tempdir().unwrap();
    let store = KeyStore::new(app.path());
    let a = generate_ed25519("a", None).unwrap();
    let b = generate_ed25519("b", None).unwrap();
    store.save_generated("k", &a).unwrap();
    // Same key again: idempotent.
    store.save_generated("k", &a).unwrap();
    match store.save_generated("k", &b) {
        Err(SshKeyError::KeyExists(p)) => assert!(p.ends_with("k")),
        other => panic!("expected KeyExists, got {other:?}"),
    }
    assert_eq!(store.load("k").unwrap().info, a.info);
    assert!(matches!(
        store.save_generated("../evil", &a),
        Err(SshKeyError::InvalidKeyId(_))
    ));
    assert!(matches!(
        store.load("missing"),
        Err(SshKeyError::NotFound(_))
    ));
    // No temp files left behind.
    let names: Vec<String> = std::fs::read_dir(store.dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn private_key_is_restricted_to_current_user() {
    let app = tempfile::tempdir().unwrap();
    let store = KeyStore::new(app.path());
    let key = generate_ed25519("perm", None).unwrap();
    let stored = store.save_generated("perm", &key).unwrap();
    let report = check_permissions(Path::new(&stored.private_key_path)).unwrap();
    assert!(report.restricted, "{report:?}");
    assert_eq!(store.check_permissions("perm").unwrap(), report);

    #[cfg(windows)]
    {
        let out = Command::new("icacls")
            .arg(&stored.private_key_path)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        let aces: Vec<&str> = text.lines().take_while(|l| !l.trim().is_empty()).collect();
        assert_eq!(aces.len(), 1, "exactly one ACE expected:\n{text}");
        assert!(!text.contains("(I)"), "inherited ACE left:\n{text}");
        assert!(aces[0].contains(":(F)"), "{text}");
        let whoami = Command::new("whoami").output().unwrap();
        let me = String::from_utf8_lossy(&whoami.stdout)
            .trim()
            .to_lowercase();
        assert!(aces[0].to_lowercase().contains(&me), "{text} vs {me}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&stored.private_key_path)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let dir_mode = std::fs::metadata(store.dir()).unwrap().permissions().mode();
        assert_eq!(dir_mode & 0o777, 0o700);
    }

    // A loosened file is detected and repaired.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            &stored.private_key_path,
            std::fs::Permissions::from_mode(0o644),
        )
        .unwrap();
    }
    #[cfg(windows)]
    {
        let out = Command::new("icacls")
            .args([
                stored.private_key_path.as_str(),
                "/grant",
                "*S-1-5-32-545:R",
            ])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    }
    assert!(!store.check_permissions("perm").unwrap().restricted);
    store.repair_permissions("perm").unwrap();
    assert!(store.check_permissions("perm").unwrap().restricted);
}

/// Win32-OpenSSH (unlike MSYS ssh) enforces the ACL check: a key readable by `Users` is
/// refused ("bad permissions"), the same file restricted by us is accepted.
#[cfg(windows)]
#[test]
fn win32_openssh_accepts_restricted_key_only() {
    let Some(root) = std::env::var_os("SystemRoot") else {
        return;
    };
    let keygen = Path::new(&root).join(r"System32\OpenSSH\ssh-keygen.exe");
    if !keygen.is_file() {
        eprintln!("Win32-OpenSSH not installed, skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("loose");
    let key = generate_ed25519("loose", None).unwrap();
    std::fs::write(&path, key.private_key.as_bytes()).unwrap();
    let out = Command::new("icacls")
        .args([path.to_str().unwrap(), "/grant", "*S-1-5-32-545:R"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let refused = run(&keygen, &["-y", "-f", path.to_str().unwrap()]);
    assert!(!refused.status.success(), "{refused:?}");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("bad permissions"));

    nanomgg_ssh_keys::restrict_to_current_user(&path, false).unwrap();
    // The explicit `Users` grant survives `/grant:r` for another SID…
    assert!(!check_permissions(&path).unwrap().restricted);
    // …which is why the store re-checks and resets (see `repair_permissions`).
    let store = KeyStore::at(dir.path());
    store.repair_permissions("loose").unwrap();
    assert!(check_permissions(&path).unwrap().restricted);
    let accepted = run(&keygen, &["-y", "-f", path.to_str().unwrap()]);
    assert!(accepted.status.success(), "{accepted:?}");
    assert_eq!(key_part(&stdout(&accepted)), key_part(&key.info.public_key));
}

#[test]
fn imports_keys_made_by_ssh_keygen() {
    let Some(keygen) = keygens().into_iter().next() else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let cases: [(&str, &[&str], &str, KeyKind, u32); 4] = [
        ("ed", &["-t", "ed25519"], "", KeyKind::Ed25519, 256),
        ("ed-enc", &["-t", "ed25519"], "pw123", KeyKind::Ed25519, 256),
        ("rsa", &["-t", "rsa", "-b", "2048"], "", KeyKind::Rsa, 2048),
        (
            "ec",
            &["-t", "ecdsa", "-b", "384"],
            "pw",
            KeyKind::Ecdsa,
            384,
        ),
    ];
    for (name, type_args, pass, kind, bits) in cases {
        let path = dir.path().join(name);
        let path_s = path.to_str().unwrap();
        let mut args = type_args.to_vec();
        args.extend(["-N", pass, "-C", "made-by-keygen", "-q", "-f", path_s]);
        let out = run(&keygen, &args);
        assert!(out.status.success(), "{name}: {out:?}");

        let passphrase = (!pass.is_empty()).then_some(pass);
        let imported = import_private_key_file(&path, passphrase).unwrap();
        assert_eq!(imported.info.kind, kind, "{name}");
        assert_eq!(imported.info.bits, Some(bits), "{name}");
        assert_eq!(imported.info.encrypted, !pass.is_empty(), "{name}");
        assert_eq!(imported.info.comment, "made-by-keygen");
        assert_eq!(imported.public_file, PublicFileCheck::Matches, "{name}");
        assert_eq!(
            imported.info.fingerprint,
            keygen_fingerprint(&keygen, &path),
            "{name}"
        );
        let pub_line = std::fs::read_to_string(format!("{path_s}.pub")).unwrap();
        assert_eq!(key_part(&pub_line), key_part(&imported.info.public_key));
        if passphrase.is_some() {
            assert!(matches!(
                import_private_key_file(&path, Some("wrong")),
                Err(SshKeyError::WrongPassphrase)
            ));
        }
    }

    // A mismatching .pub is reported, not fatal.
    std::fs::copy(dir.path().join("rsa.pub"), dir.path().join("ed.pub")).unwrap();
    let imported = import_private_key_file(&dir.path().join("ed"), None).unwrap();
    assert_eq!(imported.public_file, PublicFileCheck::Mismatch);

    // Legacy PEM is rejected with a typed error.
    let pem = dir.path().join("legacy");
    let out = run(
        &keygen,
        &[
            "-t",
            "rsa",
            "-b",
            "2048",
            "-m",
            "PEM",
            "-N",
            "",
            "-q",
            "-f",
            pem.to_str().unwrap(),
        ],
    );
    assert!(out.status.success(), "{out:?}");
    match import_private_key_file(&pem, None) {
        Err(SshKeyError::LegacyPem { label }) => assert_eq!(label, "RSA PRIVATE KEY"),
        other => panic!("expected LegacyPem, got {other:?}"),
    }
    assert!(matches!(
        import_private_key_file(&dir.path().join("rsa.pub"), None),
        Err(SshKeyError::NotAPrivateKey)
    ));
}

#[test]
fn imports_crlf_key_text() {
    let key = generate_ed25519("crlf", None).unwrap();
    let crlf = key.private_key.replace('\n', "\r\n");
    let imported = import_private_key(&crlf, None).unwrap();
    assert_eq!(imported.info, key.info);
    assert!(!imported.private_key.contains('\r'));
}

/// `ssh -G` evaluates the command line exactly like a real connection would, without
/// connecting: proves the quoting of `GIT_SSH_COMMAND` survives `sh -c` and ssh's parser.
#[test]
fn git_ssh_command_is_understood_by_ssh() {
    let shells = ["sh", r"C:\Program Files\Git\bin\sh.exe"];
    let Some(sh) = shells.iter().find(|s| {
        Command::new(s)
            .args(["-c", "ssh -V"])
            .output()
            .is_ok_and(|o| o.status.success())
    }) else {
        eprintln!("sh/ssh not found, skipping");
        return;
    };
    let app = tempfile::tempdir().unwrap();
    let base = app.path().join("App Data 100%");
    let store = KeyStore::new(&base);
    let key = generate_ed25519("cmd", None).unwrap();
    let stored = store.save_generated("cmd", &key).unwrap();
    let known_hosts = base.join("known_hosts");
    let opts = SshCommandOptions {
        known_hosts_file: Some(known_hosts.clone()),
        host_key_policy: HostKeyPolicy::AcceptNew,
        ..SshCommandOptions::with_key(&stored.private_key_path)
    };
    let cmd = git_ssh_command(&opts);
    let out = Command::new(sh)
        .args(["-c", &format!("{cmd} -G -p 2222 git@github.com")])
        .output()
        .unwrap();
    assert!(out.status.success(), "{cmd}\n{out:?}");
    let text = String::from_utf8_lossy(&out.stdout).to_lowercase();
    let line = |k: &str| {
        text.lines()
            .find(|l| l.starts_with(&format!("{k} ")))
            .map(|l| l[k.len() + 1..].to_string())
            .unwrap_or_default()
    };
    let expect_path = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
    assert_eq!(line("identitiesonly"), "yes");
    assert_eq!(line("stricthostkeychecking"), "accept-new");
    assert_eq!(line("port"), "2222");
    assert_eq!(line("identityagent"), "none");
    // Only the account key: no default `~/.ssh/id_*` once an identity is configured.
    let identities: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("identityfile "))
        .collect();
    assert_eq!(identities.len(), 1, "{identities:?}");
    // `-G` prints IdentityFile before percent expansion (`%%`), UserKnownHostsFile after.
    let identity = identities[0].replace("%%", "%");
    let known = line("userknownhostsfile");
    // MSYS ssh may print `/c/...` or `c:/...`; compare the tail.
    let tail = |s: &str| s.rsplit("app data 100%").next().unwrap_or("").to_string();
    assert!(identity.contains("app data 100%"), "{identity}");
    assert!(known.contains("app data 100%"), "{known}");
    assert_eq!(
        tail(&identity),
        tail(&expect_path(Path::new(&stored.private_key_path)))
    );
    assert_eq!(
        tail(&known).trim_matches('"'),
        tail(&expect_path(&known_hosts))
    );
}

/// Network: `cargo test -p nanomgg-ssh-keys -- --ignored scan_live`.
#[test]
#[ignore = "needs network access to github.com and gitlab.com"]
fn scan_live_hosts_match_pinned_keys() {
    use nanomgg_ssh_keys::{HostKeyVerdict, scan_host_keys};
    for (host, port) in [
        ("github.com", 22),
        ("ssh.github.com", 443),
        ("gitlab.com", 22),
    ] {
        let keys = scan_host_keys(None, host, port, 10).unwrap();
        assert!(!keys.is_empty());
        for k in keys {
            assert_eq!(k.verdict, HostKeyVerdict::Trusted, "{host}: {k:?}");
        }
    }
}
