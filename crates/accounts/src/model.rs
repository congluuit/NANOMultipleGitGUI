//! Multi-account data model (PLAN §4.1) and validation.
//!
//! Secrets are never part of these types: tokens and passphrases live in the OS keychain under
//! `nanomgg:<account_id>:<kind>` (SSH key passphrases under `nanomgg:<key_id>:ssh_passphrase`);
//! [`Account::secrets`] only records which entries exist.
//!
//! Enums serialize as camelCase for the UI; [`ProviderKind::as_str`] & co. give the snake_case
//! form stored in SQLite.

use serde::{Deserialize, Serialize};

use crate::{colors, glob, rules, url};

/// A field that failed validation, with a message suitable for the UI.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{field}: {message}")]
pub struct ValidationError {
    pub field: &'static str,
    pub message: String,
}

impl ValidationError {
    pub fn new(field: &'static str, message: impl Into<String>) -> Self {
        Self {
            field,
            message: message.into(),
        }
    }
}

fn require(field: &'static str, value: &str) -> Result<(), ValidationError> {
    if value.trim().is_empty() {
        Err(ValidationError::new(field, "must not be empty"))
    } else {
        Ok(())
    }
}

/// Implements `as_str` / `parse` for a fieldless enum with its database spelling.
macro_rules! db_enum {
    ($ty:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        impl $ty {
            pub const ALL: &'static [$ty] = &[$($ty::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $($ty::$variant => $s),+ }
            }

            pub fn parse(s: &str) -> Option<Self> {
                match s { $($s => Some($ty::$variant),)+ _ => None }
            }
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Github,
    GithubEnterprise,
    Gitlab,
    GitlabSelfHosted,
}

db_enum!(ProviderKind {
    Github => "github",
    GithubEnterprise => "github_enterprise",
    Gitlab => "gitlab",
    GitlabSelfHosted => "gitlab_self_hosted",
});

impl ProviderKind {
    pub fn is_github(self) -> bool {
        matches!(self, Self::Github | Self::GithubEnterprise)
    }

    /// Large-file thresholds in MB (warn, block): GitHub rejects blobs over 100 MB; GitLab
    /// limits vary per instance, so only warn (PLAN §13.1).
    pub fn default_large_file_limits(self) -> (u32, Option<u32>) {
        if self.is_github() {
            (50, Some(100))
        } else {
            (50, None)
        }
    }

    /// API root derived from the web base URL.
    pub fn default_api_url(self, base_url: &str) -> String {
        let base = base_url.trim_end_matches('/');
        match self {
            Self::Github => "https://api.github.com".to_owned(),
            Self::GithubEnterprise => format!("{base}/api/v3"),
            Self::Gitlab | Self::GitlabSelfHosted => format!("{base}/api/v4"),
        }
    }
}

/// A GitHub/GitLab instance. `github.com` and `gitlab.com` are built in (ids `github`, `gitlab`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub kind: ProviderKind,
    pub name: String,
    /// Web root, e.g. `https://github.com` or `https://gitlab.acme.dev:8443`.
    pub base_url: String,
    pub api_url: String,
    pub oauth_client_id: Option<String>,
    pub large_file_warn_mb: u32,
    pub large_file_block_mb: Option<u32>,
    pub builtin: bool,
}

pub const GITHUB_PROVIDER_ID: &str = "github";
pub const GITLAB_PROVIDER_ID: &str = "gitlab";

impl Provider {
    pub fn github_com() -> Self {
        Self {
            id: GITHUB_PROVIDER_ID.into(),
            kind: ProviderKind::Github,
            name: "GitHub".into(),
            base_url: "https://github.com".into(),
            api_url: "https://api.github.com".into(),
            oauth_client_id: None,
            large_file_warn_mb: 50,
            large_file_block_mb: Some(100),
            builtin: true,
        }
    }

