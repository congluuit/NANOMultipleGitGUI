//! Phase 6.2 / 6.5: data the user creates inside the app — custom color themes and personal notes
//! ("Ghi chú của tôi"). Everything lives in the `settings` table (one JSON value per key), so the
//! full-app backup picks it up without a migration.
//!
//! Every read-modify-write goes through [`LOCK`]: commands run on worker threads and two quick
//! saves must not lose each other's change.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use nanomgg_storage::Storage;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::state::AppState;

const KEY_THEMES: &str = "theme.custom";
const KEY_ACTIVE_THEME: &str = "theme.activeCustom";
const KEY_NOTES: &str = "notes.items";
const KEY_TAGS: &str = "notes.tags";

const THEME_FILE_FORMAT: &str = "nanomgg-theme";
const NOTES_FILE_FORMAT: &str = "nanomgg-notes";
const FILE_VERSION: u32 = 1;

/// Built-in read-only themes (defined by the frontend); they can be active but never stored.
pub const PRESET_THEME_IDS: [&str; 2] = ["hc-dark", "hc-light"];
/// Built-in note tags (names are translated by the frontend).
pub const BUILTIN_TAG_IDS: [&str; 7] =
    ["default", "urgent", "wip", "review", "bug", "idea", "done"];

/// Color tokens a theme may set (`tokens.css`); anything else in a theme is dropped.
pub const THEME_TOKENS: [&str; 21] = [
    "--bg",
    "--surface",
    "--surface-2",
    "--border",
    "--text",
    "--text-muted",
    "--primary",
    "--primary-hover",
    "--primary-fg",
    "--success",
    "--warning",
    "--danger",
    "--info",
    "--lane-0",
    "--lane-1",
    "--lane-2",
    "--lane-3",
    "--lane-4",
    "--lane-5",
    "--lane-6",
    "--lane-7",
];

pub const MAX_THEMES: usize = 100;
pub const MAX_THEME_NAME: usize = 80;
pub const MAX_NOTES: usize = 5000;
pub const MAX_NOTE_TITLE: usize = 200;
pub const MAX_NOTE_BODY: usize = 20_000;
pub const MAX_NOTE_TAGS: usize = 20;
pub const MAX_USER_TAGS: usize = 100;
pub const MAX_TAG_NAME: usize = 40;
const MAX_REPO_PATH: usize = 4096;
const MAX_THEME_FILE: u64 = 1024 * 1024;
const MAX_NOTES_FILE: u64 = 10 * 1024 * 1024;

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

/// Trims and checks a required single-line text field.
fn clean_text(value: &str, max: usize, what: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid(format!("{what} is empty")));
    }
    if value.chars().count() > max {
        return Err(invalid(format!("{what} is longer than {max} characters")));
    }
    if value.chars().any(char::is_control) {
        return Err(invalid(format!("{what} contains control characters")));
    }
    Ok(value.to_owned())
}

/// `#rgb` / `#rrggbb` (any case) → lowercase `#rrggbb`.
pub fn normalize_hex(value: &str) -> Option<String> {
    let hex = value.trim().strip_prefix('#')?;
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        6 => Some(format!("#{}", hex.to_ascii_lowercase())),
        3 => Some(format!(
            "#{}",
            hex.chars()
                .flat_map(|c| [c, c])
                .collect::<String>()
                .to_ascii_lowercase()
        )),
        _ => None,
    }
}

// ----- themes ----------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ThemeBase {
    Light,
    Dark,
}

/// A theme the user made: a light or dark base plus overridden color tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub base: ThemeBase,
    /// `--token` → `#rrggbb`. Tokens left out fall back to the base theme.
    pub colors: BTreeMap<String, String>,
    /// Unix time in milliseconds.
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomThemeInput {
    /// `None` creates a new theme.
    #[serde(default)]
    #[specta(optional)]
    pub id: Option<String>,
    pub name: String,
    pub base: ThemeBase,
    pub colors: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomThemes {
    pub themes: Vec<CustomTheme>,
    /// Theme used when the theme mode is "custom": a preset id, a theme id or `None`.
    pub active_id: Option<String>,
}

/// Why a file chosen by the user could not be imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UserFileProblem {
    /// Could not read the file (gone, no permission).
    Unreadable,
    TooLarge,
    /// Not JSON at all (or not UTF-8).
    NotJson,
    /// JSON, but not a file this app exported (wrong `format`).
    WrongFormat,
    /// Exported by a newer version of the app.
    NewerVersion,
    /// The right kind of file, but the content is damaged or empty.
    Invalid,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ThemeImportResult {
    pub problem: Option<UserFileProblem>,
    /// The imported theme (saved under a new id).
    pub theme: Option<CustomTheme>,
}

#[derive(Serialize, Deserialize)]
struct ThemeFile {
    format: String,
    version: u32,
    theme: ThemeFileBody,
}

#[derive(Serialize, Deserialize)]
struct ThemeFileBody {
    name: String,
    base: ThemeBase,
    colors: BTreeMap<String, String>,
}

fn clean_colors(colors: &BTreeMap<String, String>) -> AppResult<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for (token, value) in colors {
        if !THEME_TOKENS.contains(&token.as_str()) {
            continue;
        }
        let hex = normalize_hex(value)
            .ok_or_else(|| invalid(format!("{token}: \"{value}\" is not a #rrggbb color")))?;
        out.insert(token.clone(), hex);
    }
    Ok(out)
}

