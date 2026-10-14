//! `nanomgg-credential`: sidecar invoked by git and git-lfs as a credential helper, and by
//! `ssh` as `SSH_ASKPASS`.
//!
//! The app configures it per git process via `GIT_CONFIG_COUNT` and passes the socket,
//! session token and account id in `NANOMGG_*` environment variables. On `get` it asks the
//! running app for that account's credential and prints it in git's helper format. It never
//! stores anything itself. Run outside the app (variables missing) it stays silent so git
//! falls back to its normal behavior.
//!
//! Askpass mode: `ssh` runs `$SSH_ASKPASS "<prompt>"` (forced with `SSH_ASKPASS_REQUIRE=force`).
//! When `NANOMGG_ASKPASS=1` is set, an invocation whose argument is not a helper operation is
//! such a prompt; `nanomgg-credential askpass "<prompt>"` is the explicit form. The prompt is
//! forwarded to the app, which answers only SSH key passphrase prompts for a key of the bound
//! account; the passphrase is printed on stdout. Anything else exits with status 1 so ssh/git
//! treat it as "no answer".

use std::io::{self, Read, Write};
use std::process::ExitCode;

use nanomgg_credentials::protocol::{
    AskpassRequest, CredentialRequest, CredentialResponse, ENV_ACCOUNT, ENV_ASKPASS, ENV_SESSION,
    ENV_SOCKET,
};
use nanomgg_credentials::{SocketAddress, client, git_protocol};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let askpass_env = std::env::var_os(ENV_ASKPASS).is_some_and(|v| v == "1");
    match args.first().map(String::as_str) {
        Some("--version") => println!("nanomgg-credential {}", env!("CARGO_PKG_VERSION")),
        Some("get") => get(),
        Some("store" | "erase") | None => drain_stdin(),
        Some("askpass") => return askpass(&args[1..].join(" ")),
        Some(_) if askpass_env => return askpass(&args.join(" ")),
        // `store` / `erase` / unknown: tokens live in the app's keychain, nothing to do here.
        Some(_) => drain_stdin(),
    }
    ExitCode::SUCCESS
}

fn drain_stdin() {
    let _ = io::stdin().read_to_end(&mut Vec::new());
}

fn session_env() -> Option<(String, String, String)> {
    match (
        std::env::var(ENV_SOCKET),
        std::env::var(ENV_SESSION),
        std::env::var(ENV_ACCOUNT),
    ) {
        (Ok(socket), Ok(session), Ok(account)) => Some((socket, session, account)),
        _ => None,
    }
}

fn get() {
    let attrs = match git_protocol::read_attributes(io::stdin().lock()) {
        Ok(attrs) => attrs,
        Err(e) => return warn(&format!("could not read request from git: {e}")),
    };
    let Some((socket, session, account)) = session_env() else {
        return;
    };
    let request = CredentialRequest {
        session,
        account,
        protocol: attrs.get("protocol").cloned().unwrap_or_default(),
        host: attrs.get("host").cloned().unwrap_or_default(),
        path: attrs.get("path").cloned(),
    };

    match &client::request(&SocketAddress(socket), &request) {
        Ok(CredentialResponse::Credential { username, password }) => {
            let mut out = io::stdout().lock();
            let _ = out.write_all(git_protocol::format_credential(username, password).as_bytes());
            let _ = out.flush();
        }
        Ok(CredentialResponse::Denied { reason }) => {
            warn(reason);
            // Stop git from consulting other helpers or prompting for this request.
            println!("quit=1");
        }
        Ok(CredentialResponse::Secret { .. }) => warn("unexpected answer from the app"),
        Err(e) => warn(&format!("cannot reach NANOMultipleGitGUI ({e})")),
    }
}

fn askpass(prompt: &str) -> ExitCode {
    let Some((socket, session, account)) = session_env() else {
        warn("askpass used outside NANOMultipleGitGUI");
        return ExitCode::FAILURE;
    };
    let request = AskpassRequest::new(session, account, prompt.to_owned());
    match &client::askpass(&SocketAddress(socket), &request) {
        Ok(CredentialResponse::Secret { value }) => {
            let mut out = io::stdout().lock();
            let _ = out.write_all(value.as_bytes());
            let _ = out.write_all(b"\n");
            let _ = out.flush();
            ExitCode::SUCCESS
        }
        Ok(CredentialResponse::Denied { reason }) => {
            warn(reason);
            ExitCode::FAILURE
        }
        Ok(CredentialResponse::Credential { .. }) => {
            warn("unexpected answer from the app");
            ExitCode::FAILURE
        }
        Err(e) => {
            warn(&format!("cannot reach NANOMultipleGitGUI ({e})"));
            ExitCode::FAILURE
        }
    }
}

fn warn(message: &str) {
    eprintln!("nanomgg-credential: {message}");
}
