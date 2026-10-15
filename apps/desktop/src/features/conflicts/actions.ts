import i18n from "i18next";
import { runOutcome } from "@/features/commits/outcome";
import { worktreeKeys } from "@/features/worktree/queries";
import { commands, type ConflictSide, type RepoOperation, type WorkingTreeStatus } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { closeFileView, openFileView } from "@/stores/fileView";
import { useUi } from "@/stores/ui";

export const conflictKeys = {
  all: (repoId: number) => ["conflict", repoId] as const,
  file: (repoId: number, path: string) => ["conflict", repoId, "file", path] as const,
  context: (repoId: number) => ["conflict", repoId, "context"] as const,
  preview: (repoId: number, path: string, stage: number) =>
    ["conflict", repoId, "preview", path, stage] as const,
};

/** Paths still conflicted after a refresh of the status. */
async function remainingConflicts(repoId: number): Promise<string[]> {
  await queryClient.invalidateQueries({ queryKey: worktreeKeys.status(repoId) });
  const st = await queryClient
    .fetchQuery({
      queryKey: worktreeKeys.status(repoId),
      queryFn: () => commands.worktreeStatus(repoId),
      staleTime: 0,
    })
    .catch(() => null as WorkingTreeStatus | null);
  return st?.conflicted.map((e) => e.path) ?? [];
}

/** After a file was resolved: open the next conflicted file, or close and say what is next. */
export async function afterResolved(repoId: number, path: string) {
  void queryClient.invalidateQueries({ queryKey: conflictKeys.all(repoId) });
  const left = (await remainingConflicts(repoId)).filter((p) => p !== path);
  const notify = useUi.getState().showNotice;
  if (left.length > 0) {
    openFileView(repoId, { kind: "conflict", path: left[0]! });
    notify({
      kind: "info",
      message: i18n.t("conflict.resolvedNext", { path, count: left.length }),
    });
    return;
  }
  closeFileView(repoId);
  const state = await commands.repoOperationState(repoId).catch(() => null);
  const op: RepoOperation | null | undefined = state?.operation;
  notify({
    kind: "info",
    message:
      op && op !== "bisect"
        ? i18n.t("conflict.allResolved", { op: i18n.t(`ops.kind.${op}`) })
        : i18n.t("conflict.allResolvedNoOp"),
  });
}

/** Saves edited content and marks the file resolved. */
export async function saveResolution(repoId: number, path: string, content: string) {
  const done = await runOutcome(repoId, () =>
    commands.conflictResolveContent(repoId, path, content),
  );
  if (done !== undefined) await afterResolved(repoId, path);
  return done !== undefined;
}

/** Keeps one side of the file as a whole (or deletes it when that side deleted it). */
export async function takeSide(repoId: number, path: string, side: ConflictSide) {
  const done = await runOutcome(repoId, () => commands.conflictTakeSide(repoId, path, side));
  if (done !== undefined) await afterResolved(repoId, path);
  return done !== undefined;
}

export async function deleteConflicted(repoId: number, path: string) {
  const done = await runOutcome(repoId, () => commands.conflictDeleteFile(repoId, path));
  if (done !== undefined) await afterResolved(repoId, path);
  return done !== undefined;
}
