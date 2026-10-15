/**
 * Fork flows (PLAN §4.7, Phase 4.3). Each step reports its phase so the dialogs can show
 * "forking → waiting until ready → cloning / fetching".
 */
import i18n from "i18next";
import { cancelClone, runClone, type CloneRun } from "@/features/clone/actions";
import { newOpId } from "@/features/remote/plan";
import { startTransfer } from "@/features/remote/transfers";
import { invalidateAccounts } from "@/features/accounts/queries";
import { repoKeys } from "@/features/repos/queries";
import {
  commands,
  errorMessage,
  type CloneProgress,
  type CloneRequest,
  type ForkAttachMode,
  type ForkOptions,
  type ForkOutcome,
  type ForkRemoteChange,
  type ForkRepo,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import type { ForkPhase, ForkSource } from "./model";

export type ForkFailure = Exclude<ForkOutcome<unknown>, { status: "done" }>;

export type ForkStep =
  | { status: "done"; fork: ForkRepo }
  | { status: "forkFailed"; failure: ForkFailure }
  /** GitHub is still copying the data after the wait limit. */
  | { status: "notReady"; fork: ForkRepo }
  | { status: "cancelled" }
  | { status: "failed"; message: string };

/**
 * Forks `source` with `accountId` (or reuses the account's fork) and waits until it can be
 * cloned. `onStart` receives the operation id that `transfer_cancel` stops while waiting.
 */
export async function forkAndWait(
  accountId: string,
  source: string,
  options: ForkOptions,
  onPhase: (phase: ForkPhase) => void,
  onStart: (opId: string) => void = () => {},
): Promise<ForkStep> {
  try {
    onPhase("forking");
    const created = await commands.forkCreate(accountId, source, options);
    if (created.status !== "done") return { status: "forkFailed", failure: created };
    const fork = created.value;
    onPhase("waiting");
    const opId = newOpId("fork");
    onStart(opId);
    const waited = await commands.forkWait(opId, accountId, fork.fullName);
    if (waited.status !== "done") {
      if (waited.status === "failed" && waited.error.kind === "cancelled") {
        return { status: "cancelled" };
      }
      return { status: "forkFailed", failure: waited };
    }
    return waited.value ? { status: "done", fork } : { status: "notReady", fork };
  } catch (e) {
    return { status: "failed", message: errorMessage(e) };
  }
}

export type ForkCloneRun = CloneRun | Exclude<ForkStep, { status: "done" | "cancelled" }>;

/**
 * Fork & clone: fork (or reuse) → wait → clone the fork as the fork account (`request.url`
 * is replaced by the fork's URL in the same transport) → `upstream` = the source → fetch it in
 * the background.
 */
export async function forkThenClone(
  args: {
    accountId: string;
    source: ForkSource;
    options: ForkOptions;
    protocol: "https" | "ssh";
    /** Clone request; `url` and `dest` are filled in for the fork. */
    request: Omit<CloneRequest, "url" | "dest">;
    /** Destination chosen by the user; `null` = suggestion for the fork's URL. */
    dest: string | null;
  },
  onPhase: (phase: ForkPhase) => void,
  onStart: (opId: string) => void,
  onProgress: (p: CloneProgress) => void,
): Promise<ForkCloneRun> {
  const step = await forkAndWait(
    args.accountId,
    args.source.fullName,
    args.options,
    onPhase,
    onStart,
  );
  if (step.status === "cancelled") return { status: "cancelled" };
  if (step.status !== "done") return step;
  const { fork } = step;
  const url = args.protocol === "ssh" ? fork.cloneSsh : fork.cloneHttps;
  let dest = args.dest;
  if (!dest) {
    try {
      dest = (await commands.cloneDestination(args.accountId, url, null)).path;
    } catch (e) {
      return { status: "failed", message: errorMessage(e) };
    }
  }
  onPhase("cloning");
  const run = await runClone({ ...args.request, url, dest }, onStart, onProgress);
  if (run.status !== "done") return run;
  onPhase("linking");
  const repoId = run.result.tab.id;
  try {
    const change = await commands.forkAddUpstream(
      repoId,
      args.source.cloneHttps,
      args.source.cloneSsh,
    );
    void queryClient.invalidateQueries({ queryKey: repoKeys.refs(repoId) });
    void fetchRemote(repoId, change.remote);
  } catch (e) {
    useUi.getState().showNotice({
      kind: "error",
      message: i18n.t("forks.upstreamFailed", { message: errorMessage(e) }),
    });
  }
  return run;
}

/** Fetches `remote` as a transfer-queue item (progress, cancel, retry). */
export function fetchRemote(repoId: number, remote: string) {
  return startTransfer(
    repoId,
    "fetch",
    (opId) => commands.remoteFetch(repoId, opId, remote, false, true, null),
    () => i18n.t("forks.fetched", { remote }),
    remote,
  );
}

export type AttachStep =
  | { status: "done"; fork: ForkRepo; change: ForkRemoteChange }
  | Exclude<ForkStep, { status: "done" }>;

/**
 * Fork the open repository: fork (or reuse) → wait → add the fork as a remote (or swap it
 * with `origin`) bound to the fork account → fetch it.
 */
export async function forkOpenRepo(
  args: {
    repoId: number;
    accountId: string;
    sourceRemote: string;
    source: string;
    options: ForkOptions;
    mode: ForkAttachMode;
  },
  onPhase: (phase: ForkPhase) => void,
  onStart: (opId: string) => void,
): Promise<AttachStep> {
  const step = await forkAndWait(args.accountId, args.source, args.options, onPhase, onStart);
  if (step.status !== "done") return step;
  const { fork } = step;
  onPhase("linking");
  let change: ForkRemoteChange;
  try {
    change = await commands.forkAttach(
      args.repoId,
      args.accountId,
      args.sourceRemote,
      fork.cloneHttps,
      fork.cloneSsh,
      args.mode,
    );
  } catch (e) {
    return { status: "failed", message: errorMessage(e) };
  }
  void queryClient.invalidateQueries({ queryKey: repoKeys.refs(args.repoId) });
  void queryClient.invalidateQueries({ queryKey: ["hosting"] });
  invalidateAccounts();
  onPhase("fetching");
  await fetchRemote(args.repoId, change.remote);
  return { status: "done", fork, change };
}

/** Stops the wait / clone running under `opId`. */
export const cancelFork = cancelClone;