fn load_themes(db: &Storage) -> AppResult<Vec<CustomTheme>> {
    Ok(db.get_setting(KEY_THEMES)?.unwrap_or_default())
}

fn load_active(db: &Storage) -> AppResult<Option<String>> {
    Ok(db
        .get_setting::<Option<String>>(KEY_ACTIVE_THEME)?
        .flatten())
}

pub fn themes_state(db: &Storage) -> AppResult<CustomThemes> {
    let themes = load_themes(db)?;
    let mut active_id = load_active(db)?;
    if let Some(id) = &active_id
        && !PRESET_THEME_IDS.contains(&id.as_str())
        && !themes.iter().any(|t| &t.id == id)
    {
        active_id = None;
    }
    Ok(CustomThemes { themes, active_id })
}

/// Creates or updates a theme; returns the stored theme.
pub fn theme_save(db: &Storage, input: CustomThemeInput) -> AppResult<CustomTheme> {
    let _guard = lock();
    let name = clean_text(&input.name, MAX_THEME_NAME, "theme name")?;
    let colors = clean_colors(&input.colors)?;
    let mut themes = load_themes(db)?;
    let now = now_ms();
    let saved = match input.id.as_deref() {
        Some(id) => {
            let theme = themes
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or_else(|| invalid("this theme no longer exists"))?;
            theme.name = name;
            theme.base = input.base;
            theme.colors = colors;
            theme.updated_at = now;
            theme.clone()
        }
        None => {
            if themes.len() >= MAX_THEMES {
                return Err(invalid(format!("at most {MAX_THEMES} themes can be kept")));
            }
            let theme = CustomTheme {
                id: nanomgg_accounts::new_id("theme"),
                name,
                base: input.base,
                colors,
                created_at: now,
                updated_at: now,
            };
            themes.push(theme.clone());
            theme
        }
    };
    db.set_setting(KEY_THEMES, &themes)?;
    Ok(saved)
}

/// Deletes one theme; the active selection is cleared when it pointed at it.
pub fn theme_delete(db: &Storage, id: &str) -> AppResult<CustomThemes> {
    let _guard = lock();
    let mut themes = load_themes(db)?;
    themes.retain(|t| t.id != id);
    db.set_setting(KEY_THEMES, &themes)?;
    if load_active(db)?.as_deref() == Some(id) {
        db.delete_setting(KEY_ACTIVE_THEME)?;
    }
    themes_state(db)
}

/// Deletes every user theme (presets stay); clears the active selection if it was one of them.
pub fn theme_delete_all(db: &Storage) -> AppResult<CustomThemes> {
    let _guard = lock();
    db.delete_setting(KEY_THEMES)?;
    if let Some(active) = load_active(db)?
        && !PRESET_THEME_IDS.contains(&active.as_str())
    {
        db.delete_setting(KEY_ACTIVE_THEME)?;
    }
    themes_state(db)
}

pub fn theme_set_active(db: &Storage, id: Option<String>) -> AppResult<CustomThemes> {
    let _guard = lock();
    match id {
        Some(id) => {
            let known = PRESET_THEME_IDS.contains(&id.as_str())
                || load_themes(db)?.iter().any(|t| t.id == id);
            if !known {
                return Err(invalid("this theme no longer exists"));
            }
            db.set_setting(KEY_ACTIVE_THEME, &id)?;
        }
        None => {
            db.delete_setting(KEY_ACTIVE_THEME)?;
        }
    }
    themes_state(db)
}

pub fn theme_export(path: &Path, input: &CustomThemeInput) -> AppResult<()> {
    let file = ThemeFile {
        format: THEME_FILE_FORMAT.into(),
        version: FILE_VERSION,
        theme: ThemeFileBody {
            name: clean_text(&input.name, MAX_THEME_NAME, "theme name")?,
            base: input.base,
            colors: clean_colors(&input.colors)?,
        },
    };
    write_json(path, &file)
}

/// Name not used by another theme: "Name", then "Name (2)", "Name (3)"…
fn unique_name(themes: &[CustomTheme], name: &str) -> String {
    let taken = |n: &str| themes.iter().any(|t| t.name.eq_ignore_ascii_case(n));
    if !taken(name) {
        return name.to_owned();
    }
    (2..)
        .map(|i| format!("{name} ({i})"))
        .find(|n| !taken(n))
        .unwrap_or_else(|| name.to_owned())
}

pub fn theme_import(db: &Storage, path: &Path) -> AppResult<ThemeImportResult> {
    let fail = |problem| {
        Ok(ThemeImportResult {
            problem: Some(problem),
            theme: None,
        })
    };
    let value = match read_json(path, MAX_THEME_FILE, THEME_FILE_FORMAT) {
        Ok(v) => v,
        Err(problem) => return fail(problem),
    };
    let Ok(file) = serde_json::from_value::<ThemeFile>(value) else {
        return fail(UserFileProblem::Invalid);
    };
    let Ok(colors) = clean_colors(&file.theme.colors) else {
        return fail(UserFileProblem::Invalid);
    };
    let Ok(name) = clean_text(&file.theme.name, MAX_THEME_NAME, "theme name") else {
        return fail(UserFileProblem::Invalid);
    };
    let name = unique_name(&load_themes(db)?, &name);
    let theme = theme_save(
        db,
        CustomThemeInput {
            id: None,
            name,
            base: file.theme.base,
            colors,
        },
    )?;
    Ok(ThemeImportResult {
        problem: None,
        theme: Some(theme),
    })
}

