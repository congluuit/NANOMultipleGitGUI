import i18n from "i18next";
import { commands, errorMessage } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { invalidateWorktree } from "@/features/worktree/queries";
import { useUi } from "@/stores/ui";
import { invalidateLfs, lfsKeys } from "./queries";

function notify(kind: "info" | "error", message: string) {
  useUi.getState().showNotice({ kind, message });
}

/** `git lfs track`; `.gitattributes` changes, so status and the LFS panel are re-read. */
export async function trackPattern(
  repoId: number,
  pattern: string,
  lockable: boolean,
): Promise<boolean> {
  try {
    const changed = await commands.lfsTrack(repoId, pattern, lockable);
    notify("info", i18n.t(changed ? "lfs.tracked" : "lfs.alreadyTracked", { pattern }));
    return true;
  } catch (e) {
    notify("error", errorMessage(e));
    return false;
  } finally {
    invalidateLfs(repoId);
    invalidateWorktree(repoId);
  }
}

export async function untrackPattern(repoId: number, pattern: string): Promise<void> {
  try {
    await commands.lfsUntrack(repoId, pattern);
    notify("info", i18n.t("lfs.untracked", { pattern }));
  } catch (e) {
    notify("error", errorMessage(e));
  } finally {
    invalidateLfs(repoId);
    invalidateWorktree(repoId);
  }
}

/** `git lfs install --local`; the command returns the refreshed status. */
export async function installLfsHooks(repoId: number): Promise<void> {
  try {
    const status = await commands.lfsInstallLocal(repoId);
    queryClient.setQueryData(lfsKeys.status(repoId), status);
    notify("info", i18n.t("lfs.installed"));
  } catch (e) {
    notify("error", errorMessage(e));
    invalidateLfs(repoId);
  }
}
