//! Implementation of the auth commands, independent of the Tauri runtime (tests call these
//! directly with an in-memory [`AppState`] and a wiremock provider).

use nanomgg_accounts::{
    Account, AccountSecret, AuthMethod, Provider, ProviderKind as AccountProviderKind,
    SigningMethod,
};
use nanomgg_auth::{
    AuthError, DeviceFlowConfig, Feature, LoopbackLogin, PkceConfig, TokenInfo, TokenSet,
    default_scopes, needs_refresh, poll_device_token, refresh_gitlab_token, request_device_code,
    token_health, validate_token,
};
use nanomgg_providers::Provider as _;
use nanomgg_providers::{
    ProviderClient, ProviderEmail, ProviderEndpoint, ProviderError, ProviderFamily, ProviderKind,
    ProviderUser, SecretToken, now_unix,
};
use zeroize::Zeroizing;

use super::{
    AccountHealth, AuthCommandError, AuthResult, AuthRuntime, AuthStatus, DEFAULT_LOOPBACK_PORT,
    DeviceStart, PendingFlow, PkceStart, ProviderAuthConfig, REFRESH_MARGIN_SECS,
};
use crate::accounts::commands::{
    AccountInput, IdentityInput, account_save_impl, account_secret, account_store_tokens,
    identity_save_impl,
};
use crate::error::AppError;
use crate::state::AppState;

/// Settings key prefix of a provider's loopback port.
pub const KEY_LOOPBACK_PORT: &str = "auth.loopbackPort.";
/// Settings key prefix of an account's [`AuthStatus`].
pub const KEY_STATUS: &str = "auth.status.";

/// Features every account should be able to serve (badge "missing scope").
const CORE_FEATURES: [Feature; 3] = [
    Feature::GitHttps,
    Feature::ListRepositories,
    Feature::ReadEmails,
];
/// Features that only some users need (shown as hints).
const OPTIONAL_FEATURES: [Feature; 2] = [Feature::UploadSshKey, Feature::EditWorkflows];

// ----- providers -----------------------------------------------------------------------------

pub fn endpoint_kind(kind: AccountProviderKind) -> ProviderKind {
    match kind {
        AccountProviderKind::Github => ProviderKind::Github,
        AccountProviderKind::GithubEnterprise => ProviderKind::GithubEnterprise,
        AccountProviderKind::Gitlab => ProviderKind::Gitlab,
        AccountProviderKind::GitlabSelfHosted => ProviderKind::GitlabSelfHosted,
    }
}

pub fn family(provider: &Provider) -> ProviderFamily {
    endpoint_kind(provider.kind).family()
}

/// Endpoint of a stored provider row (custom URLs so self-hosted sub-paths and test servers
/// work unchanged).
pub fn endpoint_for(provider: &Provider) -> AuthResult<ProviderEndpoint> {
    Ok(ProviderEndpoint::custom(
        endpoint_kind(provider.kind),
        &provider.base_url,
        &provider.api_url,
    )?)
}

/// Client id compiled into the app for github.com / gitlab.com (none until the project
/// registers its OAuth apps, PLAN §9).
pub fn builtin_client_id(provider: &Provider) -> Option<&'static str> {
    if !provider.builtin {
        return None;
    }
    let id = match provider.kind {
        AccountProviderKind::Github => option_env!("NANOMGG_GITHUB_OAUTH_CLIENT_ID"),
        AccountProviderKind::Gitlab => option_env!("NANOMGG_GITLAB_OAUTH_CLIENT_ID"),
        _ => None,
    };
    id.filter(|s| !s.trim().is_empty())
}

/// The provider's own client id, else the built-in one.
pub fn client_id(provider: &Provider) -> Option<String> {
    provider
        .oauth_client_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| builtin_client_id(provider).map(str::to_owned))
}

/// Scopes to tick when creating a PAT (same as the OAuth defaults).
pub fn pat_scopes(provider: &Provider) -> Vec<String> {
    default_scopes(family(provider))
}

