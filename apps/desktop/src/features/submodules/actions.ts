import i18n from "i18next";
import { commands, type Outcome } from "@/ipc";
import { runOutcome } from "@/features/commits/outcome";
import { openRepository } from "@/features/repos/actions";
import { startTransfer } from "@/features/remote/transfers";
import { invalidateSubmodules } from "./queries";
import { absolutePath, baseName } from "./model";

/**
 * `git submodule update [--init] [--recursive]` as a transfer (progress + cancel in the status
 * bar). `paths` empty = every submodule.
 */
export async function updateSubmodules(
  repoId: number,
  paths: string[],
  init: boolean,
  recursive = true,
): Promise<boolean> {
  const subject = paths.length === 1 ? baseName(paths[0]!) : i18n.t("submodule.all");
  const out = await startTransfer(
    repoId,
    "fetch",
    (opId) => commands.submoduleUpdate(repoId, opId, paths, init, recursive),
    (v) =>
      v.checkedOut.length
        ? i18n.t("submodule.updated", { count: v.checkedOut.length })
        : i18n.t("submodule.nothingToUpdate"),
    subject,
  );
  invalidateSubmodules(repoId);
  return out !== undefined;
}

export async function addSubmodule(
  repoId: number,
  url: string,
  path: string,
  branch: string | null,
): Promise<boolean> {
  const out = await startTransfer(
    repoId,
    "fetch",
    (opId) => commands.submoduleAdd(repoId, opId, url, path, branch),
    (s) => i18n.t("submodule.added", { path: s.path }),
    baseName(path),
  );
  invalidateSubmodules(repoId);
  return out !== undefined;
}

export async function syncSubmodules(repoId: number, paths: string[]): Promise<void> {
  await runOutcome(
    repoId,
    () => commands.submoduleSync(repoId, paths, true),
    () => i18n.t("submodule.synced"),
  );
  invalidateSubmodules(repoId);
}

/**
 * Deinit / remove. Returns `"dirty"` when the submodule has local changes and `force` was not
 * set, so the caller can ask before discarding them.
 */
export async function removeSubmodule(
  repoId: number,
  path: string,
  mode: "deinit" | "remove",
  force: boolean,
): Promise<"done" | "dirty" | "failed"> {
  const call = () =>
    mode === "deinit"
      ? commands.submoduleDeinit(repoId, path, force)
      : commands.submoduleRemove(repoId, path, force);
  let rejectedDirty = false;
  const value = await runOutcome(
    repoId,
    async (): Promise<Outcome<null>> => {
      const out = await call();
      if (out.status === "rejected" && out.reason === "dirtyWorktree" && !force) {
        rejectedDirty = true;
        return { status: "done", value: null };
      }
      return out;
    },
    () =>
      rejectedDirty
        ? null
        : i18n.t(mode === "deinit" ? "submodule.deinited" : "submodule.removed", { path }),
  );
  invalidateSubmodules(repoId);
  if (rejectedDirty) return "dirty";
  return value === undefined ? "failed" : "done";
}

/** Opens the submodule as its own repository tab. */
export async function openSubmodule(root: string, path: string): Promise<void> {
  await openRepository(absolutePath(root, path));
}
