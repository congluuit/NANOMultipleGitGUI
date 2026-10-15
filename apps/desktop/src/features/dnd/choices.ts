import type { BranchRelation } from "@/ipc";

export type DropChoice = "fastForward" | "merge" | "rebase";

/** Items of the drop menu for a known relation (tested on its own). */
export function dropChoices(rel: BranchRelation | null): DropChoice[] {
  if (!rel) return ["merge", "rebase"];
  const out: DropChoice[] = [];
  if (rel.canFastForward) out.push("fastForward");
  if (rel.ahead > 0) out.push("merge");
  out.push("rebase");
  return out;
}
