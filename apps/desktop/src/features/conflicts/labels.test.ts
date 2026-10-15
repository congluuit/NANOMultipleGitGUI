import { beforeAll, describe, expect, it } from "vitest";
import i18n from "i18next";
import { initI18n } from "@/i18n";
import type { ConflictContext, ConflictOrigin } from "@/ipc";
import { labelVars, sideLabels } from "./labels";

const ctx = (patch: Partial<ConflictContext>): ConflictContext => ({
  origin: "merge",
  currentBranch: "main",
  ours: { oid: "1111111aaaa", summary: "main work", refs: ["main"] },
  theirs: { oid: "2222222bbbb", summary: "feature work", refs: ["feature"] },
  theirsName: "feature",
  rebaseBranch: null,
  rebaseOnto: [],
  conflicts: ["a.txt"],
  ...patch,
});

describe("sideLabels", () => {
  beforeAll(() => {
    initI18n("vi");
  });

  it("names the current branch and the merged branch during a merge", () => {
    const l = sideLabels(ctx({}), i18n.t);
    expect(l.ours).toContain("nhánh hiện tại main");
    expect(l.theirs).toContain("feature");
    expect(l.oursShort).toBe("của tôi (main)");
    expect(l.oursDetail).toBe("1111111 main work");
  });

  it("explains that a rebase swaps ours and theirs", () => {
    const l = sideLabels(
      ctx({
        origin: "rebase",
        currentBranch: null,
        theirsName: null,
        rebaseBranch: "feature",
        rebaseOnto: ["main"],
      }),
      i18n.t,
    );
    expect(l.ours).toContain("main");
    expect(l.ours).toContain("rebase lên");
    expect(l.theirsShort).toBe("commit của bạn");
    expect(l.theirs).toContain("2222222");
    expect(l.theirs).toContain("feature");
    expect(l.explain).toContain("ĐẢO NGƯỢC");
  });

  it("falls back to short ids and HEAD, for every origin and language", () => {
    expect(labelVars(null)).toEqual({
      branch: "HEAD",
      name: "?",
      onto: "HEAD",
      mine: "?",
      commit: "?",
    });
    expect(labelVars(ctx({ theirsName: null })).name).toBe("feature");
    expect(
      labelVars(ctx({ theirsName: null, theirs: { oid: "abcdef12", summary: "", refs: [] } })).name,
    ).toBe("abcdef1");
    const origins: ConflictOrigin[] = [
      "merge",
      "mergeSquash",
      "rebase",
      "cherryPick",
      "revert",
      "stash",
      "applyMailbox",
      "unknown",
    ];
    for (const lang of ["vi", "en"] as const) {
      void i18n.changeLanguage(lang);
      for (const origin of origins) {
        const l = sideLabels(ctx({ origin }), i18n.t);
        for (const text of [l.ours, l.theirs, l.oursShort, l.theirsShort, l.explain]) {
          expect(text, `${lang} ${origin}`).not.toContain("conflict.origin");
          expect(text).not.toContain("{{");
        }
      }
    }
    void i18n.changeLanguage("vi");
  });
});
