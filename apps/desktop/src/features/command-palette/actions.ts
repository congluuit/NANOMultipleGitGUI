/**
 * Actions run by built-in palette commands. Remote actions mirror the toolbar buttons
 * (`features/remote/RemoteButtons.tsx`, whose handlers live inside the component) using the
 * same exported building blocks, so queueing, notices and the LFS-first push flow match.
 */
import i18n from "i18next";
import { create } from "zustand";
import {
  asAccountOf,
  peekOneShot,
  takeOneShot,
  type OneShot,
} from "@/features/accounts/switcher/oneShot";
import { runPushGuard } from "@/features/guards";
import { conflictText } from "@/features/commits/outcome";
import { lfsKeys } from "@/features/lfs/queries";
import { offerCreatePull } from "@/features/pulls/actions";
import { pushPlan } from "@/features/remote/plan";
import { startTransfer } from "@/features/remote/transfers";
import { notifyEstimateFailure, repoAccount, runLfsPush } from "@/features/transfers/actions";
import { failureTexts } from "@/features/transfers/format";
import {
  commands,
  errorMessage,
  type PullMode,
  type PushEstimate,
  type PushRequest,
  type RepoRefs,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useGraphStore } from "@/stores/graph";
import { EMPTY_DRAFT, useWorktreeStore } from "@/stores/worktree";

// ── Confirmations shown by `PaletteConfirmHost` ─────────────────────────────────────────────

export type PaletteConfirm =
  | { kind: "force"; repoId: number; plan: PushRequest; refs: RepoRefs }
  | {
      kind: "lfs";
      repoId: number;
      plan: PushRequest;
      estimate: PushEstimate;
      /** One-shot "Push as…" override taken for this push (given back on cancel). */
      shot: OneShot | null;
    };

interface ConfirmState {
  confirm: PaletteConfirm | null;
  set: (confirm: PaletteConfirm | null) => void;
}

export const usePaletteConfirm = create<ConfirmState>()((set) => ({
  confirm: null,
  set: (confirm) => set({ confirm }),
}));

// ── Remote ──────────────────────────────────────────────────────────────────────────────────

// One-shot "… as" overrides (switcher) are taken right before each command starts.

export function fetchAll(repoId: number) {
  const shot = takeOneShot(repoId, "fetch");
  return startTransfer(
    repoId,
    "fetch",
    (op) => commands.remoteFetch(repoId, op, null, true, true, asAccountOf(shot)),
    (s) =>
      s.updatedRefs
        ? i18n.t("remote.fetched", { count: s.updatedRefs })
        : i18n.t("remote.fetchedNothing"),
    null,
    shot,
  );
}

export function pull(repoId: number, mode: PullMode) {
  const shot = takeOneShot(repoId, "fetch");
  return startTransfer(
    repoId,
    "pull",
    (op) => commands.remotePull(repoId, op, null, null, mode, asAccountOf(shot)),
    (o) => conflictText(o) || i18n.t(`remote.pulled.${o.result}`),
    null,
    shot,
  );
}

async function gitPush(repoId: number, p: PushRequest, shot: OneShot | null) {
  const s = await startTransfer(
    repoId,
    "push",
    (op) => commands.remotePush(repoId, op, p, asAccountOf(shot)),
    (s) =>
      s.upToDate
        ? i18n.t("remote.pushUpToDate")
        : i18n.t("remote.pushed", { branch: p.branch, remote: p.remote }),
    `${p.remote}/${p.remoteBranch ?? p.branch}`,
    shot,
  );
  // Phase 4.1: a new branch on GitHub/GitLab → offer "Create PR/MR".
  if (s && !s.upToDate && p.setUpstream) void offerCreatePull(repoId, p.remote, p.branch);
  return s;
}

async function usesLfs(repoId: number): Promise<boolean> {
  const status = await queryClient
    .fetchQuery({
      queryKey: lfsKeys.status(repoId),
      queryFn: () => commands.lfsStatus(repoId),
      staleTime: 10_000,
    })
    .catch(() => null);
  return status?.usesLfs ?? false;
}

