/**
 * A push refused because a file is too big: retrying sends the same file and waits minutes for
 * the same refusal. Instead, find the big files in the commits that are not on the server yet
 * (with the limit the server reported), move them to Git LFS (or take them out of those commits
 * while keeping them on disk) and push again, all from one click.
 */
import i18n from "i18next";
import { confirmLfsPush, push as pushRefs } from "@/features/command-palette/actions";
import { failureText } from "@/features/commits/outcome";
import { canMigrate, migratePaths, sharedBlobs } from "@/features/guards/model";
import {
  learnLimit,
  learnedLimit,
  parseRejectedPaths,
  parseServerLimit,
} from "@/features/guards/limits";
import { runPushGuard } from "@/features/guards/store";
import { pushPlan } from "@/features/remote/plan";
import { repoKeys } from "@/features/repos/queries";
import { invalidateRepo } from "@/features/worktree/queries";
import {
  commands,
  errorMessage,
  type LargeFileFix,
  type LargeFileFixOutcome,
  type PushGuardReport,
  type RepoRefs,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi, type Notice, type NoticeAction } from "@/stores/ui";

/** Long enough for a scan / rewrite of a big repository; replaced by the next notice. */
const WORKING_MS = 120_000;

function freshRefs(repoId: number): Promise<RepoRefs> {
  return queryClient.fetchQuery({
    queryKey: repoKeys.refs(repoId),
    queryFn: () => commands.repoRefs(repoId),
    staleTime: 0,
  });
}

const notify = (notice: Notice) => useUi.getState().showNotice(notice);

/** Git's output of a failure notice (headline + details). */
export function noticeText(notice: Pick<Notice, "message" | "detail">): string {
  return [notice.message, notice.detail].filter(Boolean).join("\n");
}

/** Button labels for a refused push of big files. */
export function largeFileActions(repoId: number, source: Notice): NoticeAction[] {
  return [
    {
      label: i18n.t("largeFiles.toLfs"),
      run: () => void fixLargePush(repoId, source, "lfs"),
    },
    {
      label: i18n.t("largeFiles.drop"),
      run: () => void fixLargePush(repoId, source, "drop"),
    },
  ];
}

/** Pushes again after the fix: LFS objects first when they were just made. */
async function pushAgain(repoId: number, mode: LargeFileFix) {
  const refs = await freshRefs(repoId);
  const plan = pushPlan(refs);
  if (!plan) return;
  if (mode === "lfs") await confirmLfsPush(repoId, plan);
  else await pushRefs(repoId, refs, false, false);
}

function doneNotice(
  repoId: number,
  mode: LargeFileFix,
  count: number,
  out: LargeFileFixOutcome,
): Notice {
  const hints: string[] = [];
  if (mode === "drop") hints.push(i18n.t("largeFiles.dropHint"));
  if (out.ignored.length) hints.push(i18n.t("largeFiles.ignored", { count: out.ignored.length }));
  if (out.aside.kept) hints.push(i18n.t("largeFiles.stashKept"));
  return {
    kind: "success",
    repoId,
    message: i18n.t(mode === "lfs" ? "largeFiles.movedPushing" : "largeFiles.droppedPushing", {
      count,
    }),
    hint: hints.join("\n") || undefined,
    durationMs: 15_000,
    actions: [
      {
        label: i18n.t("largeFiles.undo"),
        run: () =>
          void commands
            .guardUndo(repoId, out.rewrite.backupRef)
            .then((r) => {
              invalidateRepo(repoId);
              notify(
                r.status === "done"
                  ? {
                      kind: "info",
                      repoId,
                      message: i18n.t("guards.push.undone", { branch: r.value.branch }),
                    }
                  : {
                      kind: "error",
                      repoId,
                      message:
                        r.status === "rejected"
                          ? failureText(r.reason, r.message)
                          : i18n.t("ops.cancelled"),
                    },
              );
            })
            .catch((e) => notify({ kind: "error", repoId, message: errorMessage(e) })),
      },
    ],
  };
}

/** Which files to fix: every big file of the unpushed commits the guard found. */
export function filesToFix(report: PushGuardReport): string[] {
  return migratePaths(report);
}

/**
 * The one-click fix. `source`: the failure notice (its text names the server's limit and the
 * files). Falls back to the Push Guard dialog when the files cannot be rewritten here (another
 * branch is checked out, or the commits are already on the server).
 */
export async function fixLargePush(repoId: number, source: Notice, mode: LargeFileFix) {
  const text = noticeText(source);
  let refs: RepoRefs;
  try {
    refs = await freshRefs(repoId);
  } catch (e) {
    notify({ kind: "error", repoId, message: errorMessage(e) });
    return;
  }
  const plan = pushPlan(refs);
  if (!plan) {
    notify({ kind: "error", repoId, message: i18n.t("largeFiles.noBranch") });
    return;
  }
  const reported = parseServerLimit(text);
  if (reported) learnLimit(repoId, plan.remote, reported);
  const limit = reported ?? learnedLimit(repoId, plan.remote);

  notify({ kind: "info", repoId, message: i18n.t("largeFiles.scanning"), durationMs: WORKING_MS });
  let report: PushGuardReport;
  try {
    const out = await commands.pushGuard(repoId, plan.remote, plan.branch, limit);
    if (out.status !== "done") {
      notify({
        kind: "error",
        repoId,
        message:
          out.status === "rejected"
            ? failureText(out.reason, out.message)
            : i18n.t("ops.cancelled"),
      });
      return;
    }
    report = out.value;
  } catch (e) {
    notify({ kind: "error", repoId, message: errorMessage(e) });
    return;
  }

  const paths = filesToFix(report);
  if (paths.length === 0) {
    if (sharedBlobs(report).length > 0) {
      // Only in commits already on the server: the dialog explains what is possible.
      void runPushGuard(repoId, plan.remote, plan.branch, limit);
      return;
    }
    const named = parseRejectedPaths(text);
    notify({
      kind: "error",
      repoId,
      message: i18n.t("largeFiles.notFound"),
      hint: i18n.t("largeFiles.notFoundHint", { files: named.join(", ") || "-" }),
      detail: text,
    });
    return;
  }
  if (!canMigrate(report)) {
    if ((await runPushGuard(repoId, plan.remote, plan.branch, limit)) === "proceed") {
      await pushAgain(repoId, mode);
    }
    return;
  }

  notify({
    kind: "info",
    repoId,
    message: i18n.t(mode === "lfs" ? "largeFiles.moving" : "largeFiles.dropping", {
      count: paths.length,
      files: paths.join(", "),
    }),
    durationMs: WORKING_MS,
  });
  try {
    const out = await commands.guardFixLargeFiles(repoId, plan.remote, plan.branch, paths, mode);
    invalidateRepo(repoId);
    if (out.status === "cancelled") return;
    if (out.status === "rejected") {
      notify({
        kind: "error",
        repoId,
        reason: out.reason,
        message: i18n.t("largeFiles.failed"),
        hint: failureText(out.reason, out.message),
        // Moving to LFS failed (no git-lfs, server without LFS): taking them out still works.
        actions:
          mode === "lfs"
            ? [
                {
                  label: i18n.t("largeFiles.drop"),
                  run: () => void fixLargePush(repoId, source, "drop"),
                },
              ]
            : undefined,
      });
      return;
    }
    notify(doneNotice(repoId, mode, paths.length, out.value));
  } catch (e) {
    invalidateRepo(repoId);
    notify({
      kind: "error",
      repoId,
      message: i18n.t("largeFiles.failed"),
      detail: errorMessage(e),
    });
    return;
  }
  await pushAgain(repoId, mode);
}
