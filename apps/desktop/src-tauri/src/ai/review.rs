//! "AI review" of the changes about to be committed. It runs only when the user clicks the
//! button (never by itself before a commit or a push) and sends the same cut diff as the
//! commit message suggestion.

use super::context::CommitContext;
use super::prompt::strip_think;

pub const MAX_TOKENS: u32 = 900;
/// Longest review shown.
const REVIEW_MAX_CHARS: usize = 6_000;

pub fn system_prompt() -> String {
    "You are a careful code reviewer looking at a diff right before it is committed. You only \
     report what the diff itself shows; you never invent problems and you do not rewrite the \
     code."
        .into()
}

pub fn user_prompt(ctx: &CommitContext, language: &str) -> String {
    let cut = if ctx.truncated {
        "The diff was cut to fit; say so in one line and do not guess about the parts you \
         cannot see.\n"
    } else {
        ""
    };
    format!(
        "Review this diff in {language}. Look for: bugs and logic mistakes, leftover debug \
         code (console.log, print, dbg!, TODO left by accident), secrets or passwords, files \
         that look committed by mistake, and risky changes without a matching test.\n\
         Answer as a short list, most serious first, at most 8 items. Each item: the file \
         (and line when you can tell), what is wrong, and how to fix it in one sentence. \
         Plain text, no code blocks, no emoji, no praise. If you find nothing worth fixing, \
         answer with one sentence saying the changes look fine.\n{cut}\n{}",
        ctx.diff
    )
}

/// The review text of the model's answer; `None` when it is empty.
pub fn parse_review(raw: &str) -> Option<String> {
    let text = strip_think(raw);
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(match text.char_indices().nth(REVIEW_MAX_CHARS) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_answer_and_drops_thinking() {
        assert_eq!(
            parse_review("<think>hmm</think>\n1. a.rs: bug\n").as_deref(),
            Some("1. a.rs: bug")
        );
        assert_eq!(parse_review("  \n"), None);
        let long = "x".repeat(REVIEW_MAX_CHARS + 50);
        let cut = parse_review(&long).unwrap();
        assert_eq!(cut.chars().count(), REVIEW_MAX_CHARS + 1);
    }
}
