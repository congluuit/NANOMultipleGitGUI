//! Prompt for the model and a forgiving reader of its answer (JSON asked, but small local
//! models add code fences, `<think>` blocks, labels or plain text).

use super::config::{AiMessageLanguage, AiMessageStyle};
use super::context::{AiDiffSource, CommitContext};

pub const SUMMARY_MAX: usize = 72;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMessage {
    pub summary: String,
    pub body: String,
}

/// `app_language` is the app's current language (`vi` / `en`), used for [`AiMessageLanguage::App`].
pub fn resolve_language(lang: AiMessageLanguage, app_language: &str) -> &'static str {
    match lang {
        AiMessageLanguage::En => "English",
        AiMessageLanguage::Vi => "Vietnamese",
        AiMessageLanguage::App if app_language.to_ascii_lowercase().starts_with("vi") => {
            "Vietnamese"
        }
        AiMessageLanguage::App => "English",
    }
}

pub fn build_prompt(
    ctx: &CommitContext,
    language: &str,
    style: AiMessageStyle,
    include_body: bool,
) -> Prompt {
    let style_rule = match style {
        AiMessageStyle::Conventional => {
            "Use the Conventional Commits format: `type(scope): description` where type is one of \
             feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert; the scope is \
             optional and short. Keep the type keyword in English."
        }
        AiMessageStyle::Plain => {
            "Write a plain sentence starting with a capital letter, without a `type:` prefix."
        }
    };
    let body_rule = if include_body {
        "Also write a short body: 1 to 5 lines (a \"- \" list is fine) saying what changed and why, \
         each line at most 72 characters. Leave it empty if the summary says everything."
    } else {
        "Do not write a body: \"body\" must be an empty string."
    };
    let issue_rule = match &ctx.issue {
        Some(issue) if include_body => {
            format!(
                "\n- The branch refers to issue {issue}: you may add \"Refs {issue}\" as the last body line."
            )
        }
        _ => String::new(),
    };
    let system = format!(
        "You write git commit messages.\n\
         Rules:\n\
         - Summary: imperative mood (\"Add\", \"Fix\", \"Update\"), at most {SUMMARY_MAX} characters, \
         no period at the end, no markdown, no quotes or backticks around it.\n\
         - {style_rule}\n\
         - {body_rule}\n\
         - Write the message in {language}.\n\
         - Describe only what the diff shows. Do not invent changes.\n\
         - When the recent commits share a clear style, follow it.{issue_rule}\n\
         Answer with JSON only, no other text: {{\"summary\": \"...\", \"body\": \"...\"}}"
    );

    let mut user = String::new();
    if let Some(b) = &ctx.branch {
        user.push_str(&format!("Branch: {b}\n"));
    }
    if let Some(i) = &ctx.issue {
        user.push_str(&format!("Issue: {i}\n"));
    }
    if !ctx.recent_subjects.is_empty() {
        user.push_str("Recent commits:\n");
        for s in &ctx.recent_subjects {
            user.push_str(&format!("- {s}\n"));
        }
    }
    if ctx.source == AiDiffSource::Unstaged {
        user.push_str("Changes (not staged yet):\n");
    } else {
        user.push_str("Staged changes:\n");
    }
    user.push_str(&ctx.diff);
    if ctx.truncated {
        user.push_str("\n(Some of the diff was cut to keep it short.)\n");
    }
    Prompt { system, user }
}

/// Drops `<think>…</think>` blocks (reasoning models served by Ollama / LM Studio).
fn strip_think(s: &str) -> String {
    let mut out = s.to_owned();
    while let Some(start) = out.find("<think>") {
        match out[start..].find("</think>") {
            Some(end) => out.replace_range(start..start + end + "</think>".len(), ""),
            None => {
                out.truncate(start);
                break;
            }
        }
    }
    // A reply that starts mid-thought (`…</think>answer`).
    if let Some(end) = out.find("</think>") {
        out = out[end + "</think>".len()..].to_owned();
    }
    out
}

/// Content of the first ``` fence, or the text itself.
fn strip_fences(s: &str) -> String {
    let t = s.trim();
    let Some(start) = t.find("```") else {
        return t.to_owned();
    };
    let after = &t[start + 3..];
    // Skip the info string (`json`, `text`, ...).
    let after = match after.find('\n') {
        Some(nl) if !after[..nl].contains(char::is_whitespace) || after[..nl].trim().is_empty() => {
            &after[nl + 1..]
        }
        _ => after,
    };
    match after.find("```") {
        Some(end) => after[..end].trim().to_owned(),
        None => after.trim().to_owned(),
    }
}

