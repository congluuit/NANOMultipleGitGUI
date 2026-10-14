//! Account, Identity, Rule engine and repo-to-account bindings (PLAN §4.1, §4.3).
//!
//! Pure data and logic only: no database, keychain or process access. The storage crate
//! persists these types; the app turns a [`Resolution`] into a git-engine `AccountContext`.
//!
//! - [`model`]: providers, accounts, identities, SSH key metadata, bindings, rules, workspaces,
//!   plus their validation.
//! - [`url`]: remote URL parsing (`https://`, `ssh://`, scp-like `git@host:owner/repo.git`,
//!   ports, GitLab subgroups).
//! - [`rules`]: compiled rule set (host, owner/group, path glob, remote regex).
//! - [`resolve`]: the account-context resolution order of PLAN §4.3.
//! - [`colors`]: account color palette.

pub mod colors;
pub mod glob;
mod ids;
pub mod model;
pub mod resolve;
pub mod rules;
pub mod url;

pub use ids::new_id;
pub use model::{
    Account, AccountSecret, AuthMethod, BindingSource, Identity, Provider, ProviderKind,
    RepoBinding, Rule, SigningMethod, SshKey, Transport, ValidationError, Workspace,
};
pub use resolve::{
    AccountDirectory, ContextSource, RemoteInfo, RemoteResolution, Resolution, Resolved, resolve,
};
pub use rules::{MatchInput, MatchOptions, RuleSet};
pub use url::{RemoteUrl, UrlScheme};
