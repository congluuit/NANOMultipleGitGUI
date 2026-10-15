//! Commit signatures (PLAN §5.9): verification status of a commit (`%G?` & co.), the SSH
//! `allowed_signers` file the app generates for verification, a "test signing" probe that runs
//! exactly the signing path git uses for commits, and the GPG secret-key list for the identity
//! editor.
//!
//! Signing itself is configured through [`AccountContext::signing`](crate::AccountContext)
//! (`commit.gpgsign`, `gpg.format`, `user.signingkey`, injected through the environment).

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::commit::is_valid_oid;
use crate::context::{Signing, SigningFormat};
use crate::{AccountContext, GitError, GitRunner};

/// Verification result of a signed commit, from git's `%G?`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SignatureStatus {
    /// `G`: good signature from a trusted key / an allowed SSH signer.
    Verified,
    /// `U`: good signature, but the key is not trusted (unknown validity).
    Untrusted,
    /// `X`: good signature that has expired.
    Expired,
    /// `Y`: good signature made by a key that has expired since.
    ExpiredKey,
    /// `R`: good signature made by a revoked key.
    Revoked,
    /// `E`: cannot be checked (public key missing, SSH signer not in `allowed_signers`…).
    UnknownKey,
    /// `B`: the signature does not match the commit.
    Bad,
    /// `N`: not signed.
    Unsigned,
}

impl SignatureStatus {
    pub fn from_code(code: &str) -> Self {
        match code.trim() {
            "G" => Self::Verified,
            "U" => Self::Untrusted,
            "X" => Self::Expired,
            "Y" => Self::ExpiredKey,
            "R" => Self::Revoked,
            "E" => Self::UnknownKey,
            "B" => Self::Bad,
            _ => Self::Unsigned,
        }
    }
}

/// Kind of signature found in the commit object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SignatureKind {
    Gpg,
    Ssh,
    X509,
    None,
}

/// Signature details of one commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct CommitSignature {
    pub status: SignatureStatus,
    pub kind: SignatureKind,
    /// `%GS`: signer (GPG user id, or the principal/email for SSH).
    pub signer: Option<String>,
    /// `%GK`: key id used to sign (SSH: key fingerprint).
    pub key: Option<String>,
    /// `%GF`: fingerprint of the signing key.
    pub fingerprint: Option<String>,
    /// `%GT`: trust level (`ultimate`, `fully`, `marginal`, `undefined`, `never`).
    pub trust: Option<String>,
    /// Verification messages printed by gpg / ssh-keygen / git (e.g. allowedSignersFile missing).
    pub message: Option<String>,
}

/// Format of the `--format` used by [`commit_signature`].
const SIGNATURE_FORMAT: &str = "--format=%G?%x00%GS%x00%GK%x00%GF%x00%GT";

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_owned())
}

/// Parses the output of [`SIGNATURE_FORMAT`].
pub fn parse_signature(raw: &str, kind: SignatureKind) -> CommitSignature {
    let mut f = raw.trim_end_matches(['\n', '\r']).split('\0');
    let code = f.next().unwrap_or("N");
    let mut status = SignatureStatus::from_code(code);
    if status == SignatureStatus::Unsigned && kind != SignatureKind::None {
        // git prints `N` when it could not even try (e.g. SSH without allowedSignersFile).
        status = SignatureStatus::UnknownKey;
    }
    CommitSignature {
        status,
        kind,
        signer: f.next().and_then(non_empty),
        key: f.next().and_then(non_empty),
        fingerprint: f.next().and_then(non_empty),
        trust: f.next().and_then(non_empty),
        message: None,
    }
}

/// Kind of signature from the raw commit object (`gpgsig` header).
pub fn signature_kind_of(raw_commit: &str) -> SignatureKind {
    for line in raw_commit.lines() {
        if line.is_empty() {
            break; // end of headers
        }
        if let Some(rest) = line
            .strip_prefix("gpgsig ")
            .or_else(|| line.strip_prefix("gpgsig-sha256 "))
        {
            return if rest.contains("BEGIN SSH SIGNATURE") {
                SignatureKind::Ssh
            } else if rest.contains("BEGIN SIGNED MESSAGE") || rest.contains("BEGIN PKCS7") {
                SignatureKind::X509
            } else {
                SignatureKind::Gpg
            };
        }
    }
    SignatureKind::None
}

