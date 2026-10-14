//! Credential plumbing that lets several accounts on the same host coexist.
//!
//! The app runs a [`CredentialServer`] on a local socket (named pipe on Windows, Unix socket
//! elsewhere). Every git/git-lfs process it spawns gets `credential.helper` pointed at the
//! `nanomgg-credential` sidecar plus three environment variables (socket, per-launch session
//! token, account id). The sidecar forwards git's request here and gets back the token of
//! exactly that account — never anything for a host that does not belong to the account.
//! The same sidecar doubles as `SSH_ASKPASS`, answering SSH key passphrase prompts for keys
//! of that account only.
//!
//! Secrets come from the OS keychain through [`KeychainSource`] (see [`keychain`]).

pub mod client;
pub mod git_protocol;
pub mod host;
pub mod keychain;
pub mod protocol;
mod server;
mod socket;

#[cfg(feature = "os-keychain")]
pub use keychain::OsKeychain;
pub use keychain::{
    AccountDirectory, AccountRef, KeychainError, KeychainSource, MemorySecretStore, SecretKey,
    SecretKind, SecretStore, SshKeyRef,
};
pub use server::{
    AccountCredential, CredentialServer, CredentialSource, InMemorySource, answer, answer_askpass,
    passphrase_prompt_key,
};
pub use socket::SocketAddress;
