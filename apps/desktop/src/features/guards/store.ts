/**
 * Pending guard reviews + the imperative APIs `runCommitGuard` / `runPushGuard`. The dialogs
 * are rendered once by `GuardDialogsHost`, which settles the pending promise.
 */
import { create } from "zustand";
import { commands, type CommitGuardReport, type PushGuardReport, type RewriteOutcome } from "@/ipc";
import {
  commitNeedsReview,
  pushNeedsReview,
  type CommitGuardDecision,
  type PushGuardDecision,
} from "./model";

export interface CommitReview {
  repoId: number;
  report: CommitGuardReport;
  resolve: (decision: CommitGuardDecision) => void;
}

export interface PushReview {
  repoId: number;
  remote: string;
  branch: string;
  report: PushGuardReport;
  /** Last rewrite done from the dialog (shows the backup ref + Undo). */
  rewrite: RewriteOutcome | null;
  resolve: (decision: PushGuardDecision) => void;
}

interface GuardState {
  commit: CommitReview | null;
  push: PushReview | null;
  busy: boolean;
  openCommit: (review: CommitReview) => void;
  openPush: (review: PushReview) => void;
  /** Closes the commit review and settles its promise. */
  settleCommit: (decision: CommitGuardDecision) => void;
  settlePush: (decision: PushGuardDecision) => void;
  updatePush: (patch: Partial<Pick<PushReview, "report" | "rewrite">>) => void;
  setBusy: (busy: boolean) => void;
}

export const useGuardDialogs = create<GuardState>()((set, get) => ({
  commit: null,
  push: null,
  busy: false,
  openCommit: (review) => {
    // A newer request supersedes an unanswered one.
    get().commit?.resolve("cancel");
    set({ commit: review, busy: false });
  },
  openPush: (review) => {
    get().push?.resolve("cancel");
    set({ push: review, busy: false });
  },
  settleCommit: (decision) => {
    const pending = get().commit;
    set({ commit: null, busy: false });
    pending?.resolve(decision);
  },
  settlePush: (decision) => {
    const pending = get().push;
    set({ push: null, busy: false });
    pending?.resolve(decision);
  },
  updatePush: (patch) => {
    const push = get().push;
    if (push) set({ push: { ...push, ...patch } });
  },
  setBusy: (busy) => set({ busy }),
}));

/**
 * Commit Guard: resolves `"proceed"` right away when there is nothing to say or the check
 * fails (advisory: a broken check never blocks a commit); otherwise shows the dialog and
 * resolves with the user's choice — `"switched"` means the repo was re-bound to another
 * account, so the caller should re-read the identity before committing.
 */
export async function runCommitGuard(repoId: number): Promise<CommitGuardDecision> {
  let report: CommitGuardReport;
  try {
    report = await commands.commitGuard(repoId);
  } catch (e) {
    console.warn("commit guard failed", e);
    return "proceed";
  }
  if (!commitNeedsReview(report)) return "proceed";
  return new Promise((resolve) => {
    useGuardDialogs.getState().openCommit({ repoId, report, resolve });
  });
}

/**
 * Push Guard: resolves `"proceed"` when the push looks fine or the check could not run
 * (invalid names, errors — the push itself will report them); otherwise shows the dialog,
 * where the user can fix authors / move blobs to LFS (with backup + Undo) before pushing.
 */
export async function runPushGuard(
  repoId: number,
  remote: string,
  branch: string,
): Promise<PushGuardDecision> {
  let report: PushGuardReport;
  try {
    const out = await commands.pushGuard(repoId, remote, branch);
    if (out.status !== "done") return "proceed";
    report = out.value;
  } catch (e) {
    console.warn("push guard failed", e);
    return "proceed";
  }
  if (!pushNeedsReview(report)) return "proceed";
  return new Promise((resolve) => {
    useGuardDialogs.getState().openPush({ repoId, remote, branch, report, rewrite: null, resolve });
  });
}