/**
 * Push like the toolbar: in an LFS repository estimate the upload first and, when there is
 * something to upload, ask for confirmation (then LFS upload with progress, then `git push`).
 */
export async function push(repoId: number, refs: RepoRefs, force: boolean) {
  const p = pushPlan(refs, force);
  if (!p) return;
  if (
    !peekOneShot(repoId, "push") &&
    (await runPushGuard(repoId, p.remote, p.branch)) === "cancel"
  ) {
    return;
  }
  // One shot for the whole push (LFS upload + `git push`).
  const shot = takeOneShot(repoId, "push");
  if (await usesLfs(repoId)) {
    try {
      const est = await commands.lfsPushEstimate(repoId, p.remote, p.branch);
      if (est.status === "done" && est.value.objects.length > 0) {
        usePaletteConfirm
          .getState()
          .set({ kind: "lfs", repoId, plan: p, estimate: est.value, shot });
        return;
      }
      if (est.status === "failed") {
        notifyEstimateFailure(failureTexts(est.failure, repoAccount(repoId)?.label ?? "").title);
      }
    } catch (e) {
      notifyEstimateFailure(errorMessage(e));
    }
  }
  await gitPush(repoId, p, shot);
}

/** "Force push…": asks first (the dialog explains `--force-with-lease`). */
export function requestForcePush(repoId: number, refs: RepoRefs) {
  const plan = pushPlan(refs, true);
  if (plan) usePaletteConfirm.getState().set({ kind: "force", repoId, plan, refs });
}

export function confirmLfsPush(repoId: number, p: PushRequest, shot: OneShot | null = null) {
  return runLfsPush(repoId, p.remote, p.branch, () => void gitPush(repoId, p, shot), null, shot);
}

// ── Focus helpers ───────────────────────────────────────────────────────────────────────────

const byLabel = (label: string) => `[aria-label="${label.replace(/["\\]/g, "\\$&")}"]`;

/**
 * Focuses (and selects) the first element matching `selector` once it is rendered and enabled,
 * retrying for a few frames (the panel may be mounting after the palette closed).
 */
export function focusWhenReady(selector: string, frames = 30): Promise<boolean> {
  return new Promise((resolve) => {
    let left = frames;
    const tick = () => {
      const el = document.querySelector<HTMLElement>(selector);
      if (el && !(el as HTMLInputElement).disabled) {
        el.focus();
        if (el instanceof HTMLInputElement) el.select();
        resolve(true);
      } else if (left-- > 0) {
        requestAnimationFrame(tick);
      } else {
        resolve(false);
      }
    };
    requestAnimationFrame(tick);
  });
}

/** Shows the working tree panel (closes commit details) and focuses the commit summary. */
export function focusCommitSummary(repoId: number) {
  useGraphStore.getState().select(repoId, null);
  return focusWhenReady(`input${byLabel(i18n.t("commit.summary"))}`);
}

/** Turns on amend mode (prefilling the last message like the checkbox does) and focuses. */
export async function amendLastCommit(repoId: number) {
  const store = useWorktreeStore.getState();
  const draft = store.drafts[repoId] ?? EMPTY_DRAFT;
  store.updateDraft(repoId, { amend: true });
  if (!draft.amend && !draft.summary.trim() && !draft.body.trim()) {
    const last = await commands.headMessage(repoId).catch(() => null);
    if (last) {
      const [summary = "", ...rest] = last.split("\n");
      store.updateDraft(repoId, { summary, body: rest.join("\n").trim(), prefill: last });
    }
  }
  return focusCommitSummary(repoId);
}

/** Closes an open diff (the graph is hidden under it) and focuses the graph search box. */
export function focusGraphSearch(repoId: number) {
  useWorktreeStore.getState().openDiff(repoId, null);
  return focusWhenReady(`input${byLabel(i18n.t("graph.search"))}`);
}

export const SHA_PATTERN = /^[0-9a-f]{4,40}$/i;

/** Scrolls the graph to a commit (full or abbreviated SHA) and selects it. */
export function goToCommit(repoId: number, sha: string) {
  useWorktreeStore.getState().openDiff(repoId, null);
  useGraphStore.getState().requestJump(repoId, sha.trim());
}