/// Cheap check (no gpg / ssh-keygen): which kind of signature commit `oid` carries.
pub fn signature_kind(git: &GitRunner, repo: &Path, oid: &str) -> Result<SignatureKind, GitError> {
    if !is_valid_oid(oid) {
        return Err(invalid_oid(oid));
    }
    let raw = git.run(
        repo,
        &AccountContext::default(),
        ["cat-file", "commit", oid],
    )?;
    Ok(signature_kind_of(&String::from_utf8_lossy(&raw.stdout)))
}

fn invalid_oid(oid: &str) -> GitError {
    GitError::Failed {
        args: "signature".into(),
        status: "rejected".into(),
        stderr: format!("invalid commit id: {oid:?}"),
    }
}

/// Verifies the signature of commit `oid`. `allowed_signers`: file used for SSH signatures
/// (`gpg.ssh.allowedSignersFile`); without it SSH signatures report [`SignatureStatus::UnknownKey`].
pub fn commit_signature(
    git: &GitRunner,
    repo: &Path,
    oid: &str,
    allowed_signers: Option<&Path>,
) -> Result<CommitSignature, GitError> {
    let kind = signature_kind(git, repo, oid)?;
    if kind == SignatureKind::None {
        return Ok(parse_signature("N", kind));
    }
    let mut args: Vec<String> = Vec::new();
    if let Some(file) = allowed_signers {
        args.push("-c".into());
        args.push(format!(
            "gpg.ssh.allowedSignersFile={}",
            file.to_string_lossy().replace('\\', "/")
        ));
    }
    args.extend(
        [
            "-c",
            "log.showSignature=false",
            "show",
            "-s",
            SIGNATURE_FORMAT,
            oid,
            "--",
        ]
        .map(String::from),
    );
    let out = crate::ops::writer(git).run(repo, &AccountContext::default(), &args)?;
    let mut sig = parse_signature(&String::from_utf8_lossy(&out.stdout), kind);
    sig.message = non_empty(&String::from_utf8_lossy(&out.stderr));
    Ok(sig)
}

/// One line of an SSH `allowed_signers` file (`ssh-keygen(1)` ALLOWED SIGNERS): the email as
/// principal, limited to the `git` namespace. `None` for an unusable key / email.
pub fn allowed_signers_line(email: &str, public_key: &str) -> Option<String> {
    let email = email.trim();
    let key: Vec<&str> = public_key.split_whitespace().take(2).collect();
    let valid_email = !email.is_empty() && !email.contains([' ', '\t', '\n', '"', ',']);
    let valid_key = key.len() == 2
        && ["ssh-", "ecdsa-", "sk-"]
            .iter()
            .any(|p| key[0].starts_with(p));
    (valid_email && valid_key).then(|| format!("{email} namespaces=\"git\" {} {}", key[0], key[1]))
}

/// Result of [`test_signing`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SignTestResult {
    pub ok: bool,
    /// gpg / ssh-keygen / git output when it failed (or verification notes).
    pub message: Option<String>,
    /// Verification of the test signature (when signing worked).
    pub signature: Option<CommitSignature>,
}

/// Scratch repository removed on drop.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch_dir() -> std::io::Result<Scratch> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir().join(format!(
        "nanomgg-signtest-{}-{nanos}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir)?;
    Ok(Scratch(dir))
}

