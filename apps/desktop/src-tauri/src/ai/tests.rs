//! AI helper tests: no real API, no real key, no real keychain (`MemorySecretStore`), wiremock
//! for the four API shapes and Ollama, temporary git repositories for the diff.

use std::path::Path;
use std::time::Duration;

use nanomgg_credentials::{MemorySecretStore, SecretKey, SecretKind, SecretStore};
use nanomgg_git_engine::{AccountContext, GitRunner};
use nanomgg_providers::{CancellationToken, HttpConfig, build_http_client};
use nanomgg_storage::Storage;
use serde_json::{Value, json};
use wiremock::matchers::{body_partial_json, header, header_exists, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};
use zeroize::Zeroizing;

use super::client::{self, Endpoints, Target};
use super::config::{self, AiConfig, AiMessageLanguage, AiMessageStyle, AiProvider};
use super::context::{self, AiDiffSource, CommitContext, FileStat, Omit};
use super::ollama;
use super::prompt::{self, Prompt};
use super::*;

const FAKE_KEY: &str = "sk-test-not-a-real-key-123456";

fn http() -> reqwest::Client {
    build_http_client(&HttpConfig::default()).unwrap()
}

fn endpoints(server: &MockServer) -> Endpoints {
    Endpoints {
        openai: format!("{}/v1", server.uri()),
        anthropic: server.uri(),
        gemini: server.uri(),
    }
}

fn cfg(provider: AiProvider) -> AiConfig {
    AiConfig {
        enabled: true,
        provider,
        ..AiConfig::default()
    }
}

fn target(provider: AiProvider, base: &str, key: Option<&str>) -> Target {
    Target {
        provider,
        base_url: base.to_owned(),
        model: "test-model".into(),
        key: key.map(|k| Zeroizing::new(k.to_owned())),
        timeout: Duration::from_secs(5),
    }
}

fn simple_prompt() -> Prompt {
    Prompt {
        system: "SYSTEM RULES".into(),
        user: "USER DIFF".into(),
    }
}

fn answer_json() -> String {
    json!({ "summary": "feat: add login page", "body": "- Add form\n- Validate input" }).to_string()
}

// ----- config / keychain -------------------------------------------------------------------------

#[test]
fn config_defaults_and_normalization() {
    let c = AiConfig::default();
    assert!(!c.enabled, "off by default");
    assert_eq!(c.provider, AiProvider::Ollama);
    assert_eq!(c.model(), "qwen2.5-coder:1.5b");
    assert_eq!(c.ollama_url, config::DEFAULT_OLLAMA_URL);

    let c = AiConfig {
        compatible_base_url: " https://openrouter.ai/api/v1/chat/completions/ ".into(),
        ollama_url: "".into(),
        ..AiConfig::default()
    }
    .normalized()
    .unwrap();
    assert_eq!(c.compatible_base_url, "https://openrouter.ai/api/v1");
    assert_eq!(c.ollama_url, config::DEFAULT_OLLAMA_URL);

    for bad in [
        "ftp://x",
        "openrouter.ai",
        "https://user:pw@host/v1",
        "https://h/v1?k=1",
    ] {
        let r = AiConfig {
            compatible_base_url: bad.into(),
            ..AiConfig::default()
        }
        .normalized();
        assert!(matches!(r, Err(AiError::InvalidInput { .. })), "{bad}");
    }
    assert!(config::is_local_url("http://localhost:1234/v1"));
    assert!(config::is_local_url("http://127.0.0.1:11434"));
    assert!(config::is_local_url("http://[::1]:8000/v1"));
    assert!(!config::is_local_url("https://api.groq.com/openai/v1"));

    // Older / partial JSON still loads.
    let partial: AiConfig = serde_json::from_value(json!({ "enabled": true })).unwrap();
    assert!(partial.enabled && partial.include_body);
}

#[test]
fn keys_live_in_the_keychain_under_fixed_owners() {
    let tmp = tempfile::tempdir().unwrap();
    let storage = Storage::open(&tmp.path().join("app.db")).unwrap();
    let secrets = MemorySecretStore::default();
    assert_eq!(
        [
            AiProvider::Openai,
            AiProvider::Anthropic,
            AiProvider::Gemini,
            AiProvider::Compatible
        ]
        .map(|p| p.key_owner().unwrap()),
        ["ai_openai", "ai_anthropic", "ai_gemini", "ai_compatible"]
    );
    assert!(AiProvider::Ollama.secret_key().is_none());

    config::store_key(&secrets, AiProvider::Openai, &format!("  {FAKE_KEY}\n")).unwrap();
    assert_eq!(
        secrets
            .get(&SecretKey::new("ai_openai", SecretKind::Token))
            .unwrap()
            .unwrap()
            .as_str(),
        FAKE_KEY
    );
    assert!(config::store_key(&secrets, AiProvider::Openai, "   ").is_err());
    assert!(config::store_key(&secrets, AiProvider::Openai, "two words").is_err());
    assert!(config::store_key(&secrets, AiProvider::Ollama, "x").is_err());

    // The view only says "has a key"; the setting never contains it.
    let view = save_config(&storage, &secrets, cfg(AiProvider::Openai)).unwrap();
    assert!(view.keys.openai && !view.keys.anthropic);
    let raw = serde_json::to_string(&storage.settings().unwrap()).unwrap();
    assert!(!raw.contains(FAKE_KEY));
    assert!(raw.contains("ai.config"));
    assert!(!serde_json::to_string(&view).unwrap().contains(FAKE_KEY));

    assert!(config::delete_key(&secrets, AiProvider::Openai).unwrap());
    assert!(!config::delete_key(&secrets, AiProvider::Openai).unwrap());
    assert!(!settings_view(&storage, &secrets).unwrap().keys.openai);
}

