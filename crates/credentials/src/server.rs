//! In-app IPC server answering `nanomgg-credential` requests for the right account.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::JoinHandle;

use interprocess::local_socket::{ListenerOptions, Stream, prelude::*};
use zeroize::Zeroizing;

use crate::host::{self, HostCheck};
use crate::protocol::{
    AskpassRequest, CredentialRequest, CredentialResponse, ENV_ACCOUNT, ENV_SESSION, ENV_SOCKET,
    KIND_ASKPASS,
};
use crate::socket::SocketAddress;

/// HTTPS credential of one account. The token is wiped from memory when dropped.
#[derive(Clone)]
pub struct AccountCredential {
    /// Provider host the token belongs to, e.g. `github.com` or `gitlab.acme.dev:8443`.
    pub host: String,
    pub username: String,
    pub token: Zeroizing<String>,
}

impl std::fmt::Debug for AccountCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountCredential")
            .field("host", &self.host)
            .field("username", &self.username)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

/// Where the server looks up secrets. The app backs this with the OS keychain
/// ([`KeychainSource`](crate::KeychainSource)).
pub trait CredentialSource: Send + Sync + 'static {
    fn credential(&self, account_id: &str) -> Option<AccountCredential>;

    /// Passphrase of the SSH private key at `key_path`, only if that key belongs to the
    /// account. The default answers nothing (sources without SSH support).
    fn ssh_passphrase(&self, _account_id: &str, _key_path: &str) -> Option<Zeroizing<String>> {
        None
    }
}

/// Simple in-memory source, used by tests and the prototype.
#[derive(Default)]
pub struct InMemorySource {
    accounts: RwLock<HashMap<String, AccountCredential>>,
    /// (account id, key path as given to `ssh -i`) → passphrase.
    passphrases: RwLock<HashMap<(String, String), Zeroizing<String>>>,
}

impl InMemorySource {
    pub fn insert(&self, account_id: impl Into<String>, credential: AccountCredential) {
        self.accounts
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(account_id.into(), credential);
    }

    pub fn insert_ssh_passphrase(
        &self,
        account_id: impl Into<String>,
        key_path: impl Into<String>,
        passphrase: Zeroizing<String>,
    ) {
        self.passphrases
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert((account_id.into(), key_path.into()), passphrase);
    }
}

impl CredentialSource for InMemorySource {
    fn credential(&self, account_id: &str) -> Option<AccountCredential> {
        self.accounts
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(account_id)
            .cloned()
    }

    fn ssh_passphrase(&self, account_id: &str, key_path: &str) -> Option<Zeroizing<String>> {
        let wanted = crate::keychain::normalize_key_path(key_path);
        self.passphrases
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|((acc, path), _)| {
                acc == account_id && crate::keychain::normalize_key_path(path) == wanted
            })
            .map(|(_, p)| p.clone())
    }
}

fn random_hex(bytes: usize) -> io::Result<String> {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).map_err(|e| io::Error::other(e.to_string()))?;
    Ok(buf.iter().map(|b| format!("{b:02x}")).collect())
}

/// Constant-time comparison so the session token cannot be guessed byte by byte.
fn tokens_equal(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

/// Answers one request. Pure function so the security rules are unit-testable.
pub fn answer(
    request: &CredentialRequest,
    session: &str,
    source: &dyn CredentialSource,
) -> CredentialResponse {
    if !tokens_equal(&request.session, session) {
        return CredentialResponse::denied("invalid session");
    }
    let Some(account) = source.credential(&request.account) else {
        return CredentialResponse::denied(format!(
            "unknown account `{}` or no HTTPS token saved for it",
            request.account
        ));
    };
    match host::check(&account.host, &request.protocol, &request.host) {
        HostCheck::Allowed => CredentialResponse::Credential {
            username: account.username.clone(),
            password: account.token.as_str().to_owned(),
        },
        HostCheck::Refused(reason) => CredentialResponse::denied(reason),
    }
}

/// Extracts the key path from OpenSSH's passphrase prompt
/// (`Enter passphrase for key '<path>': `). Every other prompt — host key confirmation,
/// passwords, FIDO PINs, git's `Username for …` — yields `None` and is refused.
pub fn passphrase_prompt_key(prompt: &str) -> Option<&str> {
    let rest = prompt.trim().strip_prefix("Enter passphrase for key '")?;
    let path = rest
        .strip_suffix("':")
        .or_else(|| rest.strip_suffix('\''))?;
    (!path.is_empty()).then_some(path)
}

/// Answers an askpass request: an SSH key passphrase from the keychain, for a key of the
/// requesting account only.
pub fn answer_askpass(
    request: &AskpassRequest,
    session: &str,
    source: &dyn CredentialSource,
) -> CredentialResponse {
    if !tokens_equal(&request.session, session) {
        return CredentialResponse::denied("invalid session");
    }
    let Some(key_path) = passphrase_prompt_key(&request.prompt) else {
        return CredentialResponse::denied("only SSH key passphrase prompts are answered");
    };
    match source.ssh_passphrase(&request.account, key_path) {
        Some(passphrase) => CredentialResponse::Secret {
            value: passphrase.as_str().to_owned(),
        },
        None => CredentialResponse::denied(format!(
            "no stored passphrase for this key in account `{}`",
            request.account
        )),
    }
}

/// Dispatches one raw request line (credential or askpass).
fn answer_line(line: &str, session: &str, source: &dyn CredentialSource) -> CredentialResponse {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return CredentialResponse::denied("malformed request");
    };
    if value.get("kind").and_then(|k| k.as_str()) == Some(KIND_ASKPASS) {
        match serde_json::from_value::<AskpassRequest>(value) {
            Ok(request) => answer_askpass(&request, session, source),
            Err(_) => CredentialResponse::denied("malformed request"),
        }
    } else {
        match serde_json::from_value::<CredentialRequest>(value) {
            Ok(request) => answer(&request, session, source),
            Err(_) => CredentialResponse::denied("malformed request"),
        }
    }
}