// ----- notes -----------------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "camelCase")]
pub enum NotePriority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub title: String,
    /// Plain text, shown as-is (never rendered as HTML / Markdown).
    pub body: String,
    /// Tag ids (built-in or user-defined).
    pub tags: Vec<String>,
    pub priority: NotePriority,
    pub done: bool,
    pub pinned: bool,
    /// Due date `YYYY-MM-DD`.
    pub due: Option<String>,
    /// Repository the note is about (path + display name).
    pub repo_path: Option<String>,
    pub repo_name: Option<String>,
    /// Unix time in milliseconds.
    pub created_at: i64,
    pub updated_at: i64,
}

/// A note read from an import file: everything but the title may be missing.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportedNote {
    #[serde(default)]
    id: String,
    title: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    priority: NotePriority,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    due: Option<String>,
    #[serde(default)]
    repo_path: Option<String>,
    #[serde(default)]
    repo_name: Option<String>,
    #[serde(default)]
    created_at: i64,
    #[serde(default)]
    updated_at: i64,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NoteInput {
    /// `None` creates a new note.
    #[serde(default)]
    #[specta(optional)]
    pub id: Option<String>,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub priority: NotePriority,
    pub done: bool,
    pub pinned: bool,
    pub due: Option<String>,
    pub repo_path: Option<String>,
    pub repo_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NoteTag {
    pub id: String,
    pub name: String,
    /// `#rrggbb` background of the label.
    pub color: String,
}

#[derive(Debug, Clone, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NoteTagInput {
    #[serde(default)]
    #[specta(optional)]
    pub id: Option<String>,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NotesData {
    pub notes: Vec<Note>,
    /// User-defined tags (built-in tags are not stored).
    pub tags: Vec<NoteTag>,
}

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NotesImportReport {
    pub problem: Option<UserFileProblem>,
    pub added: u32,
    pub updated: u32,
    /// Notes not newer than the copy already here, or damaged.
    pub skipped: u32,
    pub tags_added: u32,
    pub data: NotesData,
}

#[derive(Serialize, Deserialize)]
struct NotesFile {
    format: String,
    version: u32,
    #[serde(default)]
    exported_at: i64,
    #[serde(default)]
    notes: Vec<serde_json::Value>,
    #[serde(default)]
    tags: Vec<serde_json::Value>,
}

fn load_notes(db: &Storage) -> AppResult<Vec<Note>> {
    Ok(db.get_setting(KEY_NOTES)?.unwrap_or_default())
}

fn load_tags(db: &Storage) -> AppResult<Vec<NoteTag>> {
    Ok(db.get_setting(KEY_TAGS)?.unwrap_or_default())
}

pub fn notes_data(db: &Storage) -> AppResult<NotesData> {
    Ok(NotesData {
        notes: load_notes(db)?,
        tags: load_tags(db)?,
    })
}

fn valid_due(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let num = |s: &str| s.parse::<u32>().ok();
    let (Some(y), Some(m), Some(d)) = (num(&value[..4]), num(&value[5..7]), num(&value[8..]))
    else {
        return false;
    };
    (1970..=9999).contains(&y) && (1..=12).contains(&m) && (1..=31).contains(&d)
}

fn optional_text(value: Option<String>, max: usize) -> Option<String> {
    value
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty() && v.chars().count() <= max)
}

