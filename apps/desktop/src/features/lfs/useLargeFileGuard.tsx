import { useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import i18n from "i18next";
import { useRepoRefs } from "@/features/repos/queries";
import { commands, type GuardReport } from "@/ipc";
import { useUi } from "@/stores/ui";
import { displaySize, guessProvider, relevantFindings } from "./guard";
import { LargeFileGuardDialog } from "./LargeFileGuardDialog";
import { cancelLfsTrack, runLfsTrack, useLfsTrackJob } from "./trackJob";

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
 *
 * "Track with LFS" runs as a background job (`trackJob`): the dialog shows its progress and
 * can be closed meanwhile; the held-back commit runs when the job is done.
 */
export function useLargeFileGuard(repoId: number) {
  const refs = useRepoRefs(repoId);
  const [review, setReview] = useState<Review | null>(null);
  const job = useLfsTrackJob(repoId);

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

  const track = (patterns: string[], paths: string[]) => {
    if (!review) return;
    const { report, proceed } = review;
    const sizes = new Map(report.findings.map((f) => [f.path, displaySize(f)]));
    const files = paths.map((path) => ({ path, size: sizes.get(path) ?? null }));
    void runLfsTrack(repoId, patterns, files, () => {
      // Still open on this review (not sent to the background): close it, then commit.
      setReview((r) => (r?.report === report ? null : r));
      proceed();
    });
  };

  const background = () => {
    close();
    useUi.getState().showNotice({
      kind: "info",
      repoId,
      message: i18n.t("lfsTrack.backgroundStarted"),
      hint: i18n.t("lfsTrack.backgroundHint"),
    });
  };

  const dialog = (
    <LargeFileGuardDialog
      key={review?.seq ?? 0}
      report={review?.report ?? null}
      job={job}
      onTrack={track}
      onCommitAnyway={continueCommit}
      onCancel={close}
      onBackground={background}
      onStop={() => cancelLfsTrack(repoId)}
    />
  );

  return { intercept, dialog, tracking: job };
}
