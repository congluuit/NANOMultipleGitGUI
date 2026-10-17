import i18n from "i18next";
import { errorMessage, type Outcome, type TransferKind } from "@/ipc";
import { settleOneShot, type OneShot } from "@/features/accounts/switcher/oneShot";
import { failureText, runOutcome } from "@/features/commits/outcome";
import { cancelQueued } from "@/features/transfers/actions";
import { useTransferQueueEvents } from "@/features/transfers/events";
import type { Notice } from "@/stores/ui";
import {
  selectRepoRunning,
  setRetry,
  useTransferQueue,
  type QueueItem,
} from "@/features/transfers/store";
import { newOpId } from "./plan";

export type RunningTransfer = QueueItem;

/** First running transfer of a repository (one at a time per repo in the toolbar). */
export const useRepoTransfer = (repoId: number | null | undefined) =>
  useTransferQueue((s) => (repoId == null ? null : selectRepoRunning(s, repoId)));

/** Keeps the transfer queue updated from backend progress events. Mount once. */
export function useTransferEvents() {
  useTransferQueueEvents();
}

/**
 * Starts a fetch / pull / push as a queue item (progress display, cancel, retry) and reports
 * the outcome in the notice bar.
 *
 * `shot`: the one-shot "… as" override the caller took (`takeOneShot`) and passed to `call` as
 * `asAccount`. It is given back when the transfer fails or is cancelled, and Retry re-runs with
 * the same account (see `settleOneShot`). The LFS upload and the `git push` of one push share
 * the same shot.
 */
export async function startTransfer<T>(
  repoId: number,
  kind: TransferKind,
  call: (opId: string) => Promise<Outcome<T>>,
  success: (value: T) => string | Notice | null,
  subject: string | null = null,
  shot: OneShot | null = null,
): Promise<T | undefined> {
  const opId = newOpId(kind);
  useTransferQueue.getState().start({ opId, repoId, kind, subject });
  const again = () => void startTransfer(repoId, kind, call, success, subject, shot);
  setRetry(opId, again);
  let final: Outcome<T> | null = null;
  let thrown: unknown = null;
  const value = await runOutcome(
    repoId,
    async () => {
      try {
        final = await call(opId);
        return final;
      } catch (e) {
        thrown = e;
        throw e;
      }
    },
    success,
    again,
  );
  const queue = useTransferQueue.getState();
  const out = final as Outcome<T> | null;
  // A transfer that reached the server consumed the override, even when it was refused: a
  // later plain Push must not silently run as that account again. Only a cancel gives it back.
  settleOneShot(repoId, shot, out?.status !== "cancelled");
  if (out?.status === "done") {
    const done = success(out.value);
    queue.finish(opId, "done", { text: (typeof done === "string" ? done : done?.message) ?? "" });
  } else if (out?.status === "rejected") {
    queue.finish(opId, "failed", { error: failureText(out.reason, out.message) });
  } else if (out?.status === "cancelled") {
    queue.finish(opId, "cancelled", { text: i18n.t("ops.cancelled") });
  } else {
    queue.finish(opId, "failed", { error: errorMessage(thrown) });
  }
  return value;
}

export const cancelTransfer = cancelQueued;

export const transferLabel = (kind: TransferKind) => i18n.t(`remote.kind.${kind}`);