/// Running server. Dropping it stops the listener and removes the socket.
pub struct CredentialServer {
    address: SocketAddress,
    session: Zeroizing<String>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl CredentialServer {
    pub fn start(source: Arc<dyn CredentialSource>) -> io::Result<Self> {
        let session = Zeroizing::new(random_hex(32)?);
        let address = SocketAddress::generate(&random_hex(8)?)?;
        let listener = ListenerOptions::new()
            .name(address.to_name()?)
            .create_sync()?;
        let stop = Arc::new(AtomicBool::new(false));

        let thread = {
            let stop = stop.clone();
            let session = session.clone();
            std::thread::Builder::new()
                .name("nanomgg-credential-server".into())
                .spawn(move || {
                    for conn in listener.incoming() {
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        let Ok(conn) = conn else { continue };
                        let source = source.clone();
                        let session = session.clone();
                        std::thread::spawn(move || {
                            let _ = serve(conn, &session, source.as_ref());
                        });
                    }
                })?
        };

        Ok(Self {
            address,
            session,
            stop,
            thread: Some(thread),
        })
    }

    pub fn address(&self) -> &SocketAddress {
        &self.address
    }

    /// Environment variables that route a git process's credential requests to `account_id`.
    pub fn env_for(&self, account_id: &str) -> Vec<(String, String)> {
        vec![
            (ENV_SOCKET.into(), self.address.0.clone()),
            (ENV_SESSION.into(), self.session.as_str().to_owned()),
            (ENV_ACCOUNT.into(), account_id.to_owned()),
        ]
    }
}

impl Drop for CredentialServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the blocking accept() so the thread notices `stop`.
        if let Ok(name) = self.address.to_name() {
            let _ = Stream::connect(name);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        self.address.cleanup();
    }
}