/// Token creation page with the scopes pre-selected.
pub fn pat_url(provider: &Provider) -> String {
    let base = provider.base_url.trim_end_matches('/');
    let scopes = pat_scopes(provider).join(",");
    match family(provider) {
        ProviderFamily::Github => {
            format!("{base}/settings/tokens/new?scopes={scopes}&description=NANOMultipleGitGUI")
        }
        ProviderFamily::Gitlab => format!(
            "{base}/-/user_settings/personal_access_tokens?name=NANOMultipleGitGUI&scopes={scopes}"
        ),
    }
}

/// Where a user/admin registers an OAuth application on the instance.
pub fn oauth_apps_url(provider: &Provider) -> String {
    let base = provider.base_url.trim_end_matches('/');
    match family(provider) {
        ProviderFamily::Github => format!("{base}/settings/applications/new"),
        ProviderFamily::Gitlab => format!("{base}/-/user_settings/applications"),
    }
}

pub fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}/callback")
}

pub fn loopback_port(st: &AppState, provider_id: &str) -> u16 {
    st.storage
        .get_setting::<u16>(&format!("{KEY_LOOPBACK_PORT}{provider_id}"))
        .ok()
        .flatten()
        .filter(|p| *p >= 1024)
        .unwrap_or(DEFAULT_LOOPBACK_PORT)
}

pub fn set_loopback_port(st: &AppState, provider_id: &str, port: u16) -> AuthResult<()> {
    if port < 1024 {
        return Err(
            AppError::InvalidInput("the port must be between 1024 and 65535".into()).into(),
        );
    }
    provider(st, provider_id)?;
    st.storage
        .set_setting(&format!("{KEY_LOOPBACK_PORT}{provider_id}"), &port)?;
    Ok(())
}

pub fn auth_config(st: &AppState, provider_id: &str) -> AuthResult<ProviderAuthConfig> {
    let p = provider(st, provider_id)?;
    let port = loopback_port(st, &p.id);
    let gitlab = family(&p) == ProviderFamily::Gitlab;
    let own = p
        .oauth_client_id
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty());
    Ok(ProviderAuthConfig {
        provider_id: p.id.clone(),
        has_client_id: client_id(&p).is_some(),
        builtin_client_id: !own && builtin_client_id(&p).is_some(),
        device_flow: true,
        pkce: gitlab,
        loopback_port: port,
        redirect_uri: redirect_uri(port),
        pat_url: pat_url(&p),
        pat_scopes: pat_scopes(&p),
        oauth_apps_url: oauth_apps_url(&p),
    })
}

fn provider(st: &AppState, id: &str) -> AuthResult<Provider> {
    st.storage
        .provider(id)?
        .ok_or_else(|| AppError::InvalidInput(format!("provider `{id}` not found")).into())
}

fn account(st: &AppState, id: &str) -> AuthResult<Account> {
    st.storage
        .account(id)?
        .ok_or_else(|| AppError::InvalidInput(format!("account `{id}` not found")).into())
}

fn missing_client_id(p: &Provider) -> AuthCommandError {
    AuthCommandError::MissingClientId {
        message: format!(
            "{} has no OAuth client id yet; sign in with a personal access token",
            p.name
        ),
        provider_id: p.id.clone(),
        pat_url: pat_url(p),
        pat_scopes: pat_scopes(p),
    }
}

fn clean_scopes(scopes: Option<Vec<String>>) -> Option<Vec<String>> {
    scopes
        .map(|list| {
            list.into_iter()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
        })
        .filter(|list| !list.is_empty())
}

// ----- status --------------------------------------------------------------------------------

