//! "Gợi ý mô tả" in Publish: one sentence for the repository description on GitHub / GitLab,
//! written by the AI service from what the project contains.
//!
//! The facts are small and never contain code: the repository name, the first lines of the
//! README, the `description` of a package manifest, the most common file types, the top-level
//! entries and recent commit subjects.

use std::collections::HashMap;
use std::path::Path;

use nanomgg_git_engine::GitRunner;

use super::context::git_capped;
use super::prompt::{strip_fences, strip_think};

/// GitHub's limit (GitLab allows more).
pub const DESCRIPTION_MAX: usize = 350;
const README_CHARS: usize = 2_500;
const TOP_ENTRIES: usize = 60;
const LIST_CAP: usize = 2 * 1024 * 1024;
pub const MAX_TOKENS: u32 = 300;

const README_NAMES: &[&str] = &[
    "README.md",
    "README",
    "README.txt",
    "README.rst",
    "readme.md",
    "Readme.md",
];
/// Manifests whose `description` says what the project is.
const MANIFESTS: &[&str] = &[
    "package.json",
    "Cargo.toml",
    "pyproject.toml",
    "composer.json",
    "pubspec.yaml",
];
/// Files that tell the kind of project on their own (engines, app frameworks).
const MARKERS: &[(&str, &str)] = &[
    ("ProjectSettings/ProjectVersion.txt", "Unity project"),
    ("project.godot", "Godot project"),
    ("pubspec.yaml", "Flutter / Dart project"),
    ("src-tauri/tauri.conf.json", "Tauri desktop app"),
    ("next.config.js", "Next.js web app"),
    ("next.config.mjs", "Next.js web app"),
    ("angular.json", "Angular web app"),
    ("go.mod", "Go module"),
    ("pom.xml", "Java (Maven) project"),
    ("build.gradle", "Gradle project"),
    ("Dockerfile", "has a Dockerfile"),
];

/// Value of `description` in a JSON / TOML / YAML manifest, without parsing the whole file.
pub fn manifest_description(text: &str) -> Option<String> {
    for line in text.lines() {
        let t = line.trim().trim_start_matches('"');
        let Some(rest) = t.strip_prefix("description") else {
            continue;
        };
        let rest = rest.trim_start_matches('"').trim_start();
        let Some(rest) = rest.strip_prefix([':', '=']) else {
            continue;
        };
        let value = rest
            .trim()
            .trim_end_matches(',')
            .trim()
            .trim_matches(['"', '\''])
            .trim();
        if !value.is_empty() && value.chars().count() <= 500 {
            return Some(value.to_owned());
        }
    }
    None
}

fn read_text(root: &Path, rel: &str, max: usize) -> Option<String> {
    let bytes = std::fs::read(root.join(rel)).ok()?;
    let text = String::from_utf8_lossy(&bytes[..bytes.len().min(max * 4)]).into_owned();
    Some(text.chars().take(max).collect())
}