#[test]
fn targets_need_keys_models_and_https() {
    let e = Endpoints::default();
    let r = Target::new(&cfg(AiProvider::Openai), None, &e, true);
    assert!(matches!(r, Err(AiError::MissingKey { .. })));
    let r = Target::new(&cfg(AiProvider::Compatible), None, &e, true);
    assert!(matches!(r, Err(AiError::MissingBaseUrl { .. })));
    let mut c = cfg(AiProvider::Compatible);
    c.compatible_base_url = "http://localhost:1234/v1".into();
    // Local server: no key needed, but a model is.
    assert!(matches!(
        Target::new(&c, None, &e, true),
        Err(AiError::MissingModel { .. })
    ));
    c.models.compatible = "local-model".into();
    let t = Target::new(&c, None, &e, true).unwrap();
    assert_eq!(t.timeout, client::LOCAL_TIMEOUT);
    // A key over plain http to another machine is refused.
    c.compatible_base_url = "http://example.com/v1".into();
    let key = Some(Zeroizing::new(FAKE_KEY.to_owned()));
    assert!(matches!(
        Target::new(&c, key.clone(), &e, true),
        Err(AiError::InvalidInput { .. })
    ));
    let t = Target::new(&cfg(AiProvider::Gemini), key, &e, true).unwrap();
    assert_eq!(t.timeout, client::CLOUD_TIMEOUT);
    assert_eq!(t.base_url, config::GEMINI_BASE_URL);
}

// ----- diff context ----------------------------------------------------------------------------

#[test]
fn numstat_parsing_flags_binary_and_generated_files() {
    let raw = "3\t1\tsrc/main.rs\0\
               -\t-\tlogo.png\0\
               120\t80\tpnpm-lock.yaml\0\
               2\t2\t\0old name.txt\0new name.txt\0\
               400\t0\tweb/app.min.js\0";
    let files = context::parse_numstat_z(raw);
    assert_eq!(files.len(), 5);
    assert_eq!(
        files[0],
        FileStat {
            path: "src/main.rs".into(),
            added: Some(3),
            deleted: Some(1),
            omit: None
        }
    );
    assert_eq!(files[1].omit, Some(Omit::Binary));
    assert_eq!(files[2].omit, Some(Omit::Generated));
    assert_eq!(files[3].path, "old name.txt → new name.txt");
    assert_eq!(files[3].omit, None);
    assert_eq!(files[4].omit, Some(Omit::Generated));
    for g in [
        "Cargo.lock",
        "apps/x/package-lock.json",
        "yarn.lock",
        "go.sum",
        "dist/a.min.css",
        "b.js.map",
    ] {
        assert!(context::is_generated(g), "{g}");
    }
    assert!(!context::is_generated("src/lock.rs"));
}

#[test]
fn water_fill_is_fair() {
    assert_eq!(context::water_fill(&[100, 100], 1000), [100, 100]);
    assert_eq!(
        context::water_fill(&[100, 5000, 5000], 1000),
        [100, 450, 450]
    );
    assert_eq!(context::water_fill(&[], 1000), Vec::<usize>::new());
    let shares = context::water_fill(&[10_000, 50, 20_000], 3000);
    assert_eq!(shares[1], 50);
    assert!(shares.iter().sum::<usize>() <= 3000);
}

fn chunk(path: &str, lines: usize) -> String {
    let mut s = format!(
        "diff --git a/{path} b/{path}\nindex 1111111..2222222 100644\n--- a/{path}\n+++ b/{path}\n@@ -1,1 +1,{lines} @@\n"
    );
    for i in 0..lines {
        s.push_str(&format!(
            "+line {i} of {path} with some text to make it longer\n"
        ));
    }
    s
}

