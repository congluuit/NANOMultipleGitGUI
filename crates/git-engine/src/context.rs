//! Per-command account context, injected through `GIT_CONFIG_COUNT` / `GIT_CONFIG_KEY_n` /
//! `GIT_CONFIG_VALUE_n` rather than `-c`: environment variables are inherited by every child
//! process, including git-lfs started from hooks and smudge/clean filters, so LFS transfers
//! use the same account as the git command that triggered them.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub email: String,
}

/// HTTPS credentials served by the app through the `nanomgg-credential` sidecar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpsAuth {
    pub helper_path: PathBuf,
    /// `NANOMGG_SOCKET` / `NANOMGG_SESSION` / `NANOMGG_ACCOUNT` for this account.
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshAuth {
    pub key_path: PathBuf,
    /// Full prebuilt `GIT_SSH_COMMAND` value. The app builds it with
    /// `nanomgg_ssh_keys::git_ssh_command` (app known_hosts file, host key policy…); `None`
    /// falls back to [`ssh_command`]`(key_path)`. git-engine stays free of SSH dependencies.
    pub command: Option<String>,
    /// Answers the key's passphrase prompt through the sidecar (`SSH_ASKPASS`), so encrypted
    /// keys work without a terminal. `None` leaves ssh's own prompting alone.
    pub askpass: Option<SshAskpass>,
}

impl SshAuth {
    pub fn new(key_path: impl Into<PathBuf>) -> Self {
        Self {
            key_path: key_path.into(),
            command: None,
            askpass: None,
        }
    }

    /// The `GIT_SSH_COMMAND` value this auth injects.
    pub fn git_ssh_command(&self) -> String {
        self.command
            .clone()
            .unwrap_or_else(|| ssh_command(&self.key_path))
    }
}

/// `SSH_ASKPASS` program (the `nanomgg-credential` sidecar) and the `NANOMGG_*` variables that
/// route its prompt to the bound account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshAskpass {
    pub program: PathBuf,
    pub env: Vec<(String, String)>,
}

/// Signature format of [`Signing`] (`gpg.format`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigningFormat {
    /// `gpg.format=openpgp`: `user.signingkey` is a GPG key id / fingerprint.
    OpenPgp,
    /// `gpg.format=ssh`: `user.signingkey` is a key file path or `key::ssh-ed25519 …`.
    Ssh,
}

impl SigningFormat {
    pub fn as_config(self) -> &'static str {
        match self {
            Self::OpenPgp => "openpgp",
            Self::Ssh => "ssh",
        }
    }
}

/// Commit signing of the identity (PLAN §4.4): `commit.gpgsign=true`, `gpg.format`,
/// `user.signingkey`. Annotated tags created by the app are signed with `-s` (see
/// [`crate::ops::create_tag`]); `tag.gpgSign` is deliberately *not* injected because it would
/// turn every lightweight tag (also in the integrated terminal) into an annotated one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signing {
    pub format: SigningFormat,
    pub key: String,
}

/// Everything that decides "as whom" a git command runs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccountContext {
    pub identity: Option<Identity>,
    pub https: Option<HttpsAuth>,
    pub ssh: Option<SshAuth>,
    /// Sign commits as this identity; `None` leaves git's own signing config alone.
    pub signing: Option<Signing>,
}

/// Forward slashes work for Git for Windows' bundled `sh` and for `ssh -i`.
fn portable_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Single-quotes `s` for POSIX `sh`, which git uses to run `!`-style helpers.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// `credential.helper` value that runs the sidecar via git's shell (`!` form), so paths with
/// spaces work on every platform.
pub fn helper_config_value(helper_path: &Path) -> String {
    format!("!{}", sh_quote(&portable_path(helper_path)))
}

/// `GIT_SSH_COMMAND` using only `key_path` (no agent). Same quoting as
/// `nanomgg_ssh_keys::git_ssh_command` with default options: the key goes in
/// `-o IdentityFile="<path>"` (ssh checks a `-i` path before `%`-expansion and silently drops
/// it), `%` is doubled for ssh's expansion, and the argument is single-quoted for `sh`.
pub fn ssh_command(key_path: &Path) -> String {
    let identity = format!(
        "IdentityFile=\"{}\"",
        portable_path(key_path).replace('%', "%%")
    );
    format!(
        "ssh -o {} -o IdentitiesOnly=yes -o IdentityAgent=none",
        sh_quote(&identity)
    )
}

