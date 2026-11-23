//! Secret Guard: before a commit, scan the lines the commit adds for credentials.
//!
//! - **Rules** are a high-signal subset of the gitleaks default config
//!   (<https://github.com/gitleaks/gitleaks/blob/master/config/gitleaks.toml>, MIT): tokens with
//!   a distinctive prefix or structure (AWS, GitHub, GitLab, Slack, Stripe, Google, OpenAI,
//!   Anthropic, npm, PyPI, private keys, JWT and more). The generic "password = ..." and
//!   entropy rules are left out on purpose so the guard rarely cries wolf. Rule ids are the
//!   gitleaks ids, so a `.gitleaksignore` written for gitleaks works here too.
//! - **What is scanned**: only the `+` lines of `git diff --cached -U0` (deleted files, binary
//!   files and the gitleaks global path allowlist are skipped). The diff output is read with a
//!   byte cap and long lines are cut, so a huge staged file cannot make a commit slow.
//! - **Ignoring**: a line containing `gitleaks:allow` is skipped; `.gitleaksignore` entries of
//!   the form `file:rule-id:line` are honoured; and the caller passes fingerprints the user
//!   chose to always ignore (stored by the app, never in the repository).
//! - **Secrets never leave this module in clear**: findings carry a masked value and a salted
//!   SHA-256 fingerprint only. Nothing here logs.

use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::sync::LazyLock;

use aho_corasick::AhoCorasick;
use regex::Regex;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{AccountContext, GitError, GitRunner};

/// Bytes of `git diff` output read at most; the rest of the diff is not scanned.
pub const MAX_DIFF_BYTES: usize = 8 * 1024 * 1024;
/// Characters of one added line that are scanned (minified bundles have megabyte lines).
pub const MAX_LINE_CHARS: usize = 16 * 1024;
/// Findings reported at most (the dialog has to stay usable).
pub const MAX_FINDINGS: usize = 200;
/// Name of the gitleaks ignore file at the repository root.
pub const IGNORE_FILE: &str = ".gitleaksignore";
/// Inline marker that silences a line (gitleaks convention).
pub const ALLOW_MARKER: &str = "gitleaks:allow";

/// One rule, taken from the gitleaks default config.
#[derive(Debug, Clone, Copy)]
pub struct RuleSpec {
    /// gitleaks rule id (used in `.gitleaksignore`).
    pub id: &'static str,
    /// Short English label shown in the dialog.
    pub name: &'static str,
    /// Lower-case substrings; the regex only runs on lines containing one of them.
    pub keywords: &'static [&'static str],
    pub regex: &'static str,
    /// Minimum Shannon entropy (bits per char) of the secret, as in gitleaks.
    pub entropy: Option<f64>,
    /// A secret matching this is an allowed example (e.g. AWS keys ending in `EXAMPLE`).
    pub allow: Option<&'static str>,
}

/// Trailing context gitleaks uses after a secret: a quote, whitespace, `;`, an escaped newline
/// or the end of the line.
macro_rules! end {
    () => {
        r#"(?:[\x60'"\s;]|\\[nr]|$)"#
    };
}

const fn rule(
    id: &'static str,
    name: &'static str,
    keywords: &'static [&'static str],
    regex: &'static str,
    entropy: Option<f64>,
) -> RuleSpec {
    RuleSpec {
        id,
        name,
        keywords,
        regex,
        entropy,
        allow: None,
    }
}

/// Rule id of the private key check (multi-line, handled separately).
pub const PRIVATE_KEY_RULE: &str = "private-key";