pub fn read_status(st: &AppState, account_id: &str) -> AuthStatus {
    st.storage
        .get_setting::<AuthStatus>(&format!("{KEY_STATUS}{account_id}"))
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn write_status(st: &AppState, account_id: &str, status: &AuthStatus) {
    if let Err(e) = st
        .storage
        .set_setting(&format!("{KEY_STATUS}{account_id}"), status)
    {
        tracing::warn!(account = account_id, error = %e, "could not save the auth status");
    }
}

fn now_ms() -> i64 {
    now_unix() * 1000
}

/// The token was rejected for good: the user has to sign in again.
fn is_rejection(e: &AuthCommandError) -> bool {
    matches!(
        e,
        AuthCommandError::Auth {
            error: AuthError::RefreshRejected { .. }
                | AuthError::TokenRevoked
                | AuthError::Provider {
                    error: ProviderError::Unauthorized { .. }
                },
            ..
        }
    )
}

fn record_failure(st: &AppState, account_id: &str, e: &AuthCommandError) {
    let mut status = read_status(st, account_id);
    status.last_error = Some(e.to_string());
    if is_rejection(e) {
        status.needs_sign_in = true;
    }
    tracing::warn!(account = account_id, error = %e, needs_sign_in = status.needs_sign_in, "token check failed");
    write_status(st, account_id, &status);
}

// ----- flows ---------------------------------------------------------------------------------

pub async fn device_start(
    st: &AppState,
    rt: &AuthRuntime,
    provider_id: &str,
    scopes: Option<Vec<String>>,
) -> AuthResult<DeviceStart> {
    let p = provider(st, provider_id)?;
    let id = client_id(&p).ok_or_else(|| missing_client_id(&p))?;
    let endpoint = endpoint_for(&p)?;
    let mut config = DeviceFlowConfig::for_endpoint(&endpoint, &id, clean_scopes(scopes))?;
    config.min_poll_interval = rt.min_poll_interval;
    let auth = request_device_code(&rt.http, &config).await?;
    let prompt = auth.prompt.clone();
    let flow_id = rt.register(PendingFlow::Device {
        provider: p,
        config,
        auth,
    });
    Ok(DeviceStart { flow_id, prompt })
}

pub async fn pkce_start(
    st: &AppState,
    rt: &AuthRuntime,
    provider_id: &str,
    scopes: Option<Vec<String>>,
) -> AuthResult<PkceStart> {
    let p = provider(st, provider_id)?;
    if family(&p) != ProviderFamily::Gitlab {
        return Err(AuthCommandError::Unsupported {
            message: "browser sign-in (PKCE) is available for GitLab; use the device flow".into(),
        });
    }
    let id = client_id(&p).ok_or_else(|| missing_client_id(&p))?;
    let endpoint = endpoint_for(&p)?;
    let mut config = PkceConfig::gitlab(&endpoint, &id, clean_scopes(scopes))?;
    config.port = loopback_port(st, &p.id);
    let login = LoopbackLogin::start(config).await?;
    let authorize_url = login.authorize_url().to_string();
    let redirect_uri = login.redirect_uri().to_owned();
    let flow_id = rt.register(PendingFlow::Pkce { provider: p, login });
    Ok(PkceStart {
        flow_id,
        authorize_url,
        redirect_uri,
    })
}

/// Waits for a device or PKCE flow and stores the result.
pub async fn flow_wait(st: &AppState, rt: &AuthRuntime, flow_id: &str) -> AuthResult<Account> {
    let (pending, cancel) = rt.take(flow_id)?;
    let result = async {
        let (provider, tokens) = match pending {
            PendingFlow::Device {
                provider,
                config,
                auth,
            } => {
                let tokens = poll_device_token(&rt.http, &config, &auth, &cancel).await?;
                (provider, tokens)
            }
            PendingFlow::Pkce { provider, login } => {
                let tokens = login.finish(&rt.http, &cancel).await?;
                (provider, tokens)
            }
        };
        complete_oauth(st, rt, &provider, tokens).await
    }
    .await;
    rt.finish(flow_id);
    result
}

/// Validates an OAuth token set and saves the account.
pub async fn complete_oauth(
    st: &AppState,
    rt: &AuthRuntime,
    provider: &Provider,
    tokens: TokenSet,
) -> AuthResult<Account> {
    let endpoint = endpoint_for(provider)?;
    let info = validate_token(&rt.http, &endpoint, &tokens.access_token).await?;
    let scopes = if tokens.scopes.is_empty() {
        info.scopes.clone()
    } else {
        Some(tokens.scopes.clone())
    };
    save_login(
        st,
        rt,
        provider,
        &endpoint,
        &info,
        Login {
            method: AuthMethod::Oauth,
            access: &tokens.access_token,
            refresh: tokens.refresh_token.as_ref(),
            expires_at: tokens.expires_at.or(info.expires_at),
            scopes,
            label: None,
        },
    )
    .await
}

pub async fn add_pat(
    st: &AppState,
    rt: &AuthRuntime,
    provider_id: &str,
    token: Zeroizing<String>,
    label: Option<String>,
) -> AuthResult<Account> {
    let p = provider(st, provider_id)?;
    let endpoint = endpoint_for(&p)?;
    let token = SecretToken::from(Zeroizing::new(token.trim().to_owned()));
    let info = validate_token(&rt.http, &endpoint, &token).await?;
    save_login(
        st,
        rt,
        &p,
        &endpoint,
        &info,
        Login {
            method: AuthMethod::Pat,
            access: &token,
            refresh: None,
            expires_at: info.expires_at,
            scopes: info.scopes.clone(),
            label: label.map(|l| l.trim().to_owned()).filter(|l| !l.is_empty()),
        },
    )
    .await
}

pub async fn update_token(
    st: &AppState,
    rt: &AuthRuntime,
    account_id: &str,
    token: Zeroizing<String>,
) -> AuthResult<Account> {
    let acc = account(st, account_id)?;
    let p = provider(st, &acc.provider_id)?;
    let endpoint = endpoint_for(&p)?;
    let token = SecretToken::from(Zeroizing::new(token.trim().to_owned()));
    let info = validate_token(&rt.http, &endpoint, &token).await?;
    if !info.user.username.eq_ignore_ascii_case(&acc.username) {
        return Err(AuthCommandError::WrongAccount {
            message: format!(
                "this token belongs to `{}`, not `{}`",
                info.user.username, acc.username
            ),
            expected: acc.username,
            actual: info.user.username,
        });
    }
    save_login(
        st,
        rt,
        &p,
        &endpoint,
        &info,
        Login {
            method: AuthMethod::Pat,
            access: &token,
            refresh: None,
            expires_at: info.expires_at,
            scopes: info.scopes.clone(),
            label: None,
        },
    )
    .await
}

/// Saves a token copied from the gh / glab CLI (PLAN 3B.1) that the caller already validated
/// with [`validate_token`] against `provider` (and checked the user name of). Same upsert as a
/// PAT, recorded as [`AuthMethod::ImportedCli`].
pub async fn save_imported(
    st: &AppState,
    rt: &AuthRuntime,
    provider: &Provider,
    info: &TokenInfo,
    token: &SecretToken,
) -> AuthResult<Account> {
    let endpoint = endpoint_for(provider)?;
    save_login(
        st,
        rt,
        provider,
        &endpoint,
        info,
        Login {
            method: AuthMethod::ImportedCli,
            access: token,
            refresh: None,
            expires_at: info.expires_at,
            scopes: info.scopes.clone(),
            label: None,
        },
    )
    .await
}

struct Login<'a> {
    method: AuthMethod,
    access: &'a SecretToken,
    refresh: Option<&'a SecretToken>,
    /// Unix seconds.
    expires_at: Option<i64>,
    /// `None` = unknown (fine-grained PAT).
    scopes: Option<Vec<String>>,
    /// New label; `None` keeps the current one.
    label: Option<String>,
}

