import { commands } from "@/ipc";
import { useGraphStore } from "@/stores/graph";
import { useWorktreeStore } from "@/stores/worktree";

/** Puts the staging panel in amend mode with the last commit's message. */
export async function startAmend(repoId: number) {
  useGraphStore.getState().select(repoId, null);
  const last = await commands.headMessage(repoId).catch(() => null);
  const [summary = "", ...rest] = (last ?? "").split("\n");
  useWorktreeStore
    .getState()
    .updateDraft(repoId, { amend: true, summary, body: rest.join("\n").trim(), prefill: last });
}