/// The rules, in the order findings prefer them when two match the same text.
pub const RULES: &[RuleSpec] = &[
    RuleSpec {
        allow: Some(r".+EXAMPLE$"),
        ..rule(
            "aws-access-token",
            "AWS access key ID",
            &["a3t", "akia", "asia", "abia", "acca"],
            r"\b((?:A3T[A-Z0-9]|AKIA|ASIA|ABIA|ACCA)[A-Z2-7]{16})\b",
            Some(3.0),
        )
    },
    rule(
        "github-pat",
        "GitHub personal access token",
        &["ghp_"],
        r"ghp_[0-9a-zA-Z]{36}",
        Some(3.0),
    ),
    rule(
        "github-fine-grained-pat",
        "GitHub fine-grained token",
        &["github_pat_"],
        r"github_pat_[0-9A-Za-z_]{82}",
        Some(3.0),
    ),
    rule(
        "github-oauth",
        "GitHub OAuth token",
        &["gho_"],
        r"gho_[0-9a-zA-Z]{36}",
        Some(3.0),
    ),
    rule(
        "github-app-token",
        "GitHub App token",
        &["ghu_", "ghs_"],
        r"(?:ghu|ghs)_[0-9a-zA-Z]{36}",
        Some(3.0),
    ),
    rule(
        "github-refresh-token",
        "GitHub refresh token",
        &["ghr_"],
        r"ghr_[0-9a-zA-Z]{36}",
        Some(3.0),
    ),
    rule(
        "gitlab-pat-routable",
        "GitLab personal access token",
        &["glpat-"],
        r"\bglpat-[0-9a-zA-Z_-]{27,300}\.[0-9a-z]{2}[0-9a-z]{7}\b",
        Some(4.0),
    ),
    rule(
        "gitlab-pat",
        "GitLab personal access token",
        &["glpat-"],
        r"glpat-[0-9A-Za-z_-]{20}",
        Some(3.0),
    ),
    rule(
        "gitlab-ptt",
        "GitLab pipeline trigger token",
        &["glptt-"],
        r"glptt-[0-9a-f]{40}",
        Some(3.0),
    ),
    rule(
        "gitlab-rrt",
        "GitLab runner registration token",
        &["gr1348941"],
        r"GR1348941[0-9A-Za-z_-]{20}",
        Some(3.0),
    ),
    rule(
        "gitlab-runner-authentication-token",
        "GitLab runner token",
        &["glrt-"],
        r"glrt-[0-9a-zA-Z_\-]{20}",
        Some(3.0),
    ),
    rule(
        "gitlab-deploy-token",
        "GitLab deploy token",
        &["gldt-"],
        r"gldt-[0-9a-zA-Z_\-]{20}",
        Some(3.0),
    ),
    rule(
        "slack-bot-token",
        "Slack bot token",
        &["xoxb"],
        r"xoxb-[0-9]{10,13}-[0-9]{10,13}[a-zA-Z0-9-]*",
        Some(3.0),
    ),
    rule(
        "slack-user-token",
        "Slack user token",
        &["xoxp-", "xoxe-"],
        r"xox[pe](?:-[0-9]{10,13}){3}-[a-zA-Z0-9-]{28,34}",
        Some(2.0),
    ),
    rule(
        "slack-app-token",
        "Slack app token",
        &["xapp"],
        r"(?i)xapp-\d-[A-Z0-9]+-\d+-[a-z0-9]+",
        Some(2.0),
    ),
    rule(
        "slack-config-access-token",
        "Slack configuration token",
        &["xoxe.xoxb-", "xoxe.xoxp-"],
        r"(?i)xoxe.xox[bp]-\d-[A-Z0-9]{163,166}",
        Some(2.0),
    ),
    rule(
        "slack-config-refresh-token",
        "Slack configuration refresh token",
        &["xoxe-"],
        r"(?i)xoxe-\d-[A-Z0-9]{146}",
        Some(2.0),
    ),
    rule(
        "slack-legacy-bot-token",
        "Slack bot token",
        &["xoxb"],
        r"xoxb-[0-9]{8,14}-[a-zA-Z0-9]{18,26}",
        Some(2.0),
    ),
    rule(
        "slack-legacy-token",
        "Slack legacy token",
        &["xoxo", "xoxs"],
        r"xox[os]-\d+-\d+-\d+-[a-fA-F\d]+",
        Some(2.0),
    ),
    rule(
        "slack-webhook-url",
        "Slack webhook URL",
        &["hooks.slack.com"],
        r"(?:https?://)?hooks.slack.com/(?:services|workflows|triggers)/[A-Za-z0-9+/]{43,56}",
        None,
    ),
    rule(
        "stripe-access-token",
        "Stripe API key",
        &[
            "sk_test", "sk_live", "sk_prod", "rk_test", "rk_live", "rk_prod",
        ],
        concat!(
            r"\b((?:sk|rk)_(?:test|live|prod)_[a-zA-Z0-9]{10,99})",
            end!()
        ),
        Some(2.0),
    ),
    rule(
        "gcp-api-key",
        "Google API key",
        &["aiza"],
        concat!(r"\b(AIza[0-9A-Za-z_-]{35})", end!()),
        Some(4.0),
    ),
    rule(
        "openai-api-key",
        "OpenAI API key",
        &["t3blbkfj"],
        concat!(
            r"\b(sk-(?:proj|svcacct|admin)-(?:[A-Za-z0-9_-]{74}|[A-Za-z0-9_-]{58})T3BlbkFJ(?:[A-Za-z0-9_-]{74}|[A-Za-z0-9_-]{58})\b|sk-[a-zA-Z0-9]{20}T3BlbkFJ[a-zA-Z0-9]{20})",
            end!()
        ),
        Some(3.0),
    ),
    rule(
        "anthropic-api-key",
        "Anthropic API key",
        &["sk-ant-api03"],
        concat!(r"\b(sk-ant-api03-[a-zA-Z0-9_\-]{93}AA)", end!()),
        None,
    ),
    rule(
        "anthropic-admin-api-key",
        "Anthropic admin key",
        &["sk-ant-admin01"],
        concat!(r"\b(sk-ant-admin01-[a-zA-Z0-9_\-]{93}AA)", end!()),
        None,
    ),
    rule(
        "npm-access-token",
        "npm access token",
        &["npm_"],
        concat!(r"(?i)\b(npm_[a-z0-9]{36})", end!()),
        Some(2.0),
    ),
    rule(
        "pypi-upload-token",
        "PyPI upload token",
        &["pypi-ageichlwas5vcmc"],
        r"pypi-AgEIcHlwaS5vcmc[0-9A-Za-z_-]{50,1000}",
        Some(3.0),
    ),
    rule(
        "jwt",
        "JSON Web Token",
        // gitleaks uses "ey"; every JWT starts with `eyJ` (base64 of `{"`), and "ey" alone
        // matches every line containing "key".
        &["eyj"],
        concat!(
            r"\b(ey[a-zA-Z0-9]{17,}\.ey[a-zA-Z0-9\/\\_-]{17,}\.(?:[a-zA-Z0-9\/\\_-]{10,}={0,2})?)",
            end!()
        ),
        Some(3.0),
    ),
    rule(
        "sendgrid-api-token",
        "SendGrid API key",
        &["sg."],
        concat!(r"\b(SG\.(?i)[a-z0-9=_\-\.]{66})", end!()),
        Some(2.0),
    ),
    rule(
        "huggingface-access-token",
        "Hugging Face token",
        &["hf_"],
        concat!(r"\b(hf_(?i:[a-z]{34}))", end!()),
        Some(2.0),
    ),
    rule(
        "digitalocean-pat",
        "DigitalOcean token",
        &["dop_v1_"],
        concat!(r"\b(dop_v1_[a-f0-9]{64})", end!()),
        Some(3.0),
    ),
    rule(
        "digitalocean-access-token",
        "DigitalOcean OAuth token",
        &["doo_v1_"],
        concat!(r"\b(doo_v1_[a-f0-9]{64})", end!()),
        Some(3.0),
    ),
    rule(
        "shopify-access-token",
        "Shopify access token",
        &["shpat_"],
        r"shpat_[a-fA-F0-9]{32}",
        Some(2.0),
    ),
    rule(
        "shopify-custom-access-token",
        "Shopify custom app token",
        &["shpca_"],
        r"shpca_[a-fA-F0-9]{32}",
        Some(2.0),
    ),
    rule(
        "shopify-private-app-access-token",
        "Shopify private app token",
        &["shppa_"],
        r"shppa_[a-fA-F0-9]{32}",
        Some(2.0),
    ),
    rule(
        "shopify-shared-secret",
        "Shopify shared secret",
        &["shpss_"],
        r"shpss_[a-fA-F0-9]{32}",
        Some(2.0),
    ),
    rule(
        "azure-ad-client-secret",
        "Azure AD client secret",
        &["q~"],
        r#"(?:^|[\\'"\x60\s>=:(,)])([a-zA-Z0-9_~.]{3}\dQ~[a-zA-Z0-9_~.-]{31,34})(?:$|[\\'"\x60\s<),])"#,
        Some(3.0),
    ),
    rule(
        "databricks-api-token",
        "Databricks token",
        &["dapi"],
        concat!(r"\b(dapi[a-f0-9]{32}(?:-\d)?)", end!()),
        Some(3.0),
    ),
    rule(
        "doppler-api-token",
        "Doppler token",
        &["dp.pt."],
        r"dp\.pt\.(?i)[a-z0-9]{43}",
        Some(2.0),
    ),
    rule(
        "dynatrace-api-token",
        "Dynatrace token",
        &["dt0c01."],
        r"dt0c01\.(?i)[a-z0-9]{24}\.[a-z0-9]{64}",
        Some(4.0),
    ),
    rule(
        "linear-api-key",
        "Linear API key",
        &["lin_api_"],
        r"lin_api_(?i)[a-z0-9]{40}",
        Some(2.0),
    ),
    rule(
        "postman-api-token",
        "Postman API key",
        &["pmak-"],
        concat!(r"\b(PMAK-(?i)[a-f0-9]{24}\-[a-f0-9]{34})", end!()),
        Some(3.0),
    ),
    rule(
        "pulumi-api-token",
        "Pulumi token",
        &["pul-"],
        concat!(r"\b(pul-[a-f0-9]{40})", end!()),
        Some(2.0),
    ),
    rule(
        "rubygems-api-token",
        "RubyGems API key",
        &["rubygems_"],
        concat!(r"\b(rubygems_[a-f0-9]{48})", end!()),
        Some(2.0),
    ),
    rule(
        "sentry-user-token",
        "Sentry user token",
        &["sntryu_"],
        concat!(r"\b(sntryu_[a-f0-9]{64})", end!()),
        Some(3.5),
    ),
    rule(
        "grafana-service-account-token",
        "Grafana service account token",
        &["glsa_"],
        concat!(r"(?i)\b(glsa_[A-Za-z0-9]{32}_[A-Fa-f0-9]{8})", end!()),
        Some(3.0),
    ),
    rule(
        "planetscale-api-token",
        "PlanetScale token",
        &["pscale_tkn_"],
        concat!(r"\b(pscale_tkn_(?i)[\w=\.-]{32,64})", end!()),
        Some(3.0),
    ),
    rule(
        "planetscale-password",
        "PlanetScale password",
        &["pscale_pw_"],
        concat!(r"(?i)\b(pscale_pw_(?i)[\w=\.-]{32,64})", end!()),
        Some(3.0),
    ),
    rule(
        "hashicorp-tf-api-token",
        "Terraform Cloud token",
        &["atlasv1"],
        r"(?i)[a-z0-9]{14}\.(?-i:atlasv1)\.[a-z0-9\-_=]{60,70}",
        Some(3.5),
    ),
    rule(
        "age-secret-key",
        "age secret key",
        &["age-secret-key-1"],
        r"AGE-SECRET-KEY-1[QPZRY9X8GF2TVDW0S3JN54KHCE6MUA7L]{58}",
        None,
    ),
    rule(
        "telegram-bot-api-token",
        "Telegram bot token",
        &["telegr"],
        concat!(
            r#"(?i)[\w.-]{0,50}?(?:telegr)(?:[ \t\w.-]{0,20})[\s'"]{0,3}(?:=|>|:{1,3}=|\|\||:|=>|\?=|,)[\x60'"\s=]{0,5}([0-9]{5,16}:(?-i:A)[a-z0-9_\-]{34})"#,
            end!()
        ),
        None,
    ),
];