    pub fn gitlab_com() -> Self {
        Self {
            id: GITLAB_PROVIDER_ID.into(),
            kind: ProviderKind::Gitlab,
            name: "GitLab.com".into(),
            base_url: "https://gitlab.com".into(),
            api_url: "https://gitlab.com/api/v4".into(),
            oauth_client_id: None,
            large_file_warn_mb: 50,
            large_file_block_mb: None,
            builtin: true,
        }
    }

    pub fn builtins() -> Vec<Self> {
        vec![Self::github_com(), Self::gitlab_com()]
    }

    /// `host[:port]` of the base URL — what the credential server compares requests against.
    pub fn host(&self) -> Option<String> {
        url::RemoteUrl::parse(&self.base_url)
            .filter(|u| !u.is_local())
            .map(|u| u.host_port())
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        require("id", &self.id)?;
        require("name", &self.name)?;
        for (field, value) in [("baseUrl", &self.base_url), ("apiUrl", &self.api_url)] {
            match url::RemoteUrl::parse(value) {
                Some(u) if matches!(u.scheme, url::UrlScheme::Https | url::UrlScheme::Http) => {
                    if u.scheme == url::UrlScheme::Http && !is_loopback(&u.host) {
                        return Err(ValidationError::new(field, "must use https://"));
                    }
                }
                _ => return Err(ValidationError::new(field, "must be an http(s) URL")),
            }
        }
        if matches!(self.kind, ProviderKind::Github | ProviderKind::Gitlab) && !self.builtin {
            return Err(ValidationError::new(
                "kind",
                "github.com and gitlab.com are built in; use the Enterprise/self-hosted kind",
            ));
        }
        if self.large_file_warn_mb == 0 {
            return Err(ValidationError::new("largeFileWarnMb", "must be positive"));
        }
        if let Some(block) = self.large_file_block_mb
            && block < self.large_file_warn_mb
        {
            return Err(ValidationError::new(
                "largeFileBlockMb",
                "must not be lower than the warning threshold",
            ));
        }
        Ok(())
    }
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "::1")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum AuthMethod {
    Oauth,
    Pat,
    /// No HTTPS token: the account is only used over SSH (e.g. imported from `~/.ssh/config`).
    SshOnly,
    /// Token copied from the `gh` / `glab` CLI (PLAN 3B.1). Treated like a PAT: validated and
    /// re-validated, never refreshed by the app; the CLI's own config is never modified.
    ImportedCli,
}

db_enum!(AuthMethod {
    Oauth => "oauth",
    Pat => "pat",
    SshOnly => "ssh_only",
    ImportedCli => "imported_cli",
});

/// Which keychain entries of an account exist (the values never leave the keychain).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum AccountSecret {
    Token,
    RefreshToken,
}

