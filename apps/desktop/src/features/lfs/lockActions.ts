import i18n from "i18next";
import {
  commands,
  errorMessage,
  type LockFailure,
  type LockOutcome,
  type TransferSummary,
} from "@/ipc";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { newOpId } from "@/features/remote/plan";
import { runLfsPull } from "@/features/transfers/actions";
import { formatBytes } from "@/features/transfers/format";
import { setRetry, useTransferQueue } from "@/features/transfers/store";
import { invalidateRepo } from "@/features/worktree/queries";
import { useUi } from "@/stores/ui";
import { invalidateLfs, invalidateLfsLocks } from "./queries";

function notifyFailure(failure: LockFailure, path: string | null) {
  const actions =
    failure.kind === "authRequired" || failure.kind === "noPermission"
      ? [
          {
            label: i18n.t("failureHelp.action.accounts"),
            run: () => useAccountDialogs.getState().openManager(),
          },
        ]
      : [];
  useUi.getState().showNotice({
    kind: "error",
    message: i18n.t(`lfsLocks.failure.${failure.kind}`, { path: path ?? "" }),
    hint: i18n.t(`lfsLocks.help.${failure.kind}`),
    detail: failure.message || undefined,
    actions,
  });
}

async function runLock<T>(
  repoId: number,
  path: string | null,
  call: () => Promise<LockOutcome<T>>,
  success: string,
): Promise<T | undefined> {
  try {
    const out = await call();
    if (out.status === "done") {
      useUi.getState().showNotice({ kind: "info", message: success });
      return out.value;
    }
    notifyFailure(out.failure, path);
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
  } finally {
    invalidateLfsLocks(repoId);
    invalidateRepo(repoId);
  }
  return undefined;
}

export const lockFile = (repoId: number, path: string) =>
  runLock(
    repoId,
    path,
    () => commands.lfsLock(repoId, path),
    i18n.t("lfsLocks.lockedOk", { path }),
  );

/** Unlock (own lock) or force unlock (someone else's: needs permission on the server). */
export const unlockFile = (
  repoId: number,
  target: { path: string | null; id: string | null },
  force: boolean,
) =>
  runLock(
    repoId,
    target.path,
    () => commands.lfsUnlock(repoId, target.path, target.id, force),
    i18n.t(force ? "lfsLocks.forceUnlockedOk" : "lfsLocks.unlockedOk", {
      path: target.path ?? target.id ?? "",
    }),
  );

export async function setLockable(repoId: number, pattern: string, lockable: boolean) {
  try {
    await commands.lfsSetLockable(repoId, pattern, lockable);
    useUi.getState().showNotice({
      kind: "info",
      message: i18n.t(lockable ? "lfsLocks.markedLockable" : "lfsLocks.unmarkedLockable", {
        pattern,
      }),
    });
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
  } finally {
    invalidateLfs(repoId);
    invalidateRepo(repoId);
  }
}

/**
 * Downloads selected pointer files: one file through the usual "Tải file này" flow, several
 * in one `git lfs pull --include=a,b,…` queue item after a disk-space check (returns
 * `"noSpace"` with the sizes so the caller can ask before `force`).
 */
export async function downloadFiles(
  repoId: number,
  paths: string[],
  force = false,
): Promise<"started" | { noSpace: { needed: number; free: number } }> {
  if (paths.length === 1) {
    void runLfsPull(repoId, paths[0]!, force).then(() => invalidateLfs(repoId));
    return "started";
  }
  if (!force) {
    try {
      const est = await commands.lfsDownloadEstimate(repoId, { paths });
      if (est.status === "done" && !est.value.enoughSpace) {
        return { noSpace: { needed: est.value.neededBytes, free: est.value.freeBytes ?? 0 } };
      }
    } catch {
      // The download reports its own errors.
    }
  }
  void runPullFiles(repoId, paths);
  return "started";
}

async function runPullFiles(repoId: number, paths: string[]): Promise<TransferSummary | undefined> {
  const opId = newOpId("lfsDownload");
  const queue = useTransferQueue.getState();
  queue.start({
    opId,
    repoId,
    kind: "lfsDownload",
    subject: i18n.t("lfsFiles.nFiles", { count: paths.length }),
  });
  setRetry(opId, () => void runPullFiles(repoId, paths));
  try {
    // Space was checked (or the user chose to continue) before.
    const out = await commands.lfsPullFiles(repoId, opId, paths, true, null);
    if (out.status === "done") {
      const { files, bytes } = out.value;
      useTransferQueue.getState().finish(opId, "done", {
        text: i18n.t("transfers.downloaded", { files, size: formatBytes(bytes) }),
      });
      return out.value;
    }
    if (out.status === "failed") {
      useTransferQueue.getState().finish(opId, "failed", { failure: out.failure });
    } else {
      useTransferQueue
        .getState()
        .finish(opId, "cancelled", { text: i18n.t("transfers.cancelled") });
    }
  } catch (e) {
    useTransferQueue.getState().finish(opId, "failed", { error: errorMessage(e) });
  } finally {
    invalidateRepo(repoId);
    invalidateLfs(repoId);
  }
  return undefined;
}
