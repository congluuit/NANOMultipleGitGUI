import { isTauri } from "@tauri-apps/api/core";
import i18n from "i18next";
import { runOutcome, setSuccessHook } from "@/features/commits/outcome";
import { commands, type UndoOutcome } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi, type Notice } from "@/stores/ui";
import { describeOp, isRunnable } from "./label";
import { useUndoStore } from "./store";

/**
 * Undoes (`forward = false`) or redoes the next step of `repoId`. Undoing an operation whose
 * commits are already pushed asks first (only the local branch changes).
 */
export async function runUndo(
  repoId: number,
  forward: boolean,
  confirmed = false,
): Promise<UndoOutcome | undefined> {
  const state = useUndoStore.getState().byRepo[repoId];
  const step = forward ? state?.redo : state?.undo;
  if (!forward && isRunnable(step) && step.pushed && !confirmed) {
    useUndoStore.getState().setConfirm({ repoId, step });
    return undefined;
  }
  const out = await runOutcome(repoId, () =>
    forward ? commands.redoRun(repoId) : commands.undoRun(repoId),
  );
  void queryClient.invalidateQueries({ queryKey: ["undo", repoId] });
  if (!out) return undefined;
  useUndoStore.getState().setState(repoId, out.state);
  const what = describeOp(i18n.t, out.step.op);
  const next = forward ? out.state.undo : out.state.redo;
  useUi.getState().showNotice({
    kind: "info",
    message: i18n.t(forward ? "undo.redone" : "undo.done", { what }),
    actions: isRunnable(next)
      ? [
          {
            label: i18n.t(forward ? "undo.undoAction" : "undo.redoAction"),
            run: () => void runUndo(repoId, !forward),
          },
        ]
      : [],
  });
  return out;
}

/**
 * After an operation succeeds: when it is now the step Undo would revert, its notice gets an
 * "Undo" button (so a wrong commit, reset or branch delete is one click away from undone).
 * Notices that already carry buttons keep their own.
 */
export async function offerUndo(repoId: number, notice: Notice, startedAt: number) {
  if (!isTauri() || notice.kind === "error" || notice.actions?.length) return;
  let state;
  try {
    state = await commands.undoState(repoId);
  } catch {
    return;
  }
  useUndoStore.getState().setState(repoId, state);
  const step = state.undo;
  // A step recorded before this operation started belongs to an earlier one.
  if (!isRunnable(step) || step.time < startedAt - 1) return;
  if (useUi.getState().notice !== notice) return;
  useUi.getState().showNotice({
    ...notice,
    actions: [{ label: i18n.t("undo.undoAction"), run: () => void runUndo(repoId, false) }],
  });
}

setSuccessHook((repoId, notice, startedAt) => void offerUndo(repoId, notice, startedAt));
