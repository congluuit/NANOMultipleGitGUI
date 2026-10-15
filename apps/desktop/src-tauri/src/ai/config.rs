//! Settings of the AI commit-message helper (`ai.config`) and the API keys in the keychain.
//!
//! The setting never holds a key: keys live in the keychain under a fixed owner per service
//! (`nanomgg:ai_openai:token`, ...), and the UI only learns whether one is stored.

use nanomgg_credentials::{SecretKey, SecretKind, SecretStore};
use serde::{Deserialize, Serialize};
use specta::Type;
use zeroize::Zeroizing;

use super::AiError;

/// Setting: [`AiConfig`].
pub const KEY_AI_CONFIG: &str = "ai.config";

pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com";
pub const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com";

/// Where the message is written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiProvider {
    /// Ollama on this computer: free, the code never leaves the machine.
    #[default]
    Ollama,
    Openai,
    Anthropic,
    Gemini,
    /// Any OpenAI-compatible endpoint (OpenRouter, Groq, LM Studio, vLLM...).
    Compatible,
}

impl AiProvider {
    /// Keychain owner of the API key (fixed list, also read by the backup feature).
    pub fn key_owner(self) -> Option<&'static str> {
        match self {
            Self::Ollama => None,
            Self::Openai => Some("ai_openai"),
            Self::Anthropic => Some("ai_anthropic"),
            Self::Gemini => Some("ai_gemini"),
            Self::Compatible => Some("ai_compatible"),
        }
    }

    pub fn secret_key(self) -> Option<SecretKey> {
        self.key_owner()
            .map(|owner| SecretKey::new(owner, SecretKind::Token))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiMessageLanguage {
    /// Same language as the app.
    #[default]
    App,
    En,
    Vi,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiMessageStyle {
    /// `feat: add login page`.
    #[default]
    Conventional,
    /// `Add login page`.
    Plain,
}

/// Chosen model of each service (kept when switching service).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AiModels {
    pub ollama: String,
    pub openai: String,
    pub anthropic: String,
    pub gemini: String,
    pub compatible: String,
}

impl Default for AiModels {
    fn default() -> Self {
        Self {
            ollama: "qwen2.5-coder:1.5b".into(),
            openai: "gpt-4.1-mini".into(),
            anthropic: "claude-haiku-4-5".into(),
            gemini: "gemini-2.5-flash-lite".into(),
            compatible: String::new(),
        }
    }
}

impl AiModels {
    pub fn get(&self, provider: AiProvider) -> &str {
        match provider {
            AiProvider::Ollama => &self.ollama,
            AiProvider::Openai => &self.openai,
            AiProvider::Anthropic => &self.anthropic,
            AiProvider::Gemini => &self.gemini,
            AiProvider::Compatible => &self.compatible,
        }
    }
}

/// Stored under [`KEY_AI_CONFIG`]. Off by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AiConfig {
    pub enabled: bool,
    pub provider: AiProvider,
    pub models: AiModels,
    /// Base URL of the OpenAI-compatible service, e.g. `https://openrouter.ai/api/v1`.
    pub compatible_base_url: String,
    pub ollama_url: String,
    pub language: AiMessageLanguage,
    pub style: AiMessageStyle,
    /// Also write a body under the summary line.
    pub include_body: bool,
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: AiProvider::default(),
            models: AiModels::default(),
            compatible_base_url: String::new(),
            ollama_url: DEFAULT_OLLAMA_URL.into(),
            language: AiMessageLanguage::default(),
            style: AiMessageStyle::default(),
            include_body: true,
        }
    }
}

impl AiConfig {
    /// Trims the fields and checks the URLs; empty URLs fall back to the defaults.
    pub fn normalized(mut self) -> Result<Self, AiError> {
        for m in [
            &mut self.models.ollama,
            &mut self.models.openai,
            &mut self.models.anthropic,
            &mut self.models.gemini,
            &mut self.models.compatible,
        ] {
            *m = m.trim().to_owned();
            if m.len() > 200 || m.chars().any(char::is_control) {
                return Err(AiError::invalid("model name is not valid"));
            }
        }
        self.compatible_base_url = normalize_base_url(&self.compatible_base_url)?;
        self.ollama_url = match normalize_base_url(&self.ollama_url)? {
            u if u.is_empty() => DEFAULT_OLLAMA_URL.into(),
            u => u,
        };
        Ok(self)
    }