db_enum!(AccountSecret {
    Token => "token",
    RefreshToken => "refresh_token",
});

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub provider_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub auth_method: AuthMethod,
    pub scopes: Vec<String>,
    /// Milliseconds since the Unix epoch; `None` for tokens that do not expire.
    pub token_expires_at: Option<i64>,
    /// `#rrggbb`, one of [`colors::ACCOUNT_COLORS`] by default.
    pub color: String,
    /// Short user label ("Công ty", "Cá nhân"…).
    pub label: Option<String>,
    /// Clone destination root for this account (`{clone_dir}/{owner}/{repo}`).
    pub default_clone_dir: Option<String>,
    /// Keychain entries present for this account.
    pub secrets: Vec<AccountSecret>,
    /// Position in the switcher (`Ctrl/Cmd+Alt+1..9`).
    pub sort_order: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Account {
    pub fn has_secret(&self, kind: AccountSecret) -> bool {
        self.secrets.contains(&kind)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        require("id", &self.id)?;
        require("providerId", &self.provider_id)?;
        require("username", &self.username)?;
        if self.username.contains(char::is_whitespace) || self.username.contains(['/', ':', '@']) {
            return Err(ValidationError::new(
                "username",
                "contains invalid characters",
            ));
        }
        if !colors::is_hex_color(&self.color) {
            return Err(ValidationError::new("color", "must be a #rrggbb color"));
        }
        if let Some(dir) = &self.default_clone_dir {
            require("defaultCloneDir", dir)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum SigningMethod {
    #[default]
    None,
    Gpg,
    Ssh,
}

db_enum!(SigningMethod {
    None => "none",
    Gpg => "gpg",
    Ssh => "ssh",
});

/// Commit author identity; an account can have several (work and personal email…).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub email: String,
    pub signing: SigningMethod,
    /// GPG key id, or SSH public key path / `key::ssh-ed25519 …` for SSH signing.
    pub signing_key_ref: Option<String>,
    /// Used when a binding or rule does not name an identity.
    pub is_default: bool,
    pub created_at: i64,
}

impl Identity {
    pub fn validate(&self) -> Result<(), ValidationError> {
        require("id", &self.id)?;
        require("accountId", &self.account_id)?;
        require("name", &self.name)?;
        if self.name.contains(['<', '>', '\n']) {
            return Err(ValidationError::new("name", "contains invalid characters"));
        }
        let email = self.email.trim();
        let valid_email = email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && !domain.is_empty() && !domain.contains('@')
        }) && !email.contains(['<', '>', ' ', '\n']);
        if !valid_email {
            return Err(ValidationError::new(
                "email",
                "is not a valid email address",
            ));
        }
        if self.signing != SigningMethod::None
            && self
                .signing_key_ref
                .as_deref()
                .is_none_or(|k| k.trim().is_empty())
        {
            return Err(ValidationError::new(
                "signingKeyRef",
                "is required when signing is enabled",
            ));
        }
        Ok(())
    }
}

/// SSH key metadata. The private key stays in a user-only file (or the keychain); its
/// passphrase, if any, is in the keychain under `nanomgg:<key_id>:ssh_passphrase`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SshKey {
    pub id: String,
    /// `None` for an imported key not yet assigned to an account.
    pub account_id: Option<String>,
    /// `ed25519`, `rsa`, `ecdsa`…
    pub key_type: String,
    /// `SHA256:…` as printed by `ssh-keygen -lf`.
    pub fingerprint: String,
    pub public_key: String,
    /// Private key file used for `ssh -i`; `None` when the key is not available as a file.
    pub private_key_path: Option<String>,
    pub has_passphrase: bool,
    pub uploaded_to_provider: bool,
    /// Key id on the provider after upload (for later removal).
    pub provider_key_id: Option<String>,
    pub comment: Option<String>,
    pub created_at: i64,
}

impl SshKey {
    pub fn validate(&self) -> Result<(), ValidationError> {
        require("id", &self.id)?;
        require("keyType", &self.key_type)?;
        require("fingerprint", &self.fingerprint)?;
        require("publicKey", &self.public_key)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum Transport {
    Https,
    Ssh,
}

db_enum!(Transport {
    Https => "https",
    Ssh => "ssh",
});

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum BindingSource {
    /// Remembered result of the rule engine.
    AutoRule,
    /// Chosen by the user ("Dùng cho repo này").
    Manual,
    /// Created when the repo was cloned through the app.
    Clone,
}

db_enum!(BindingSource {
    AutoRule => "auto_rule",
    Manual => "manual",
    Clone => "clone",
});

/// Repo → account binding, either for one remote or (remote `None`) as the repo default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RepoBinding {
    pub repo_id: i64,
    pub remote_name: Option<String>,
    pub account_id: String,
    /// `None` → the account's default identity.
    pub identity_id: Option<String>,
    /// Preferred transport for clone/new remotes; existing remotes follow their URL.
    pub transport: Option<Transport>,
    pub source: BindingSource,
    /// Pinned by the user: wins over everything else (PLAN §4.3 step 1).
    pub locked: bool,
    pub updated_at: i64,
}

impl RepoBinding {
    pub fn validate(&self) -> Result<(), ValidationError> {
        require("accountId", &self.account_id)?;
        if let Some(remote) = &self.remote_name {
            require("remoteName", remote)?;
        }
        Ok(())
    }
}

/// "If a repo matches these conditions, use this account". All given conditions must match;
/// rules are tried in ascending `priority`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: String,
    pub priority: i64,
    /// Remote host, glob allowed (`*.acme.dev`); with `:port` it must match the port too.
    pub host: Option<String>,
    /// Owner or group path prefix of the remote, glob per segment (`acme-corp`, `acme/platform`).
    pub owner_or_group: Option<String>,
    /// Glob on the repository folder (`D:/Work/**`); a plain folder also matches below it.
    pub path_glob: Option<String>,
    /// Regex on the remote URL as configured (password removed).
    pub remote_regex: Option<String>,
    pub account_id: String,
    pub identity_id: Option<String>,
    pub enabled: bool,
    pub created_at: i64,
}