fn find_account(st: &AppState, provider_id: &str, username: &str) -> AuthResult<Option<Account>> {
    Ok(st
        .storage
        .accounts()?
        .into_iter()
        .find(|a| a.provider_id == provider_id && a.username.eq_ignore_ascii_case(username)))
}

fn secret(token: &SecretToken) -> Zeroizing<String> {
    Zeroizing::new(token.expose().to_owned())
}

/// Upserts the account (unique provider + username), stores the token(s), clears the
/// "needs sign-in" state and creates a default identity when the account has none.
async fn save_login(
    st: &AppState,
    rt: &AuthRuntime,
    provider: &Provider,
    endpoint: &ProviderEndpoint,
    info: &TokenInfo,
    login: Login<'_>,
) -> AuthResult<Account> {
    let existing = find_account(st, &provider.id, &info.user.username)?;
    let expires_ms = login.expires_at.map(|s| s * 1000);
    let scopes = login.scopes.clone().unwrap_or_default();
    let input = AccountInput {
        id: existing.as_ref().map(|a| a.id.clone()),
        provider_id: provider.id.clone(),
        username: info.user.username.clone(),
        display_name: info
            .user
            .display_name
            .clone()
            .or_else(|| existing.as_ref().and_then(|a| a.display_name.clone())),
        avatar_url: info
            .user
            .avatar_url
            .clone()
            .or_else(|| existing.as_ref().and_then(|a| a.avatar_url.clone())),
        auth_method: login.method,
        scopes: scopes.clone(),
        token_expires_at: expires_ms,
        color: existing.as_ref().map(|a| a.color.clone()),
        label: login
            .label
            .clone()
            .or_else(|| existing.as_ref().and_then(|a| a.label.clone())),
        default_clone_dir: existing.as_ref().and_then(|a| a.default_clone_dir.clone()),
    };
    let account = account_save_impl(st, input)?;
    let account = account_store_tokens(
        st,
        &account.id,
        secret(login.access),
        login.refresh.map(secret),
        expires_ms,
        scopes,
    )?;
    write_status(
        st,
        &account.id,
        &AuthStatus {
            needs_sign_in: false,
            last_error: None,
            token_kind: Some(info.token_kind),
            scopes_known: Some(login.scopes.is_some()),
            checked_at: Some(now_ms()),
        },
    );
    tracing::info!(account = %account.id, provider = %provider.id, method = login.method.as_str(), "account signed in");
    if let Err(e) =
        ensure_default_identity(st, rt, endpoint, login.access, &account, &info.user).await
    {
        tracing::warn!(account = %account.id, error = %e, "could not create a default identity");
    }
    Ok(account)
}