/// "Thử ký": signs a throw-away commit in a scratch repository with `signing` (through git, so
/// `gpg.program`, gpg-agent's passphrase prompt and ssh-keygen are exercised exactly like a
/// real commit), then verifies it. `ctx` supplies the rest (identity, SSH askpass).
pub fn test_signing(
    git: &GitRunner,
    ctx: &AccountContext,
    signing: &Signing,
    allowed_signers: Option<&Path>,
) -> Result<SignTestResult, GitError> {
    let scratch = scratch_dir()?;
    let dir = &scratch.0;
    let mut ctx = ctx.clone();
    ctx.signing = Some(signing.clone());
    if ctx.identity.is_none() {
        ctx.identity = Some(crate::Identity {
            name: "NANOMultipleGitGUI".into(),
            email: "signing-test@nanomgg.invalid".into(),
        });
    }
    let w = crate::ops::writer(git);
    w.run(dir, &AccountContext::default(), ["init", "-q"])?;
    let tree = w.run_with_input(dir, &AccountContext::default(), ["mktree"], b"")?;
    let tree = String::from_utf8_lossy(&tree.stdout).trim().to_owned();
    let signed = w.run_with_input(
        dir,
        &ctx,
        ["commit-tree", "-S", &tree],
        b"nanomgg signing test\n",
    );
    let oid = match signed {
        Ok(out) => String::from_utf8_lossy(&out.stdout).trim().to_owned(),
        Err(GitError::Failed { stderr, .. }) => {
            return Ok(SignTestResult {
                ok: false,
                message: non_empty(&stderr),
                signature: None,
            });
        }
        Err(e) => return Err(e),
    };
    let signature = commit_signature(git, dir, &oid, allowed_signers).ok();
    Ok(SignTestResult {
        ok: true,
        message: signature.as_ref().and_then(|s| s.message.clone()),
        signature,
    })
}

/// A GPG secret key usable for signing (`gpg --list-secret-keys --with-colons`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct GpgKey {
    /// Long key id (16 hex digits).
    pub key_id: String,
    pub fingerprint: Option<String>,
    /// User ids (`Name <email>`).
    pub uids: Vec<String>,
    /// Creation time (Unix seconds).
    pub created: Option<i64>,
    /// Expiry time (Unix seconds), `None` = never.
    pub expires: Option<i64>,
    /// The key (or one of its subkeys) can sign and is not expired / revoked.
    pub can_sign: bool,
    /// Revoked (`r`) or expired (`e`).
    pub invalid: bool,
}

/// Unescapes gpg's `\xNN` sequences in colon listings.
fn gpg_unescape(s: &str) -> String {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1] == b'x'
            && let Ok(v) = u8::from_str_radix(&s[i + 2..i + 4], 16)
        {
            out.push(v);
            i += 4;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Parses `gpg --list-secret-keys --with-colons --fixed-list-mode`.
pub fn parse_gpg_secret_keys(raw: &str) -> Vec<GpgKey> {
    let mut keys: Vec<GpgKey> = Vec::new();
    // Whether the last `sec`/`ssb` record seen belongs to a primary key (for `fpr`).
    let mut last_primary = false;
    for line in raw.lines() {
        let f: Vec<&str> = line.split(':').collect();
        let field = |i: usize| f.get(i).copied().unwrap_or("");
        match field(0) {
            "sec" => {
                let validity = field(1);
                let caps = field(11);
                keys.push(GpgKey {
                    key_id: field(4).to_owned(),
                    fingerprint: None,
                    uids: Vec::new(),
                    created: field(5).parse().ok(),
                    expires: field(6).parse().ok(),
                    // Uppercase capabilities on the primary record = usable by the whole key.
                    can_sign: caps.contains('S') && !matches!(validity, "r" | "e" | "d"),
                    invalid: matches!(validity, "r" | "e" | "d"),
                });
                last_primary = true;
            }
            "ssb" => last_primary = false,
            "fpr" if last_primary => {
                if let Some(k) = keys.last_mut()
                    && k.fingerprint.is_none()
                {
                    k.fingerprint = non_empty(field(9));
                }
            }
            "uid" => {
                if let Some(k) = keys.last_mut()
                    && !matches!(field(1), "r" | "e")
                    && let Some(uid) = non_empty(&gpg_unescape(field(9)))
                {
                    k.uids.push(uid);
                }
            }
            _ => {}
        }
    }
    keys
}

/// The `gpg` git will run: `gpg.program` if configured, else the one bundled with Git for
/// Windows (`<git>/usr/bin/gpg.exe`, which git's own PATH puts first), else `gpg` on PATH.
pub fn gpg_program(git: &GitRunner, cwd: &Path) -> PathBuf {
    let none = AccountContext::default();
    if let Ok(p) = git.run_text(cwd, &none, ["config", "--get", "gpg.program"])
        && !p.is_empty()
    {
        return PathBuf::from(p);
    }
    if cfg!(windows)
        && let Ok(exec) = git.run_text(cwd, &none, ["--exec-path"])
    {
        // <git>/mingw64/libexec/git-core → <git>/usr/bin/gpg.exe
        let exec = PathBuf::from(exec);
        if let Some(root) = exec.ancestors().nth(3) {
            let bundled = root.join("usr").join("bin").join("gpg.exe");
            if bundled.is_file() {
                return bundled;
            }
        }
    }
    PathBuf::from("gpg")
}

/// Outcome of listing GPG secret keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum GpgKeyList {
    /// gpg ran; `keys` may be empty (no secret key yet).
    Found { program: String, keys: Vec<GpgKey> },
    /// gpg could not be started (not installed / `gpg.program` wrong).
    NotInstalled { program: String, message: String },
}