#[test]
fn render_diff_cuts_big_files_and_skips_lock_files() {
    let files = context::parse_numstat_z(
        "2\t0\tsmall.rs\0\
         5000\t0\tpnpm-lock.yaml\0\
         3000\t0\tbig.rs\0\
         -\t-\timg.png\0",
    );
    let patch = format!(
        "{}{}{}diff --git a/img.png b/img.png\nBinary files a/img.png and b/img.png differ\n",
        chunk("small.rs", 2),
        chunk("pnpm-lock.yaml", 5000),
        chunk("big.rs", 3000)
    );
    let (out, truncated) = context::render_diff(&files, &[], &patch, 4000);
    assert!(truncated);
    assert!(out.starts_with("Files changed: 4 (+8002 -0 lines)"));
    assert!(out.contains("- pnpm-lock.yaml (+5000 -0, generated/lock file, diff omitted)"));
    assert!(out.contains("- img.png (binary file)"));
    // Lock file content and the binary marker are not sent.
    assert!(!out.contains("line 0 of pnpm-lock.yaml"));
    assert!(!out.contains("Binary files a/img.png"));
    // The small file is whole, the big one cut with a note.
    assert!(out.contains("+line 1 of small.rs"));
    assert!(out.contains("+line 0 of big.rs"));
    assert!(!out.contains("+line 2999 of big.rs"));
    assert!(out.contains("more lines of this file cut]"));
    assert!(out.len() < 4000 + 1000, "{}", out.len());
}

#[test]
fn render_diff_lists_many_files_once() {
    let raw: String = (0..120).map(|i| format!("1\t0\tf{i}.txt\0")).collect();
    let files = context::parse_numstat_z(&raw);
    let (out, truncated) = context::render_diff(&files, &[], "", 1000);
    assert!(truncated);
    assert!(out.contains("- f79.txt (+1 -0)"));
    assert!(!out.contains("- f80.txt"));
    assert!(out.contains("... and 40 more files"));
}

#[test]
fn issue_numbers_from_branch_names() {
    for (branch, want) in [
        ("feature/123-login", Some("#123")),
        ("fix/#45-crash", Some("#45")),
        ("issue-12", Some("#12")),
        ("ABC-321-crash-on-start", Some("ABC-321")),
        ("bugfix/PROJ-7", Some("PROJ-7")),
        ("fix/123/login", Some("#123")),
        ("main", None),
        ("release/1.2", None),
        ("feature/v2-ui", None),
    ] {
        assert_eq!(
            context::issue_from_branch(branch).as_deref(),
            want,
            "{branch}"
        );
    }
}

fn git(dir: &Path, args: &[&str]) {
    GitRunner::default()
        .run(
            dir,
            &AccountContext::default(),
            ["-c", "user.name=Test", "-c", "user.email=t@example.com"]
                .iter()
                .chain(args),
        )
        .unwrap();
}

fn repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    git(d, &["init", "-q", "-b", "feature/42-login"]);
    git(d, &["config", "core.autocrlf", "false"]);
    std::fs::write(d.join("app.rs"), "fn main() {}\n").unwrap();
    git(d, &["add", "."]);
    git(d, &["commit", "-q", "-m", "chore: initial commit"]);
    git(
        d,
        &["commit", "-q", "--allow-empty", "-m", "docs: explain setup"],
    );
    tmp
}

#[test]
fn collects_staged_changes_from_a_repo() {
    let tmp = repo();
    let d = tmp.path();
    std::fs::write(d.join("app.rs"), "fn main() {\n    login();\n}\n").unwrap();
    std::fs::write(d.join("Cargo.lock"), "# lock\n".repeat(500)).unwrap();
    std::fs::write(d.join("pic.bin"), [0u8, 1, 2, 3, 0, 255]).unwrap();
    git(d, &["add", "app.rs", "Cargo.lock", "pic.bin"]);
    std::fs::write(d.join("notes.txt"), "not staged\n").unwrap();

    let ctx = context::collect(&GitRunner::default(), d, false).unwrap();
    assert_eq!(ctx.source, AiDiffSource::Staged);
    assert_eq!(ctx.branch.as_deref(), Some("feature/42-login"));
    assert_eq!(ctx.issue.as_deref(), Some("#42"));
    assert_eq!(
        ctx.recent_subjects,
        ["docs: explain setup", "chore: initial commit"]
    );
    assert_eq!(ctx.files.len(), 3);
    assert!(ctx.diff.contains("+    login();"));
    assert!(
        ctx.diff
            .contains("Cargo.lock (+500 -0, generated/lock file, diff omitted)")
    );
    assert!(ctx.diff.contains("pic.bin (binary file)"));
    assert!(!ctx.diff.contains("# lock"));
    assert!(!ctx.diff.contains("notes.txt"), "unstaged files stay out");
}

