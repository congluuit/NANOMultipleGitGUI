/**
 * Pull/Push of the current branch vs. the repo's account (see `trackingMismatch`): point the
 * branch at the remote of the repo's account, or remember that it stays where it is.
 */
import i18n from "i18next";
import { runOutcome } from "@/features/commits/outcome";
import { startTransfer } from "@/features/remote/transfers";
import { commands, type RemoteAccountContext } from "@/ipc";
import { accountLabel, type TrackingMismatch } from "./model";

const KEEP_KEY = "nanomgg.keepTracking";

const keepKey = (repoId: number, m: TrackingMismatch) =>
  `${repoId}:${m.branch}:${m.current.remote}`;

function readKept(): string[] {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(KEEP_KEY) ?? "[]");
    return Array.isArray(value) ? value.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

/** "Keep using origin": Push/Pull of this branch stop asking. */
export function keepTracking(repoId: number, m: TrackingMismatch) {
  const kept = readKept();
  const key = keepKey(repoId, m);
  if (kept.includes(key)) return;
  try {
    localStorage.setItem(KEEP_KEY, JSON.stringify([...kept, key].slice(-200)));
  } catch {
    // Not remembered: the question comes back next time.
  }
}

export const isTrackingKept = (repoId: number, m: TrackingMismatch) =>
  readKept().includes(keepKey(repoId, m));

/** Account a remote runs as, for messages ("Pret", or "no account"). */
export const remoteAccountName = (r: RemoteAccountContext) =>
  r.account ? accountLabel(r.account) : i18n.t("switcher.unboundShort");

/** `origin (github.com)`: remote names alone mean little to someone new to git. */
export const remoteWithHost = (r: RemoteAccountContext) =>
  r.host ? `${r.remote} (${r.host.replace(/:\d+$/, "")})` : r.remote;

/** Text values shared by the tracking messages. */
export const trackingVars = (m: TrackingMismatch) => ({
  branch: m.branch,
  remote: m.current.remote,
  from: remoteWithHost(m.current),
  account: remoteAccountName(m.current),
  target: m.target.remote,
  to: remoteWithHost(m.target),
  targetAccount: remoteAccountName(m.target),
});

/**
 * Makes the branch pull from / push to `m.target` (the branch there is created by the next
 * push when missing). `fetch`: also fetch that remote so ahead/behind are right at once (not
 * needed when a pull or push follows). Returns whether the branch was switched.
 */
export async function followAccountRemote(
  repoId: number,
  m: TrackingMismatch,
  fetch = true,
): Promise<boolean> {
  const done = await runOutcome(
    repoId,
    () => commands.branchTrackRemote(repoId, m.branch, m.target.remote),
    () => ({ kind: "success", message: i18n.t("switcher.tracking.done", trackingVars(m)) }),
  );
  if (done === undefined) return false;
  if (fetch) {
    void startTransfer(
      repoId,
      "fetch",
      (op) => commands.remoteFetch(repoId, op, m.target.remote, false, false, null),
      () => null,
      m.target.remote,
    );
  }
  return true;
}
