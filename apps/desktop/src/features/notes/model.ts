// Phase 6.5: "My notes" — tags, filtering, sorting, Markdown export (pure logic).
import type { TFunction } from "i18next";
import type { Note, NoteInput, NotePriority, NoteTag } from "@/ipc";

export const PRIORITIES: readonly NotePriority[] = ["low", "normal", "high", "urgent"];
const PRIORITY_RANK: Record<NotePriority, number> = { low: 0, normal: 1, high: 2, urgent: 3 };

export type BuiltinTagId = "default" | "urgent" | "wip" | "review" | "bug" | "idea" | "done";

/** Built-in tags (names are translated; same ids as `BUILTIN_TAG_IDS` in the backend). */
export const BUILTIN_TAGS: readonly { id: BuiltinTagId; color: string }[] = [
  { id: "default", color: "#868e96" },
  { id: "urgent", color: "#c92a2a" },
  { id: "wip", color: "#f08c00" },
  { id: "review", color: "#1971c2" },
  { id: "bug", color: "#8b1a1a" },
  { id: "idea", color: "#7048e8" },
  { id: "done", color: "#2f9e44" },
];

/** Colors offered for the user's own tags (label text is black or white, ≥ 4.5:1 — tested). */
export const TAG_COLORS: readonly string[] = [
  "#868e96",
  "#c92a2a",
  "#8b1a1a",
  "#f08c00",
  "#fab005",
  "#82c91e",
  "#2f9e44",
  "#0ca678",
  "#15aabf",
  "#1971c2",
  "#364fc7",
  "#7048e8",
  "#ae3ec9",
  "#d6336c",
  "#5c3d2e",
  "#343a40",
];

export interface TagDef {
  id: string;
  /** User tags only; built-in names come from `tagName()`. */
  name: string;
  color: string;
  builtin: boolean;
}

export function allTags(user: readonly NoteTag[]): TagDef[] {
  return [
    ...BUILTIN_TAGS.map((tag) => ({ ...tag, name: "", builtin: true })),
    ...user.map((tag) => ({ ...tag, builtin: false })),
  ];
}

export function tagName(t: TFunction, tag: TagDef): string {
  return tag.builtin ? t(`notes.builtinTag.${tag.id as BuiltinTagId}`) : tag.name;
}

/** Lower case without Vietnamese accents, so "ghi chu" finds "Ghi chú". */
export function foldText(text: string): string {
  return text
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/đ/g, "d")
    .replace(/Đ/g, "D")
    .toLowerCase();
}

export interface NoteFilter {
  query: string;
  /** Notes with at least one of these tags (none = every note). */
  tags: readonly string[];
  priorities: readonly NotePriority[];
  showDone: boolean;
}

export function filterNotes(
  notes: readonly Note[],
  filter: NoteFilter,
  tagLabel: (id: string) => string,
): Note[] {
  const words = foldText(filter.query).split(/\s+/).filter(Boolean);
  return notes.filter((note) => {
    if (!filter.showDone && note.done) return false;
    if (filter.tags.length && !note.tags.some((id) => filter.tags.includes(id))) return false;
    if (filter.priorities.length && !filter.priorities.includes(note.priority)) return false;
    if (!words.length) return true;
    const haystack = foldText(
      [note.title, note.body, note.repoName ?? "", ...note.tags.map(tagLabel)].join("\n"),
    );
    return words.every((w) => haystack.includes(w));
  });
}

export type NoteSort = "priority" | "due" | "updated";
export const NOTE_SORTS: readonly NoteSort[] = ["priority", "due", "updated"];

/** Pinned first, then open before done, then by the chosen key (latest edit breaks ties). */
export function sortNotes(notes: readonly Note[], sort: NoteSort): Note[] {
  const due = (n: Note) => n.due ?? "9999-99-99";
  const byKey = (a: Note, b: Note): number => {
    switch (sort) {
      case "priority":
        return (
          PRIORITY_RANK[b.priority] - PRIORITY_RANK[a.priority] || due(a).localeCompare(due(b))
        );
      case "due":
        return (
          due(a).localeCompare(due(b)) || PRIORITY_RANK[b.priority] - PRIORITY_RANK[a.priority]
        );
      case "updated":
        return 0;
    }
  };
  return [...notes].sort(
    (a, b) =>
      Number(b.pinned) - Number(a.pinned) ||
      Number(a.done) - Number(b.done) ||
      byKey(a, b) ||
      b.updatedAt - a.updatedAt,
  );
}

/** Local date as `YYYY-MM-DD` (the format of `<input type="date">`). */
export function todayIso(now: Date = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

export type DueState = "overdue" | "today" | "upcoming";

export function dueState(note: Pick<Note, "due" | "done">, today: string): DueState | null {
  if (!note.due || note.done) return null;
  if (note.due < today) return "overdue";
  return note.due === today ? "today" : "upcoming";
}

/** Same folder whatever the slashes / letter case (Windows paths). */
export function samePath(a: string, b: string): boolean {
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
  return norm(a) === norm(b);
}

export function toInput(note: Note, patch: Partial<NoteInput> = {}): NoteInput {
  return {
    id: note.id,
    title: note.title,
    body: note.body,
    tags: note.tags,
    priority: note.priority,
    done: note.done,
    pinned: note.pinned,
    due: note.due,
    repoPath: note.repoPath,
    repoName: note.repoName,
    ...patch,
  };
}

export function newNoteInput(title: string): NoteInput {
  return {
    title,
    body: "",
    tags: [],
    priority: "normal",
    done: false,
    pinned: false,
    due: null,
    repoPath: null,
    repoName: null,
  };
}

/** Readable copy of the notes (plain Markdown; the note text is kept as written). */
export function toMarkdown(
  notes: readonly Note[],
  t: TFunction,
  tagLabel: (id: string) => string,
  locale: string,
): string {
  const date = (ms: number) =>
    new Date(ms).toLocaleString(locale, { dateStyle: "medium", timeStyle: "short" });
  const lines = [`# ${t("notes.title")}`, ""];
  for (const note of notes) {
    const title = note.title || t("notes.untitled");
    lines.push(
      `## ${note.done ? "[x]" : "[ ]"} ${title}${note.pinned ? ` (${t("notes.pinned")})` : ""}`,
      "",
    );
    const meta = [
      `${t("notes.priority")}: ${t(`notes.priorityLevel.${note.priority}`)}`,
      note.tags.length ? `${t("notes.tags")}: ${note.tags.map(tagLabel).join(", ")}` : null,
      note.due ? `${t("notes.due")}: ${note.due}` : null,
      note.repoName ? `${t("notes.repo")}: ${note.repoName}` : null,
      `${t("notes.updated")}: ${date(note.updatedAt)}`,
    ].filter(Boolean);
    lines.push(...meta.map((m) => `- ${m}`), "");
    if (note.body.trim()) lines.push(note.body.trimEnd(), "");
  }
  return lines.join("\n");
}