#[test]
fn nothing_staged_offers_the_unstaged_changes() {
    let tmp = repo();
    let d = tmp.path();
    let g = GitRunner::default();
    match context::collect(&g, d, false) {
        Err(AiError::NothingStaged { has_unstaged, .. }) => assert!(!has_unstaged),
        other => panic!("{other:?}"),
    }
    std::fs::write(d.join("app.rs"), "fn main() { run(); }\n").unwrap();
    std::fs::write(d.join("new.txt"), "hello\n").unwrap();
    match context::collect(&g, d, false) {
        Err(AiError::NothingStaged { has_unstaged, .. }) => assert!(has_unstaged),
        other => panic!("{other:?}"),
    }
    let ctx = context::collect(&g, d, true).unwrap();
    assert_eq!(ctx.source, AiDiffSource::Unstaged);
    assert!(ctx.diff.contains("run();"));
    assert_eq!(ctx.untracked, ["new.txt"]);
    assert!(ctx.diff.contains("new.txt (new file, not added yet)"));
}

// ----- prompt / answer -------------------------------------------------------------------------

fn ctx() -> CommitContext {
    CommitContext {
        source: AiDiffSource::Staged,
        branch: Some("feature/42-login".into()),
        issue: Some("#42".into()),
        recent_subjects: vec!["fix: typo".into()],
        files: vec![],
        untracked: vec![],
        diff: "Files changed: 1\n- a.rs (+1 -0)\n".into(),
        truncated: true,
    }
}

#[test]
fn prompt_carries_rules_and_context() {
    let p = prompt::build_prompt(&ctx(), "Vietnamese", AiMessageStyle::Conventional, true);
    assert!(p.system.contains("at most 72 characters"));
    assert!(p.system.contains("Conventional Commits"));
    assert!(p.system.contains("Write the message in Vietnamese"));
    assert!(p.system.contains("Refs #42"));
    assert!(p.system.contains("\"summary\""));
    assert!(p.user.contains("Branch: feature/42-login"));
    assert!(p.user.contains("- fix: typo"));
    assert!(p.user.contains("- a.rs (+1 -0)"));
    assert!(p.user.contains("cut to keep it short"));
    let p = prompt::build_prompt(&ctx(), "English", AiMessageStyle::Plain, false);
    assert!(p.system.contains("without a `type:` prefix"));
    assert!(p.system.contains("must be an empty string"));
    assert!(!p.system.contains("Refs #42"));

    assert_eq!(
        prompt::resolve_language(AiMessageLanguage::App, "vi"),
        "Vietnamese"
    );
    assert_eq!(
        prompt::resolve_language(AiMessageLanguage::App, "en"),
        "English"
    );
    assert_eq!(
        prompt::resolve_language(AiMessageLanguage::Vi, "en"),
        "Vietnamese"
    );
}

#[test]
fn answers_are_read_forgivingly() {
    let p = |raw: &str| prompt::parse_answer(raw, true).unwrap();
    let m = p(&answer_json());
    assert_eq!(m.summary, "feat: add login page");
    assert_eq!(m.body, "- Add form\n- Validate input");

    let m = p(
        "Here you go:\n```json\n{\"summary\": \"fix: handle empty input.\", \"body\": \"\"}\n```",
    );
    assert_eq!(m.summary, "fix: handle empty input");
    assert_eq!(m.body, "");

    let m = p(
        "<think>The user changed the parser...</think>\n{\"title\":\"`Refactor parser`\",\"body\":[\"Split into functions\",\"- Add tests\"]}",
    );
    assert_eq!(m.summary, "Refactor parser");
    assert_eq!(m.body, "- Split into functions\n- Add tests");

    let m = p("Commit message:\n\n**Update README with setup steps**\n\nExplain how to install.\n");
    assert_eq!(m.summary, "Update README with setup steps");
    assert_eq!(m.body, "Explain how to install.");

    let m = p("{\"message\": \"feat: add x\\n\\nLonger text\"}");
    assert_eq!(m.summary, "feat: add x");
    assert_eq!(m.body, "Longer text");

    let long = format!(
        "{{\"summary\": \"{}\"}}",
        "feat: add a very long summary line that keeps going and going past the limit of seventy two"
    );
    let m = p(&long);
    assert!(m.summary.chars().count() <= prompt::SUMMARY_MAX);
    assert!(m.summary.starts_with("feat: add a very long"));
    assert!(!m.summary.ends_with(' '));

    let m = prompt::parse_answer(&answer_json(), false).unwrap();
    assert_eq!(m.body, "");
    assert!(prompt::parse_answer("   ", true).is_none());
    assert!(prompt::parse_answer("<think>only thinking", true).is_none());
}

// ----- the four APIs ---------------------------------------------------------------------------