fn serve(conn: Stream, session: &str, source: &dyn CredentialSource) -> io::Result<()> {
    let mut reader = BufReader::new(conn);
    let mut line = Zeroizing::new(String::new());
    reader.read_line(&mut line)?;
    let response = answer_line(&line, session, source);
    let mut out = Zeroizing::new(serde_json::to_string(&response)?);
    out.push('\n');
    reader.get_mut().write_all(out.as_bytes())?;
    reader.get_mut().flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client;

    fn source() -> Arc<InMemorySource> {
        let s = InMemorySource::default();
        s.insert(
            "acc-alice",
            AccountCredential {
                host: "github.com".into(),
                username: "alice".into(),
                token: Zeroizing::new("ghp_alice".into()),
            },
        );
        s.insert(
            "acc-bob",
            AccountCredential {
                host: "github.com".into(),
                username: "bob".into(),
                token: Zeroizing::new("ghp_bob".into()),
            },
        );
        Arc::new(s)
    }

    fn req(session: &str, account: &str, host: &str) -> CredentialRequest {
        CredentialRequest {
            session: session.into(),
            account: account.into(),
            protocol: "https".into(),
            host: host.into(),
            path: None,
        }
    }

    #[test]
    fn same_host_different_accounts_get_their_own_token() {
        let src = source();
        let a = answer(&req("s", "acc-alice", "github.com"), "s", src.as_ref());
        let b = answer(&req("s", "acc-bob", "github.com"), "s", src.as_ref());
        assert!(
            matches!(&a, CredentialResponse::Credential { username, password } if username == "alice" && password == "ghp_alice")
        );
        assert!(
            matches!(&b, CredentialResponse::Credential { username, password } if username == "bob" && password == "ghp_bob")
        );
    }

    #[test]
    fn rejects_bad_session_unknown_account_and_foreign_host() {
        let src = source();
        for (r, why) in [
            (req("wrong", "acc-alice", "github.com"), "invalid session"),
            (req("s", "acc-carol", "github.com"), "unknown account"),
            (req("s", "acc-alice", "lfs.evil.io"), "does not belong"),
        ] {
            match &answer(&r, "s", src.as_ref()) {
                CredentialResponse::Denied { reason } => assert!(reason.contains(why), "{reason}"),
                CredentialResponse::Credential { .. } | CredentialResponse::Secret { .. } => {
                    panic!("expected denial for {r:?}")
                }
            }
        }
    }

    #[test]
    fn askpass_answers_only_passphrase_prompts_for_own_keys() {
        let src = source();
        src.insert_ssh_passphrase("acc-alice", "C:/keys/alice", Zeroizing::new("pp".into()));
        let ask = |session: &str, account: &str, prompt: &str| {
            answer_askpass(
                &AskpassRequest::new(session.into(), account.into(), prompt.into()),
                "s",
                src.as_ref(),
            )
        };
        assert!(matches!(
            &ask("s", "acc-alice", "Enter passphrase for key 'C:/keys/alice': "),
            CredentialResponse::Secret { value } if value == "pp"
        ));
        assert!(matches!(
            &ask(
                "s",
                "acc-alice",
                "Enter passphrase for key '/c/keys/alice':"
            ),
            CredentialResponse::Secret { .. }
        ));
        for (session, account, prompt) in [
            (
                "bad",
                "acc-alice",
                "Enter passphrase for key 'C:/keys/alice': ",
            ),
            ("s", "acc-bob", "Enter passphrase for key 'C:/keys/alice': "),
            (
                "s",
                "acc-alice",
                "Enter passphrase for key 'C:/keys/other': ",
            ),
            (
                "s",
                "acc-alice",
                "Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
            ),
            ("s", "acc-alice", "git@github.com's password: "),
            ("s", "acc-alice", "Username for 'https://github.com': "),
            ("s", "acc-alice", "Enter passphrase for key '': "),
        ] {
            assert!(
                matches!(
                    ask(session, account, prompt),
                    CredentialResponse::Denied { .. }
                ),
                "{session} {account} {prompt}"
            );
        }
        assert_eq!(
            passphrase_prompt_key("Enter passphrase for key 'C:/a b/k': "),
            Some("C:/a b/k")
        );
    }

    #[test]
    fn askpass_over_socket_and_malformed_lines() {
        let src = source();
        src.insert_ssh_passphrase(
            "acc-bob",
            "/home/bob/.ssh/id",
            Zeroizing::new("bob-pp".into()),
        );
        let server = CredentialServer::start(src.clone()).unwrap();
        let env: HashMap<_, _> = server.env_for("acc-bob").into_iter().collect();
        let res = client::askpass(
            server.address(),
            &AskpassRequest::new(
                env[ENV_SESSION].clone(),
                "acc-bob".into(),
                "Enter passphrase for key '/home/bob/.ssh/id': ".into(),
            ),
        )
        .unwrap();
        assert!(matches!(&res, CredentialResponse::Secret { value } if value == "bob-pp"));
        for line in ["not json", "{\"kind\":\"askpass\"}", "{}"] {
            assert!(matches!(
                answer_line(line, "s", src.as_ref()),
                CredentialResponse::Denied { .. }
            ));
        }
    }

    #[test]
    fn debug_output_hides_token() {
        let src = source();
        let dbg = format!("{:?}", src.credential("acc-alice").unwrap());
        assert!(!dbg.contains("ghp_alice"), "{dbg}");
    }

    #[test]
    fn end_to_end_over_local_socket() {
        let server = CredentialServer::start(source()).unwrap();
        let env: HashMap<_, _> = server.env_for("acc-bob").into_iter().collect();

        let ok = client::request(
            &SocketAddress(env[ENV_SOCKET].clone()),
            &CredentialRequest {
                session: env[ENV_SESSION].clone(),
                account: env[ENV_ACCOUNT].clone(),
                protocol: "https".into(),
                host: "github.com".into(),
                path: Some("acme/api.git".into()),
            },
        )
        .unwrap();
        assert!(
            matches!(&ok, CredentialResponse::Credential { username, .. } if username == "bob")
        );

        let bad = client::request(
            server.address(),
            &req("not-the-session", "acc-bob", "github.com"),
        )
        .unwrap();
        assert!(matches!(bad, CredentialResponse::Denied { .. }));

        let address = server.address().clone();
        drop(server);
        assert!(client::request(&address, &req("x", "acc-bob", "github.com")).is_err());
    }
}
