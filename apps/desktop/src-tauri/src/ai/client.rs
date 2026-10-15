//! Calls to the four API shapes: OpenAI Chat Completions (also OpenAI-compatible services),
//! Anthropic Messages, Gemini `generateContent` and Ollama `/api/chat`. Keys go in headers only
//! (never in a URL), error bodies are shortened and scrubbed of the key before reaching the UI.

use std::future::Future;
use std::time::Duration;

use nanomgg_providers::CancellationToken;
use serde_json::{Value, json};
use zeroize::Zeroizing;

use super::AiError;
use super::config::{
    ANTHROPIC_BASE_URL, AiConfig, AiProvider, GEMINI_BASE_URL, OPENAI_BASE_URL, is_local_url,
    url_parse,
};
use super::prompt::Prompt;

pub const CLOUD_TIMEOUT: Duration = Duration::from_secs(60);
/// A local model may need to load from disk first.
pub const LOCAL_TIMEOUT: Duration = Duration::from_secs(120);
const LIST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_TOKENS: u32 = 1024;
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Base URLs of the cloud services (tests point them at a mock server).
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub openai: String,
    pub anthropic: String,
    pub gemini: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            openai: OPENAI_BASE_URL.into(),
            anthropic: ANTHROPIC_BASE_URL.into(),
            gemini: GEMINI_BASE_URL.into(),
        }
    }
}

/// One resolved service: where, which model, with which key.
pub struct Target {
    pub provider: AiProvider,
    pub base_url: String,
    pub model: String,
    pub key: Option<Zeroizing<String>>,
    pub timeout: Duration,
}

impl Target {
    /// `need_model = false` for the connection test (the model list is fetched instead).
    pub fn new(
        cfg: &AiConfig,
        key: Option<Zeroizing<String>>,
        endpoints: &Endpoints,
        need_model: bool,
    ) -> Result<Self, AiError> {
        let provider = cfg.provider;
        let base_url = match provider {
            AiProvider::Ollama => cfg.ollama_url.clone(),
            AiProvider::Openai => endpoints.openai.clone(),
            AiProvider::Anthropic => endpoints.anthropic.clone(),
            AiProvider::Gemini => endpoints.gemini.clone(),
            AiProvider::Compatible => cfg.compatible_base_url.clone(),
        };
        if base_url.trim().is_empty() {
            return Err(AiError::MissingBaseUrl {
                message: "no address set for the OpenAI-compatible service".into(),
            });
        }
        let base_url = base_url.trim_end_matches('/').to_owned();
        let model = cfg.model().trim().to_owned();
        if need_model && model.is_empty() {
            return Err(AiError::MissingModel {
                message: "no model chosen".into(),
            });
        }
        let key = key.filter(|k| !k.trim().is_empty());
        let local = is_local_url(&base_url);
        match provider {
            AiProvider::Openai | AiProvider::Anthropic | AiProvider::Gemini if key.is_none() => {
                return Err(AiError::MissingKey {
                    message: "no API key stored for this service".into(),
                });
            }
            _ => {}
        }
        // A key must not travel unencrypted to another machine.
        if key.is_some() && !local && url_parse(&base_url).is_some_and(|u| !u.https) {
            return Err(AiError::invalid(
                "use an https:// address: the API key would be sent unencrypted",
            ));
        }
        let timeout = if provider == AiProvider::Ollama || local {
            LOCAL_TIMEOUT
        } else {
            CLOUD_TIMEOUT
        };
        Ok(Self {
            provider,
            base_url,
            model,
            key,
            timeout,
        })
    }

    fn key(&self) -> &str {
        self.key.as_ref().map_or("", |k| k.as_str())
    }

    fn request(
        &self,
        http: &reqwest::Client,
        method: reqwest::Method,
        url: &str,
    ) -> reqwest::RequestBuilder {
        let mut req = http
            .request(method, url)
            .header(reqwest::header::ACCEPT, "application/json");
        match self.provider {
            AiProvider::Openai | AiProvider::Compatible => {
                if self.key.is_some() {
                    req = req.bearer_auth(self.key());
                }
            }
            AiProvider::Anthropic => {
                req = req
                    .header("x-api-key", self.key())
                    .header("anthropic-version", ANTHROPIC_VERSION);
            }
            AiProvider::Gemini => req = req.header("x-goog-api-key", self.key()),
            AiProvider::Ollama => {}
        }
        req
    }

    fn gemini_model(&self) -> &str {
        self.model.strip_prefix("models/").unwrap_or(&self.model)
    }
}

/// Races `fut` against `cancel`.
pub async fn with_cancel<T>(
    cancel: &CancellationToken,
    fut: impl Future<Output = Result<T, AiError>>,
) -> Result<T, AiError> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(AiError::Cancelled { message: "cancelled".into() }),
        r = fut => r,
    }
}

