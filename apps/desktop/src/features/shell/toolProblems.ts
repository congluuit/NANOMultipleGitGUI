import type { ToolInfo, Tools, Version } from "@/ipc";

const v = (x: Version | null | undefined) => (x ? `${x.major}.${x.minor}.${x.patch}` : "");

export type ToolProblem = {
  tool: "git" | "lfs";
  issue: "missing" | "old";
  version: string;
  min: string;
} | null;

function problemOf(tool: "git" | "lfs", info: ToolInfo): ToolProblem {
  if (info.ok) return null;
  return {
    tool,
    issue: info.version ? "old" : "missing",
    version: v(info.version),
    min: v(info.minimum),
  };
}

/** Git problems block everything; git-lfs problems are only a warning. */
export function toolProblems(tools: Tools): { blocking: ToolProblem; warning: ToolProblem } {
  return { blocking: problemOf("git", tools.git), warning: problemOf("lfs", tools.gitLfs) };
}