/// Commit email for a new account: primary verified → verified commit email → any verified →
/// public profile email → the provider's no-reply address.
pub fn pick_identity_email(
    emails: &[ProviderEmail],
    user: &ProviderUser,
    family: ProviderFamily,
    host: &str,
) -> String {
    let verified = |e: &&ProviderEmail| e.verified && !e.email.trim().is_empty();
    emails
        .iter()
        .filter(verified)
        .find(|e| e.primary)
        .or_else(|| emails.iter().filter(verified).find(|e| e.commit_email))
        .or_else(|| emails.iter().find(verified))
        .map(|e| e.email.clone())
        .or_else(|| user.email.clone().filter(|e| e.contains('@')))
        .unwrap_or_else(|| {
            let host = host.split(':').next().unwrap_or(host);
            match family {
                ProviderFamily::Github => {
                    format!("{}+{}@users.noreply.{host}", user.id, user.username)
                }
                ProviderFamily::Gitlab => {
                    format!("{}-{}@users.noreply.{host}", user.id, user.username)
                }
            }
        })
}

async fn ensure_default_identity(
    st: &AppState,
    rt: &AuthRuntime,
    endpoint: &ProviderEndpoint,
    token: &SecretToken,
    account: &Account,
    user: &ProviderUser,
) -> AuthResult<()> {
    if !st.storage.identities(Some(&account.id))?.is_empty() {
        return Ok(());
    }
    let client = ProviderClient::new(rt.http.clone(), endpoint.clone(), token.clone());
    let emails = match client.emails().await {
        Ok(list) => list,
        Err(e) => {
            tracing::info!(account = %account.id, error = %e, "provider emails unavailable");
            Vec::new()
        }
    };
    let email = pick_identity_email(
        &emails,
        user,
        endpoint.kind().family(),
        &endpoint.web_host(),
    );
    let name = user
        .display_name
        .clone()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| user.username.clone());
    identity_save_impl(
        st,
        IdentityInput {
            id: None,
            account_id: account.id.clone(),
            name,
            email,
            signing: SigningMethod::None,
            signing_key_ref: None,
            is_default: true,
        },
    )?;
    Ok(())
}

// ----- health & refresh ----------------------------------------------------------------------

/// The app refreshes this account's token itself.
pub fn auto_refresh(account: &Account, provider: &Provider) -> bool {
    account.auth_method == AuthMethod::Oauth
        && family(provider) == ProviderFamily::Gitlab
        && account.has_secret(AccountSecret::RefreshToken)
}