/// Note fields checked and normalized; `known_tags` drops references to deleted tags.
fn clean_note(input: NoteInput, known_tags: &HashSet<String>) -> AppResult<NoteInput> {
    let title = input.title.trim().to_owned();
    let body = input.body.replace("\r\n", "\n");
    if title.is_empty() && body.trim().is_empty() {
        return Err(invalid("a note needs a title or some text"));
    }
    if title.chars().count() > MAX_NOTE_TITLE {
        return Err(invalid(format!(
            "the title is longer than {MAX_NOTE_TITLE} characters"
        )));
    }
    if body.chars().count() > MAX_NOTE_BODY {
        return Err(invalid(format!(
            "the text is longer than {MAX_NOTE_BODY} characters"
        )));
    }
    let mut tags = Vec::new();
    for tag in input.tags {
        if known_tags.contains(&tag) && !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags.truncate(MAX_NOTE_TAGS);
    let due = input.due.filter(|d| valid_due(d.trim()));
    let repo_path = optional_text(input.repo_path, MAX_REPO_PATH);
    let repo_name = repo_path
        .as_ref()
        .and_then(|_| optional_text(input.repo_name, 256));
    Ok(NoteInput {
        id: input.id,
        title: title.replace(['\r', '\n'], " "),
        body,
        tags,
        priority: input.priority,
        done: input.done,
        pinned: input.pinned,
        due: due.map(|d| d.trim().to_owned()),
        repo_path,
        repo_name,
    })
}

fn known_tag_ids(tags: &[NoteTag]) -> HashSet<String> {
    BUILTIN_TAG_IDS
        .iter()
        .map(|s| (*s).to_owned())
        .chain(tags.iter().map(|t| t.id.clone()))
        .collect()
}

pub fn note_save(db: &Storage, input: NoteInput) -> AppResult<NotesData> {
    let _guard = lock();
    let tags = load_tags(db)?;
    let input = clean_note(input, &known_tag_ids(&tags))?;
    let mut notes = load_notes(db)?;
    let now = now_ms();
    match input.id.as_deref() {
        Some(id) => {
            let note = notes
                .iter_mut()
                .find(|n| n.id == id)
                .ok_or_else(|| invalid("this note no longer exists"))?;
            note.title = input.title;
            note.body = input.body;
            note.tags = input.tags;
            note.priority = input.priority;
            note.done = input.done;
            note.pinned = input.pinned;
            note.due = input.due;
            note.repo_path = input.repo_path;
            note.repo_name = input.repo_name;
            note.updated_at = now;
        }
        None => {
            if notes.len() >= MAX_NOTES {
                return Err(invalid(format!("at most {MAX_NOTES} notes can be kept")));
            }
            notes.push(Note {
                id: nanomgg_accounts::new_id("note"),
                title: input.title,
                body: input.body,
                tags: input.tags,
                priority: input.priority,
                done: input.done,
                pinned: input.pinned,
                due: input.due,
                repo_path: input.repo_path,
                repo_name: input.repo_name,
                created_at: now,
                updated_at: now,
            });
        }
    }
    db.set_setting(KEY_NOTES, &notes)?;
    Ok(NotesData { notes, tags })
}

pub fn note_delete(db: &Storage, ids: &[String]) -> AppResult<NotesData> {
    let _guard = lock();
    let mut notes = load_notes(db)?;
    notes.retain(|n| !ids.contains(&n.id));
    db.set_setting(KEY_NOTES, &notes)?;
    notes_data(db)
}

/// Deletes every note, and the user's tags too when `with_tags`.
pub fn notes_clear(db: &Storage, with_tags: bool) -> AppResult<NotesData> {
    let _guard = lock();
    db.delete_setting(KEY_NOTES)?;
    if with_tags {
        db.delete_setting(KEY_TAGS)?;
    }
    notes_data(db)
}

fn clean_tag(input: &NoteTagInput) -> AppResult<(String, String)> {
    let name = clean_text(&input.name, MAX_TAG_NAME, "tag name")?;
    let color = normalize_hex(&input.color)
        .ok_or_else(|| invalid(format!("\"{}\" is not a #rrggbb color", input.color)))?;
    Ok((name, color))
}

pub fn tag_save(db: &Storage, input: NoteTagInput) -> AppResult<NotesData> {
    let _guard = lock();
    let (name, color) = clean_tag(&input)?;
    let mut tags = load_tags(db)?;
    let clash =
        |t: &NoteTag| t.name.eq_ignore_ascii_case(&name) && Some(&t.id) != input.id.as_ref();
    if tags.iter().any(clash) {
        return Err(invalid("a tag with this name already exists"));
    }
    match input.id.as_deref() {
        Some(id) => {
            let tag = tags
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or_else(|| invalid("this tag no longer exists"))?;
            tag.name = name;
            tag.color = color;
        }
        None => {
            if tags.len() >= MAX_USER_TAGS {
                return Err(invalid(format!("at most {MAX_USER_TAGS} tags can be kept")));
            }
            tags.push(NoteTag {
                id: nanomgg_accounts::new_id("tag"),
                name,
                color,
            });
        }
    }
    db.set_setting(KEY_TAGS, &tags)?;
    notes_data(db)
}

/// Deletes a user tag and removes it from every note.
pub fn tag_delete(db: &Storage, id: &str) -> AppResult<NotesData> {
    let _guard = lock();
    let mut tags = load_tags(db)?;
    tags.retain(|t| t.id != id);
    db.set_setting(KEY_TAGS, &tags)?;
    let mut notes = load_notes(db)?;
    let mut changed = false;
    for note in &mut notes {
        let before = note.tags.len();
        note.tags.retain(|t| t != id);
        changed |= note.tags.len() != before;
    }
    if changed {
        db.set_setting(KEY_NOTES, &notes)?;
    }
    Ok(NotesData { notes, tags })
}

/// Writes the notes with the given ids (every note when `None`) plus the user's tags.
pub fn notes_export(db: &Storage, path: &Path, ids: Option<&[String]>) -> AppResult<u32> {
    let data = notes_data(db)?;
    let notes: Vec<serde_json::Value> = data
        .notes
        .iter()
        .filter(|n| ids.is_none_or(|ids| ids.contains(&n.id)))
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let count = notes.len() as u32;
    let file = NotesFile {
        format: NOTES_FILE_FORMAT.into(),
        version: FILE_VERSION,
        exported_at: now_ms(),
        notes,
        tags: data
            .tags
            .iter()
            .map(serde_json::to_value)
            .collect::<Result<_, _>>()
            .map_err(|e| AppError::Internal(e.to_string()))?,
    };
    write_json(path, &file)?;
    Ok(count)
}

/// Merges a notes file: new ids are added, known ids take the copy edited last.
pub fn notes_import(db: &Storage, path: &Path) -> AppResult<NotesImportReport> {
    let fail = |problem, data| {
        Ok(NotesImportReport {
            problem: Some(problem),
            added: 0,
            updated: 0,
            skipped: 0,
            tags_added: 0,
            data,
        })
    };
    let value = match read_json(path, MAX_NOTES_FILE, NOTES_FILE_FORMAT) {
        Ok(v) => v,
        Err(problem) => return fail(problem, notes_data(db)?),
    };
    let Ok(file) = serde_json::from_value::<NotesFile>(value) else {
        return fail(UserFileProblem::Invalid, notes_data(db)?);
    };

    let _guard = lock();
    let mut tags = load_tags(db)?;
    let mut tags_added = 0;
    for raw in file.tags {
        let Ok(tag) = serde_json::from_value::<NoteTag>(raw) else {
            continue;
        };
        if BUILTIN_TAG_IDS.contains(&tag.id.as_str()) || tags.iter().any(|t| t.id == tag.id) {
            continue;
        }
        let input = NoteTagInput {
            id: None,
            name: tag.name,
            color: tag.color,
        };
        let Ok((name, color)) = clean_tag(&input) else {
            continue;
        };
        if tags.len() >= MAX_USER_TAGS || tags.iter().any(|t| t.name.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let id = if tag.id.is_empty() || tag.id.len() > 64 {
            nanomgg_accounts::new_id("tag")
        } else {
            tag.id
        };
        tags.push(NoteTag { id, name, color });
        tags_added += 1;
    }
    let known = known_tag_ids(&tags);

    let mut notes = load_notes(db)?;
    let (mut added, mut updated, mut skipped) = (0, 0, 0);
    let now = now_ms();
    for raw in file.notes {
        let Ok(note) = serde_json::from_value::<ImportedNote>(raw) else {
            skipped += 1;
            continue;
        };
        let Ok(clean) = clean_note(
            NoteInput {
                id: None,
                title: note.title,
                body: note.body,
                tags: note.tags,
                priority: note.priority,
                done: note.done,
                pinned: note.pinned,
                due: note.due,
                repo_path: note.repo_path,
                repo_name: note.repo_name,
            },
            &known,
        ) else {
            skipped += 1;
            continue;
        };
        let created_at = if note.created_at > 0 {
            note.created_at
        } else {
            now
        };
        let updated_at = note.updated_at.max(created_at);
        let merged = Note {
            id: note.id.clone(),
            title: clean.title,
            body: clean.body,
            tags: clean.tags,
            priority: clean.priority,
            done: clean.done,
            pinned: clean.pinned,
            due: clean.due,
            repo_path: clean.repo_path,
            repo_name: clean.repo_name,
            created_at,
            updated_at,
        };
        let existing = (!note.id.is_empty() && note.id.len() <= 64)
            .then(|| notes.iter().position(|n| n.id == note.id))
            .flatten();
        match existing {
            Some(i) if notes[i].updated_at < merged.updated_at => {
                notes[i] = merged;
                updated += 1;
            }
            Some(_) => skipped += 1,
            None if notes.len() >= MAX_NOTES => skipped += 1,
            None => {
                let id = if note.id.is_empty() || note.id.len() > 64 {
                    nanomgg_accounts::new_id("note")
                } else {
                    note.id
                };
                notes.push(Note { id, ..merged });
                added += 1;
            }
        }
    }
    if tags_added > 0 {
        db.set_setting(KEY_TAGS, &tags)?;
    }
    if added + updated > 0 {
        db.set_setting(KEY_NOTES, &notes)?;
    }
    Ok(NotesImportReport {
        problem: None,
        added,
        updated,
        skipped,
        tags_added,
        data: NotesData { notes, tags },
    })
}

/// Writes a readable copy of the notes (Markdown made by the frontend).
pub fn notes_export_text(path: &Path, content: &str) -> AppResult<()> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(ext.as_deref(), Some("md" | "txt")) {
        return Err(invalid("only .md or .txt files can be written"));
    }
    if content.len() as u64 > MAX_NOTES_FILE {
        return Err(invalid("the text is too large"));
    }
    std::fs::write(path, content).map_err(|e| AppError::Internal(e.to_string()))
}

// ----- summary (Settings → Storage) ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UserDataSummary {
    pub themes: u32,
    pub notes: u32,
    pub user_tags: u32,
    /// Approximate size of the stored JSON.
    pub themes_bytes: u32,
    pub notes_bytes: u32,
}

