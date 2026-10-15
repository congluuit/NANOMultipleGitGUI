import i18n from "i18next";
import { runOutcome } from "@/features/commits/outcome";
import { runUndo } from "@/features/undo/actions";
import { commands, type Outcome, type RebaseOutcome } from "@/ipc";
import { useUi } from "@/stores/ui";

/** Notice for a rebase that went through or stopped (conflicts, `edit`). */
export function reportRebase(repoId: number, o: RebaseOutcome, fallbackBranch?: string | null) {
  const branch = o.branch ?? fallbackBranch ?? "HEAD";
  const notify = useUi.getState().showNotice;
  switch (o.status) {
    case "completed":
      notify({
        kind: "info",
        message: i18n.t(o.autostashConflict ? "rebase.doneAutostashConflict" : "rebase.done", {
          branch,
        }),
        actions: [{ label: i18n.t("rebase.undo"), run: () => void runUndo(repoId, false) }],
      });
      break;
    case "upToDate":
      notify({ kind: "info", message: i18n.t("rebase.upToDate", { branch }) });
      break;
    case "conflicts":
      notify({
        kind: "info",
        message: i18n.t("rebase.conflicts", { count: o.conflicts.length }),
      });
      break;
    case "stopped":
      notify({
        kind: "info",
        message:
          o.step !== null && o.totalSteps !== null
            ? i18n.t("rebase.stopped", { step: o.step, total: o.totalSteps })
            : i18n.t("rebase.stoppedOther"),
        detail: o.message ?? undefined,
      });
      break;
  }
}

/** Runs a rebase command and reports its outcome; returns it when it went through. */
export async function runRebase(
  repoId: number,
  call: () => Promise<Outcome<RebaseOutcome>>,
  branch?: string | null,
): Promise<RebaseOutcome | undefined> {
  const out = await runOutcome(repoId, call);
  if (out) reportRebase(repoId, out, branch);
  return out;
}

export const continueRebase = (repoId: number) =>
  runRebase(repoId, () => commands.rebaseContinue(repoId));

export const skipRebaseCommit = (repoId: number) =>
  runRebase(repoId, () => commands.rebaseSkip(repoId));