/// Paths gitleaks never scans (lock files, vendored dependencies, media); see its global
/// allowlist.
const PATH_ALLOWLIST: &[&str] = &[
    r"(?:^|/)\.?gitleaks\.toml$",
    r"(?:^|/)\.gitleaksignore$",
    r"(?i)\.(?:bmp|gif|jpe?g|png|svg|tiff?|ico|webp)$",
    r"(?i)\.(?:eot|[ot]tf|woff2?)$",
    r"(?i)\.(?:docx?|xlsx?|pdf|bin|socket|vsidx|v2|suo|wsuo|dll|pdb|exe|gltf)$",
    r"go\.(?:mod|sum|work(?:\.sum)?)$",
    r"(?:^|/)vendor/modules\.txt$",
    r"(?:^|/)gradlew(?:\.bat)?$",
    r"(?:^|/)gradle\.lockfile$",
    r"(?:^|/)mvnw(?:\.cmd)?$",
    r"(?:^|/)node_modules(?:/.*)?$",
    r"(?:^|/)(?:deno\.lock|npm-shrinkwrap\.json|package-lock\.json|pnpm-lock\.yaml|yarn\.lock|Cargo\.lock|composer\.lock|Gemfile\.lock)$",
    r"(?:^|/)bower_components(?:/.*)?$",
    r"(?:^|/)(?:angular|bootstrap|jquery(?:-?ui)?|plotly|swagger-?ui)[a-zA-Z0-9.-]*(?:\.min)?\.js(?:\.map)?$",
    r"(?:^|/)(?:Pipfile|poetry)\.lock$",
    r"(?i)(?:^|/)(?:v?env|virtualenv)/lib(?:64)?(?:/.*)?$",
    r"(?:^|/)vendor/(?:bundle|ruby)(?:/.*?)?$",
    r"\.gem$",
];

struct Compiled {
    spec: &'static RuleSpec,
    regex: Regex,
    allow: Option<Regex>,
}

