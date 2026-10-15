// @vitest-environment node
import { describe, expect, it } from "vitest";
import type { TFunction } from "i18next";
import type { Note } from "@/ipc";
import { contrastText } from "@/features/graph/avatars";
import { contrastRatio } from "@/theme/contrast";
import {
  allTags,
  BUILTIN_TAGS,
  dueState,
  filterNotes,
  foldText,
  samePath,
  sortNotes,
  TAG_COLORS,
  todayIso,
  toInput,
  toMarkdown,
} from "./model";

let seq = 0;
const note = (over: Partial<Note> = {}): Note => ({
  id: `note_${++seq}`,
  title: "Title",
  body: "",
  tags: [],
  priority: "normal",
  done: false,
  pinned: false,
  due: null,
  repoPath: null,
  repoName: null,
  createdAt: 1000,
  updatedAt: 1000,
  ...over,
});

const filter = { query: "", tags: [], priorities: [], showDone: true };
const label = (id: string) => ({ urgent: "Gấp", tag_1: "Khách hàng" })[id] ?? id;

describe("tag colors", () => {
  it.each([...TAG_COLORS, ...BUILTIN_TAGS.map((t) => t.color)])(
    "label text on %s is readable (≥ 4.5:1)",
    (color) => {
      expect(contrastRatio(color, contrastText(color))).toBeGreaterThanOrEqual(4.5);
    },
  );

  it("lists built-in tags before the user's", () => {
    const tags = allTags([{ id: "tag_1", name: "Client", color: "#1971c2" }]);
    expect(tags.map((t) => t.id)).toEqual([...BUILTIN_TAGS.map((t) => t.id), "tag_1"]);
    expect(tags.at(-1)?.builtin).toBe(false);
  });
});

describe("filterNotes", () => {
  const notes = [
    note({ title: "Sửa lỗi đăng nhập", tags: ["urgent"], priority: "high" }),
    note({ title: "Viết tài liệu", body: "Ghi chú cho khách", done: true }),
    note({ title: "Gọi điện", tags: ["tag_1"], repoName: "web-app" }),
  ];

  it("searches title, text, tag names and repo, ignoring accents and case", () => {
    expect(filterNotes(notes, { ...filter, query: "dang nhap" }, label)).toHaveLength(1);
    expect(filterNotes(notes, { ...filter, query: "KHACH" }, label)).toHaveLength(2);
    expect(filterNotes(notes, { ...filter, query: "web" }, label)).toHaveLength(1);
    expect(filterNotes(notes, { ...filter, query: "gap sua" }, label)).toHaveLength(1);
  });

  it("filters by tag, priority and done", () => {
    expect(filterNotes(notes, { ...filter, tags: ["urgent", "tag_1"] }, label)).toHaveLength(2);
    expect(filterNotes(notes, { ...filter, priorities: ["high"] }, label)).toHaveLength(1);
    expect(filterNotes(notes, { ...filter, showDone: false }, label)).toHaveLength(2);
  });

  it("folds Vietnamese text", () => {
    expect(foldText("Đường Ghi Chú")).toBe("duong ghi chu");
  });
});

describe("sortNotes", () => {
  const a = note({ title: "a", priority: "low", updatedAt: 5 });
  const b = note({ title: "b", priority: "urgent", due: "2026-12-01", updatedAt: 1 });
  const c = note({ title: "c", priority: "high", due: "2026-10-20", updatedAt: 3 });
  const d = note({ title: "d", pinned: true, priority: "low", updatedAt: 0 });
  const e = note({ title: "e", done: true, priority: "urgent", updatedAt: 9 });

  const titles = (list: Note[]) => list.map((n) => n.title).join("");

  it("puts pinned notes first and finished notes last", () => {
    expect(titles(sortNotes([a, b, c, d, e], "priority"))).toBe("dbcae");
    expect(titles(sortNotes([a, b, c, d, e], "due"))).toBe("dcbae");
    expect(titles(sortNotes([a, b, c, d, e], "updated"))).toBe("dacbe");
  });
});

describe("dates and paths", () => {
  it("formats the local date and flags due notes", () => {
    expect(todayIso(new Date(2026, 0, 5))).toBe("2026-01-05");
    const today = "2026-10-15";
    expect(dueState({ due: "2026-10-14", done: false }, today)).toBe("overdue");
    expect(dueState({ due: today, done: false }, today)).toBe("today");
    expect(dueState({ due: "2026-11-01", done: false }, today)).toBe("upcoming");
    expect(dueState({ due: "2026-10-14", done: true }, today)).toBeNull();
    expect(dueState({ due: null, done: false }, today)).toBeNull();
  });

  it("compares Windows paths loosely", () => {
    expect(samePath("D:\\Work\\Repo\\", "d:/work/repo")).toBe(true);
    expect(samePath("D:/Work/Repo", "D:/Work/Repo2")).toBe(false);
  });
});

describe("export helpers", () => {
  it("builds a save input with a patch", () => {
    const n = note({ title: "x", tags: ["bug"] });
    expect(toInput(n, { done: true })).toMatchObject({ id: n.id, title: "x", done: true });
  });

  it("writes readable Markdown keeping the text as-is", () => {
    const t = ((key: string) => key) as unknown as TFunction;
    const md = toMarkdown(
      [note({ title: "Hello", body: "<b>not html</b>\nline 2", tags: ["urgent"], pinned: true })],
      t,
      label,
      "en",
    );
    expect(md).toContain("## [ ] Hello (notes.pinned)");
    expect(md).toContain("- notes.tags: Gấp");
    expect(md).toContain("<b>not html</b>\nline 2");
  });
});