/// Pure health computation (`now` in Unix seconds).
pub fn compute_health(
    account: &Account,
    provider: &Provider,
    status: &AuthStatus,
    now: i64,
) -> AccountHealth {
    let fam = family(provider);
    let has_token = account.has_secret(AccountSecret::Token);
    let auto = auto_refresh(account, provider);
    let scopes_known = has_token && status.scopes_known.unwrap_or(!account.scopes.is_empty());
    let scopes = scopes_known.then_some(account.scopes.as_slice());
    let expires = if auto || !has_token {
        None
    } else {
        account.token_expires_at.map(|ms| ms.div_euclid(1000))
    };
    let core = token_health(fam, scopes, expires, &CORE_FEATURES, now);
    let optional = token_health(fam, scopes, None, &OPTIONAL_FEATURES, now);
    let needs_sign_in = status.needs_sign_in
        || (account.auth_method != AuthMethod::SshOnly && !has_token)
        || core.expired;
    AccountHealth {
        account_id: account.id.clone(),
        has_token,
        auto_refresh: auto,
        scopes_known,
        missing_scopes: core.missing,
        optional_missing: optional.missing,
        expires_at: account.token_expires_at,
        expired: core.expired,
        expires_soon: core.expires_soon,
        days_left: core.days_left,
        needs_sign_in,
        last_error: status.last_error.clone(),
        token_kind: status.token_kind,
    }
}

pub fn health(st: &AppState, account_id: &str) -> AuthResult<AccountHealth> {
    let acc = account(st, account_id)?;
    let p = provider(st, &acc.provider_id)?;
    Ok(compute_health(
        &acc,
        &p,
        &read_status(st, &acc.id),
        now_unix(),
    ))
}

pub fn health_all(st: &AppState) -> AuthResult<Vec<AccountHealth>> {
    let providers = st.storage.providers()?;
    let now = now_unix();
    Ok(st
        .storage
        .accounts()?
        .iter()
        .filter_map(|a| {
            let p = providers.iter().find(|p| p.id == a.provider_id)?;
            Some(compute_health(a, p, &read_status(st, &a.id), now))
        })
        .collect())
}

/// Exchanges the refresh token and stores the rotated pair. Caller holds `rt.refresh_lock`.
async fn refresh_oauth(
    st: &AppState,
    rt: &AuthRuntime,
    acc: &Account,
    p: &Provider,
) -> AuthResult<()> {
    let id = client_id(p).ok_or_else(|| missing_client_id(p))?;
    let endpoint = endpoint_for(p)?;
    let refresh = account_secret(st, &acc.id, AccountSecret::RefreshToken)?.ok_or_else(|| {
        AuthCommandError::from(AuthError::RefreshRejected {
            message: "no refresh token stored".into(),
        })
    })?;
    let tokens =
        refresh_gitlab_token(&rt.http, &endpoint, &id, &SecretToken::from(refresh)).await?;
    let scopes = if tokens.scopes.is_empty() {
        acc.scopes.clone()
    } else {
        tokens.scopes.clone()
    };
    // GitLab rotates refresh tokens: always persist the returned one (the crate falls back to
    // the old token for servers that do not rotate).
    account_store_tokens(
        st,
        &acc.id,
        secret(&tokens.access_token),
        tokens.refresh_token.as_ref().map(secret),
        tokens.expires_at.map(|s| s * 1000),
        scopes,
    )?;
    let mut status = read_status(st, &acc.id);
    status.needs_sign_in = false;
    status.last_error = None;
    status.checked_at = Some(now_ms());
    write_status(st, &acc.id, &status);
    tracing::info!(account = %acc.id, "OAuth token refreshed");
    Ok(())
}