static COMPILED: LazyLock<Vec<Compiled>> = LazyLock::new(|| {
    RULES
        .iter()
        .map(|spec| Compiled {
            spec,
            regex: Regex::new(spec.regex).expect("secret rule regex"),
            allow: spec
                .allow
                .map(|a| Regex::new(a).expect("secret allow regex")),
        })
        .collect()
});

/// Every rule keyword in one automaton (case-insensitive), so a line is searched once to know
/// which rules can match it. `KEYWORD_RULE[pattern]` is the index of the rule in [`RULES`].
static KEYWORDS: LazyLock<(AhoCorasick, Vec<usize>)> = LazyLock::new(|| {
    let mut patterns = Vec::new();
    let mut owner = Vec::new();
    for (i, spec) in RULES.iter().enumerate() {
        for k in spec.keywords {
            patterns.push(*k);
            owner.push(i);
        }
    }
    let ac = AhoCorasick::builder()
        .ascii_case_insensitive(true)
        .build(patterns)
        .expect("keyword automaton");
    (ac, owner)
});

/// Bit `i` set: rule `i` has a keyword in `text`.
fn candidate_rules(text: &str) -> u64 {
    let (ac, owner) = &*KEYWORDS;
    ac.find_overlapping_iter(text)
        .fold(0, |mask, m| mask | 1 << owner[m.pattern().as_usize()])
}

static PATHS_ALLOWED: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    PATH_ALLOWLIST
        .iter()
        .map(|p| Regex::new(p).expect("path allowlist regex"))
        .collect()
});

static PRIVATE_KEY_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)-----BEGIN[ A-Z0-9_-]{0,100}PRIVATE KEY(?: BLOCK)?-----")
        .expect("private key regex")
});

/// Base64 body characters of a PEM block.
static PEM_BODY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z0-9+/=]{8,}").expect("pem body regex"));

/// Compiles every rule now (first use otherwise pays it, ~a few ms).
pub fn warm_up() {
    LazyLock::force(&COMPILED);
    LazyLock::force(&KEYWORDS);
    LazyLock::force(&PATHS_ALLOWED);
    LazyLock::force(&PRIVATE_KEY_HEADER);
}

/// The path is on the gitleaks global allowlist (lock files, vendored code, media).
pub fn path_allowed(path: &str) -> bool {
    PATHS_ALLOWED.iter().any(|r| r.is_match(path))
}

/// Shannon entropy in bits per character.
pub fn shannon_entropy(s: &str) -> f64 {
    let mut counts = std::collections::HashMap::<char, usize>::new();
    let mut n = 0usize;
    for c in s.chars() {
        *counts.entry(c).or_default() += 1;
        n += 1;
    }
    if n == 0 {
        return 0.0;
    }
    let n = n as f64;
    counts
        .values()
        .map(|&c| {
            let p = c as f64 / n;
            -p * p.log2()
        })
        .sum()
}

/// Masked form of a secret for display: a short head, `****`, and the last two characters of
/// long secrets. Never more than a quarter of the secret (and at most 8 characters) is shown.
pub fn mask(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    let n = chars.len();
    if n < 12 {
        return "********".to_owned();
    }
    let head = (n / 4).min(8);
    let tail = if n >= 24 { 2 } else { 0 };
    let mut out: String = chars[..head].iter().collect();
    out.push_str("****");
    out.extend(&chars[n - tail..]);
    out
}

/// Stable id of a finding for "always ignore": a salted SHA-256 of rule, path and secret
/// (32 hex characters). It does not reveal the secret and survives the line moving.
pub fn fingerprint(rule_id: &str, path: &str, secret: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"nanomgg-secret-guard\0");
    h.update(rule_id.as_bytes());
    h.update([0]);
    h.update(path.as_bytes());
    h.update([0]);
    h.update(secret.as_bytes());
    h.finalize()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

// ----- added lines -----------------------------------------------------------------------------

/// The lines a commit adds to one file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddedFile {
    /// Repository-relative path with `/` separators.
    pub path: String,
    /// `(line number in the new file, text)`, text cut at [`MAX_LINE_CHARS`].
    pub lines: Vec<(u32, String)>,
    pub binary: bool,
}

/// Undoes git's C-style quoting of a path (`"a\tb"`, octal escapes for non-ASCII bytes).
pub fn unquote_path(raw: &str) -> String {
    let Some(inner) = raw.strip_prefix('"').and_then(|r| r.strip_suffix('"')) else {
        return raw.to_owned();
    };
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' || i + 1 >= bytes.len() {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let c = bytes[i + 1];
        i += 2;
        match c {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'a' => out.push(7),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'v' => out.push(11),
            b'0'..=b'7' => {
                let mut v = u32::from(c - b'0');
                let mut digits = 1;
                while digits < 3 && i < bytes.len() && (b'0'..=b'7').contains(&bytes[i]) {
                    v = v * 8 + u32::from(bytes[i] - b'0');
                    i += 1;
                    digits += 1;
                }
                out.push(v as u8);
            }
            other => out.push(other),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `+++ b/<path>` gives `<path>` (git adds a tab after names containing a space).
fn new_path(header: &str) -> Option<String> {
    let rest = header.strip_prefix("+++ ")?.trim_end_matches(['\t', '\r']);
    if rest == "/dev/null" {
        return None;
    }
    let path = unquote_path(rest);
    Some(path.strip_prefix("b/").unwrap_or(&path).to_owned())
}

/// `@@ -a,b +c,d @@` gives `c`.
fn hunk_new_start(line: &str) -> Option<u32> {
    let plus = line.strip_prefix("@@ ")?.split(' ').nth(1)?;
    plus.strip_prefix('+')?.split(',').next()?.parse().ok()
}

fn cut(line: &str) -> String {
    match line.char_indices().nth(MAX_LINE_CHARS) {
        Some((i, _)) => line[..i].to_owned(),
        None => line.to_owned(),
    }
}

/// Parses `git diff -U0` output into the added lines of each file.
pub fn parse_added(diff: &[u8]) -> Vec<AddedFile> {
    let text = String::from_utf8_lossy(diff);
    let mut files: Vec<AddedFile> = Vec::new();
    let mut next_line: Option<u32> = None;
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.starts_with("diff --git ") {
            files.push(AddedFile::default());
            next_line = None;
            continue;
        }
        let Some(file) = files.last_mut() else {
            continue;
        };
        if next_line.is_none() {
            if let Some(path) = line.strip_prefix("+++ ").and_then(|_| new_path(line)) {
                file.path = path;
            } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
                file.binary = true;
            }
        }
        if line.starts_with("@@ ") {
            next_line = hunk_new_start(line);
            continue;
        }
        let Some(n) = next_line.as_mut() else {
            continue;
        };
        if let Some(added) = line.strip_prefix('+') {
            file.lines.push((*n, cut(added)));
            *n += 1;
        } else if line.starts_with(' ') {
            *n += 1;
        }
    }
    files.retain(|f| !f.path.is_empty());
    files
}