pub fn summary(db: &Storage) -> AppResult<UserDataSummary> {
    let size = |v: &serde_json::Value| serde_json::to_string(v).map_or(0, |s| s.len() as u32);
    let raw = |key: &str| -> AppResult<serde_json::Value> {
        Ok(db.get_setting(key)?.unwrap_or(serde_json::Value::Null))
    };
    let themes = raw(KEY_THEMES)?;
    let notes = raw(KEY_NOTES)?;
    let tags = raw(KEY_TAGS)?;
    let count = |v: &serde_json::Value| v.as_array().map_or(0, |a| a.len() as u32);
    Ok(UserDataSummary {
        themes: count(&themes),
        notes: count(&notes),
        user_tags: count(&tags),
        themes_bytes: if themes.is_null() { 0 } else { size(&themes) },
        notes_bytes: [&notes, &tags]
            .iter()
            .filter(|v| !v.is_null())
            .map(|v| size(v))
            .sum(),
    })
}

// ----- files -----------------------------------------------------------------------------------

fn write_json<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    let json =
        serde_json::to_string_pretty(value).map_err(|e| AppError::Internal(e.to_string()))?;
    std::fs::write(path, json + "\n").map_err(|e| AppError::Internal(e.to_string()))
}

/// Reads a JSON file exported by this app, checking size, `format` and `version`.
fn read_json(path: &Path, max: u64, format: &str) -> Result<serde_json::Value, UserFileProblem> {
    let len = std::fs::metadata(path)
        .map_err(|_| UserFileProblem::Unreadable)?
        .len();
    if len > max {
        return Err(UserFileProblem::TooLarge);
    }
    let bytes = std::fs::read(path).map_err(|_| UserFileProblem::Unreadable)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| UserFileProblem::NotJson)?;
    let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}'))
        .map_err(|_| UserFileProblem::NotJson)?;
    if value.get("format").and_then(|f| f.as_str()) != Some(format) {
        return Err(UserFileProblem::WrongFormat);
    }
    match value.get("version").and_then(serde_json::Value::as_u64) {
        Some(v) if v > u64::from(FILE_VERSION) => Err(UserFileProblem::NewerVersion),
        Some(_) => Ok(value),
        None => Err(UserFileProblem::Invalid),
    }
}