/// Facts sent to the model.
pub fn collect_facts(git: &GitRunner, root: &Path, name: &str) -> String {
    let mut lines = vec![format!("Repository name: {name}")];
    let files: Vec<String> = git_capped(
        git,
        root,
        &[
            "-c",
            "core.quotePath=false",
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        LIST_CAP,
    )
    .ok()
    .flatten()
    .unwrap_or_default()
    .split('\0')
    .filter(|s| !s.is_empty())
    .map(|s| s.replace('\\', "/"))
    .collect();

    let kinds: Vec<&str> = MARKERS
        .iter()
        .filter(|(file, _)| files.iter().any(|f| f == file))
        .map(|(_, kind)| *kind)
        .collect();
    let uproject = files
        .iter()
        .any(|f| !f.contains('/') && f.ends_with(".uproject"));
    let mut kinds = kinds;
    if uproject {
        kinds.push("Unreal Engine project");
    }
    if !kinds.is_empty() {
        lines.push(format!("Looks like: {}", kinds.join(", ")));
    }

    for manifest in MANIFESTS {
        if files.iter().any(|f| f == manifest)
            && let Some(desc) = read_text(root, manifest, 20_000)
                .as_deref()
                .and_then(manifest_description)
        {
            lines.push(format!("{manifest} description: {desc}"));
        }
    }

    let mut by_ext: HashMap<String, usize> = HashMap::new();
    for f in &files {
        let file = f.rsplit('/').next().unwrap_or(f);
        if let Some((stem, ext)) = file.rsplit_once('.')
            && !stem.is_empty()
            && ext.len() <= 10
        {
            *by_ext.entry(ext.to_ascii_lowercase()).or_default() += 1;
        }
    }
    let mut exts: Vec<(String, usize)> = by_ext.into_iter().collect();
    exts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    if !exts.is_empty() {
        let top: Vec<String> = exts
            .iter()
            .take(12)
            .map(|(e, n)| format!(".{e} ({n})"))
            .collect();
        lines.push(format!(
            "Files: {} in total; most common types: {}",
            files.len(),
            top.join(", ")
        ));
    }

    let mut top: Vec<String> = Vec::new();
    for f in &files {
        let entry = match f.split_once('/') {
            Some((dir, _)) => format!("{dir}/"),
            None => f.clone(),
        };
        if !top.contains(&entry) {
            top.push(entry);
        }
        if top.len() >= TOP_ENTRIES {
            break;
        }
    }
    if !top.is_empty() {
        lines.push(format!("Top-level entries: {}", top.join(", ")));
    }

    let subjects = git_capped(
        git,
        root,
        &["log", "-12", "--no-merges", "--format=%s"],
        64 * 1024,
    )
    .ok()
    .flatten()
    .unwrap_or_default();
    let subjects: Vec<&str> = subjects.lines().filter(|l| !l.trim().is_empty()).collect();
    if !subjects.is_empty() {
        lines.push(format!("Recent commits: {}", subjects.join(" | ")));
    }

    if let Some(readme) = README_NAMES
        .iter()
        .find(|n| files.iter().any(|f| f == *n))
        .and_then(|n| read_text(root, n, README_CHARS))
    {
        let readme = readme.trim();
        if !readme.is_empty() {
            lines.push(String::new());
            lines.push("README (start):".into());
            lines.push(readme.to_owned());
        }
    }
    lines.join("\n")
}

pub fn system_prompt() -> String {
    "You write the one-line description shown under a repository's name on GitHub or GitLab. \
     It tells a visitor what the project is and what it does."
        .into()
}

pub fn user_prompt(facts: &str, language: &str) -> String {
    format!(
        "Write the description in {language}: one sentence, at most 120 characters, saying what \
         this project is and does. Plain text only: no quotes, no emoji, no markdown, no \
         dashes to join clauses, no final period, do not start with \"This repository\" or the \
         repository name. When the facts are thin, describe it from its file types and names. \
         Reply with the sentence only.\n\nFacts:\n{facts}"
    )
}

/// The description in the model's answer: first real line, labels / quotes / markdown and
/// long dashes removed, cut to [`DESCRIPTION_MAX`] characters. `None` when nothing is left.
pub fn parse_description(raw: &str) -> Option<String> {
    let text = strip_fences(&strip_think(raw));
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.ends_with(':'))?;
    let mut s = line.trim_start_matches(['#', '>']).trim().to_owned();
    for bullet in ["- ", "* "] {
        if let Some(rest) = s.strip_prefix(bullet) {
            s = rest.trim().to_owned();
        }
    }
    for label in ["Description:", "description:", "Mô tả:", "Mo ta:"] {
        if let Some(rest) = s.strip_prefix(label) {
            s = rest.trim().to_owned();
        }
    }
    loop {
        let t = s.trim();
        let stripped = [
            ("\"", "\""),
            ("'", "'"),
            ("`", "`"),
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
    let s = s
        .replace(" \u{2014} ", ", ")
        .replace(" \u{2013} ", ", ")
        .replace(['\u{2014}', '\u{2013}'], ", ")
        .replace(" -- ", ", ");
    let mut s: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    while s.ends_with('.') && !s.ends_with("...") {
        s.pop();
    }
    if s.chars().count() > DESCRIPTION_MAX {
        let cut: String = s.chars().take(DESCRIPTION_MAX).collect();
        s = match cut.rfind(' ') {
            Some(sp) if sp > DESCRIPTION_MAX / 2 => {
                cut[..sp].trim_end_matches([',', ';', ':']).to_owned()
            }
            _ => cut,
        };
    }
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_manifest_descriptions() {
        let json = "{\n  \"name\": \"x\",\n  \"description\": \"A tiny CLI\",\n}";
        assert_eq!(manifest_description(json).as_deref(), Some("A tiny CLI"));
        let toml = "[package]\nname = \"x\"\ndescription = 'Fast parser'\n";
        assert_eq!(manifest_description(toml).as_deref(), Some("Fast parser"));
        assert_eq!(manifest_description("descriptionless = 1\n"), None);
        assert_eq!(manifest_description("{\"description\": \"\"}"), None);
    }

    #[test]
    fn cleans_the_answer() {
        assert_eq!(
            parse_description("<think>hmm</think>\n\"A 2D platformer made in Unity.\"").as_deref(),
            Some("A 2D platformer made in Unity")
        );
        assert_eq!(
            parse_description("Description: Git GUI \u{2014} many accounts at once").as_deref(),
            Some("Git GUI, many accounts at once")
        );
        assert_eq!(
            parse_description("```\nSample text\n```").as_deref(),
            Some("Sample text")
        );
        assert_eq!(
            parse_description("Here is a description:\n\n**Tool for X**").as_deref(),
            Some("Tool for X")
        );
        assert_eq!(parse_description("  \n<think>only"), None);
        let long = "word ".repeat(200);
        assert!(parse_description(&long).unwrap().chars().count() <= DESCRIPTION_MAX);
    }

    #[test]
    fn collects_facts_from_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let git = GitRunner::default();
        git.run(root, &Default::default(), ["init", "-q"]).unwrap();
        std::fs::create_dir_all(root.join("ProjectSettings")).unwrap();
        std::fs::write(
            root.join("ProjectSettings/ProjectVersion.txt"),
            "m_EditorVersion: 6",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("Assets/Scripts")).unwrap();
        std::fs::write(root.join("Assets/Scripts/Player.cs"), "class Player {}").unwrap();
        std::fs::write(root.join("README.md"), "# Space Hop\nA tiny jumping game.").unwrap();
        let facts = collect_facts(&git, root, "space-hop");
        assert!(facts.contains("Repository name: space-hop"), "{facts}");
        assert!(facts.contains("Unity project"), "{facts}");
        assert!(facts.contains(".cs (1)"), "{facts}");
        assert!(facts.contains("Assets/"), "{facts}");
        assert!(facts.contains("A tiny jumping game."), "{facts}");
    }
}
