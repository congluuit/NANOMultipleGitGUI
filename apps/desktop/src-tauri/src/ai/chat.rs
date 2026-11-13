//! "Hỏi AI" about an error: a short conversation with the service set up in the AI settings.
//!
//! - The UI starts a chat from an error notice with `context` (the error, git's output and
//!   facts about the repository, never a token). The user agreed to send it before the first
//!   question; `context` goes with every question of that chat so the model keeps the facts.
//! - Chats are kept on this computer only, in the settings table (`ai.chats`), newest first,
//!   at most [`MAX_CHATS`]. Settings → AI and the chat window can delete one or all of them.
//! - Text is scrubbed of credentials (`nanomgg_logging::redact`) before it is stored or sent.

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use nanomgg_credentials::SecretStore;
use nanomgg_providers::CancellationToken;
use nanomgg_storage::Storage;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use super::client::{self, Endpoints, Speaker, Target};
use super::config::{self, AiProvider};
use super::{AiError, AiResult, Running, blocking, http, load_config};
use crate::state::AppState;

pub const KEY_AI_CHATS: &str = "ai.chats";
pub const MAX_CHATS: usize = 50;
/// Messages kept per chat; the oldest answers go first (the first question stays).
pub const MAX_MESSAGES: usize = 40;
pub const MAX_MESSAGE_CHARS: usize = 8_000;
pub const MAX_CONTEXT_CHARS: usize = 12_000;
const MAX_TITLE_CHARS: usize = 200;
/// Turns sent back to the model with a new question (the whole chat would grow without end).
const SENT_TURNS: usize = 12;
const CHAT_MAX_TOKENS: u32 = 2048;

/// Serializes the read-modify-write of the stored chats.
static SAVE: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum AiChatRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiChatMessage {
    pub role: AiChatRole,
    pub content: String,
    /// Unix milliseconds.
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiChat {
    pub id: String,
    /// The error the chat is about (the notice's headline).
    pub title: String,
    /// Facts sent with every question: the error, git's output, the repository.
    pub context: String,
    pub repo_name: Option<String>,
    pub provider: AiProvider,
    pub model: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub messages: Vec<AiChatMessage>,
}

/// A new chat, started from an error notice.
#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiChatStart {
    pub title: String,
    pub context: String,
    pub repo_name: Option<String>,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Scrubbed of credentials, trimmed and cut to `max` characters.
fn clean(text: &str, max: usize) -> String {
    let text = nanomgg_logging::redact(text.trim());
    if text.chars().count() > max {
        text.chars().take(max).collect::<String>() + "…"
    } else {
        text.into_owned()
    }
}

pub fn load_chats(storage: &Storage) -> AiResult<Vec<AiChat>> {
    let mut chats: Vec<AiChat> = storage.get_setting(KEY_AI_CHATS)?.unwrap_or_default();
    chats.sort_by_key(|c| std::cmp::Reverse(c.updated_at));
    Ok(chats)
}

fn save_chats(storage: &Storage, mut chats: Vec<AiChat>) -> AiResult<Vec<AiChat>> {
    chats.sort_by_key(|c| std::cmp::Reverse(c.updated_at));
    chats.truncate(MAX_CHATS);
    if chats.is_empty() {
        storage.delete_setting(KEY_AI_CHATS)?;
    } else {
        storage.set_setting(KEY_AI_CHATS, &chats)?;
    }
    Ok(chats)
}

pub fn delete_chat(storage: &Storage, id: &str) -> AiResult<Vec<AiChat>> {
    let _g = SAVE.lock().unwrap_or_else(|e| e.into_inner());
    let mut chats = load_chats(storage)?;
    chats.retain(|c| c.id != id);
    save_chats(storage, chats)
}

pub fn delete_all(storage: &Storage) -> AiResult<Vec<AiChat>> {
    let _g = SAVE.lock().unwrap_or_else(|e| e.into_inner());
    save_chats(storage, Vec::new())
}

/// Drops the oldest messages after the first question so the chat stays under the limit.
fn trim_messages(messages: &mut Vec<AiChatMessage>) {
    if messages.len() > MAX_MESSAGES {
        let extra = messages.len() - MAX_MESSAGES;
        messages.drain(1..1 + extra);
    }
}

/// Rules for the model: plain words for someone new to git, the app's buttons first.
pub fn system_prompt(language: &str, context: &str) -> String {
    format!(
        "You are the help assistant inside NANO Multiple Git GUI, a desktop Git app made for \
         people who are new to Git. The user hit a problem in the app. The facts below come \
         from the app itself.\n\n\
         Answer in {language}. Be short and concrete:\n\
         1. In one or two sentences, say what happened in plain words. Explain any Git word \
         you use.\n\
         2. Give numbered steps to fix it. Prefer the app's own buttons and menus. Give a git \
         command only when the app cannot do it, inside a code block.\n\
         3. Before any step that can lose work (force push, reset --hard, deleting files or \
         branches), warn clearly and say how to stay safe.\n\
         4. The facts may list \"Actions the app can run\" as `[id] what it does`. The user \
         can run them with one click under your answer. When one or more of them fixes the \
         problem, say in your steps what each of those does, and end your answer with one last \
         line of exactly this form: {ACTIONS_PREFIX} id1, id2 (one to three ids from that \
         list, the best first). Never invent an id; leave the line out when none fits.\n\
         Never ask for passwords, tokens or keys, and never tell the user to paste them \
         anywhere. If the facts are not enough, say exactly what to check.\n\n\
         Facts:\n{context}"
    )
}

/// Last line of an answer naming the app actions the model recommends (see [`system_prompt`]).
pub const ACTIONS_PREFIX: &str = "ACTIONS:";

pub fn language_of(app_language: &str) -> &'static str {
    if app_language.to_ascii_lowercase().starts_with("vi") {
        "Vietnamese"
    } else {
        "English"
    }
}