fn json_message(s: &str) -> Option<ParsedMessage> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end <= start {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&s[start..=end]).ok()?;
    let obj = v.as_object()?;
    let text = |keys: &[&str]| {
        keys.iter().find_map(|k| match obj.get(*k) {
            Some(serde_json::Value::String(s)) => Some(s.clone()),
            Some(serde_json::Value::Array(a)) => Some(
                a.iter()
                    .filter_map(|x| x.as_str())
                    .map(|x| {
                        if x.starts_with("- ") {
                            x.to_owned()
                        } else {
                            format!("- {x}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => None,
        })
    };
    let body = text(&["body", "description", "details"]).unwrap_or_default();
    match text(&["summary", "title", "subject", "header"]) {
        Some(summary) => Some(ParsedMessage { summary, body }),
        None => {
            let message = text(&["message", "commit_message", "commitMessage", "commit"])?;
            Some(split_plain(&message))
        }
    }
}

fn strip_label(line: &str) -> &str {
    let t = line.trim();
    for label in [
        "commit message:",
        "summary:",
        "title:",
        "subject:",
        "tiêu đề:",
        "tóm tắt:",
    ] {
        if t.len() >= label.len()
            && t.is_char_boundary(label.len())
            && t[..label.len()].eq_ignore_ascii_case(label)
        {
            return t[label.len()..].trim();
        }
    }
    t
}

fn split_plain(text: &str) -> ParsedMessage {
    let mut lines = text.lines().map(str::trim_end).skip_while(|l| {
        let t = strip_label(l);
        t.is_empty() || t.eq_ignore_ascii_case("commit message:")
    });
    let summary = lines.next().map(strip_label).unwrap_or_default().to_owned();
    let body: Vec<&str> = lines
        .map(|l| {
            let t = l.trim_start();
            for label in ["Body:", "body:", "Description:"] {
                if let Some(rest) = t.strip_prefix(label) {
                    return rest.trim_start();
                }
            }
            l
        })
        .collect();
    ParsedMessage {
        summary,
        body: body.join("\n"),
    }
}

/// Cleans the summary: one line, no wrapping quotes/backticks/markdown, no final period, cut at
/// a word boundary to [`SUMMARY_MAX`] characters.
pub fn clean_summary(raw: &str) -> String {
    let mut s = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default()
        .to_owned();
    s = s.trim_start_matches(['#', '>']).trim().to_owned();
    for bullet in ["- ", "* "] {
        if let Some(rest) = s.strip_prefix(bullet) {
            s = rest.trim().to_owned();
        }
    }
    loop {
        let t = s.trim();
        let stripped = [
            ("`", "`"),
            ("\"", "\""),
            ("'", "'"),
            ("**", "**"),
            ("“", "”"),
        ]
        .iter()
        .find_map(|(a, b)| {
            (t.len() >= a.len() + b.len() && t.starts_with(a) && t.ends_with(b))
                .then(|| t[a.len()..t.len() - b.len()].to_owned())
        });
        match stripped {
            Some(x) => s = x,
            None => break,
        }
    }
    let mut s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    while s.ends_with('.') && !s.ends_with("...") {
        s.pop();
    }
    if s.chars().count() > SUMMARY_MAX {
        let cut: String = s.chars().take(SUMMARY_MAX).collect();
        s = match cut.rfind(' ') {
            Some(sp) if sp > SUMMARY_MAX / 2 => {
                cut[..sp].trim_end_matches([',', ';', ':']).to_owned()
            }
            _ => cut,
        };
    }
    s
}

fn clean_body(raw: &str) -> String {
    let body = strip_fences(raw);
    let lines: Vec<&str> = body.lines().map(str::trim_end).collect();
    let start = lines.iter().position(|l| !l.trim().is_empty());
    let end = lines.iter().rposition(|l| !l.trim().is_empty());
    match (start, end) {
        (Some(s), Some(e)) => lines[s..=e].join("\n"),
        _ => String::new(),
    }
}

/// Reads the model's answer. `None` when no summary could be found.
pub fn parse_answer(raw: &str, include_body: bool) -> Option<ParsedMessage> {
    let text = strip_think(raw);
    let inner = strip_fences(&text);
    let msg = json_message(&inner)
        .or_else(|| json_message(&text))
        .unwrap_or_else(|| split_plain(&inner));
    let summary = clean_summary(&msg.summary);
    if summary.is_empty() {
        return None;
    }
    let body = if include_body {
        clean_body(&msg.body)
    } else {
        String::new()
    };
    Some(ParsedMessage { summary, body })
}