// ----- commands --------------------------------------------------------------------------------

async fn blocking<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&Storage) -> AppResult<T> + Send + 'static,
) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>().storage))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn custom_theme_list(app: AppHandle) -> AppResult<CustomThemes> {
    blocking(app, themes_state).await
}

#[tauri::command]
#[specta::specta]
pub async fn custom_theme_save(app: AppHandle, input: CustomThemeInput) -> AppResult<CustomTheme> {
    blocking(app, move |db| theme_save(db, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn custom_theme_delete(app: AppHandle, id: String) -> AppResult<CustomThemes> {
    blocking(app, move |db| theme_delete(db, &id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn custom_theme_delete_all(app: AppHandle) -> AppResult<CustomThemes> {
    blocking(app, theme_delete_all).await
}

#[tauri::command]
#[specta::specta]
pub async fn custom_theme_set_active(
    app: AppHandle,
    id: Option<String>,
) -> AppResult<CustomThemes> {
    blocking(app, move |db| theme_set_active(db, id)).await
}

#[tauri::command]
#[specta::specta]
pub async fn custom_theme_export(
    app: AppHandle,
    path: String,
    input: CustomThemeInput,
) -> AppResult<()> {
    blocking(app, move |_| theme_export(Path::new(&path), &input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn custom_theme_import(app: AppHandle, path: String) -> AppResult<ThemeImportResult> {
    blocking(app, move |db| theme_import(db, Path::new(&path))).await
}

#[tauri::command]
#[specta::specta]
pub async fn notes_load(app: AppHandle) -> AppResult<NotesData> {
    blocking(app, notes_data).await
}

#[tauri::command]
#[specta::specta]
pub async fn notes_save(app: AppHandle, input: NoteInput) -> AppResult<NotesData> {
    blocking(app, move |db| note_save(db, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn notes_delete(app: AppHandle, ids: Vec<String>) -> AppResult<NotesData> {
    blocking(app, move |db| note_delete(db, &ids)).await
}

#[tauri::command]
#[specta::specta]
pub async fn notes_delete_all(app: AppHandle, with_tags: bool) -> AppResult<NotesData> {
    blocking(app, move |db| notes_clear(db, with_tags)).await
}

#[tauri::command]
#[specta::specta]
pub async fn notes_tag_save(app: AppHandle, input: NoteTagInput) -> AppResult<NotesData> {
    blocking(app, move |db| tag_save(db, input)).await
}

#[tauri::command]
#[specta::specta]
pub async fn notes_tag_delete(app: AppHandle, id: String) -> AppResult<NotesData> {
    blocking(app, move |db| tag_delete(db, &id)).await
}

/// `ids: None` exports every note. Returns how many notes were written.
#[tauri::command]
#[specta::specta]
pub async fn notes_export_file(
    app: AppHandle,
    path: String,
    ids: Option<Vec<String>>,
) -> AppResult<u32> {
    blocking(app, move |db| {
        notes_export(db, Path::new(&path), ids.as_deref())
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn notes_import_file(app: AppHandle, path: String) -> AppResult<NotesImportReport> {
    blocking(app, move |db| notes_import(db, Path::new(&path))).await
}

#[tauri::command]
#[specta::specta]
pub async fn notes_export_markdown(app: AppHandle, path: String, content: String) -> AppResult<()> {
    blocking(app, move |_| notes_export_text(Path::new(&path), &content)).await
}

#[tauri::command]
#[specta::specta]
pub async fn user_data_summary(app: AppHandle) -> AppResult<UserDataSummary> {
    blocking(app, summary).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Storage {
        Storage::open_in_memory().unwrap()
    }

    fn colors(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn theme_input(name: &str) -> CustomThemeInput {
        CustomThemeInput {
            id: None,
            name: name.into(),
            base: ThemeBase::Dark,
            colors: colors(&[("--bg", "#000"), ("--text", "#FFFFFF")]),
        }
    }

    fn note_input(title: &str) -> NoteInput {
        NoteInput {
            id: None,
            title: title.into(),
            body: String::new(),
            tags: vec![],
            priority: NotePriority::Normal,
            done: false,
            pinned: false,
            due: None,
            repo_path: None,
            repo_name: None,
        }
    }

    #[test]
    fn hex_is_normalized() {
        assert_eq!(normalize_hex("#ABC").as_deref(), Some("#aabbcc"));
        assert_eq!(normalize_hex(" #A0b1C2 ").as_deref(), Some("#a0b1c2"));
        assert_eq!(normalize_hex("a0b1c2"), None);
        assert_eq!(normalize_hex("#12345"), None);
        assert_eq!(normalize_hex("#gggggg"), None);
    }

    #[test]
    fn theme_save_validates_and_normalizes() {
        let db = db();
        let mut input = theme_input("  Mine  ");
        input.colors.insert("--unknown".into(), "#123456".into());
        let t = theme_save(&db, input).unwrap();
        assert!(t.id.starts_with("theme_"));
        assert_eq!(t.name, "Mine");
        assert_eq!(t.colors["--bg"], "#000000");
        assert_eq!(t.colors["--text"], "#ffffff");
        assert!(!t.colors.contains_key("--unknown"));
        assert!(t.created_at > 0 && t.created_at == t.updated_at);

        let mut bad = theme_input("Bad");
        bad.colors.insert("--primary".into(), "red".into());
        assert!(theme_save(&db, bad).is_err());
        assert!(theme_save(&db, theme_input("   ")).is_err());
        assert!(theme_save(&db, theme_input(&"x".repeat(MAX_THEME_NAME + 1))).is_err());

        let mut update = theme_input("Renamed");
        update.id = Some(t.id.clone());
        update.base = ThemeBase::Light;
        let u = theme_save(&db, update).unwrap();
        assert_eq!(u.id, t.id);
        assert_eq!(u.base, ThemeBase::Light);
        assert_eq!(themes_state(&db).unwrap().themes.len(), 1);

        let mut gone = theme_input("Gone");
        gone.id = Some("theme_missing".into());
        assert!(theme_save(&db, gone).is_err());
    }

    #[test]
    fn deleting_the_active_theme_clears_the_selection() {
        let db = db();
        let a = theme_save(&db, theme_input("A")).unwrap();
        let b = theme_save(&db, theme_input("B")).unwrap();
        assert_eq!(
            theme_set_active(&db, Some(a.id.clone())).unwrap().active_id,
            Some(a.id.clone())
        );
        assert!(theme_set_active(&db, Some("theme_nope".into())).is_err());
        let s = theme_delete(&db, &b.id).unwrap();
        assert_eq!(s.active_id, Some(a.id.clone()));
        let s = theme_delete(&db, &a.id).unwrap();
        assert_eq!(s.active_id, None);
        assert!(s.themes.is_empty());

        theme_set_active(&db, Some("hc-dark".into())).unwrap();
        theme_save(&db, theme_input("C")).unwrap();
        let s = theme_delete_all(&db).unwrap();
        assert!(s.themes.is_empty());
        assert_eq!(s.active_id.as_deref(), Some("hc-dark"));
    }

    #[test]
    fn theme_file_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.json");
        let db = db();
        theme_export(&path, &theme_input("Shared")).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"nanomgg-theme\""));

        let first = theme_import(&db, &path).unwrap();
        assert_eq!(first.problem, None);
        assert_eq!(first.theme.unwrap().name, "Shared");
        let second = theme_import(&db, &path).unwrap().theme.unwrap();
        assert_eq!(second.name, "Shared (2)");
        assert_eq!(second.colors["--bg"], "#000000");
    }

    #[test]
    fn bad_files_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let db = db();
        let write = |name: &str, text: &str| {
            let p = dir.path().join(name);
            std::fs::write(&p, text).unwrap();
            p
        };
        let problem = |p: &Path| theme_import(&db, p).unwrap().problem;
        assert_eq!(
            problem(&dir.path().join("missing.json")),
            Some(UserFileProblem::Unreadable)
        );
        assert_eq!(
            problem(&write("a.json", "not json")),
            Some(UserFileProblem::NotJson)
        );
        assert_eq!(
            problem(&write(
                "b.json",
                r#"{"format":"nanomgg-notes","version":1}"#
            )),
            Some(UserFileProblem::WrongFormat)
        );
        assert_eq!(
            problem(&write(
                "c.json",
                r#"{"format":"nanomgg-theme","version":9}"#
            )),
            Some(UserFileProblem::NewerVersion)
        );
        assert_eq!(
            problem(&write(
                "d.json",
                r##"{"format":"nanomgg-theme","version":1,"theme":{"name":"x","base":"dark","colors":{"--bg":"nope"}}}"##
            )),
            Some(UserFileProblem::Invalid)
        );
        let big = "x".repeat(MAX_THEME_FILE as usize + 1);
        assert_eq!(
            problem(&write("e.json", &big)),
            Some(UserFileProblem::TooLarge)
        );
        let notes = notes_import(&db, &write("f.json", "{}")).unwrap();
        assert_eq!(notes.problem, Some(UserFileProblem::WrongFormat));
        assert!(themes_state(&db).unwrap().themes.is_empty());
    }

    #[test]
    fn notes_are_created_updated_and_validated() {
        let db = db();
        let data = note_save(&db, note_input("  First  ")).unwrap();
        let note = data.notes[0].clone();
        assert!(note.id.starts_with("note_"));
        assert_eq!(note.title, "First");

        let mut edit = note_input("First");
        edit.id = Some(note.id.clone());
        edit.body = "line 1\r\nline 2".into();
        edit.tags = vec!["urgent".into(), "urgent".into(), "tag_unknown".into()];
        edit.due = Some("2026-13-01".into());
        edit.priority = NotePriority::Urgent;
        edit.repo_path = Some("  ".into());
        edit.repo_name = Some("x".into());
        let n = note_save(&db, edit).unwrap().notes[0].clone();
        assert_eq!(n.body, "line 1\nline 2");
        assert_eq!(n.tags, vec!["urgent".to_owned()]);
        assert_eq!(n.due, None);
        assert_eq!(n.repo_path, None);
        assert_eq!(n.repo_name, None);
        assert_eq!(n.priority, NotePriority::Urgent);
        assert!(n.updated_at >= n.created_at);

        let mut dated = note_input("Dated");
        dated.due = Some("2026-10-20".into());
        dated.repo_path = Some("D:/repo".into());
        dated.repo_name = Some("repo".into());
        let data = note_save(&db, dated).unwrap();
        let d = data.notes.iter().find(|n| n.title == "Dated").unwrap();
        assert_eq!(d.due.as_deref(), Some("2026-10-20"));
        assert_eq!(d.repo_name.as_deref(), Some("repo"));

        assert!(note_save(&db, note_input("   ")).is_err());
        let mut body_only = note_input("");
        body_only.body = "just text".into();
        assert!(note_save(&db, body_only).is_ok());
        assert!(note_save(&db, note_input(&"t".repeat(MAX_NOTE_TITLE + 1))).is_err());

        let data = note_delete(&db, &[note.id]).unwrap();
        assert_eq!(data.notes.len(), 2);
        assert!(notes_clear(&db, false).unwrap().notes.is_empty());
    }

    #[test]
    fn tags_are_unique_and_deleting_one_untags_notes() {
        let db = db();
        let data = tag_save(
            &db,
            NoteTagInput {
                id: None,
                name: "Client".into(),
                color: "#1971C2".into(),
            },
        )
        .unwrap();
        let tag = data.tags[0].clone();
        assert!(tag.id.starts_with("tag_"));
        assert_eq!(tag.color, "#1971c2");
        let dup = NoteTagInput {
            id: None,
            name: "client".into(),
            color: "#000000".into(),
        };
        assert!(tag_save(&db, dup).is_err());
        let rename = NoteTagInput {
            id: Some(tag.id.clone()),
            name: "CLIENT".into(),
            color: "#000".into(),
        };
        assert_eq!(tag_save(&db, rename).unwrap().tags[0].name, "CLIENT");

        let mut note = note_input("Tagged");
        note.tags = vec![tag.id.clone(), "bug".into()];
        let data = note_save(&db, note).unwrap();
        assert_eq!(data.notes[0].tags.len(), 2);
        let data = tag_delete(&db, &tag.id).unwrap();
        assert!(data.tags.is_empty());
        assert_eq!(data.notes[0].tags, vec!["bug".to_owned()]);
        tag_save(
            &db,
            NoteTagInput {
                id: None,
                name: "Again".into(),
                color: "#000".into(),
            },
        )
        .unwrap();
        assert!(notes_clear(&db, true).unwrap().tags.is_empty());
    }

    #[test]
    fn notes_import_merges_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.json");
        let source = db();
        tag_save(
            &source,
            NoteTagInput {
                id: None,
                name: "Mine".into(),
                color: "#7048e8".into(),
            },
        )
        .unwrap();
        let tag_id = load_tags(&source).unwrap()[0].id.clone();
        let mut a = note_input("A");
        a.tags = vec![tag_id.clone()];
        note_save(&source, a).unwrap();
        note_save(&source, note_input("B")).unwrap();
        assert_eq!(notes_export(&source, &path, None).unwrap(), 2);

        // Fresh install: everything is added.
        let target = db();
        let r = notes_import(&target, &path).unwrap();
        assert_eq!(r.problem, None);
        assert_eq!((r.added, r.updated, r.skipped, r.tags_added), (2, 0, 0, 1));
        let a = r.data.notes.iter().find(|n| n.title == "A").unwrap();
        assert_eq!(a.tags, vec![tag_id]);

        // Same file again: nothing newer.
        let r = notes_import(&target, &path).unwrap();
        assert_eq!((r.added, r.updated, r.skipped, r.tags_added), (0, 0, 2, 0));

        // Edited here after the export: the local copy wins; edited in the file: the file wins.
        let mut notes = load_notes(&target).unwrap();
        notes[0].updated_at += 10_000;
        notes[0].title = "Local edit".into();
        target.set_setting(KEY_NOTES, &notes).unwrap();
        let mut file: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let other = &mut file["notes"][1];
        other["title"] = "File edit".into();
        other["updatedAt"] = (other["updatedAt"].as_i64().unwrap() + 20_000).into();
        file["notes"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({ "id": "x", "title": 5 }));
        std::fs::write(&path, file.to_string()).unwrap();
        let r = notes_import(&target, &path).unwrap();
        assert_eq!((r.added, r.updated, r.skipped), (0, 1, 2));
        let titles: Vec<_> = r.data.notes.iter().map(|n| n.title.as_str()).collect();
        assert!(titles.contains(&"Local edit") && titles.contains(&"File edit"));

        // Export of a subset.
        let only = vec![r.data.notes[0].id.clone()];
        assert_eq!(notes_export(&target, &path, Some(&only)).unwrap(), 1);
    }

    #[test]
    fn markdown_export_only_writes_text_files() {
        let dir = tempfile::tempdir().unwrap();
        assert!(notes_export_text(&dir.path().join("n.md"), "# Notes").is_ok());
        assert!(notes_export_text(&dir.path().join("n.exe"), "x").is_err());
    }

    #[test]
    fn summary_counts_stored_data() {
        let db = db();
        assert_eq!(
            summary(&db).unwrap(),
            UserDataSummary {
                themes: 0,
                notes: 0,
                user_tags: 0,
                themes_bytes: 0,
                notes_bytes: 0
            }
        );
        theme_save(&db, theme_input("T")).unwrap();
        note_save(&db, note_input("N")).unwrap();
        let s = summary(&db).unwrap();
        assert_eq!((s.themes, s.notes, s.user_tags), (1, 1, 0));
        assert!(s.themes_bytes > 0 && s.notes_bytes > 0);
    }

    #[test]
    fn due_dates_are_checked() {
        assert!(valid_due("2026-01-31"));
        assert!(!valid_due("2026-1-31"));
        assert!(!valid_due("2026-00-10"));
        assert!(!valid_due("abcd-ef-gh"));
    }
}