impl AccountContext {
    /// Config entries in the order git applies them.
    pub fn config_entries(&self) -> Vec<(String, String)> {
        let mut entries = Vec::new();
        if let Some(id) = &self.identity {
            entries.push(("user.name".into(), id.name.clone()));
            entries.push(("user.email".into(), id.email.clone()));
        }
        if let Some(https) = &self.https {
            // An empty value resets the helper list, disabling GCM/osxkeychain/store
            // configured globally, so no other account's token can be picked up.
            entries.push(("credential.helper".into(), String::new()));
            entries.push((
                "credential.helper".into(),
                helper_config_value(&https.helper_path),
            ));
            entries.push(("credential.useHttpPath".into(), "true".into()));
        }
        if let Some(signing) = &self.signing {
            entries.push(("commit.gpgsign".into(), "true".into()));
            entries.push(("gpg.format".into(), signing.format.as_config().into()));
            entries.push(("user.signingkey".into(), signing.key.clone()));
        }
        entries
    }

    /// Environment for a git process: config entries plus credential/SSH plumbing.
    pub fn env(&self) -> Vec<(String, String)> {
        let entries = self.config_entries();
        let mut env = vec![("GIT_CONFIG_COUNT".to_owned(), entries.len().to_string())];
        for (i, (key, value)) in entries.into_iter().enumerate() {
            env.push((format!("GIT_CONFIG_KEY_{i}"), key));
            env.push((format!("GIT_CONFIG_VALUE_{i}"), value));
        }
        // Never block on an interactive username/password prompt.
        env.push(("GIT_TERMINAL_PROMPT".into(), "0".into()));
        if let Some(https) = &self.https {
            env.extend(https.env.iter().cloned());
        }
        if let Some(ssh) = &self.ssh {
            env.push(("GIT_SSH_COMMAND".into(), ssh.git_ssh_command()));
            if let Some(askpass) = &ssh.askpass {
                for (key, value) in &askpass.env {
                    if !env.iter().any(|(k, _)| k == key) {
                        env.push((key.clone(), value.clone()));
                    }
                }
                env.push(("SSH_ASKPASS".into(), portable_path(&askpass.program)));
                // OpenSSH ≥ 8.4: use the askpass program even without DISPLAY / with a tty.
                env.push(("SSH_ASKPASS_REQUIRE".into(), "force".into()));
                env.push(("NANOMGG_ASKPASS".into(), "1".into()));
            }
        }
        env
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> AccountContext {
        AccountContext {
            identity: Some(Identity {
                name: "Alice".into(),
                email: "alice@acme.dev".into(),
            }),
            https: Some(HttpsAuth {
                helper_path: PathBuf::from(r"C:\Program Files\NANO MGG\nanomgg-credential.exe"),
                env: vec![("NANOMGG_ACCOUNT".into(), "acc-1".into())],
            }),
            ssh: None,
            signing: None,
        }
    }

    #[test]
    fn empty_context_only_disables_prompts() {
        let env = AccountContext::default().env();
        assert_eq!(
            env,
            vec![
                ("GIT_CONFIG_COUNT".to_owned(), "0".to_owned()),
                ("GIT_TERMINAL_PROMPT".to_owned(), "0".to_owned()),
            ]
        );
    }

    #[test]
    fn identity_and_helper_are_injected_in_order() {
        let env = ctx().env();
        let get = |k: &str| {
            env.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(get("GIT_CONFIG_COUNT"), Some("5"));
        assert_eq!(get("GIT_CONFIG_KEY_0"), Some("user.name"));
        assert_eq!(get("GIT_CONFIG_VALUE_1"), Some("alice@acme.dev"));
        assert_eq!(get("GIT_CONFIG_KEY_2"), Some("credential.helper"));
        assert_eq!(get("GIT_CONFIG_VALUE_2"), Some(""));
        assert_eq!(
            get("GIT_CONFIG_VALUE_3"),
            Some("!'C:/Program Files/NANO MGG/nanomgg-credential.exe'")
        );
        assert_eq!(get("GIT_CONFIG_KEY_4"), Some("credential.useHttpPath"));
        assert_eq!(get("NANOMGG_ACCOUNT"), Some("acc-1"));
        assert_eq!(get("GIT_TERMINAL_PROMPT"), Some("0"));
    }

    #[test]
    fn signing_injects_gpgsign_format_and_key() {
        let ctx = AccountContext {
            signing: Some(Signing {
                format: SigningFormat::Ssh,
                key: "C:/keys/id_ed25519".into(),
            }),
            ..Default::default()
        };
        assert_eq!(
            ctx.config_entries(),
            vec![
                ("commit.gpgsign".to_owned(), "true".to_owned()),
                ("gpg.format".to_owned(), "ssh".to_owned()),
                (
                    "user.signingkey".to_owned(),
                    "C:/keys/id_ed25519".to_owned()
                ),
            ]
        );
        assert_eq!(SigningFormat::OpenPgp.as_config(), "openpgp");
    }

    #[test]
    fn helper_value_escapes_single_quotes() {
        assert_eq!(
            helper_config_value(Path::new("/opt/it's here/nanomgg-credential")),
            r"!'/opt/it'\''s here/nanomgg-credential'"
        );
    }

    #[test]
    fn ssh_key_sets_git_ssh_command() {
        let ctx = AccountContext {
            ssh: Some(SshAuth::new(r"C:\Users\me\keys\work_ed25519")),
            ..Default::default()
        };
        let env = ctx.env();
        assert!(env.contains(&(
            "GIT_SSH_COMMAND".into(),
            r#"ssh -o 'IdentityFile="C:/Users/me/keys/work_ed25519"' -o IdentitiesOnly=yes -o IdentityAgent=none"#
                .into()
        )));
        assert!(!env.iter().any(|(k, _)| k == "SSH_ASKPASS"));
    }

    #[test]
    fn ssh_command_quotes_like_ssh_keys_crate() {
        assert_eq!(
            ssh_command(Path::new("/home/o'neil/100%/key")),
            r#"ssh -o 'IdentityFile="/home/o'\''neil/100%%/key"' -o IdentitiesOnly=yes -o IdentityAgent=none"#
        );
    }

    #[test]
    fn prebuilt_ssh_command_wins() {
        let ctx = AccountContext {
            ssh: Some(SshAuth {
                command: Some("ssh -o IdentityFile=x -o UserKnownHostsFile=y".into()),
                ..SshAuth::new("/k")
            }),
            ..Default::default()
        };
        assert!(ctx.env().contains(&(
            "GIT_SSH_COMMAND".into(),
            "ssh -o IdentityFile=x -o UserKnownHostsFile=y".into()
        )));
    }

    #[test]
    fn askpass_routes_passphrase_prompts_to_the_sidecar() {
        let mut ctx = ctx();
        ctx.ssh = Some(SshAuth {
            askpass: Some(SshAskpass {
                program: PathBuf::from(r"C:\Program Files\NANO MGG\nanomgg-credential.exe"),
                env: vec![
                    ("NANOMGG_ACCOUNT".into(), "acc-1".into()),
                    ("NANOMGG_SESSION".into(), "s".into()),
                ],
            }),
            ..SshAuth::new("/keys/k")
        });
        let env = ctx.env();
        let get = |k: &str| {
            env.iter()
                .filter(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            get("SSH_ASKPASS"),
            ["C:/Program Files/NANO MGG/nanomgg-credential.exe"]
        );
        assert_eq!(get("SSH_ASKPASS_REQUIRE"), ["force"]);
        assert_eq!(get("NANOMGG_ASKPASS"), ["1"]);
        // Shared with the HTTPS helper env: not duplicated.
        assert_eq!(get("NANOMGG_ACCOUNT"), ["acc-1"]);
        assert_eq!(get("NANOMGG_SESSION"), ["s"]);
    }
}