    pub fn model(&self) -> &str {
        self.models.get(self.provider)
    }
}

/// `http(s)://host[:port][/path]` without a trailing slash; a pasted `/chat/completions`
/// endpoint is cut back to its base. Empty stays empty.
pub fn normalize_base_url(raw: &str) -> Result<String, AiError> {
    let mut s = raw.trim().trim_end_matches('/').to_owned();
    if s.is_empty() {
        return Ok(s);
    }
    for suffix in ["/chat/completions", "/completions", "/models"] {
        if let Some(base) = s.strip_suffix(suffix) {
            s = base.trim_end_matches('/').to_owned();
        }
    }
    let url = url_parse(&s).ok_or_else(|| {
        AiError::invalid("the address must start with http:// or https:// (for example https://openrouter.ai/api/v1)")
    })?;
    if url.query.is_some() || url.has_credentials {
        return Err(AiError::invalid(
            "the address must not contain a query or a password",
        ));
    }
    Ok(s)
}

/// Minimal URL facts (the app has no `url` dependency of its own).
pub(crate) struct UrlFacts {
    pub host: String,
    pub query: Option<String>,
    pub has_credentials: bool,
    pub https: bool,
}

pub(crate) fn url_parse(s: &str) -> Option<UrlFacts> {
    let (https, rest) = if let Some(r) = s.strip_prefix("https://") {
        (true, r)
    } else {
        (false, s.strip_prefix("http://")?)
    };
    let (rest, query) = match rest.split_once('?') {
        Some((a, q)) => (a, Some(q.to_owned())),
        None => (rest, None),
    };
    let authority = rest.split('/').next().unwrap_or_default();
    let has_credentials = authority.contains('@');
    let host_port = authority.rsplit('@').next().unwrap_or_default();
    let host = if let Some(v6) = host_port.strip_prefix('[') {
        v6.split(']').next().unwrap_or_default().to_owned()
    } else {
        host_port.split(':').next().unwrap_or_default().to_owned()
    };
    if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    Some(UrlFacts {
        host: host.to_ascii_lowercase(),
        query,
        has_credentials,
        https,
    })
}

/// The URL points at this computer (no cloud warning, longer timeout).
pub fn is_local_url(s: &str) -> bool {
    url_parse(s).is_some_and(|u| {
        u.host == "localhost"
            || u.host == "::1"
            || u.host.starts_with("127.")
            || u.host == "0.0.0.0"
            || u.host.ends_with(".localhost")
    })
}

/// Whether the API key of each service is stored (values never leave the backend).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiKeyStatus {
    pub openai: bool,
    pub anthropic: bool,
    pub gemini: bool,
    pub compatible: bool,
}

pub fn key_status(secrets: &dyn SecretStore) -> AiKeyStatus {
    let has = |p: AiProvider| {
        p.secret_key()
            .is_some_and(|k| secrets.get(&k).ok().flatten().is_some())
    };
    AiKeyStatus {
        openai: has(AiProvider::Openai),
        anthropic: has(AiProvider::Anthropic),
        gemini: has(AiProvider::Gemini),
        compatible: has(AiProvider::Compatible),
    }
}

pub fn read_key(
    secrets: &dyn SecretStore,
    provider: AiProvider,
) -> Result<Option<Zeroizing<String>>, AiError> {
    match provider.secret_key() {
        None => Ok(None),
        Some(k) => secrets
            .get(&k)
            .map_err(|e| AiError::internal(format!("keychain: {e}"))),
    }
}

pub fn store_key(
    secrets: &dyn SecretStore,
    provider: AiProvider,
    key: &str,
) -> Result<(), AiError> {
    let k = provider
        .secret_key()
        .ok_or_else(|| AiError::invalid("this service does not use an API key"))?;
    let key = key.trim();
    if key.is_empty() {
        return Err(AiError::invalid("the API key is empty"));
    }
    if key.len() > 4096 || key.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(AiError::invalid(
            "the API key contains spaces or invalid characters",
        ));
    }
    secrets
        .set(&k, key)
        .map_err(|e| AiError::internal(format!("keychain: {e}")))
}

pub fn delete_key(secrets: &dyn SecretStore, provider: AiProvider) -> Result<bool, AiError> {
    match provider.secret_key() {
        None => Ok(false),
        Some(k) => secrets
            .delete(&k)
            .map_err(|e| AiError::internal(format!("keychain: {e}"))),
    }
}
