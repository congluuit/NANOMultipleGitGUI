import { describe, expect, it } from "vitest";
import type { PullComment } from "@/ipc";
import { PROVIDERS, makeAccount } from "@/features/accounts/switcher/testData";
import {
  branchLabel,
  familyOfAccount,
  lineThreads,
  parsePatch,
  pullNumber,
  rowAnchor,
  shortTerm,
  splitLabels,
  splitLinks,
  splitNames,
  threadsOfFile,
  timeline,
} from "./model";

function comment(over: Partial<PullComment> & { id: string }): PullComment {
  return {
    kind: "general",
    author: null,
    body: "",
    createdAt: null,
    updatedAt: null,
    threadId: null,
    anchor: null,
    reviewState: null,
    resolved: null,
    webUrl: null,
    ...over,
  };
}

describe("parsePatch", () => {
  it("numbers old and new lines across hunks", () => {
    const rows = parsePatch(
      "@@ -1,3 +1,3 @@ fn\n a\n-b\n+c\n d\n@@ -10 +10,2 @@\n x\n+y\n\\ No newline at end of file\n",
    );
    expect(rows.map((r) => r.kind)).toEqual([
      "hunk",
      "ctx",
      "del",
      "add",
      "ctx",
      "hunk",
      "ctx",
      "add",
      "meta",
    ]);
    const lines = rows.flatMap((r) => ("oldLine" in r ? [[r.oldLine, r.newLine]] : []));
    expect(lines).toEqual([
      [1, 1],
      [2, null],
      [null, 2],
      [3, 3],
      [10, 10],
      [null, 11],
    ]);
  });

  it("ignores text before the first hunk and CRLF", () => {
    const rows = parsePatch("diff --git a b\r\n@@ -1 +1 @@\r\n-a\r\n+b\r\n");
    expect(rows).toHaveLength(3);
    expect(rows[1]).toMatchObject({ kind: "del", text: "a" });
  });

  it("anchors comments on the new side unless the line was deleted", () => {
    const [, ctx, del, add] = parsePatch("@@ -4,2 +7,2 @@\n a\n-b\n+c\n");
    if (ctx?.kind !== "ctx" || del?.kind !== "del" || add?.kind !== "add") throw new Error();
    expect(rowAnchor(ctx)).toEqual({ line: 7, side: "new", pairedLine: 4 });
    expect(rowAnchor(del)).toEqual({ line: 5, side: "old", pairedLine: null });
    expect(rowAnchor(add)).toEqual({ line: 8, side: "new", pairedLine: null });
  });
});

describe("threads", () => {
  const anchor = (line: number | null) => ({
    path: "a.rs",
    line,
    side: "new" as const,
    originalLine: line,
    diffHunk: null,
  });
  const comments: PullComment[] = [
    comment({ id: "issue-1", body: "general" }),
    // GitHub: replies are line comments pointing at the root.
    comment({ id: "line-10", kind: "line", threadId: "10", anchor: anchor(3) }),
    comment({ id: "line-11", kind: "line", threadId: "10", anchor: anchor(3) }),
    // GitLab: a reply in a line discussion has no position.
    comment({ id: "note-1", kind: "line", threadId: "d1", anchor: anchor(null), resolved: false }),
    comment({ id: "note-2", threadId: "d1", resolved: true }),
    // A threaded general discussion stays in the conversation.
    comment({ id: "note-3", threadId: "d2" }),
    comment({ id: "note-4", threadId: "d2" }),
  ];

  it("groups line threads and joins GitLab replies", () => {
    const threads = lineThreads(comments);
    expect(threads.map((t) => [t.id, t.comments.length])).toEqual([
      ["10", 2],
      ["d1", 2],
    ]);
    expect(threads[1]?.resolved).toBe(true);
    const { byLine, outdated } = threadsOfFile(threads, "a.rs");
    expect(byLine.get("new:3")?.[0]?.id).toBe("10");
    expect(outdated.map((t) => t.id)).toEqual(["d1"]);
  });

  it("keeps only thread roots in the timeline", () => {
    expect(timeline(comments).map((c) => c.id)).toEqual([
      "issue-1",
      "line-10",
      "note-1",
      "note-3",
      "note-4",
    ]);
  });
});

describe("display helpers", () => {
  it("names per provider", () => {
    expect(shortTerm("gitlab")).toBe("MR");
    expect(shortTerm(null)).toBe("PR");
    expect(pullNumber("gitlab", 4)).toBe("!4");
    expect(pullNumber("github", 4)).toBe("#4");
    expect(familyOfAccount(makeAccount({ id: "a", providerId: "gl-acme" }), PROVIDERS)).toBe(
      "gitlab",
    );
    expect(familyOfAccount(makeAccount({ id: "a" }), PROVIDERS)).toBe("github");
    expect(familyOfAccount(null, PROVIDERS)).toBeNull();
  });

  it("splits names and labels", () => {
    expect(splitNames(" @bob, alice  bob\ncarol")).toEqual(["bob", "alice", "carol"]);
    expect(splitLabels("good first issue, ui ,, ui")).toEqual(["good first issue", "ui"]);
  });

  it("labels fork branches", () => {
    expect(branchLabel("main", "fork/app", "acme/app")).toBe("fork:main");
    expect(branchLabel("feat", "Acme/App", "acme/app")).toBe("feat");
    expect(branchLabel("feat", null, "acme/app")).toBe("feat");
  });

  it("only turns https links into links", () => {
    expect(splitLinks("see https://x.dev/a. or javascript:alert(1) http://y")).toEqual([
      { text: "see ", url: false },
      { text: "https://x.dev/a", url: true },
      { text: ". or javascript:alert(1) http://y", url: false },
    ]);
  });
});
