/**
 * Commit Guard & Push Guard (PLAN §4.6, §6.3) — imperative APIs + dialogs.
 *
 * `GuardDialogsHost` is mounted once in `App.tsx`. Both APIs are advisory and fail open: if the
 * check itself errors (or the repo/branch is invalid) they resolve `"proceed"` and the real
 * git command reports the problem.
 *
 * ## Wiring — commit flow (CommitBox submit, "Commit as…")
 * Call it FIRST, before the Large File Guard (`useLargeFileGuard().intercept`):
 * ```ts
 * import { runCommitGuard } from "@/features/guards";
 *
 * const decision = await runCommitGuard(repoId);
 * if (decision === "cancel") return;            // keep the message, do nothing
 * // "switched": the repo was re-bound to another account (bindRepo already notified and
 * // invalidated the identity query) — continue, the backend resolves the new identity.
 * // "proceed": commit anyway / nothing to report.
 * if (await largeFileGuard.intercept(paths, doCommit)) return;
 * await doCommit();
 * ```
 * Skip it for a one-shot "Commit as <account>" (`takeOneShot`): the user already chose the
 * account explicitly.
 *
 * ## Wiring — push flow (toolbar Push, palette "Push", "Push as…")
 * Call it after the remote/branch are known and BEFORE the LFS estimate
 * (`lfs_push_estimate`) / transfer is queued:
 * ```ts
 * import { runPushGuard } from "@/features/guards";
 *
 * if ((await runPushGuard(repoId, remote, branch)) === "cancel") return;
 * // then: LFS estimate → LFS upload → git push (unchanged)
 * ```
 * The dialog may rewrite unpushed commits ("Sửa tác giả", "Chuyển sang LFS"); it always
 * creates `refs/nanomgg/backup/<ts>` first, offers Undo, and refreshes the repo queries.
 * After `"proceed"`, push the branch as usual (HEAD may have changed — use the branch name,
 * not a cached oid). "Push anyway" is disabled when the provider denies push access or a
 * blob exceeds the hard limit, so `"proceed"` never means "push that will surely fail"
 * unless the check could not run.
 *
 * Backups: `commands.guardBackups(repoId)`, `commands.guardUndo(repoId, ref)`,
 * `commands.guardBackupDelete(repoId, ref)` (e.g. for a future "Backups" panel).
 */
export { CommitGuardDialog } from "./CommitGuardDialog";
export { GuardDialogsHost } from "./GuardDialogsHost";
export { PushGuardDialog } from "./PushGuardDialog";
export type { CommitGuardDecision, PushGuardDecision } from "./model";
export { runCommitGuard, runPushGuard, useGuardDialogs } from "./store";