/// Re-validates a stored token: updates scopes, expiry and profile.
async fn revalidate(
    st: &AppState,
    rt: &AuthRuntime,
    acc: &Account,
    p: &Provider,
) -> AuthResult<()> {
    let endpoint = endpoint_for(p)?;
    let token = account_secret(st, &acc.id, AccountSecret::Token)?.ok_or_else(|| {
        AuthCommandError::from(AuthError::Provider {
            error: ProviderError::Unauthorized {
                message: "no token stored for this account".into(),
            },
        })
    })?;
    let info = validate_token(&rt.http, &endpoint, &SecretToken::from(token)).await?;
    let mut updated = acc.clone();
    updated.scopes = info.scopes.clone().unwrap_or_default();
    // OAuth tokens without expiry info keep what the token endpoint said.
    if info.expires_at.is_some()
        || matches!(acc.auth_method, AuthMethod::Pat | AuthMethod::ImportedCli)
    {
        updated.token_expires_at = info.expires_at.map(|s| s * 1000);
    }
    if info.user.display_name.is_some() {
        updated.display_name = info.user.display_name.clone();
    }
    if info.user.avatar_url.is_some() {
        updated.avatar_url = info.user.avatar_url.clone();
    }
    st.storage.update_account(&updated)?;
    write_status(
        st,
        &acc.id,
        &AuthStatus {
            needs_sign_in: false,
            last_error: None,
            token_kind: Some(info.token_kind),
            scopes_known: Some(info.scopes.is_some()),
            checked_at: Some(now_ms()),
        },
    );
    Ok(())
}

/// `account_refresh`: refresh (GitLab OAuth) or re-validate now. Failures are recorded in the
/// returned health (`lastError`, `needsSignIn`) rather than returned as errors.
pub async fn refresh_account(
    st: &AppState,
    rt: &AuthRuntime,
    account_id: &str,
) -> AuthResult<AccountHealth> {
    let _guard = rt.refresh_lock.lock().await;
    let acc = account(st, account_id)?;
    let p = provider(st, &acc.provider_id)?;
    if acc.auth_method != AuthMethod::SshOnly || acc.has_secret(AccountSecret::Token) {
        let result = if auto_refresh(&acc, &p) {
            refresh_oauth(st, rt, &acc, &p).await
        } else {
            revalidate(st, rt, &acc, &p).await
        };
        if let Err(e) = result {
            record_failure(st, &acc.id, &e);
        }
    }
    health(st, account_id)
}

/// One pass of the background task: refreshes every GitLab OAuth token that expires within
/// [`REFRESH_MARGIN_SECS`]. Returns the number of refreshed accounts.
pub async fn refresh_due(st: &AppState, rt: &AuthRuntime, now: i64) -> usize {
    let (accounts, providers) = match (st.storage.accounts(), st.storage.providers()) {
        (Ok(a), Ok(p)) => (a, p),
        _ => return 0,
    };
    let due = |a: &Account| {
        needs_refresh(
            a.token_expires_at.map(|ms| ms / 1000),
            now,
            REFRESH_MARGIN_SECS,
        )
    };
    let mut refreshed = 0;
    for acc in accounts {
        let Some(p) = providers.iter().find(|p| p.id == acc.provider_id) else {
            continue;
        };
        if !auto_refresh(&acc, p) || !due(&acc) || read_status(st, &acc.id).needs_sign_in {
            continue;
        }
        let _guard = rt.refresh_lock.lock().await;
        // Another refresh may have run while waiting for the lock.
        let Ok(Some(current)) = st.storage.account(&acc.id) else {
            continue;
        };
        if !due(&current) {
            continue;
        }
        match refresh_oauth(st, rt, &current, p).await {
            Ok(()) => refreshed += 1,
            Err(e) => record_failure(st, &current.id, &e),
        }
    }
    refreshed
}

pub async fn emails(
    st: &AppState,
    rt: &AuthRuntime,
    account_id: &str,
) -> AuthResult<Vec<ProviderEmail>> {
    let acc = account(st, account_id)?;
    let p = provider(st, &acc.provider_id)?;
    let endpoint = endpoint_for(&p)?;
    let token = account_secret(st, &acc.id, AccountSecret::Token)?
        .ok_or_else(|| AppError::InvalidInput("this account has no token; sign in first".into()))?;
    let client = ProviderClient::new(rt.http.clone(), endpoint, SecretToken::from(token));
    Ok(client.emails().await?)
}
