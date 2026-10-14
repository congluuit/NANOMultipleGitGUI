import { describe, expect, it } from "vitest";
import type { ToolInfo } from "@/ipc";
import { toolProblems } from "./toolProblems";

const tool = (
  version: [number, number, number] | null,
  ok: boolean,
  min: [number, number, number],
): ToolInfo => ({
  version: version && { major: version[0], minor: version[1], patch: version[2] },
  raw: null,
  minimum: { major: min[0], minor: min[1], patch: min[2] },
  ok,
});

describe("toolProblems", () => {
  it("reports nothing when both tools are fine", () => {
    expect(
      toolProblems({
        git: tool([2, 47, 0], true, [2, 35, 0]),
        gitLfs: tool([3, 5, 1], true, [3, 0, 0]),
      }),
    ).toEqual({
      blocking: null,
      warning: null,
    });
  });

  it("treats git as blocking and git-lfs as a warning", () => {
    const r = toolProblems({
      git: tool([2, 30, 1], false, [2, 35, 0]),
      gitLfs: tool(null, false, [3, 0, 0]),
    });
    expect(r.blocking).toEqual({ tool: "git", issue: "old", version: "2.30.1", min: "2.35.0" });
    expect(r.warning).toEqual({ tool: "lfs", issue: "missing", version: "", min: "3.0.0" });
  });
});