#[tokio::test]
async fn openai_chat_completions() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", format!("Bearer {FAKE_KEY}").as_str()))
        .and(body_partial_json(json!({
            "model": "test-model",
            "max_completion_tokens": 1024,
            "messages": [{ "role": "system", "content": "SYSTEM RULES" }, { "role": "user", "content": "USER DIFF" }],
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{ "message": { "role": "assistant", "content": answer_json() } }]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let t = target(
        AiProvider::Openai,
        &format!("{}/v1", server.uri()),
        Some(FAKE_KEY),
    );
    let raw = client::complete(&http(), &t, &simple_prompt(), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(
        prompt::parse_answer(&raw, true).unwrap().summary,
        "feat: add login page"
    );
}

#[tokio::test]
async fn compatible_service_without_key_sends_no_auth_header() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(
            json!({ "max_tokens": 1024, "temperature": 0.2 }),
        ))
        .respond_with(|req: &Request| {
            assert!(!req.headers.contains_key("authorization"));
            ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "Add tests" } }]
            }))
        })
        .mount(&server)
        .await;
    let t = target(
        AiProvider::Compatible,
        &format!("{}/v1", server.uri()),
        None,
    );
    let raw = client::complete(&http(), &t, &simple_prompt(), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(raw, "Add tests");
}