impl Rule {
    pub fn has_conditions(&self) -> bool {
        [
            &self.host,
            &self.owner_or_group,
            &self.path_glob,
            &self.remote_regex,
        ]
        .iter()
        .any(|c| c.as_deref().is_some_and(|s| !s.trim().is_empty()))
    }

    /// Whether the rule needs a remote URL to match (host, owner or regex condition).
    pub fn needs_remote(&self) -> bool {
        self.host.is_some() || self.owner_or_group.is_some() || self.remote_regex.is_some()
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        require("id", &self.id)?;
        require("accountId", &self.account_id)?;
        if !self.has_conditions() {
            return Err(ValidationError::new(
                "match",
                "a rule needs at least one condition (host, owner/group, folder or regex)",
            ));
        }
        for (field, value) in [
            ("host", &self.host),
            ("ownerOrGroup", &self.owner_or_group),
            ("pathGlob", &self.path_glob),
            ("remoteRegex", &self.remote_regex),
        ] {
            if let Some(v) = value
                && v.trim().is_empty()
            {
                return Err(ValidationError::new(
                    field,
                    "must not be blank (omit it instead)",
                ));
            }
        }
        if let Some(host) = &self.host
            && host.contains(['/', '@', ' '])
        {
            return Err(ValidationError::new(
                "host",
                "must be a host name like github.com",
            ));
        }
        rules::CompiledRule::compile(self, &rules::MatchOptions::default()).map(|_| ())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub repo_ids: Vec<i64>,
    pub default_account_id: Option<String>,
    pub created_at: i64,
}

impl Workspace {
    pub fn validate(&self) -> Result<(), ValidationError> {
        require("id", &self.id)?;
        require("name", &self.name)
    }
}

/// Validates a glob for UI feedback.
pub fn validate_glob(field: &'static str, pattern: &str) -> Result<(), ValidationError> {
    glob::compile(pattern, false)
        .map(|_| ())
        .map_err(|e| ValidationError::new(field, e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> Account {
        Account {
            id: "acc_1".into(),
            provider_id: "github".into(),
            username: "alice".into(),
            display_name: None,
            avatar_url: None,
            auth_method: AuthMethod::Pat,
            scopes: vec!["repo".into()],
            token_expires_at: None,
            color: colors::ACCOUNT_COLORS[0].into(),
            label: Some("Công ty".into()),
            default_clone_dir: None,
            secrets: vec![],
            sort_order: 0,
            created_at: 0,
            updated_at: 0,
        }
    }

    pub(crate) fn rule() -> Rule {
        Rule {
            id: "rul_1".into(),
            priority: 0,
            host: Some("github.com".into()),
            owner_or_group: None,
            path_glob: None,
            remote_regex: None,
            account_id: "acc_1".into(),
            identity_id: None,
            enabled: true,
            created_at: 0,
        }
    }

    #[test]
    fn db_spelling_round_trips() {
        for k in ProviderKind::ALL {
            assert_eq!(ProviderKind::parse(k.as_str()), Some(*k));
        }
        assert_eq!(
            ProviderKind::GitlabSelfHosted.as_str(),
            "gitlab_self_hosted"
        );
        assert_eq!(
            BindingSource::parse("auto_rule"),
            Some(BindingSource::AutoRule)
        );
        assert_eq!(AuthMethod::parse("nope"), None);
        for m in AuthMethod::ALL {
            assert_eq!(AuthMethod::parse(m.as_str()), Some(*m));
        }
        assert_eq!(AuthMethod::ImportedCli.as_str(), "imported_cli");
        assert_eq!(
            serde_json::to_value(AuthMethod::ImportedCli).unwrap(),
            serde_json::json!("importedCli")
        );
        assert_eq!(
            serde_json::to_value(ProviderKind::GithubEnterprise).unwrap(),
            serde_json::json!("githubEnterprise")
        );
    }

    #[test]
    fn builtin_providers_are_valid_and_expose_hosts() {
        for p in Provider::builtins() {
            p.validate().unwrap();
        }
        assert_eq!(Provider::github_com().host().as_deref(), Some("github.com"));
        let mut ghe = Provider::github_com();
        ghe.id = "prv_x".into();
        ghe.builtin = false;
        assert!(ghe.validate().is_err(), "custom github.com provider");
        ghe.kind = ProviderKind::GithubEnterprise;
        ghe.base_url = "https://git.acme.dev:8443/".into();
        ghe.api_url = ProviderKind::GithubEnterprise.default_api_url(&ghe.base_url);
        assert_eq!(ghe.api_url, "https://git.acme.dev:8443/api/v3");
        ghe.validate().unwrap();
        assert_eq!(ghe.host().as_deref(), Some("git.acme.dev:8443"));
        ghe.base_url = "http://git.acme.dev".into();
        assert_eq!(ghe.validate().unwrap_err().field, "baseUrl");
        ghe.base_url = "http://127.0.0.1:8080".into();
        ghe.validate().unwrap();
        ghe.large_file_block_mb = Some(10);
        assert_eq!(ghe.validate().unwrap_err().field, "largeFileBlockMb");
    }

    #[test]
    fn account_validation() {
        account().validate().unwrap();
        let mut a = account();
        a.username = "al ice".into();
        assert_eq!(a.validate().unwrap_err().field, "username");
        let mut a = account();
        a.color = "red".into();
        assert_eq!(a.validate().unwrap_err().field, "color");
    }

    #[test]
    fn identity_validation() {
        let mut id = Identity {
            id: "idn_1".into(),
            account_id: "acc_1".into(),
            name: "Alice".into(),
            email: "alice@acme.dev".into(),
            signing: SigningMethod::None,
            signing_key_ref: None,
            is_default: true,
            created_at: 0,
        };
        id.validate().unwrap();
        id.email = "alice".into();
        assert_eq!(id.validate().unwrap_err().field, "email");
        id.email = "a <b@c>".into();
        assert!(id.validate().is_err());
        id.email = "alice@acme.dev".into();
        id.signing = SigningMethod::Ssh;
        assert_eq!(id.validate().unwrap_err().field, "signingKeyRef");
        id.signing_key_ref = Some("~/.ssh/id_ed25519.pub".into());
        id.validate().unwrap();
    }

    #[test]
    fn rule_validation() {
        rule().validate().unwrap();
        let mut r = rule();
        r.host = None;
        assert_eq!(r.validate().unwrap_err().field, "match");
        r.remote_regex = Some("([unclosed".into());
        assert_eq!(r.validate().unwrap_err().field, "remoteRegex");
        r.remote_regex = Some("  ".into());
        assert!(r.validate().is_err());
        let mut r = rule();
        r.host = Some("https://github.com/".into());
        assert_eq!(r.validate().unwrap_err().field, "host");
        let mut r = rule();
        r.host = None;
        r.path_glob = Some("D:/Work/**".into());
        assert!(!r.needs_remote());
        r.validate().unwrap();
    }
}
