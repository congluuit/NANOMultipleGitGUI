import { useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import i18n from "i18next";
import { invalidateWorktree } from "@/features/worktree/queries";
import { commands, errorMessage, type SecretReport } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { remainingPaths, type SecretPlan } from "./model";
import { prefsKey } from "./prefs";
import { SecretGuardDialog } from "./SecretGuardDialog";

interface Review {
  report: SecretReport;
  staged: string[];
  /** Continues the held-back commit with the paths still staged. */
  proceed: (remaining: string[]) => void;
  seq: number;
}

/**
 * Secret Guard for the commit box: `intercept(stagedPaths, proceed)` scans the staged changes
 * and, when something looks like a secret, holds the commit back and shows the dialog
 * (`dialog` must be rendered). Returns whether the commit was held back. `proceed` receives
 * the staged paths left after the user's choices (files may have been unstaged).
 */
export function useSecretGuard(repoId: number) {
  const [review, setReview] = useState<Review | null>(null);
  const [busy, setBusy] = useState(false);

  const intercept = async (
    staged: string[],
    proceed: (remaining: string[]) => void,
  ): Promise<boolean> => {
    if (!isTauri()) return false;
    try {
      const check = await commands.secretGuardCheck(repoId);
      if (!check.enabled || check.report.findings.length === 0) return false;
      setReview((prev) => ({ report: check.report, staged, proceed, seq: (prev?.seq ?? 0) + 1 }));
      return true;
    } catch (e) {
      // Advisory like the other guards: a failed scan never blocks the commit.
      console.warn("secret guard check failed", errorMessage(e));
      return false;
    }
  };

  const close = () => setReview(null);

  const finish = (remaining: string[]) => {
    const proceed = review?.proceed;
    close();
    proceed?.(remaining);
  };

  const apply = async (plan: SecretPlan) => {
    if (!review) return;
    setBusy(true);
    try {
      if (plan.always.length > 0) {
        queryClient.setQueryData(prefsKey, await commands.secretGuardIgnore(repoId, plan.always));
      }
      if (plan.unstage.length > 0) await commands.unstageFiles(repoId, plan.unstage);
    } catch (e) {
      useUi.getState().showNotice({
        kind: "error",
        message: i18n.t("secretGuard.applyFailed", { message: errorMessage(e) }),
      });
      return;
    } finally {
      setBusy(false);
      if (plan.unstage.length > 0) invalidateWorktree(repoId);
    }
    finish(remainingPaths(review.staged, plan.unstage));
  };

  const dialog = (
    <SecretGuardDialog
      key={review?.seq ?? 0}
      report={review?.report ?? null}
      busy={busy}
      onApply={(plan) => void apply(plan)}
      onCommitAnyway={() => finish(review?.staged ?? [])}
      onCancel={close}
    />
  );

  return { intercept, dialog };
}
