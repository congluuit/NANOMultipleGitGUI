//! Wire format between the app (server) and `nanomgg-credential` (client):
//! one JSON request line, one JSON response line, then the connection closes.

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

/// Environment variables the app sets on every git process it spawns.
pub const ENV_SOCKET: &str = "NANOMGG_SOCKET";
pub const ENV_SESSION: &str = "NANOMGG_SESSION";
pub const ENV_ACCOUNT: &str = "NANOMGG_ACCOUNT";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CredentialRequest {
    pub session: String,
    pub account: String,
    /// `protocol` from the git credential request (`https`, `http`).
    pub protocol: String,
    /// `host` from the git credential request; includes `:port` when non-default.
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// Set (to `1`) on git processes whose `SSH_ASKPASS`/`GIT_ASKPASS` points at the sidecar, so it
/// knows that an invocation with a prompt as its only argument is an askpass call.
pub const ENV_ASKPASS: &str = "NANOMGG_ASKPASS";

/// `kind` value of an [`AskpassRequest`]. Credential requests carry no `kind`, which keeps
/// helpers built before askpass existed working unchanged.
pub const KIND_ASKPASS: &str = "askpass";

/// Prompt shown by `ssh` (through `SSH_ASKPASS`) that the sidecar forwards to the app. Only SSH
/// key passphrase prompts for a key of the requesting account are answered.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AskpassRequest {
    /// Always [`KIND_ASKPASS`].
    pub kind: String,
    pub session: String,
    pub account: String,
    pub prompt: String,
}

impl AskpassRequest {
    pub fn new(session: String, account: String, prompt: String) -> Self {
        Self {
            kind: KIND_ASKPASS.to_owned(),
            session,
            account,
            prompt,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "result", rename_all = "camelCase")]
pub enum CredentialResponse {
    Credential {
        username: String,
        password: String,
    },
    /// Answer to an [`AskpassRequest`].
    Secret {
        value: String,
    },
    Denied {
        reason: String,
    },
}

impl Drop for CredentialResponse {
    fn drop(&mut self) {
        match self {
            Self::Credential { password, .. } => password.zeroize(),
            Self::Secret { value } => value.zeroize(),
            Self::Denied { .. } => {}
        }
    }
}

impl CredentialResponse {
    pub fn denied(reason: impl Into<String>) -> Self {
        Self::Denied {
            reason: reason.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_json_shape() {
        let ok = CredentialResponse::Credential {
            username: "alice".into(),
            password: "t0ken".into(),
        };
        assert_eq!(
            serde_json::to_value(&ok).unwrap(),
            serde_json::json!({"result": "credential", "username": "alice", "password": "t0ken"})
        );
        let denied = CredentialResponse::denied("host mismatch");
        assert_eq!(
            serde_json::to_value(&denied).unwrap(),
            serde_json::json!({"result": "denied", "reason": "host mismatch"})
        );
        let secret = CredentialResponse::Secret { value: "pw".into() };
        assert_eq!(
            serde_json::to_value(&secret).unwrap(),
            serde_json::json!({"result": "secret", "value": "pw"})
        );
    }

    #[test]
    fn askpass_request_json_shape() {
        let req = AskpassRequest::new("s".into(), "acc".into(), "Enter passphrase".into());
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
            serde_json::json!({"kind": "askpass", "session": "s", "account": "acc", "prompt": "Enter passphrase"})
        );
        // Old credential requests (no `kind`) still parse as before.
        let old: CredentialRequest = serde_json::from_value(serde_json::json!({
            "session": "s", "account": "a", "protocol": "https", "host": "github.com"
        }))
        .unwrap();
        assert_eq!(old.path, None);
    }
}