/// Error message of a provider's error body, shortened and without the key.
pub fn error_detail(body: &str, key: Option<&str>) -> String {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let msg = parsed
        .as_ref()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("error").filter(|e| e.is_string()))
                .or_else(|| v.get("message"))
                .or_else(|| v.pointer("/0/error/message"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| body.trim().to_owned());
    let mut msg = match key {
        Some(k) if k.len() >= 4 => msg.replace(k, "***"),
        _ => msg,
    };
    msg = nanomgg_logging::redact(&msg).into_owned();
    if msg.chars().count() > 300 {
        msg = msg.chars().take(300).collect::<String>() + "…";
    }
    msg
}

/// Turns an HTTP error response into something the UI can explain.
pub fn classify_status(status: u16, body: &str, target: &Target) -> AiError {
    let message = error_detail(body, target.key.as_deref().map(|k| k.as_str()));
    let lower = body.to_ascii_lowercase();
    let key_invalid = lower.contains("api_key_invalid")
        || lower.contains("api key not valid")
        || lower.contains("invalid api key")
        || lower.contains("invalid x-api-key")
        || lower.contains("incorrect api key");
    match status {
        401 => AiError::InvalidKey { message },
        400 if key_invalid => AiError::InvalidKey { message },
        403 if key_invalid => AiError::InvalidKey { message },
        403 => AiError::AccessDenied { message },
        402 => AiError::QuotaExceeded { message },
        429 if lower.contains("insufficient_quota")
            || lower.contains("credit balance")
            || lower.contains("billing") =>
        {
            AiError::QuotaExceeded { message }
        }
        429 | 529 => AiError::RateLimited { message },
        404 => AiError::ModelNotFound {
            message,
            model: target.model.clone(),
        },
        // Ollama answers 400/500 "model 'x' not found, try pulling it first".
        _ if lower.contains("not found") && lower.contains("model") => AiError::ModelNotFound {
            message,
            model: target.model.clone(),
        },
        _ => AiError::Service { message, status },
    }
}

/// Network-level failure; Ollama refusing the connection means it is not running.
pub fn classify_reqwest(e: reqwest::Error, target: &Target) -> AiError {
    if e.is_timeout() {
        return AiError::Timeout {
            message: format!("no answer within {} seconds", target.timeout.as_secs()),
        };
    }
    let connect = e.is_connect();
    // The URL may be a user-typed base URL: leave it out, keep the root cause.
    let e = e.without_url();
    let mut message = e.to_string();
    let mut source = std::error::Error::source(&e);
    let mut deepest = None;
    while let Some(s) = source {
        deepest = Some(s.to_string());
        source = s.source();
    }
    if let Some(d) = deepest {
        message = format!("{message}: {d}");
    }
    let message = nanomgg_logging::redact(&message).into_owned();
    if target.provider == AiProvider::Ollama && connect {
        return AiError::OllamaNotRunning { message };
    }
    AiError::Network { message }
}

async fn send_json(
    http: &reqwest::Client,
    target: &Target,
    method: reqwest::Method,
    url: &str,
    body: Option<&Value>,
    timeout: Duration,
) -> Result<Value, AiError> {
    let mut req = target.request(http, method, url).timeout(timeout);
    if let Some(body) = body {
        req = req
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(body).map_err(|e| AiError::internal(e.to_string()))?);
    }
    let resp = req.send().await.map_err(|e| classify_reqwest(e, target))?;
    let status = resp.status().as_u16();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| classify_reqwest(e, target))?;
    if !(200..300).contains(&status) {
        return Err(classify_status(
            status,
            &String::from_utf8_lossy(&bytes),
            target,
        ));
    }
    serde_json::from_slice(&bytes).map_err(|_| AiError::Service {
        message: "the service sent an answer the app cannot read".into(),
        status,
    })
}

