import i18n from "i18next";
import {
  commands,
  errorMessage,
  type LfsOutcome,
  type TransferOptions,
  type TransferSummary,
} from "@/ipc";
import { repoAccountSummary } from "@/features/accounts/switcher/hooks";
import type { AccountSummary } from "@/features/accounts/switcher/model";
import { asAccountOf, settleOneShot, type OneShot } from "@/features/accounts/switcher/oneShot";
import { newOpId } from "@/features/remote/plan";
import { invalidateRepo } from "@/features/worktree/queries";
import { useUi } from "@/stores/ui";
import { formatBytes } from "./format";
import { setRetry, useTransferQueue, type QueueKind } from "./store";

/**
 * Account a repository's transfers run as (from the account-context query cache, so `null`
 * until the account pill has loaded it — and for unbound repos).
 */
export const repoAccount = (repoId: number): AccountSummary | null => repoAccountSummary(repoId);

export const accountLabel = (account: AccountSummary | null) =>
  account?.label ?? i18n.t("transfers.noAccount");

/**
 * Runs an LFS command as a queue item: progress comes from `lfsTransferProgress` events, the
 * outcome sets the final state. Failed/cancelled items get a Retry that runs `again`.
 * `shot` (a taken one-shot override, passed to `call` as `asAccount`) is given back on
 * failure / cancel (`settleOneShot`).
 */
async function runLfs(
  repoId: number,
  kind: QueueKind,
  subject: string | null,
  call: (opId: string) => Promise<LfsOutcome<TransferSummary>>,
  again: () => Promise<TransferSummary | undefined>,
  shot: OneShot | null = null,
): Promise<TransferSummary | undefined> {
  const opId = newOpId(kind);
  const queue = useTransferQueue.getState();
  queue.start({ opId, repoId, kind, subject });
  setRetry(opId, () => void again());
  let cancelled = false;
  try {
    const out = await call(opId);

    if (out.status === "done") {
      const { files, bytes } = out.value;
      const text =
        kind === "lfsUpload"
          ? files
            ? i18n.t("transfers.lfsUploaded", { files, size: formatBytes(bytes) })
            : i18n.t("transfers.lfsNothingToUpload")
          : i18n.t("transfers.downloaded", { files, size: formatBytes(bytes) });
      useTransferQueue.getState().finish(opId, "done", { text });
      return out.value;
    }
    if (out.status === "failed") {
      useTransferQueue.getState().finish(opId, "failed", { failure: out.failure });
    } else {
      cancelled = true;
      useTransferQueue
        .getState()
        .finish(opId, "cancelled", { text: i18n.t("transfers.cancelled") });
    }
  } catch (e) {
    useTransferQueue.getState().finish(opId, "failed", { error: errorMessage(e) });
  } finally {
    // Consumed once the transfer ran (done or refused); only a cancel gives it back.
    settleOneShot(repoId, shot, !cancelled);
    invalidateRepo(repoId);
  }
  return undefined;
}

/**
 * `git lfs push <remote> <branch>` as a queue item. `then` runs after a successful upload,
 * also when it succeeds through Retry (the push flow continues with `git push`). `shot`: the
 * one-shot "Push as…" override taken for the whole push (the `git push` after it uses it too).
 */
export async function runLfsPush(
  repoId: number,
  remote: string,
  branch: string,
  then?: () => void,
  options: TransferOptions | null = null,
  shot: OneShot | null = null,
): Promise<TransferSummary | undefined> {
  const summary = await runLfs(
    repoId,
    "lfsUpload",
    `${remote}/${branch}`,
    (op) => commands.lfsPush(repoId, op, remote, branch, options, asAccountOf(shot)),
    () => runLfsPush(repoId, remote, branch, then, options, shot),
    shot,
  );
  if (summary) then?.();
  return summary;
}

/**
 * `git lfs pull` (whole checkout, or one pointer file with `include`). Also the API for the
 * clone option "download later" and the "Tải file này" button on pointer files. `shot`: a taken
 * one-shot override to download as (none by default: background downloads never consume one).
 */
export function runLfsPull(
  repoId: number,
  include: string | null = null,
  ignoreDiskSpace = false,
  options: TransferOptions | null = null,
  shot: OneShot | null = null,
): Promise<TransferSummary | undefined> {
  return runLfs(
    repoId,
    "lfsDownload",
    include,
    (op) => commands.lfsPull(repoId, op, include, ignoreDiskSpace, options, asAccountOf(shot)),
    () => runLfsPull(repoId, include, ignoreDiskSpace, options, shot),
    shot,
  );
}

/** "Tải file này": downloads one LFS pointer file. */
export const downloadLfsFile = (repoId: number, path: string) => runLfsPull(repoId, path);

/** `git lfs fetch --all` as a queue item. */
export function runLfsFetchAll(
  repoId: number,
  ignoreDiskSpace = false,
  options: TransferOptions | null = null,
): Promise<TransferSummary | undefined> {
  return runLfs(
    repoId,
    "lfsDownload",
    "--all",
    (op) => commands.lfsFetchAll(repoId, op, ignoreDiskSpace, options),
    () => runLfsFetchAll(repoId, ignoreDiskSpace, options),
  );
}

/** Same download again, ignoring the disk-space check (the "Download anyway" action). */
export function downloadAnyway(item: { repoId: number; subject: string | null }) {
  return item.subject === "--all"
    ? runLfsFetchAll(item.repoId, true)
    : runLfsPull(item.repoId, item.subject, true);
}

export async function cancelQueued(opId: string) {
  useTransferQueue.getState().patch(opId, { cancelling: true });
  await commands.transferCancel(opId).catch(() => false);
}

/** Shows an LFS estimate failure in the notice bar. */
export function notifyEstimateFailure(message: string) {
  useUi
    .getState()
    .showNotice({ kind: "error", message: i18n.t("transfers.estimateFailed", { message }) });
}
