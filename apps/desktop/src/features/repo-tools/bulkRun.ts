/**
 * Bulk push and quick commit over the repositories picked in "All repositories".
 *
 * Every repository goes through the same checks as a single commit or push: Commit Guard and
 * the Large File Guard before a commit, the "remote of another account" question, Push Guard
 * and the LFS upload before a push. Those checks normally open a dialog. Here nobody is there
 * to answer it, so a repository whose check has something to say is not committed or pushed:
 * it is marked "needs attention" with the reason and the user opens it to decide.
 */
import i18n from "i18next";
import { create } from "zustand";
import { accountKeys } from "@/features/accounts/queries";
import { trackingMismatch } from "@/features/accounts/switcher/model";
import { isTrackingKept } from "@/features/accounts/switcher/tracking";
import { failureText } from "@/features/commits/outcome";
import { learnedLimit } from "@/features/guards/limits";
import { commitNeedsReview, pushNeedsReview } from "@/features/guards/model";
import { guessProvider, relevantFindings } from "@/features/lfs/guard";
import { lfsKeys } from "@/features/lfs/queries";
import { newOpId, pushPlan } from "@/features/remote/plan";
import { repoKeys } from "@/features/repos/queries";
import { notifyEstimateFailure, repoAccount, runLfsPush } from "@/features/transfers/actions";
import { failureTexts } from "@/features/transfers/format";
import {
  fixForFailure,
  selectRepoRunning,
  setFix,
  useTransferQueue,
} from "@/features/transfers/store";
import { invalidateRepo, worktreeKeys } from "@/features/worktree/queries";
import {
  commands,
  errorMessage,
  type CommitGuardReport,
  type PushGuardReport,
  type PushRequest,
  type RepoAccountContext,
  type RepoOverview,
  type RepoRefs,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { refreshOverview } from "./queries";

export type BulkKind = "push" | "commit";

export type BulkStatus =
  | "waiting"
  | "checking"
  | "running"
  | "done"
  /** Nothing to do here (nothing staged, nothing to push). */
  | "skipped"
  /** A guard or question would have stopped a single commit or push: open the repo. */
  | "attention"
  | "failed"
  /** Not run: cancelled, or stopped after an earlier failure. */
  | "notRun";

export interface BulkItem {
  repoId: number;
  name: string;
  status: BulkStatus;
  /** Why it was skipped, needs attention or failed. */
  reason: string | null;
}

export interface BulkRun {
  kind: BulkKind;
  items: BulkItem[];
  running: boolean;
  /** Cancel asked: stop after the current repository. */
  cancelling: boolean;
}

interface BulkState {
  run: BulkRun | null;
  begin: (kind: BulkKind, rows: readonly RepoOverview[]) => boolean;
  update: (repoId: number, status: BulkStatus, reason?: string | null) => void;
  cancel: () => void;
  end: () => void;
  clear: () => void;
}

export const useBulkRun = create<BulkState>()((set, get) => ({
  run: null,
  begin: (kind, rows) => {
    if (get().run?.running) return false;
    set({
      run: {
        kind,
        items: rows.map((r) => ({
          repoId: r.repoId,
          name: r.name,
          status: "waiting",
          reason: null,
        })),
        running: true,
        cancelling: false,
      },
    });
    return true;
  },
  update: (repoId, status, reason = null) => {
    const run = get().run;
    if (!run) return;
    set({
      run: {
        ...run,
        items: run.items.map((i) => (i.repoId === repoId ? { ...i, status, reason } : i)),
      },
    });
  },
  cancel: () => {
    const run = get().run;
    if (run?.running) set({ run: { ...run, cancelling: true } });
  },
  end: () => {
    const run = get().run;
    if (run) set({ run: { ...run, running: false } });
  },
  clear: () => {
    if (!get().run?.running) set({ run: null });
  },
}));

/** Result of the checks of one repository. */
type Check<T> =
  { ok: true; value: T } | { ok: false; status: "skipped" | "attention"; reason: string };

// Keys are built from a status or finding kind, so they are typed loosely here.
const t = (key: string, options?: Record<string, unknown>): string =>
  (i18n.t as unknown as (k: string, o?: Record<string, unknown>) => string)(key, options);
const stop = (status: "skipped" | "attention", key: string, options?: Record<string, unknown>) =>
  ({ ok: false, status, reason: t(`overview.bulk.reason.${key}`, options) }) as const;

function fresh<T>(queryKey: readonly unknown[], queryFn: () => Promise<T>): Promise<T> {
  return queryClient.fetchQuery({ queryKey, queryFn, staleTime: 0, retry: false });
}

/** Why Commit Guard would have stopped the commit, in one line. */
export function commitGuardReason(report: CommitGuardReport): string {
  return report.findings
    .map((f) => {
      switch (f.kind) {
        case "emailMismatch":
          return t("overview.bulk.reason.emailMismatch", { email: f.email });
        case "unbound":
          return t("overview.bulk.reason.unbound");
        case "emailNotVerified":
          return t("overview.bulk.reason.emailNotVerified", { email: f.email });
        case "signingMissing":
          return t("overview.bulk.reason.signingMissing");
      }
    })
    .join(" ");
}

/** Why Push Guard would have stopped the push, in one line. */
export function pushGuardReason(report: PushGuardReport): string {
  return report.findings
    .map((f) => {
      switch (f.kind) {
        case "authorMismatch":
          return t("overview.bulk.reason.authorMismatch", { count: f.count, email: f.email });
        case "permissionDenied":
          return t("overview.bulk.reason.permissionDenied", {
            account: f.accountLabel ?? t("overview.bulk.thisAccount"),
          });
        case "heavyBlobs":
          return t("overview.bulk.reason.heavyBlobs", { count: f.block + f.warn });
        case "heavyBlobsShared":
          return t("overview.bulk.reason.heavyBlobsShared", { count: f.count });
      }
    })
    .join(" ");
}

// ── Push ────────────────────────────────────────────────────────────────────────────────────

/** Everything a single Push checks before it starts, without asking anything. */
export async function checkPush(row: RepoOverview): Promise<Check<PushRequest>> {
  if (row.missing || row.error) return stop("skipped", "unreadable");
  if (row.operation || row.conflicts > 0) return stop("attention", "operation");
  let refs: RepoRefs;
  try {
    refs = await fresh(repoKeys.refs(row.repoId), () => commands.repoRefs(row.repoId));
  } catch (e) {
    return { ok: false, status: "attention", reason: errorMessage(e) };
  }
  const plan = pushPlan(refs);
  if (!plan) return stop("skipped", refs.head.detached ? "detached" : "noRemote");
  const local = refs.local.find((b) => b.name === plan.branch);
  if (!local?.upstream || local.upstreamGone) return stop("skipped", "notOnServer");
  if (local.behind > 0) return stop("attention", "behind", { count: local.behind });
  if (local.ahead === 0) return stop("skipped", "nothingToPush");

  // The branch pushes through a remote of another account than the repo's: Push asks first.
  try {
    const ctx = await fresh<RepoAccountContext>(accountKeys.repoContext(row.repoId), () =>
      commands.repoAccountContext(row.repoId),
    );
    const m = trackingMismatch(ctx, refs);
    if (m && !isTrackingKept(row.repoId, m)) {
      return stop("attention", "tracking", { remote: plan.remote });
    }
  } catch (e) {
    console.warn("account context not read", e);
  }

  // Push Guard. Like the single push it is advisory when the check itself cannot run: the
  // push then reports any real problem.
  try {
    const out = await commands.pushGuard(
      row.repoId,
      plan.remote,
      plan.branch,
      learnedLimit(row.repoId, plan.remote),
    );
    if (out.status === "done" && pushNeedsReview(out.value)) {
      return { ok: false, status: "attention", reason: pushGuardReason(out.value) };
    }
  } catch (e) {
    console.warn("push guard failed", e);
  }
  return { ok: true, value: plan };
}

async function usesLfs(repoId: number): Promise<boolean> {
  const status = await fresh(lfsKeys.status(repoId), () => commands.lfsStatus(repoId)).catch(
    () => null,
  );
  return status?.usesLfs ?? false;
}

/** `git push` as a quiet queue item (progress in the transfer list, no notice per repo). */
async function gitPush(row: RepoOverview, plan: PushRequest): Promise<string | null> {
  const opId = newOpId("push");
  const queue = useTransferQueue.getState();
  queue.start({ opId, repoId: row.repoId, kind: "push", subject: `${plan.remote}/${plan.branch}` });
  const finish = useTransferQueue.getState().finish;
  try {
    const out = await commands.remotePush(row.repoId, opId, plan, null);
    if (out.status === "done") {
      finish(opId, "done", { text: t("overview.bulk.status.done") });
      return null;
    }
    if (out.status === "rejected") {
      // Same fixes as a single push (a file over the server's limit offers "Move to LFS").
      const fix = fixForFailure(row.repoId, out.reason, out.message, () => void gitPush(row, plan));
      if (fix) setFix(opId, fix);
      const error = failureText(out.reason, out.message);
      finish(opId, "failed", { error });
      return t(`failure.${out.reason}`);
    }
    finish(opId, "cancelled", { text: t("ops.cancelled") });
    return t("ops.cancelled");
  } catch (e) {
    const error = errorMessage(e);
    finish(opId, "failed", { error });
    return error;
  } finally {
    invalidateRepo(row.repoId);
  }
}

/**
 * The LFS upload a push would start, as shown to the user before the run. A single push asks
 * for confirmation with this estimate; in bulk mode the click on "Push N repositories" is that
 * confirmation, so the estimate must be on screen first.
 */
export type LfsPreview =
  /** Not an LFS repository, or nothing to upload. */
  | { status: "none" }
  | { status: "upload"; oids: string[]; count: number; totalBytes: number; unknownSizes: number }
  /** The estimate failed: shown like the single push shows it, the push is still allowed. */
  | { status: "failed"; message: string };

async function lfsEstimate(repoId: number, plan: PushRequest): Promise<LfsPreview> {
  if (!(await usesLfs(repoId))) return { status: "none" };
  try {
    const est = await commands.lfsPushEstimate(repoId, plan.remote, plan.branch);
    if (est.status === "failed") {
      return {
        status: "failed",
        message: failureTexts(est.failure, repoAccount(repoId)?.label ?? "").title,
      };
    }
    if (est.status !== "done" || est.value.objects.length === 0) return { status: "none" };
    return {
      status: "upload",
      oids: est.value.objects.map((o) => o.oid),
      count: est.value.objects.length,
      totalBytes: est.value.totalBytes,
      unknownSizes: est.value.unknownSizes,
    };
  } catch (e) {
    return { status: "failed", message: errorMessage(e) };
  }
}

/** Estimate of the LFS upload of one repository's push, for the list before the run. */
export async function previewLfs(row: RepoOverview): Promise<LfsPreview> {
  if (row.missing || row.error) return { status: "none" };
  const refs = await fresh(repoKeys.refs(row.repoId), () => commands.repoRefs(row.repoId)).catch(
    () => null,
  );
  const plan = refs ? pushPlan(refs) : null;
  return plan ? lfsEstimate(row.repoId, plan) : { status: "none" };
}

/** Whether the upload found now was shown before the run (no object the user did not see). */
export function lfsUploadApproved(now: LfsPreview, shown: LfsPreview | undefined): boolean {
  if (now.status !== "upload") return true;
  if (shown?.status !== "upload") return false;
  const seen = new Set(shown.oids);
  return now.oids.every((oid) => seen.has(oid));
}

type PushResult = { status: "done" | "failed" | "attention"; reason: string | null };

/** Pushes one repository after its checks pass. */
async function pushOne(
  row: RepoOverview,
  plan: PushRequest,
  shown: LfsPreview | undefined,
): Promise<PushResult> {
  const now = await lfsEstimate(row.repoId, plan);
  if (!lfsUploadApproved(now, shown)) {
    return { status: "attention", reason: t("overview.bulk.reason.lfsChanged") };
  }
  if (now.status === "upload") {
    const uploaded = await runLfsPush(row.repoId, plan.remote, plan.branch);
    if (!uploaded) return { status: "failed", reason: t("overview.bulk.reason.lfsFailed") };
  } else if (now.status === "failed" && shown?.status !== "failed") {
    // Like the single push: say the estimate failed (unless the row already said so), then
    // push; git's own hook uploads what is needed.
    notifyEstimateFailure(now.message);
  }
  const error = await gitPush(row, plan);
  return { status: error ? "failed" : "done", reason: error };
}

export interface BulkSummary {
  done: number;
  skipped: number;
  attention: number;
  failed: number;
  notRun: number;
}

function summarize(): BulkSummary {
  const items = useBulkRun.getState().run?.items ?? [];
  const count = (s: BulkStatus) => items.filter((i) => i.status === s).length;
  return {
    done: count("done"),
    skipped: count("skipped"),
    attention: count("attention"),
    failed: count("failed"),
    notRun: count("notRun"),
  };
}

function report(kind: BulkKind, s: BulkSummary) {
  const bad = s.failed + s.attention;
  useUi.getState().showNotice({
    kind: s.failed ? "error" : bad ? "info" : "success",
    message: t(`overview.bulk.result.${kind}`, { count: s.done }),
    hint:
      [
        s.attention ? t("overview.bulk.result.attention", { count: s.attention }) : null,
        s.failed ? t("overview.bulk.result.failed", { count: s.failed }) : null,
        s.notRun ? t("overview.bulk.result.notRun", { count: s.notRun }) : null,
      ]
        .filter(Boolean)
        .join(" ") || undefined,
  });
}

/**
 * Runs `work` on each repository in turn. Stops before the next one when cancel was asked,
 * or (with `stopOnFailure`) once one failed or needs attention: a repository later in the
 * list may depend on it.
 */
async function sequence(
  kind: BulkKind,
  rows: readonly RepoOverview[],
  stopOnFailure: boolean,
  work: (row: RepoOverview) => Promise<void>,
): Promise<BulkSummary | null> {
  const store = useBulkRun.getState();
  if (!store.begin(kind, rows)) return null;
  for (const row of rows) {
    const run = useBulkRun.getState().run;
    const failedBefore = run?.items.some((i) => i.status === "failed" || i.status === "attention");
    if (run?.cancelling || (stopOnFailure && failedBefore)) {
      useBulkRun.getState().update(row.repoId, "notRun");
      continue;
    }
    try {
      await work(row);
    } catch (e) {
      useBulkRun.getState().update(row.repoId, "failed", errorMessage(e));
    }
  }
  useBulkRun.getState().end();
  await refreshOverview();
  const summary = summarize();
  report(kind, summary);
  return summary;
}

/**
 * Pushes `rows` one at a time, in the order given. `shown`: the LFS estimates the user saw
 * before starting (by repository id); an upload that was not shown is never started.
 */
export function bulkPush(
  rows: readonly RepoOverview[],
  stopOnFailure: boolean,
  shown: ReadonlyMap<number, LfsPreview> = new Map(),
) {
  return sequence("push", rows, stopOnFailure, async (row) => {
    const { update } = useBulkRun.getState();
    // Another fetch / pull / push of this repository is still running.
    if (selectRepoRunning(useTransferQueue.getState(), row.repoId)) {
      update(row.repoId, "attention", t("overview.bulk.reason.busy"));
      return;
    }
    update(row.repoId, "checking");
    const check = await checkPush(row);
    if (!check.ok) {
      update(row.repoId, check.status, check.reason);
      return;
    }
    update(row.repoId, "running");
    const result = await pushOne(row, check.value, shown.get(row.repoId));
    update(row.repoId, result.status, result.reason);
  });
}

// ── Quick commit ────────────────────────────────────────────────────────────────────────────

/** Everything a single commit checks before it runs, without asking anything. */
export async function checkCommit(row: RepoOverview): Promise<Check<string[]>> {
  if (row.missing || row.error) return stop("skipped", "unreadable");
  if (row.operation) return stop("attention", "operation");
  let staged: string[];
  try {
    const status = await fresh(worktreeKeys.status(row.repoId), () =>
      commands.worktreeStatus(row.repoId),
    );
    if (status.conflicted.length > 0) return stop("attention", "conflicts");
    staged = status.staged.map((e) => e.path);
  } catch (e) {
    return { ok: false, status: "attention", reason: errorMessage(e) };
  }
  if (staged.length === 0) return stop("skipped", "nothingStaged");

  // Commit Guard: advisory when the check itself fails, like the commit box.
  try {
    const report = await commands.commitGuard(row.repoId);
    if (commitNeedsReview(report)) {
      return { ok: false, status: "attention", reason: commitGuardReason(report) };
    }
  } catch (e) {
    console.warn("commit guard failed", e);
  }

  // Large File Guard on the staged files.
  try {
    const refs = await fresh(repoKeys.refs(row.repoId), () => commands.repoRefs(row.repoId)).catch(
      () => undefined,
    );
    const found = await commands.largeFileCheck(row.repoId, staged, guessProvider(refs), null);
    const relevant = relevantFindings(found);
    if (relevant.length > 0) {
      return stop("attention", "largeFiles", { count: relevant.length });
    }
  } catch (e) {
    console.warn("large file check failed", e);
  }
  return { ok: true, value: staged };
}

/** Commits the already staged changes of each repository with the same message. */
export function bulkCommit(rows: readonly RepoOverview[], message: string) {
  return sequence("commit", rows, false, async (row) => {
    const { update } = useBulkRun.getState();
    update(row.repoId, "checking");
    const check = await checkCommit(row);
    if (!check.ok) {
      update(row.repoId, check.status, check.reason);
      return;
    }
    update(row.repoId, "running");
    try {
      await commands.commitCreate(
        row.repoId,
        message,
        { amend: false, signoff: false, allowEmpty: false },
        null,
      );
      update(row.repoId, "done");
    } catch (e) {
      update(row.repoId, "failed", errorMessage(e));
    } finally {
      invalidateRepo(row.repoId);
    }
  });
}
