import { describe, expect, it } from "vitest";
import { guideEn } from "./en";
import { fold, guideFor, searchGuide } from "./index";
import { guideVi } from "./vi";

const shape = (topics: typeof guideEn) =>
  topics.map((t) => ({
    id: t.id,
    sections: t.sections.map((s) => ({
      id: s.id,
      blocks: s.blocks.map((b) =>
        b.kind === "steps" || b.kind === "list"
          ? `${b.kind}:${b.items.length}`
          : b.kind === "keys"
            ? `keys:${b.rows.length}`
            : b.kind,
      ),
    })),
  }));

describe("guide content", () => {
  it("has the same topics, sections and blocks in both languages", () => {
    expect(shape(guideVi)).toEqual(shape(guideEn));
  });

  it("covers the required topics with unique ids and no empty text", () => {
    const ids = guideEn.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
    for (const required of [
      "start",
      "repos",
      "accounts",
      "commit",
      "branches",
      "sync",
      "conflicts",
      "history",
      "stash",
      "undo",
      "lfs",
      "pulls",
      "mywork",
      "terminal",
      "submodules",
      "signing",
      "appearance",
      "backup",
      "storage",
      "updates",
      "shortcuts",
      "faq",
    ]) {
      expect(ids).toContain(required);
    }
    for (const topics of [guideEn, guideVi]) {
      for (const t of topics) {
        expect(t.title.trim(), t.id).not.toBe("");
        expect(t.summary.trim(), t.id).not.toBe("");
        const sectionIds = t.sections.map((s) => s.id);
        expect(new Set(sectionIds).size, t.id).toBe(sectionIds.length);
        for (const s of t.sections) {
          expect(s.blocks.length, `${t.id}.${s.id}`).toBeGreaterThan(0);
        }
      }
    }
  });

  it("keeps shortcut keys identical in both languages", () => {
    const keys = (topics: typeof guideEn) =>
      topics.flatMap((t) =>
        t.sections.flatMap((s) =>
          s.blocks.flatMap((b) =>
            b.kind === "keys" ? b.rows.map((r) => r[0].replace(/\(.*\)/, "").trim()) : [],
          ),
        ),
      );
    expect(keys(guideVi)).toEqual(keys(guideEn));
  });
});

describe("guide search", () => {
  it("folds accents", () => {
    expect(fold("Chuyển nhánh Đúp")).toBe("chuyen nhanh dup");
  });

  it("finds topics by any text, without accents, all words required", () => {
    expect(searchGuide(guideVi, "").length).toBe(guideVi.length);
    expect(searchGuide(guideVi, "nhap dup").map((t) => t.id)).toContain("branches");
    expect(searchGuide(guideEn, "autostash").map((t) => t.id)).toContain("branches");
    expect(searchGuide(guideEn, "zzz-nothing")).toEqual([]);
  });

  it("picks the language", () => {
    expect(guideFor("vi")).toBe(guideVi);
    expect(guideFor("en")).toBe(guideEn);
    expect(guideFor("ja")).toBe(guideEn);
  });
});
