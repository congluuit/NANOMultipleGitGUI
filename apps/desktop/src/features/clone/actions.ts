import i18n from "i18next";
import {
  commands,
  errorMessage,
  events,
  type CloneProgress,
  type CloneRequest,
  type CloneResult,
  type FailureKind,
} from "@/ipc";
import { invalidateAccounts } from "@/features/accounts/queries";
import { failureText } from "@/features/commits/outcome";
import { newOpId } from "@/features/remote/plan";
import { runLfsPull } from "@/features/transfers/actions";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";

export type CloneRun =
  | { status: "done"; result: CloneResult }
  | { status: "rejected"; reason: FailureKind; message: string }
  | { status: "cancelled" }
  | { status: "failed"; message: string };

/**
 * Runs `clone_start` with progress events for its operation id. On success the new tab is
 * focused, account queries refresh (new binding) and, for "download LFS later", `git lfs pull`
 * starts in the transfer queue.
 */
export async function runClone(
  request: CloneRequest,
  onStart: (opId: string) => void,
  onProgress: (p: CloneProgress) => void,
): Promise<CloneRun> {
  const opId = newOpId("clone");
  onStart(opId);
  let unlisten: (() => void) | null = null;
  try {
    unlisten = await events.cloneProgress.listen(({ payload }) => {
      if (payload.opId === opId) onProgress(payload);
    });
  } catch {
    // No event channel (browser preview): run without progress.
  }
  try {
    const out = await commands.cloneStart(opId, request);
    if (out.status !== "done") return out;
    await afterClone(out.value);
    return { status: "done", result: out.value };
  } catch (e) {
    return { status: "failed", message: errorMessage(e) };
  } finally {
    unlisten?.();
  }
}

async function afterClone(result: CloneResult) {
  const { tab } = result;
  try {
    // Already registered by the backend: this focuses the tab and adds it to the store.
    await useWorkspace.getState().open(tab.path);
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
  }
  void queryClient.invalidateQueries({ queryKey: ["recent"] });
  invalidateAccounts();
  if (result.lfsPending) {
    void runLfsPull(tab.id);
  }
  if (result.lfsError) {
    useUi.getState().showNotice({
      kind: "error",
      message: i18n.t("clone.lfsPartialFailed", { message: result.lfsError }),
    });
  } else {
    useUi
      .getState()
      .showNotice({ kind: "info", message: i18n.t("clone.done", { name: tab.name }) });
  }
}

export async function cancelClone(opId: string) {
  await commands.transferCancel(opId).catch(() => false);
}

/** Message for a refused or failed clone. */
export function cloneFailureText(run: CloneRun): string | null {
  if (run.status === "rejected") return failureText(run.reason, run.message);
  if (run.status === "failed") return run.message;
  return null;
}