/// `git diff --cached -U0` of the repository, read up to [`MAX_DIFF_BYTES`]. Returns the bytes
/// and whether the output was cut.
pub fn staged_diff(git: &GitRunner, repo: &Path) -> Result<(Vec<u8>, bool), GitError> {
    let args = [
        "-c",
        "core.quotePath=false",
        "diff",
        "--cached",
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--ignore-submodules",
        "-U0",
        "-M",
        "--diff-filter=d",
        "--src-prefix=a/",
        "--dst-prefix=b/",
    ];
    let mut child = git
        .command(repo, &AccountContext::default(), args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut out = Vec::new();
    let stdout = child.stdout.take().expect("piped stdout");
    stdout
        .take(MAX_DIFF_BYTES as u64 + 1)
        .read_to_end(&mut out)?;
    if out.len() > MAX_DIFF_BYTES {
        out.truncate(MAX_DIFF_BYTES);
        // The rest is not needed: stop git instead of waiting for a huge diff.
        let _ = child.kill();
        let _ = child.wait();
        return Ok((out, true));
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(GitError::Failed {
            args: "diff --cached".to_owned(),
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    Ok((out, false))
}

// ----- scanning --------------------------------------------------------------------------------

/// What the guard reports about one secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SecretFinding {
    /// gitleaks rule id.
    pub rule_id: String,
    /// Short English label of the rule.
    pub rule_name: String,
    pub path: String,
    /// Line in the new version of the file (1-based).
    pub line: u32,
    /// The secret with most of it hidden (see [`mask`]).
    pub masked: String,
    /// See [`fingerprint`].
    pub fingerprint: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SecretReport {
    pub findings: Vec<SecretFinding>,
    pub files_scanned: u32,
    /// Findings skipped because of `gitleaks:allow`, `.gitleaksignore` or a stored ignore.
    pub ignored: u32,
    /// Part of the staged diff was not scanned (size cap or too many findings).
    pub truncated: bool,
}

/// What to leave out.
#[derive(Debug, Clone, Default)]
pub struct Ignores {
    /// `file:rule-id:line` entries from `.gitleaksignore`.
    pub gitleaks: HashSet<String>,
    /// Fingerprints the user chose to always ignore in this repository.
    pub fingerprints: HashSet<String>,
}

impl Ignores {
    /// Global entries of a `.gitleaksignore` file (`file:rule-id:line`). Commit-specific
    /// entries (`commit:file:rule-id:line`) cannot match a commit that does not exist yet.
    pub fn parse_gitleaksignore(text: &str) -> HashSet<String> {
        text.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .filter_map(|l| {
                let parts: Vec<&str> = l.split(':').collect();
                (parts.len() == 3)
                    .then(|| format!("{}:{}:{}", parts[0].replace('\\', "/"), parts[1], parts[2]))
            })
            .collect()
    }

    /// Reads `<repo>/.gitleaksignore` when present.
    pub fn load_gitleaksignore(repo: &Path) -> HashSet<String> {
        std::fs::read_to_string(repo.join(IGNORE_FILE))
            .map(|t| Self::parse_gitleaksignore(&t))
            .unwrap_or_default()
    }
}

/// A match before ignores are applied. Holds the clear secret: never log or serialize it.
struct Hit {
    rule: &'static RuleSpec,
    line: u32,
    secret: String,
    masked: String,
}

/// First capture group that took part in the match, else the whole match (gitleaks' default
/// `secretGroup`).
fn secret_of<'h>(caps: &regex::Captures<'h>) -> &'h str {
    (1..caps.len())
        .find_map(|i| caps.get(i))
        .or_else(|| caps.get(0))
        .map_or("", |m| m.as_str())
}

const PRIVATE_KEY_SPEC: RuleSpec = rule(PRIVATE_KEY_RULE, "Private key", &[], "", None);

/// Private keys span lines: a `-----BEGIN ... PRIVATE KEY-----` header followed by at least 64
/// characters of base64 body (on the same line, e.g. `\n`-escaped in JSON, or the next lines).
fn private_key_hits(lines: &[(u32, String)], out: &mut Vec<Hit>) {
    for (i, (n, text)) in lines.iter().enumerate() {
        if !text.contains("-----") {
            continue;
        }
        let Some(m) = PRIVATE_KEY_HEADER.find(text) else {
            continue;
        };
        if text.contains(ALLOW_MARKER) {
            continue;
        }
        let mut body = String::new();
        let rest = &text[m.end()..];
        let following = lines[i + 1..]
            .iter()
            .take_while(|(next, _)| *next <= n + 4)
            .take(4)
            .map(|(_, t)| t.as_str());
        for chunk in std::iter::once(rest).chain(following) {
            for b in PEM_BODY.find_iter(chunk) {
                body.push_str(b.as_str());
            }
            if body.len() >= 64 || chunk.contains("-----END") {
                break;
            }
        }
        if body.len() < 64 {
            continue;
        }
        let key: String = body.chars().take(64).collect();
        out.push(Hit {
            rule: &PRIVATE_KEY_SPEC,
            line: *n,
            masked: format!("{} ****", m.as_str()),
            secret: key,
        });
    }
}

/// Secrets in the added lines of one file (before ignores).
fn file_hits(file: &AddedFile) -> Vec<Hit> {
    let mut hits = Vec::new();
    private_key_hits(&file.lines, &mut hits);
    for (n, text) in &file.lines {
        if text.len() < 8 || text.contains(ALLOW_MARKER) {
            continue;
        }
        let candidates = candidate_rules(text);
        if candidates == 0 {
            continue;
        }
        for (i, c) in COMPILED.iter().enumerate() {
            if candidates & (1 << i) == 0 {
                continue;
            }
            for caps in c.regex.captures_iter(text) {
                let secret = secret_of(&caps);
                if secret.is_empty()
                    || c.spec.entropy.is_some_and(|e| shannon_entropy(secret) < e)
                    || c.allow.as_ref().is_some_and(|a| a.is_match(secret))
                {
                    continue;
                }
                // One report per secret on a line, from the first rule that matched it.
                if hits.iter().any(|h: &Hit| {
                    h.line == *n && (h.secret.contains(secret) || secret.contains(&h.secret))
                }) {
                    continue;
                }
                hits.push(Hit {
                    rule: c.spec,
                    line: *n,
                    masked: mask(secret),
                    secret: secret.to_owned(),
                });
            }
        }
    }
    hits.sort_by_key(|h| h.line);
    hits
}

/// Scans parsed added lines. `truncated` is carried into the report.
pub fn scan_files(files: &[AddedFile], ignores: &Ignores, truncated: bool) -> SecretReport {
    let mut report = SecretReport {
        truncated,
        ..Default::default()
    };
    for file in files {
        if file.binary || file.lines.is_empty() || path_allowed(&file.path) {
            continue;
        }
        report.files_scanned += 1;
        for hit in file_hits(file) {
            let global = format!("{}:{}:{}", file.path, hit.rule.id, hit.line);
            let fp = fingerprint(hit.rule.id, &file.path, &hit.secret);
            if ignores.gitleaks.contains(&global) || ignores.fingerprints.contains(&fp) {
                report.ignored += 1;
                continue;
            }
            if report.findings.len() >= MAX_FINDINGS {
                report.truncated = true;
                return report;
            }
            report.findings.push(SecretFinding {
                rule_id: hit.rule.id.to_owned(),
                rule_name: hit.rule.name.to_owned(),
                path: file.path.clone(),
                line: hit.line,
                masked: hit.masked,
                fingerprint: fp,
            });
        }
    }
    report
}

/// Scans the staged changes of `repo` (the Secret Guard check run before a commit).
pub fn scan_staged(
    git: &GitRunner,
    repo: &Path,
    fingerprints: HashSet<String>,
) -> Result<SecretReport, GitError> {
    let (diff, truncated) = staged_diff(git, repo)?;
    let files = parse_added(&diff);
    let ignores = Ignores {
        gitleaks: Ignores::load_gitleaksignore(repo),
        fingerprints,
    };
    Ok(scan_files(&files, &ignores, truncated))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic pseudo-random text over `alphabet`. Test secrets are built at run time so
    /// this file itself never contains a token a scanner (or GitHub push protection) would flag.
    fn noise(len: usize, alphabet: &str, seed: u64) -> String {
        let chars: Vec<char> = alphabet.chars().collect();
        let mut x = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (0..len)
            .map(|_| {
                x = x
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                chars[((x >> 33) % chars.len() as u64) as usize]
            })
            .collect()
    }

    const ALNUM: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const LOWER_NUM: &str = "abcdefghijklmnopqrstuvwxyz0123456789";
    const HEX: &str = "0123456789abcdef";
    const UPPER_B32: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

    fn ids(text: &str) -> Vec<String> {
        let file = AddedFile {
            path: "src/config.ts".into(),
            lines: text
                .lines()
                .enumerate()
                .map(|(i, l)| (i as u32 + 1, l.to_owned()))
                .collect(),
            binary: false,
        };
        scan_files(&[file], &Ignores::default(), false)
            .findings
            .into_iter()
            .map(|f| f.rule_id)
            .collect()
    }

    fn found(text: &str, id: &str) {
        let got = ids(text);
        assert!(got.iter().any(|g| g == id), "{id} not found, got {got:?}");
    }

    fn clean(text: &str) {
        let got = ids(text);
        assert!(got.is_empty(), "expected nothing, got {got:?}");
    }

    #[test]
    fn every_rule_compiles() {
        warm_up();
        assert_eq!(COMPILED.len(), RULES.len());
        // Candidate rules are a 64-bit mask.
        assert!(RULES.len() <= 64);
        let mut seen = HashSet::new();
        for r in RULES {
            assert!(seen.insert(r.id), "duplicate rule id {}", r.id);
            assert!(!r.keywords.is_empty(), "{} has no keyword", r.id);
            for k in r.keywords {
                assert_eq!(
                    *k,
                    k.to_ascii_lowercase(),
                    "{} keyword not lower case",
                    r.id
                );
            }
        }
    }

    #[test]
    fn finds_cloud_and_forge_tokens() {
        found(
            &format!("aws_key = \"AKIA{}\"", noise(16, UPPER_B32, 1)),
            "aws-access-token",
        );
        found(&format!("token: ghp_{}", noise(36, ALNUM, 2)), "github-pat");
        found(
            &format!("GH=github_pat_{}", noise(82, ALNUM, 3)),
            "github-fine-grained-pat",
        );
        found(
            &format!("x = 'gho_{}'", noise(36, ALNUM, 4)),
            "github-oauth",
        );
        found(&format!("ghs_{}", noise(36, ALNUM, 5)), "github-app-token");
        found(&format!("glpat-{}", noise(20, ALNUM, 6)), "gitlab-pat");
        found(
            &format!("glrt-{}", noise(20, ALNUM, 7)),
            "gitlab-runner-authentication-token",
        );
        found(
            &format!("key=\"AIza{}\"", noise(35, ALNUM, 8)),
            "gcp-api-key",
        );
    }

    #[test]
    fn finds_chat_payment_and_ai_keys() {
        found(
            &format!(
                "SLACK=xoxb-{}-{}-{}",
                noise(12, "0123456789", 9),
                noise(12, "0123456789", 10),
                noise(24, ALNUM, 11)
            ),
            "slack-bot-token",
        );
        found(
            &format!(
                "url = \"https://hooks.slack.com/services/{}\"",
                noise(44, ALNUM, 12)
            ),
            "slack-webhook-url",
        );
        found(
            &format!("stripe = \"{}_live_{}\"", "sk", noise(24, ALNUM, 13)),
            "stripe-access-token",
        );
        found(
            &format!(
                "OPENAI_API_KEY=sk-proj-{}T3BlbkFJ{}",
                noise(74, ALNUM, 14),
                noise(74, ALNUM, 15)
            ),
            "openai-api-key",
        );
        found(
            &format!(
                "ANTHROPIC_API_KEY={}-api03-{}AA",
                "sk-ant",
                noise(93, ALNUM, 16)
            ),
            "anthropic-api-key",
        );
        found(
            &format!(
                "//registry.npmjs.org/:_authToken=npm_{}",
                noise(36, LOWER_NUM, 17)
            ),
            "npm-access-token",
        );
        found(
            &format!("password = pypi-AgEIcHlwaS5vcmc{}", noise(60, ALNUM, 18)),
            "pypi-upload-token",
        );
        found(
            &format!(
                "Authorization: Bearer eyJ{}.eyJ{}.{}",
                noise(30, ALNUM, 19),
                noise(40, ALNUM, 20),
                noise(43, ALNUM, 21)
            ),
            "jwt",
        );
        found(
            &format!("dop_v1_{}", noise(64, HEX, 22)),
            "digitalocean-pat",
        );
    }

    #[test]
    fn ignores_look_alikes() {
        // Too short, wrong alphabet, placeholders, documentation examples.
        clean("const prefix = \"ghp_\";");
        clean(&format!("ghp_{}", "x".repeat(36)));
        clean("aws_access_key_id = AKIAIOSFODNN7EXAMPLE");
        clean(&format!("AKIA{}", "A".repeat(16)));
        clean("sk_live_short");
        clean("see https://hooks.slack.com/services/ for webhooks");
        clean("let jwt = \"eyJhbGciOiJIUzI1NiJ9\";");
        clean("glpat-xxxxxxxxxxxxxxxxxxxx");
        clean("const re = /-----BEGIN RSA PRIVATE KEY-----/;");
        clean("xoxb-not-a-token");
        clean("export const STRIPE_PREFIX = 'sk_test_';");
    }

    #[test]
    fn private_key_needs_a_body() {
        let body: Vec<String> = (0..4).map(|i| noise(64, ALNUM, 30 + i)).collect();
        let pem = format!(
            "-----BEGIN RSA PRIVATE KEY-----\n{}\n-----END RSA PRIVATE KEY-----",
            body.join("\n")
        );
        found(&pem, PRIVATE_KEY_RULE);
        // `\n`-escaped in a JSON value, all on one line.
        found(
            &format!(
                "\"private_key\": \"-----BEGIN PRIVATE KEY-----\\n{}\\n-----END PRIVATE KEY-----\\n\"",
                body.join("\\n")
            ),
            PRIVATE_KEY_RULE,
        );
        clean("-----BEGIN OPENSSH PRIVATE KEY-----\n-----END OPENSSH PRIVATE KEY-----");
        clean("-----BEGIN PUBLIC KEY-----\nAAAA\n-----END PUBLIC KEY-----");
    }

    #[test]
    fn reports_one_finding_per_secret_and_the_right_line() {
        let token = format!(
            "xoxb-{}-{}-{}",
            noise(12, "0123456789", 40),
            noise(12, "0123456789", 41),
            noise(24, ALNUM, 42)
        );
        let file = AddedFile {
            path: "app/.env".into(),
            lines: vec![(10, "A=1".into()), (11, format!("SLACK={token}"))],
            binary: false,
        };
        let r = scan_files(&[file], &Ignores::default(), false);
        assert_eq!(r.findings.len(), 1, "{:?}", r.findings);
        let f = &r.findings[0];
        assert_eq!((f.path.as_str(), f.line), ("app/.env", 11));
        assert!(!f.masked.contains(&token));
        assert_eq!(f.fingerprint, fingerprint(&f.rule_id, "app/.env", &token));
    }

    #[test]
    fn masking_hides_most_of_the_secret() {
        assert_eq!(mask("short"), "********");
        let t = format!("ghp_{}", noise(36, ALNUM, 50));
        let m = mask(&t);
        assert!(m.starts_with("ghp_"));
        assert!(m.contains("****"));
        assert!(m.ends_with(&t[t.len() - 2..]));
        let visible = m.chars().filter(|c| *c != '*').count();
        assert!(visible <= t.len() / 4 + 2, "{m}");
        let aws = format!("AKIA{}", noise(16, UPPER_B32, 51));
        assert_eq!(mask(&aws), format!("{}****", &aws[..5]));
        // Non-ASCII does not panic.
        assert_eq!(
            mask("ééééééééééééé").chars().filter(|c| *c == '*').count(),
            4
        );
    }

    #[test]
    fn gitleaks_allow_and_ignore_files() {
        let token = format!("ghp_{}", noise(36, ALNUM, 60));
        clean(&format!("t = \"{token}\" # gitleaks:allow"));

        let ignore = Ignores::parse_gitleaksignore(
            "# comment\n\nsrc\\config.ts:github-pat:2\nabc123:src/a.ts:github-pat:1\nbad\n",
        );
        assert!(ignore.contains("src/config.ts:github-pat:2"));
        assert_eq!(ignore.len(), 1);

        let file = AddedFile {
            path: "src/config.ts".into(),
            lines: vec![(1, format!("a={token}")), (2, format!("b={token}x"))],
            binary: false,
        };
        let ignores = Ignores {
            gitleaks: ignore,
            fingerprints: HashSet::new(),
        };
        let r = scan_files(std::slice::from_ref(&file), &ignores, false);
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].line, 1);
        assert_eq!(r.ignored, 1);

        // "Always ignore": the stored fingerprint survives the line moving.
        let fp = r.findings[0].fingerprint.clone();
        let moved = AddedFile {
            lines: vec![(7, format!("a={token}"))],
            ..file
        };
        let ignores = Ignores {
            gitleaks: HashSet::new(),
            fingerprints: [fp].into(),
        };
        let r = scan_files(&[moved], &ignores, false);
        assert!(r.findings.is_empty());
        assert_eq!(r.ignored, 1);
    }

    #[test]
    fn skips_binary_and_allowlisted_paths() {
        let token = format!("ghp_{}", noise(36, ALNUM, 70));
        let file = |path: &str, binary| AddedFile {
            path: path.into(),
            lines: vec![(1, token.clone())],
            binary,
        };
        let r = scan_files(
            &[
                file("pnpm-lock.yaml", false),
                file("web/node_modules/x/index.js", false),
                file("logo.png", false),
                file("data.bin2", true),
                file("src/main.rs", false),
            ],
            &Ignores::default(),
            false,
        );
        assert_eq!(r.files_scanned, 1);
        assert_eq!(r.findings.len(), 1);
        assert_eq!(r.findings[0].path, "src/main.rs");
    }

    #[test]
    fn parses_added_lines_with_numbers_and_paths() {
        let diff = "diff --git a/a.txt b/a.txt\n\
index 1..2 100644\n\
--- a/a.txt\n\
+++ b/a.txt\n\
@@ -3,0 +4,2 @@ ctx\n\
+one\n\
+two\r\n\
@@ -10 +12 @@\n\
-old\n\
+new\n\
\\ No newline at end of file\n\
diff --git a/my file.txt b/my file.txt\n\
new file mode 100644\n\
--- /dev/null\n\
+++ b/my file.txt\t\n\
@@ -0,0 +1 @@\n\
+hello\n\
diff --git a/img.png b/img.png\n\
Binary files a/img.png and b/img.png differ\n\
diff --git \"a/t\\303\\251\\\"st\" \"b/t\\303\\251\\\"st\"\n\
--- \"a/t\\303\\251\\\"st\"\n\
+++ \"b/t\\303\\251\\\"st\"\n\
@@ -0,0 +1 @@\n\
++++x\n";
        let files = parse_added(diff.as_bytes());
        assert_eq!(files.len(), 3, "{files:?}");
        assert_eq!(files[0].path, "a.txt");
        assert_eq!(
            files[0].lines,
            vec![(4, "one".into()), (5, "two".into()), (12, "new".into())]
        );
        assert_eq!(files[1].path, "my file.txt");
        assert_eq!(files[1].lines, vec![(1, "hello".into())]);
        assert_eq!(files[2].path, "t\u{e9}\"st");
        assert_eq!(files[2].lines, vec![(1, "+++x".into())]);
        // The binary file has no `+++` header, so it has no path and is dropped.
    }

    #[test]
    fn long_lines_are_cut() {
        let long = "a".repeat(MAX_LINE_CHARS * 2);
        let diff = format!("diff --git a/x b/x\n+++ b/x\n@@ -0,0 +1 @@\n+{long}\n");
        let files = parse_added(diff.as_bytes());
        assert_eq!(files[0].lines[0].1.len(), MAX_LINE_CHARS);
    }

    #[test]
    fn entropy() {
        assert_eq!(shannon_entropy(""), 0.0);
        assert_eq!(shannon_entropy("aaaa"), 0.0);
        assert!((shannon_entropy("abcd") - 2.0).abs() < 1e-9);
    }

    /// Timing of a full-size scan: `cargo test -p nanomgg-git-engine --release secrets::tests::
    /// scan_speed -- --ignored --nocapture`.
    #[test]
    #[ignore = "timing only"]
    fn scan_speed() {
        let line =
            "    let key = config.get(\"api_key\").unwrap_or_default(); // token for the server";
        let mut diff = String::from("diff --git a/a.rs b/a.rs\n+++ b/a.rs\n@@ -0,0 +1,100000 @@\n");
        while diff.len() < MAX_DIFF_BYTES {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
        let start = std::time::Instant::now();
        let files = parse_added(diff.as_bytes());
        let parsed = start.elapsed();
        let report = scan_files(&files, &Ignores::default(), false);
        eprintln!(
            "scanned {} lines ({} bytes): parse {:?}, total {:?}",
            files[0].lines.len(),
            diff.len(),
            parsed,
            start.elapsed()
        );
        assert!(report.findings.is_empty());
    }

    #[test]
    fn scans_a_real_staged_diff() {
        let dir = tempfile::tempdir().unwrap();
        let git = GitRunner::default();
        let ctx = AccountContext::default();
        let p = dir.path();
        git.run(p, &ctx, ["init", "-q"]).unwrap();
        let token = format!("ghp_{}", noise(36, ALNUM, 80));
        std::fs::write(p.join("notes.txt"), "hello\n").unwrap();
        std::fs::write(p.join("config.env"), format!("A=1\nTOKEN={token}\n")).unwrap();
        std::fs::write(p.join("blob.bin"), [0u8, 1, 2, 3, b'g', b'h', b'p']).unwrap();
        git.run(p, &ctx, ["add", "."]).unwrap();
        // No commit yet: `diff --cached` compares with the empty tree.
        let r = scan_staged(&git, p, HashSet::new()).unwrap();
        assert_eq!(r.findings.len(), 1, "{r:?}");
        assert_eq!(r.findings[0].path, "config.env");
        assert_eq!(r.findings[0].line, 2);
        assert!(!r.truncated);

        // `.gitleaksignore` in the working tree.
        std::fs::write(p.join(IGNORE_FILE), "config.env:github-pat:2\n").unwrap();
        let r = scan_staged(&git, p, HashSet::new()).unwrap();
        assert!(r.findings.is_empty());
        assert_eq!(r.ignored, 1);
    }
}