fn texts(v: Option<&Value>) -> String {
    v.and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter(|p| p.get("thought").and_then(Value::as_bool) != Some(true))
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// Request body + URL of a chat completion for `target`.
pub fn completion_request(target: &Target, prompt: &Prompt) -> (String, Value) {
    let base = &target.base_url;
    match target.provider {
        AiProvider::Openai | AiProvider::Compatible => {
            let mut body = json!({
                "model": target.model,
                "messages": [
                    { "role": "system", "content": prompt.system },
                    { "role": "user", "content": prompt.user },
                ],
            });
            if target.provider == AiProvider::Openai {
                // Newer OpenAI models only accept `max_completion_tokens` and the default
                // temperature.
                body["max_completion_tokens"] = json!(MAX_TOKENS);
            } else {
                body["max_tokens"] = json!(MAX_TOKENS);
                body["temperature"] = json!(0.2);
            }
            (format!("{base}/chat/completions"), body)
        }
        AiProvider::Anthropic => (
            format!("{base}/v1/messages"),
            json!({
                "model": target.model,
                "max_tokens": MAX_TOKENS,
                "temperature": 0.2,
                "system": prompt.system,
                "messages": [{ "role": "user", "content": prompt.user }],
            }),
        ),
        AiProvider::Gemini => (
            format!(
                "{base}/v1beta/models/{}:generateContent",
                target.gemini_model()
            ),
            json!({
                "systemInstruction": { "parts": [{ "text": prompt.system }] },
                "contents": [{ "role": "user", "parts": [{ "text": prompt.user }] }],
                "generationConfig": { "temperature": 0.2, "maxOutputTokens": 2 * MAX_TOKENS },
            }),
        ),
        AiProvider::Ollama => (
            format!("{base}/api/chat"),
            json!({
                "model": target.model,
                "stream": false,
                "format": "json",
                "messages": [
                    { "role": "system", "content": prompt.system },
                    { "role": "user", "content": prompt.user },
                ],
                // The default context (2–4k tokens) would cut the diff.
                "options": { "temperature": 0.2, "num_ctx": 8192 },
            }),
        ),
    }
}

/// Text of a completion answer.
pub fn completion_text(provider: AiProvider, v: &Value) -> Result<String, AiError> {
    let text = match provider {
        AiProvider::Openai | AiProvider::Compatible => {
            let content = v.pointer("/choices/0/message/content");
            match content {
                Some(Value::String(s)) => s.clone(),
                other => texts(other),
            }
        }
        AiProvider::Anthropic => texts(v.get("content")),
        AiProvider::Gemini => {
            if v.get("candidates")
                .and_then(Value::as_array)
                .is_none_or(Vec::is_empty)
            {
                let reason = v
                    .pointer("/promptFeedback/blockReason")
                    .and_then(Value::as_str)
                    .unwrap_or("no answer");
                return Err(AiError::EmptyAnswer {
                    message: format!("Gemini returned no answer ({reason})"),
                });
            }
            texts(v.pointer("/candidates/0/content/parts"))
        }
        AiProvider::Ollama => v
            .pointer("/message/content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
    };
    if text.trim().is_empty() {
        return Err(AiError::EmptyAnswer {
            message: "the model returned an empty answer".into(),
        });
    }
    Ok(text)
}

/// Sends `prompt` and returns the raw answer text.
pub async fn complete(
    http: &reqwest::Client,
    target: &Target,
    prompt: &Prompt,
    cancel: &CancellationToken,
) -> Result<String, AiError> {
    let (url, body) = completion_request(target, prompt);
    with_cancel(cancel, async {
        let v = send_json(
            http,
            target,
            reqwest::Method::POST,
            &url,
            Some(&body),
            target.timeout,
        )
        .await?;
        completion_text(target.provider, &v)
    })
    .await
}

/// Models offered by the service (also proves the key works).
pub async fn list_models(
    http: &reqwest::Client,
    target: &Target,
    cancel: &CancellationToken,
) -> Result<Vec<String>, AiError> {
    let base = &target.base_url;
    let url = match target.provider {
        AiProvider::Openai | AiProvider::Compatible => format!("{base}/models"),
        AiProvider::Anthropic => format!("{base}/v1/models?limit=100"),
        AiProvider::Gemini => format!("{base}/v1beta/models?pageSize=200"),
        AiProvider::Ollama => format!("{base}/api/tags"),
    };
    let timeout = LIST_TIMEOUT.min(target.timeout);
    let v = with_cancel(
        cancel,
        send_json(http, target, reqwest::Method::GET, &url, None, timeout),
    )
    .await?;
    let ids = |arr: Option<&Value>, field: &str| -> Vec<String> {
        arr.and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|m| m.get(field).and_then(Value::as_str))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut models = match target.provider {
        AiProvider::Openai | AiProvider::Compatible | AiProvider::Anthropic => {
            ids(v.get("data"), "id")
        }
        AiProvider::Gemini => v
            .get("models")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter(|m| {
                        m.get("supportedGenerationMethods")
                            .and_then(Value::as_array)
                            .is_none_or(|g| g.iter().any(|x| x == "generateContent"))
                    })
                    .filter_map(|m| m.get("name").and_then(Value::as_str))
                    .map(|n| n.strip_prefix("models/").unwrap_or(n).to_owned())
                    .collect()
            })
            .unwrap_or_default(),
        AiProvider::Ollama => ids(v.get("models"), "name"),
    };
    models.sort();
    models.dedup();
    models.truncate(500);
    Ok(models)
}

/// `model` is offered (aliases like `claude-haiku-4-5` match dated ids, `llama3.2` matches
/// `llama3.2:latest`).
pub fn model_listed(models: &[String], model: &str) -> bool {
    let model = model.strip_prefix("models/").unwrap_or(model);
    models.iter().any(|m| {
        m == model
            || m.strip_prefix(model)
                .is_some_and(|rest| rest.starts_with('-') || rest == ":latest")
    })
}