#[tokio::test]
async fn anthropic_messages() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", FAKE_KEY))
        .and(header("anthropic-version", "2023-06-01"))
        .and(body_partial_json(json!({
            "model": "test-model",
            "system": "SYSTEM RULES",
            "messages": [{ "role": "user", "content": "USER DIFF" }],
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "content": [{ "type": "text", "text": answer_json() }],
            "stop_reason": "end_turn"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let t = target(AiProvider::Anthropic, &server.uri(), Some(FAKE_KEY));
    let raw = client::complete(&http(), &t, &simple_prompt(), &CancellationToken::new())
        .await
        .unwrap();
    assert!(raw.contains("feat: add login page"));
}

#[tokio::test]
async fn gemini_generate_content_key_in_header_only() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1beta/models/test-model:generateContent"))
        .and(header("x-goog-api-key", FAKE_KEY))
        .and(body_partial_json(json!({
            "systemInstruction": { "parts": [{ "text": "SYSTEM RULES" }] },
            "contents": [{ "role": "user", "parts": [{ "text": "USER DIFF" }] }],
        })))
        .respond_with(|req: &Request| {
            assert!(
                !req.url.as_str().contains(FAKE_KEY),
                "key must not be in the URL"
            );
            ResponseTemplate::new(200).set_body_json(json!({
                "candidates": [{ "content": { "parts": [
                    { "text": "thinking...", "thought": true },
                    { "text": answer_json() }
                ] } }]
            }))
        })
        .expect(1)
        .mount(&server)
        .await;
    let mut t = target(AiProvider::Gemini, &server.uri(), Some(FAKE_KEY));
    t.model = "models/test-model".into();
    let raw = client::complete(&http(), &t, &simple_prompt(), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(raw, answer_json());

    // Blocked prompt → no candidates.
    let v = json!({ "promptFeedback": { "blockReason": "SAFETY" } });
    assert!(matches!(
        client::completion_text(AiProvider::Gemini, &v),
        Err(AiError::EmptyAnswer { .. })
    ));
}

#[tokio::test]
async fn ollama_chat() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .and(body_partial_json(json!({
            "model": "test-model",
            "stream": false,
            "format": "json",
            "options": { "num_ctx": 8192 },
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "message": { "role": "assistant", "content": answer_json() },
            "done": true
        })))
        .expect(1)
        .mount(&server)
        .await;
    let t = target(AiProvider::Ollama, &server.uri(), None);
    let raw = client::complete(&http(), &t, &simple_prompt(), &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(raw, answer_json());
}

async fn error_for(provider: AiProvider, status: u16, body: Value) -> AiError {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    let base = if provider == AiProvider::Openai {
        format!("{}/v1", server.uri())
    } else {
        server.uri()
    };
    let t = target(provider, &base, Some(FAKE_KEY));
    client::complete(&http(), &t, &simple_prompt(), &CancellationToken::new())
        .await
        .unwrap_err()
}

#[tokio::test]
async fn http_errors_become_plain_kinds_without_the_key() {
    let e = error_for(
        AiProvider::Openai,
        401,
        json!({ "error": { "message": format!("Incorrect API key provided: {FAKE_KEY}"), "type": "invalid_request_error" } }),
    )
    .await;
    match &e {
        AiError::InvalidKey { message } => {
            assert!(!message.contains(FAKE_KEY), "{message}");
            assert!(message.contains("Incorrect API key"));
        }
        other => panic!("{other:?}"),
    }
    assert!(!serde_json::to_string(&e).unwrap().contains(FAKE_KEY));

    let e = error_for(
        AiProvider::Gemini,
        400,
        json!({ "error": { "code": 400, "message": "API key not valid. Please pass a valid API key.", "status": "INVALID_ARGUMENT" } }),
    )
    .await;
    assert!(matches!(e, AiError::InvalidKey { .. }), "{e:?}");

    let e = error_for(
        AiProvider::Openai,
        429,
        json!({ "error": { "message": "Rate limit reached" } }),
    )
    .await;
    assert!(matches!(e, AiError::RateLimited { .. }), "{e:?}");
    let e = error_for(
        AiProvider::Openai,
        429,
        json!({ "error": { "message": "You exceeded your current quota", "code": "insufficient_quota" } }),
    )
    .await;
    assert!(matches!(e, AiError::QuotaExceeded { .. }), "{e:?}");
    let e = error_for(
        AiProvider::Anthropic,
        529,
        json!({ "error": { "message": "Overloaded" } }),
    )
    .await;
    assert!(matches!(e, AiError::RateLimited { .. }), "{e:?}");
    let e = error_for(
        AiProvider::Anthropic,
        404,
        json!({ "error": { "message": "model: x" } }),
    )
    .await;
    assert!(matches!(e, AiError::ModelNotFound { .. }), "{e:?}");
    let e = error_for(
        AiProvider::Ollama,
        404,
        json!({ "error": "model \"test-model\" not found, try pulling it first" }),
    )
    .await;
    match e {
        AiError::ModelNotFound { model, .. } => assert_eq!(model, "test-model"),
        other => panic!("{other:?}"),
    }
    let e = error_for(
        AiProvider::Gemini,
        503,
        json!({ "error": { "message": "busy" } }),
    )
    .await;
    assert!(matches!(e, AiError::Service { status: 503, .. }), "{e:?}");
}

#[tokio::test]
async fn ollama_down_and_cancel() {
    // Nothing listens on this port (bound then released).
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let t = target(
        AiProvider::Ollama,
        &format!("http://127.0.0.1:{port}"),
        None,
    );
    let e = client::complete(&http(), &t, &simple_prompt(), &CancellationToken::new())
        .await
        .unwrap_err();
    assert!(matches!(e, AiError::OllamaNotRunning { .. }), "{e:?}");
    assert!(ollama::version(&http(), &t.base_url).await.is_none());

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(10)))
        .mount(&server)
        .await;
    let t = target(AiProvider::Ollama, &server.uri(), None);
    let cancel = CancellationToken::new();
    let c2 = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        c2.cancel();
    });
    let start = std::time::Instant::now();
    let e = client::complete(&http(), &t, &simple_prompt(), &cancel)
        .await
        .unwrap_err();
    assert!(matches!(e, AiError::Cancelled { .. }), "{e:?}");
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn connection_test_lists_models() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header(
            "authorization",
            format!("Bearer {FAKE_KEY}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{ "id": "gpt-4.1-nano" }, { "id": "gpt-4.1-mini" }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("x-api-key", FAKE_KEY))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "data": [{ "id": "claude-haiku-4-5-20251001" }]
        })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1beta/models"))
        .and(header_exists("x-goog-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "models": [
                { "name": "models/gemini-2.5-flash-lite", "supportedGenerationMethods": ["generateContent"] },
                { "name": "models/text-embedding-004", "supportedGenerationMethods": ["embedContent"] }
            ]
        })))
        .mount(&server)
        .await;
    let secrets = MemorySecretStore::default();
    let e = endpoints(&server);
    let cancel = CancellationToken::new();

    // No key yet.
    let r = test_impl(&http(), &secrets, &e, cfg(AiProvider::Openai), &cancel).await;
    assert!(matches!(r, Err(AiError::MissingKey { .. })));

    for p in [
        AiProvider::Openai,
        AiProvider::Anthropic,
        AiProvider::Gemini,
    ] {
        config::store_key(&secrets, p, FAKE_KEY).unwrap();
    }
    let r = test_impl(&http(), &secrets, &e, cfg(AiProvider::Openai), &cancel)
        .await
        .unwrap();
    assert_eq!(r.models, ["gpt-4.1-mini", "gpt-4.1-nano"]);
    assert_eq!(r.model_found, Some(true));
    let r = test_impl(&http(), &secrets, &e, cfg(AiProvider::Anthropic), &cancel)
        .await
        .unwrap();
    assert_eq!(r.model_found, Some(true), "alias matches a dated id");
    let r = test_impl(&http(), &secrets, &e, cfg(AiProvider::Gemini), &cancel)
        .await
        .unwrap();
    assert_eq!(r.models, ["gemini-2.5-flash-lite"]);
    assert_eq!(r.model_found, Some(true));

    assert!(client::model_listed(
        &["llama3.2:latest".into()],
        "llama3.2"
    ));
    assert!(!client::model_listed(
        &["llama3.2:3b".into()],
        "llama3.2:1b"
    ));
}