/// Asks the model `message` in chat `chat_id` (or a new chat from `start`) and stores both the
/// question and the answer. Nothing is stored when the service fails, so the question can be
/// sent again.
#[allow(clippy::too_many_arguments)]
pub async fn send_impl(
    http: &reqwest::Client,
    storage: &Storage,
    secrets: &dyn SecretStore,
    endpoints: &Endpoints,
    chat_id: Option<String>,
    start: Option<AiChatStart>,
    message: &str,
    app_language: &str,
    cancel: &CancellationToken,
) -> AiResult<AiChat> {
    let cfg = load_config(storage)?;
    if !cfg.enabled {
        return Err(AiError::NotEnabled {
            message: "AI is turned off".into(),
        });
    }
    let question = clean(message, MAX_MESSAGE_CHARS);
    if question.is_empty() {
        return Err(AiError::invalid("empty question"));
    }
    let now = now_ms();
    let mut chat = match (&chat_id, start) {
        (Some(id), _) => load_chats(storage)?
            .into_iter()
            .find(|c| &c.id == id)
            .ok_or_else(|| AiError::invalid("this chat was deleted"))?,
        (None, Some(start)) => AiChat {
            id: nanomgg_accounts::new_id("chat"),
            title: clean(&start.title, MAX_TITLE_CHARS),
            context: clean(&start.context, MAX_CONTEXT_CHARS),
            repo_name: start.repo_name.map(|n| clean(&n, MAX_TITLE_CHARS)),
            provider: cfg.provider,
            model: String::new(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        },
        (None, None) => return Err(AiError::invalid("no chat to continue")),
    };
    let key = config::read_key(secrets, cfg.provider)?;
    let target = Target::new(&cfg, key, endpoints, true)?;
    let system = system_prompt(language_of(app_language), &chat.context);
    let history = chat.messages.len().saturating_sub(SENT_TURNS);
    let mut turns: Vec<(Speaker, &str)> = chat.messages[history..]
        .iter()
        .map(|m| {
            let who = match m.role {
                AiChatRole::User => Speaker::User,
                AiChatRole::Assistant => Speaker::Assistant,
            };
            (who, m.content.as_str())
        })
        .collect();
    // Some services refuse a conversation that starts with the assistant.
    while turns.first().is_some_and(|t| t.0 == Speaker::Assistant) {
        turns.remove(0);
    }
    turns.push((Speaker::User, question.as_str()));
    tracing::info!(
        provider = ?target.provider,
        model = %target.model,
        turns = turns.len(),
        "ai help question"
    );
    let answer = client::converse(http, &target, &system, &turns, CHAT_MAX_TOKENS, cancel).await?;
    let answer = clean(&answer, MAX_MESSAGE_CHARS * 2);

    let _g = SAVE.lock().unwrap_or_else(|e| e.into_inner());
    let mut chats = load_chats(storage)?;
    if let Some(stored) = chats.iter().find(|c| c.id == chat.id) {
        // Keep what another window may have added meanwhile.
        chat.messages = stored.messages.clone();
    } else if chat_id.is_some() {
        return Err(AiError::invalid("this chat was deleted"));
    }
    let done = now_ms();
    chat.messages.push(AiChatMessage {
        role: AiChatRole::User,
        content: question,
        at: now,
    });
    chat.messages.push(AiChatMessage {
        role: AiChatRole::Assistant,
        content: answer,
        at: done,
    });
    trim_messages(&mut chat.messages);
    chat.provider = target.provider;
    chat.model = target.model;
    chat.updated_at = done;
    chats.retain(|c| c.id != chat.id);
    chats.push(chat.clone());
    save_chats(storage, chats)?;
    Ok(chat)
}

// ----- commands ------------------------------------------------------------------------------

/// Stored chats, newest first.
#[tauri::command]
#[specta::specta]
pub async fn ai_chat_list(app: AppHandle) -> AiResult<Vec<AiChat>> {
    blocking(app, |st| load_chats(&st.storage)).await
}

/// Asks a question in chat `chatId`, or in a new chat from `start`. Cancel with
/// `ai_cancel(requestId)`.
#[tauri::command]
#[specta::specta]
pub async fn ai_chat_send(
    app: AppHandle,
    request_id: String,
    chat_id: Option<String>,
    start: Option<AiChatStart>,
    message: String,
    app_language: String,
) -> AiResult<AiChat> {
    let (_running, cancel) = Running::start(&request_id)?;
    let st = app.state::<AppState>();
    send_impl(
        &http(&app),
        &st.storage,
        st.accounts.secrets.as_ref(),
        &Endpoints::default(),
        chat_id,
        start,
        &message,
        &app_language,
        &cancel,
    )
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn ai_chat_delete(app: AppHandle, id: String) -> AiResult<Vec<AiChat>> {
    blocking(app, move |st| delete_chat(&st.storage, &id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn ai_chat_delete_all(app: AppHandle) -> AiResult<Vec<AiChat>> {
    blocking(app, |st| delete_all(&st.storage)).await
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nanomgg_credentials::MemorySecretStore;
    use nanomgg_providers::{HttpConfig, build_http_client};
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use zeroize::Zeroizing;

    use super::*;
    use crate::ai::client::messages_request;
    use crate::ai::config::AiConfig;
    use crate::ai::save_config;

    fn ollama_storage(server: &MockServer) -> Storage {
        let storage = Storage::open_in_memory().unwrap();
        let secrets = MemorySecretStore::default();
        save_config(
            &storage,
            &secrets,
            AiConfig {
                enabled: true,
                provider: AiProvider::Ollama,
                ollama_url: server.uri(),
                ..AiConfig::default()
            },
        )
        .unwrap();
        storage
    }

    fn start() -> AiChatStart {
        AiChatStart {
            title: "Push failed".into(),
            context: "error: failed to push to https://x:ghp_secret1234567890abcdef@github.com/o/r"
                .into(),
            repo_name: Some("backend-api".into()),
        }
    }

    async fn send(
        storage: &Storage,
        chat_id: Option<String>,
        start: Option<AiChatStart>,
        message: &str,
    ) -> AiResult<AiChat> {
        send_impl(
            &build_http_client(&HttpConfig::default()).unwrap(),
            storage,
            &MemorySecretStore::default(),
            &Endpoints::default(),
            chat_id,
            start,
            message,
            "vi",
            &CancellationToken::new(),
        )
        .await
    }

    #[test]
    fn every_service_gets_the_turns_in_order_with_its_own_roles() {
        let turns = [
            (Speaker::User, "Q1"),
            (Speaker::Assistant, "A1"),
            (Speaker::User, "Q2"),
        ];
        let target = |provider| Target {
            provider,
            base_url: "https://x".into(),
            model: "m".into(),
            key: Some(Zeroizing::new("k".into())),
            timeout: Duration::from_secs(5),
        };
        let (_, b) = messages_request(&target(AiProvider::Openai), "S", &turns, false, 10);
        assert_eq!(b["messages"][0]["role"], "system");
        assert_eq!(b["messages"][2]["role"], "assistant");
        assert_eq!(b["messages"][3]["content"], "Q2");
        let (_, b) = messages_request(&target(AiProvider::Anthropic), "S", &turns, false, 10);
        assert_eq!(b["system"], "S");
        assert_eq!(b["messages"].as_array().unwrap().len(), 3);
        let (_, b) = messages_request(&target(AiProvider::Gemini), "S", &turns, false, 10);
        assert_eq!(b["contents"][1]["role"], "model");
        let (_, b) = messages_request(&target(AiProvider::Ollama), "S", &turns, false, 10);
        assert!(b.get("format").is_none(), "a chat answer is plain text");
        let (_, b) = messages_request(&target(AiProvider::Ollama), "S", &turns, true, 10);
        assert_eq!(b["format"], "json");
    }

    #[tokio::test]
    async fn a_chat_is_stored_continued_and_deleted() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "message": { "role": "assistant", "content": "Bấm Pull rồi Push." }
            })))
            .mount(&server)
            .await;
        let storage = ollama_storage(&server);

        let chat = send(&storage, None, Some(start()), "Lỗi này là gì?")
            .await
            .unwrap();
        assert_eq!(chat.messages.len(), 2);
        assert_eq!(chat.messages[1].content, "Bấm Pull rồi Push.");
        assert!(
            !chat.context.contains("ghp_secret"),
            "tokens never reach the model or the disk: {}",
            chat.context
        );
        let first = server.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&first[0].body).unwrap();
        assert!(
            body["messages"][0]["content"]
                .as_str()
                .unwrap()
                .contains("Vietnamese")
        );

        // The second question carries the first exchange.
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .and(body_partial_json(json!({
                "messages": [{ "role": "system" }, { "content": "Lỗi này là gì?" }]
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "message": { "content": "Đúng vậy." }
            })))
            .mount(&server)
            .await;
        let chat = send(&storage, Some(chat.id.clone()), None, "Chắc không?")
            .await
            .unwrap();
        assert_eq!(chat.messages.len(), 4);
        assert_eq!(load_chats(&storage).unwrap().len(), 1);

        assert!(delete_chat(&storage, &chat.id).unwrap().is_empty());
        let gone = send(&storage, Some(chat.id), None, "?").await.unwrap_err();
        assert!(matches!(gone, AiError::InvalidInput { .. }));
    }

    #[tokio::test]
    async fn nothing_is_stored_when_the_service_fails_or_ai_is_off() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;
        let storage = ollama_storage(&server);
        assert!(send(&storage, None, Some(start()), "?").await.is_err());
        assert!(load_chats(&storage).unwrap().is_empty());

        let off = Storage::open_in_memory().unwrap();
        let e = send(&off, None, Some(start()), "?").await.unwrap_err();
        assert!(matches!(e, AiError::NotEnabled { .. }));
    }

    #[test]
    fn long_chats_keep_the_first_question_and_the_latest_messages() {
        let mut messages: Vec<AiChatMessage> = (0..MAX_MESSAGES + 4)
            .map(|i| AiChatMessage {
                role: if i % 2 == 0 {
                    AiChatRole::User
                } else {
                    AiChatRole::Assistant
                },
                content: i.to_string(),
                at: i as i64,
            })
            .collect();
        trim_messages(&mut messages);
        assert_eq!(messages.len(), MAX_MESSAGES);
        assert_eq!(messages[0].content, "0");
        assert_eq!(messages[1].content, "5");
        assert_eq!(
            messages.last().unwrap().content,
            (MAX_MESSAGES + 3).to_string()
        );
    }

    #[test]
    fn delete_all_empties_the_history() {
        let storage = Storage::open_in_memory().unwrap();
        let chat = AiChat {
            id: "chat-1".into(),
            title: "t".into(),
            context: "c".into(),
            repo_name: None,
            provider: AiProvider::Ollama,
            model: "m".into(),
            created_at: 1,
            updated_at: 1,
            messages: vec![],
        };
        save_chats(&storage, vec![chat]).unwrap();
        assert_eq!(load_chats(&storage).unwrap().len(), 1);
        assert!(delete_all(&storage).unwrap().is_empty());
        assert!(load_chats(&storage).unwrap().is_empty());
    }
}
