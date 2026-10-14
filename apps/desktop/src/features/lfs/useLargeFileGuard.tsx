import { useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import i18n from "i18next";
import { useRepoRefs } from "@/features/repos/queries";
import { invalidateWorktree } from "@/features/worktree/queries";
import { commands, errorMessage, type GuardReport } from "@/ipc";
import { useUi } from "@/stores/ui";
import { guessProvider, relevantFindings } from "./guard";
import { LargeFileGuardDialog } from "./LargeFileGuardDialog";
import { invalidateLfs } from "./queries";

interface Review {
  report: GuardReport;
  /** Continues the held-back commit. */
  proceed: () => void;
  seq: number;
}

/**
 * Large File Guard for the commit box: `intercept(paths, proceed)` runs `large_file_check`
 * and, when something should go to LFS, holds the commit back and shows the dialog
 * (`dialog` must be rendered). Returns whether the commit was held back.
 */
export function useLargeFileGuard(repoId: number) {
  const refs = useRepoRefs(repoId);
  const [review, setReview] = useState<Review | null>(null);
  const [busy, setBusy] = useState(false);

  const intercept = async (paths: string[], proceed: () => void): Promise<boolean> => {
    if (paths.length === 0 || !isTauri()) return false;
    try {
      const report = await commands.largeFileCheck(repoId, paths, guessProvider(refs.data), null);
      if (relevantFindings(report).length === 0) return false;
      setReview((prev) => ({ report, proceed, seq: (prev?.seq ?? 0) + 1 }));
      return true;
    } catch (e) {
      // The guard is advisory: never block a commit because the check itself failed.
      console.warn("large file check failed", e);
      return false;
    }
  };

  const close = () => setReview(null);

  const continueCommit = () => {
    const proceed = review?.proceed;
    close();
    proceed?.();
  };

  const track = async (patterns: string[], paths: string[]) => {
    setBusy(true);
    try {
      for (const pattern of patterns) await commands.lfsTrack(repoId, pattern, false);
      await commands.lfsRestage(repoId, paths);
    } catch (e) {
      useUi.getState().showNotice({
        kind: "error",
        message: i18n.t("guard.trackFailed", { message: errorMessage(e) }),
      });
      return;
    } finally {
      setBusy(false);
      invalidateLfs(repoId);
      invalidateWorktree(repoId);
    }
    continueCommit();
  };

  const dialog = (
    <LargeFileGuardDialog
      key={review?.seq ?? 0}
      report={review?.report ?? null}
      busy={busy}
      onTrack={(patterns, paths) => void track(patterns, paths)}
      onCommitAnyway={continueCommit}
      onCancel={close}
    />
  );

  return { intercept, dialog };
}