#[tokio::test]
async fn generate_end_to_end_with_a_mock_service() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(|req: &Request| {
            let body: Value = serde_json::from_slice(&req.body).unwrap();
            let user = body["messages"][1]["content"].as_str().unwrap();
            assert!(user.contains("+hello world"), "{user}");
            assert!(user.contains("Branch: feature/42-login"));
            let system = body["messages"][0]["content"].as_str().unwrap();
            assert!(system.contains("Vietnamese"));
            ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{ "message": { "content": "```json\n{\"summary\":\"feat: thêm lời chào\",\"body\":\"Refs #42\"}\n```" } }]
            }))
        })
        .expect(1)
        .mount(&server)
        .await;
    let tmp = repo();
    let d = tmp.path();
    std::fs::write(d.join("hello.txt"), "hello world\n").unwrap();
    git(d, &["add", "hello.txt"]);
    let db = tempfile::tempdir().unwrap();
    let storage = Storage::open(&db.path().join("app.db")).unwrap();
    let secrets = MemorySecretStore::default();
    let e = endpoints(&server);
    let run = async |storage: &Storage| {
        generate_impl(
            &http(),
            storage,
            &secrets,
            &e,
            GitRunner::default(),
            d.to_path_buf(),
            "vi",
            false,
            &CancellationToken::new(),
        )
        .await
    };

    // Off by default.
    let r = run(&storage).await;
    assert!(matches!(r, Err(AiError::NotEnabled { .. })), "{r:?}");

    save_config(&storage, &secrets, cfg(AiProvider::Openai)).unwrap();
    config::store_key(&secrets, AiProvider::Openai, FAKE_KEY).unwrap();
    let s = run(&storage).await.unwrap();
    assert_eq!(s.summary, "feat: thêm lời chào");
    assert_eq!(s.body, "Refs #42");
    assert_eq!(s.source, AiDiffSource::Staged);
    assert_eq!(s.provider, AiProvider::Openai);
    assert_eq!(s.model, "gpt-4.1-mini");
}

// ----- Ollama management -----------------------------------------------------------------------

#[tokio::test]
async fn ollama_version_tags_pull_and_delete() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/version"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "version": "0.6.2" })))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "models": [
            { "name": "qwen2.5-coder:1.5b", "size": 986_000_000u64,
              "details": { "parameter_size": "1.5B", "quantization_level": "Q4_K_M" } },
            { "name": "gemma3:1b", "size": 815_000_000u64, "details": {} }
        ] })))
        .mount(&server)
        .await;
    let lines = [
        json!({ "status": "pulling manifest" }),
        json!({ "status": "pulling aaa", "digest": "sha256:aaa", "total": 1000, "completed": 0 }),
        json!({ "status": "pulling aaa", "digest": "sha256:aaa", "total": 1000, "completed": 500 }),
        json!({ "status": "pulling bbb", "digest": "sha256:bbb", "total": 1000, "completed": 1000 }),
        json!({ "status": "pulling aaa", "digest": "sha256:aaa", "total": 1000, "completed": 1000 }),
        json!({ "status": "verifying sha256 digest" }),
        json!({ "status": "success" }),
    ];
    let stream: String = lines.iter().map(|l| format!("{l}\n")).collect();
    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .and(body_partial_json(
            json!({ "model": "gemma3:1b", "stream": true }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(stream, "application/x-ndjson"))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/pull"))
        .and(body_partial_json(json!({ "model": "nope:1b" })))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "{\"status\":\"pulling manifest\"}\n{\"error\":\"pull model manifest: file does not exist\"}\n",
            "application/x-ndjson",
        ))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/delete"))
        .and(body_partial_json(json!({ "model": "gemma3:1b" })))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .and(path("/api/delete"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({ "error": "model not found" })),
        )
        .mount(&server)
        .await;
    let base = server.uri();
    let h = http();
    assert_eq!(ollama::version(&h, &base).await.as_deref(), Some("0.6.2"));
    let models = ollama::models(&h, &base).await.unwrap();
    assert_eq!(models.len(), 2);
    assert_eq!(models[0].name, "gemma3:1b");
    assert_eq!(models[1].parameter_size.as_deref(), Some("1.5B"));
    assert_eq!(models[1].quantization.as_deref(), Some("Q4_K_M"));

    let mut seen = Vec::new();
    ollama::pull(
        &h,
        &base,
        "gemma3:1b",
        "req-1",
        &CancellationToken::new(),
        |p| seen.push(p),
    )
    .await
    .unwrap();
    let last = seen.last().unwrap();
    assert!(last.done && last.status == "success");
    assert_eq!(
        (last.completed, last.total, last.percent),
        (2000, 2000, Some(100))
    );
    assert_eq!(last.request_id, "req-1");
    assert!(seen.iter().any(|p| p.status == "pulling manifest"));
    assert!(seen.iter().any(|p| p.status == "verifying sha256 digest"));

    let e = ollama::pull(
        &h,
        &base,
        "nope:1b",
        "req-2",
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap_err();
    assert!(matches!(e, AiError::ModelNotFound { .. }), "{e:?}");
    let e = ollama::pull(
        &h,
        &base,
        "--bad name",
        "r",
        &CancellationToken::new(),
        |_| {},
    )
    .await
    .unwrap_err();
    assert!(matches!(e, AiError::InvalidInput { .. }));

    ollama::delete(&h, &base, "gemma3:1b").await.unwrap();
    let e = ollama::delete(&h, &base, "other:1b").await.unwrap_err();
    assert!(matches!(e, AiError::ModelNotFound { .. }), "{e:?}");
}