/// `gpg --list-secret-keys --with-colons` with the program git would use.
pub fn list_gpg_secret_keys(git: &GitRunner, cwd: &Path) -> GpgKeyList {
    let program = gpg_program(git, cwd);
    let shown = program.to_string_lossy().into_owned();
    let mut cmd = Command::new(&program);
    cmd.args([
        "--batch",
        "--no-tty",
        "--list-secret-keys",
        "--with-colons",
        "--fixed-list-mode",
    ])
    .env("LC_ALL", "C");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    match cmd.output() {
        Ok(out) => GpgKeyList::Found {
            program: shown,
            keys: parse_gpg_secret_keys(&String::from_utf8_lossy(&out.stdout)),
        },
        Err(e) => GpgKeyList::NotInstalled {
            program: shown,
            message: e.to_string(),
        },
    }
}

/// Signing settings for a key reference stored on an identity: GPG key id as is; for SSH a
/// public key path (`…/id.pub`) is replaced by its private key when that file exists next to
/// it (the app runs ssh without an agent), `key::…` literals and other paths are kept.
pub fn signing_for(format: SigningFormat, key_ref: &str) -> Option<Signing> {
    let key_ref = key_ref.trim();
    if key_ref.is_empty() {
        return None;
    }
    let key = match format {
        SigningFormat::OpenPgp => key_ref.to_owned(),
        SigningFormat::Ssh => {
            let private = key_ref
                .strip_suffix(".pub")
                .map(PathBuf::from)
                .filter(|p| p.is_file());
            match private {
                Some(p) => p.to_string_lossy().replace('\\', "/"),
                None if key_ref.starts_with("key::") => key_ref.to_owned(),
                None => key_ref.replace('\\', "/"),
            }
        }
    };
    Some(Signing { format, key })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::progress::test_support::TestRepo;

    #[test]
    fn status_codes() {
        assert_eq!(SignatureStatus::from_code("G"), SignatureStatus::Verified);
        assert_eq!(SignatureStatus::from_code("U"), SignatureStatus::Untrusted);
        assert_eq!(SignatureStatus::from_code("B"), SignatureStatus::Bad);
        assert_eq!(SignatureStatus::from_code("E"), SignatureStatus::UnknownKey);
        assert_eq!(SignatureStatus::from_code("R"), SignatureStatus::Revoked);
        assert_eq!(SignatureStatus::from_code("N"), SignatureStatus::Unsigned);
    }

    #[test]
    fn parses_signature_fields() {
        let s = parse_signature(
            "G\0t@x\0SHA256:abc\0SHA256:abc\0fully\n",
            SignatureKind::Ssh,
        );
        assert_eq!(s.status, SignatureStatus::Verified);
        assert_eq!(s.signer.as_deref(), Some("t@x"));
        assert_eq!(s.key.as_deref(), Some("SHA256:abc"));
        assert_eq!(s.trust.as_deref(), Some("fully"));
        // Signed but git could not check (no allowed signers): not "unsigned".
        let s = parse_signature("N\0\0\0\0\n", SignatureKind::Ssh);
        assert_eq!(s.status, SignatureStatus::UnknownKey);
        assert_eq!(s.signer, None);
    }

    #[test]
    fn detects_signature_kind_from_headers() {
        let ssh = "tree 4b82\nauthor T <t@x> 1 +0000\ncommitter T <t@x> 1 +0000\ngpgsig -----BEGIN SSH SIGNATURE-----\n U1NI\n -----END SSH SIGNATURE-----\n\nmsg\n";
        assert_eq!(signature_kind_of(ssh), SignatureKind::Ssh);
        let gpg = "tree 4b82\ngpgsig -----BEGIN PGP SIGNATURE-----\n x\n\nmsg";
        assert_eq!(signature_kind_of(gpg), SignatureKind::Gpg);
        // "gpgsig" in the message body is not a header.
        let none = "tree 4b82\nauthor T <t@x> 1 +0000\n\ngpgsig -----BEGIN SSH SIGNATURE-----\n";
        assert_eq!(signature_kind_of(none), SignatureKind::None);
    }

    #[test]
    fn allowed_signers_lines() {
        assert_eq!(
            allowed_signers_line("a@b.dev", "ssh-ed25519 AAAAC3Nz comment here").as_deref(),
            Some("a@b.dev namespaces=\"git\" ssh-ed25519 AAAAC3Nz")
        );
        assert_eq!(allowed_signers_line("a b@c", "ssh-ed25519 AAAA"), None);
        assert_eq!(allowed_signers_line("a@b", "not a key"), None);
        assert_eq!(allowed_signers_line("a@b", ""), None);
    }

    #[test]
    fn parses_gpg_colon_listing() {
        let raw = "sec:u:255:22:AABBCCDDEEFF0011:1700000000:::u:::scESC:::+:::ed25519:::0:\n\
fpr:::::::::0123456789ABCDEF0123456789ABCDEFAABBCCDD:\n\
grp:::::::::ABC:\n\
uid:u::::1700000000::HASH::Nguy\\x3a\\x20A <a@b.dev>::::::::::0:\n\
uid:r::::1700000000::HASH::Old <old@b.dev>::::::::::0:\n\
ssb:u:255:18:1122334455667788:1700000000::::::e:::+:::cv25519::\n\
fpr:::::::::FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF:\n\
sec:e:255:22:9999999999999999:1600000000:1650000000::u:::scSC:::+:::ed25519:::0:\n\
fpr:::::::::9999999999999999999999999999999999999999:\n\
uid:e::::1600000000::HASH::Expired <e@b.dev>::::::::::0:\n";
        let keys = parse_gpg_secret_keys(raw);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].key_id, "AABBCCDDEEFF0011");
        assert_eq!(
            keys[0].fingerprint.as_deref(),
            Some("0123456789ABCDEF0123456789ABCDEFAABBCCDD"),
            "the subkey fingerprint is not taken"
        );
        assert_eq!(keys[0].uids, vec!["Nguy: A <a@b.dev>".to_owned()]);
        assert!(keys[0].can_sign && !keys[0].invalid);
        assert!(!keys[1].can_sign && keys[1].invalid);
        assert_eq!(keys[1].expires, Some(1_650_000_000));
        assert!(parse_gpg_secret_keys("").is_empty());
    }

    #[test]
    fn ssh_key_refs_prefer_the_private_key_next_to_the_pub_file() {
        let dir = tempfile::tempdir().unwrap();
        let private = dir.path().join("id_test");
        std::fs::write(&private, "x").unwrap();
        let pub_path = format!("{}.pub", private.display());
        let s = signing_for(SigningFormat::Ssh, &pub_path).unwrap();
        assert_eq!(s.key, private.to_string_lossy().replace('\\', "/"));
        let s = signing_for(SigningFormat::Ssh, "key::ssh-ed25519 AAAA").unwrap();
        assert_eq!(s.key, "key::ssh-ed25519 AAAA");
        let s = signing_for(SigningFormat::OpenPgp, " ABCD ").unwrap();
        assert_eq!(s.key, "ABCD");
        assert!(signing_for(SigningFormat::OpenPgp, "  ").is_none());
    }

    /// `ssh-keygen` from PATH or next to git (Git for Windows ships it in `usr/bin`).
    fn ssh_keygen(git: &GitRunner) -> Option<PathBuf> {
        let candidates = [PathBuf::from("ssh-keygen")].into_iter().chain(
            git.run_text(Path::new("."), &AccountContext::default(), ["--exec-path"])
                .ok()
                .and_then(|e| {
                    PathBuf::from(e)
                        .ancestors()
                        .nth(3)
                        .map(|r| r.join("usr/bin/ssh-keygen.exe"))
                }),
        );
        candidates
            .into_iter()
            .find(|p| Command::new(p).arg("-?").output().is_ok())
    }

    fn generate_key(dir: &Path, keygen: &Path) -> PathBuf {
        let key = dir.join("signing key");
        let out = Command::new(keygen)
            .args([
                "-q",
                "-t",
                "ed25519",
                "-N",
                "",
                "-C",
                "tester@example.com",
                "-f",
            ])
            .arg(&key)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        key
    }

    #[test]
    fn ssh_signed_commit_verifies_with_allowed_signers() {
        let r = TestRepo::new();
        let Some(keygen) = ssh_keygen(&r.git) else {
            eprintln!("ssh-keygen not available; skipping");
            return;
        };
        let keys = tempfile::tempdir().unwrap();
        let key = generate_key(keys.path(), &keygen);
        let public = std::fs::read_to_string(format!("{}.pub", key.display())).unwrap();
        let signing = signing_for(SigningFormat::Ssh, &format!("{}.pub", key.display())).unwrap();
        let mut ctx = r.ctx.clone();
        ctx.signing = Some(signing.clone());

        r.write("s.txt", "signed\n");
        r.git(&["add", "s.txt"]);
        r.git
            .run(&r.path, &ctx, ["commit", "-q", "-m", "signed"])
            .unwrap();
        let signed = r.head();
        let unsigned = r.commit("u.txt", "u\n", "unsigned");

        assert_eq!(
            signature_kind(&r.git, &r.path, &signed).unwrap(),
            SignatureKind::Ssh
        );
        // Without allowed signers git cannot check SSH signatures.
        let s = commit_signature(&r.git, &r.path, &signed, None).unwrap();
        assert_eq!(s.status, SignatureStatus::UnknownKey);
        assert!(s.message.is_some_and(|m| m.contains("allowedSignersFile")));

        let allowed = keys.path().join("allowed_signers");
        let line = allowed_signers_line("tester@example.com", &public).unwrap();
        std::fs::write(&allowed, format!("{line}\n")).unwrap();
        let s = commit_signature(&r.git, &r.path, &signed, Some(&allowed)).unwrap();
        assert_eq!(s.status, SignatureStatus::Verified, "{s:?}");
        assert_eq!(s.signer.as_deref(), Some("tester@example.com"));
        assert!(s.key.is_some_and(|k| k.starts_with("SHA256:")));

        let s = commit_signature(&r.git, &r.path, &unsigned, Some(&allowed)).unwrap();
        assert_eq!(s.status, SignatureStatus::Unsigned);
        assert_eq!(s.kind, SignatureKind::None);
        assert!(commit_signature(&r.git, &r.path, "--output=x", None).is_err());

        // "Test signing" goes through the same path and verifies.
        let t = test_signing(&r.git, &r.ctx, &signing, Some(&allowed)).unwrap();
        assert!(t.ok, "{t:?}");
        assert_eq!(
            t.signature.map(|s| s.status),
            Some(SignatureStatus::Verified)
        );
        let t = test_signing(&r.git, &r.ctx, &signing, None).unwrap();
        assert!(t.ok);
        assert_eq!(
            t.signature.map(|s| s.status),
            Some(SignatureStatus::UnknownKey),
            "SSH signatures cannot be checked without allowed signers"
        );

        // Annotated tags are signed too.
        crate::ops::create_tag(&r.git, &r.path, &ctx, "v1", &signed, Some("release")).unwrap();
        let tag = r.git(&["cat-file", "tag", "v1"]);
        assert!(tag.contains("BEGIN SSH SIGNATURE"), "{tag}");
        crate::ops::create_tag(&r.git, &r.path, &ctx, "light", &signed, None).unwrap();
        assert_eq!(r.git(&["cat-file", "-t", "light"]), "commit");

        // A missing key fails with a recognisable kind.
        let mut bad = r.ctx.clone();
        bad.signing = signing_for(
            SigningFormat::Ssh,
            &keys.path().join("nope").to_string_lossy(),
        );
        let err = r
            .git
            .run(&r.path, &bad, ["commit", "-q", "--allow-empty", "-m", "x"])
            .unwrap_err();
        let GitError::Failed { stderr, .. } = err else {
            panic!("expected a git failure");
        };
        assert_eq!(
            crate::ops::failure_kind(&stderr),
            Some(crate::ops::FailureKind::SigningFailed),
            "{stderr}"
        );
        let t = test_signing(&r.git, &r.ctx, bad.signing.as_ref().unwrap(), None).unwrap();
        assert!(!t.ok && t.message.is_some());
    }
}
