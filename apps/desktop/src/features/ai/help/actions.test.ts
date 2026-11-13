import { beforeAll, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import {
  actionsContext,
  chatActions,
  helpActions,
  rememberChatActions,
  splitActions,
  suggested,
} from "./actions";

beforeAll(async () => {
  await initI18n("vi");
});

describe("splitActions", () => {
  it("takes the ACTIONS line off the answer", () => {
    expect(splitActions("Do this.\n\nACTIONS: fix1, fetch")).toEqual({
      text: "Do this.",
      ids: ["fix1", "fetch"],
    });
    expect(splitActions("Steps\n**ACTIONS:** `pull`, [stash], pull, retry, push\n")).toEqual({
      text: "Steps",
      ids: ["pull", "stash", "retry"],
    });
    expect(splitActions("No buttons here.")).toEqual({ text: "No buttons here.", ids: [] });
  });

  it("leaves an ACTIONS word early in the text alone", () => {
    const text = "ACTIONS: fix1\nline 2\nline 3\nline 4\nline 5";
    expect(splitActions(text)).toEqual({ text, ids: [] });
  });
});

describe("helpActions", () => {
  it("lists the notice's fixes, retry and the safe everyday actions", () => {
    const fix = vi.fn();
    const retry = vi.fn();
    const actions = helpActions(
      {
        kind: "error",
        message: "x",
        repoId: 1,
        retry,
        actions: [
          { label: "Gộp hai lịch sử (pull)", run: fix },
          { label: "Hỏi AI cách sửa", run: vi.fn() },
        ],
      },
      ["Hỏi AI cách sửa"],
    );
    const ids = actions.map((a) => a.id);
    expect(ids).toEqual(["fix1", "retry", "fetch", "pull", "push", "stash", "lfs", "accounts"]);
    expect(actions[0]!.label).toBe("Gộp hai lịch sử (pull)");
    const context = actionsContext(actions);
    expect(context).toContain('[fix1] the app\'s own fix for this error: "Gộp hai lịch sử (pull)"');
    expect(context).not.toContain("Hỏi AI");

    // The model's ids pick buttons; unknown ids are ignored.
    const picks = suggested(actions, ["retry", "rm -rf", "fix1"]);
    expect(picks.map((a) => a.id)).toEqual(["retry", "fix1"]);
    picks[1]!.run();
    expect(fix).toHaveBeenCalled();

    rememberChatActions("chat-1", actions);
    expect(chatActions("chat-1")).toHaveLength(actions.length);
    expect(chatActions("other")).toEqual([]);
  });

  it("offers the everyday actions again for a chat of an earlier session", () => {
    localStorage.clear();
    rememberChatActions("old-chat", [], "D:/Work/api");
    rememberChatActions("no-repo", [], "");
    const tabs = [{ id: 7, path: "D:/Work/api", missing: false }];
    expect(chatActions("old-chat", tabs).map((a) => a.id)).toEqual([
      "fetch",
      "pull",
      "push",
      "stash",
      "lfs",
      "accounts",
    ]);
    // Its repository is not open: nothing to act on.
    expect(chatActions("old-chat", [])).toEqual([]);
    expect(chatActions("old-chat", [{ ...tabs[0]!, missing: true }])).toEqual([]);
    expect(chatActions("no-repo", tabs).map((a) => a.id)).toEqual(["accounts"]);
    expect(chatActions("unknown", tabs)).toEqual([]);
  });

  it("only offers the account manager without a repository", () => {
    expect(helpActions({ kind: "error", message: "x" }).map((a) => a.id)).toEqual(["accounts"]);
  });
});