#[test]
fn pull_tally_sums_layers() {
    let mut t = ollama::PullTally::default();
    t.update(&json!({ "digest": "a", "total": 100, "completed": 50 }));
    t.update(&json!({ "digest": "b", "total": 300 }));
    t.update(&json!({ "digest": "a", "total": 100, "completed": 150 }));
    assert_eq!(t.sum(), (100, 400));
}

#[test]
fn model_names_are_checked() {
    for ok in ["qwen2.5-coder:1.5b", "llama3.2", "hf.co/user/repo:Q4_K_M"] {
        assert!(ollama::valid_model_name(ok), "{ok}");
    }
    for bad in ["", "-x", "a b", "x;rm", "../x", &"a".repeat(201)] {
        assert!(!ollama::valid_model_name(bad), "{bad}");
    }
}

#[test]
fn ollama_install_whitelist_and_locations() {
    use crate::onboarding::{InstallPlan, Managers, ProbeEnv, SetupPackageManager, TargetOs};
    use std::path::PathBuf;
    let winget = Managers {
        winget: Some(PathBuf::from("C:/wa/winget.exe")),
        brew: None,
    };
    let plan: InstallPlan = ollama::install_plan(TargetOs::Windows, &winget).unwrap();
    assert_eq!(plan.manager, SetupPackageManager::Winget);
    assert_eq!(
        plan.display(),
        "winget install --id Ollama.Ollama -e --source winget --accept-source-agreements --accept-package-agreements"
    );
    let brew = Managers {
        winget: None,
        brew: Some(PathBuf::from("/opt/homebrew/bin/brew")),
    };
    assert_eq!(
        ollama::install_plan(TargetOs::Macos, &brew).unwrap().args,
        ["install", "--cask", "ollama"]
    );
    assert!(ollama::install_plan(TargetOs::Macos, &Managers::default()).is_none());
    assert!(ollama::install_plan(TargetOs::Linux, &brew).is_none());
    assert!(ollama::install_plan(TargetOs::Windows, &Managers::default()).is_none());

    // Windows: found in %LOCALAPPDATA%\Programs\Ollama when not on PATH.
    let tmp = tempfile::tempdir().unwrap();
    let lad = tmp.path().join("Local");
    let dir = lad.join("Programs").join("Ollama");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("ollama.exe"), b"").unwrap();
    let empty = tmp.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let env = ProbeEnv {
        os: TargetOs::Windows,
        path: Some(std::env::join_paths([&empty]).unwrap()),
        exts: vec![".exe".into()],
        program_files: Vec::new(),
        local_app_data: Some(lad),
        system_root: None,
        unix_dirs: Vec::new(),
    };
    let (cli, app) = ollama::locate(&env, &[]);
    assert_eq!(cli.unwrap(), dir.join("ollama.exe"));
    assert!(app.is_none());
    let env = ProbeEnv {
        local_app_data: None,
        ..env
    };
    assert_eq!(ollama::locate(&env, &[]), (None, None));
}

#[test]
fn cancel_registry_and_install_slot() {
    let (running, token) = Running::start("abc").unwrap();
    assert!(cancel_request("abc"));
    assert!(token.is_cancelled());
    drop(running);
    assert!(!cancel_request("abc"));
    assert!(Running::start("").is_err());

    let (slot, _flag) = InstallSlot::take().unwrap();
    assert!(matches!(InstallSlot::take(), Err(AiError::Busy { .. })));
    drop(slot);
    assert!(InstallSlot::take().is_ok());
}

#[test]
fn errors_serialize_with_kind_and_message() {
    let e = AiError::NothingStaged {
        message: "nothing is staged".into(),
        has_unstaged: true,
    };
    assert_eq!(
        serde_json::to_value(&e).unwrap(),
        json!({ "kind": "nothingStaged", "message": "nothing is staged", "hasUnstaged": true })
    );
    assert_eq!(
        serde_json::to_value(AiError::invalid("x")).unwrap(),
        json!({ "kind": "invalidInput", "message": "x" })
    );
}
