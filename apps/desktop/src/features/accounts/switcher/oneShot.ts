/**
 * "Chỉ cho thao tác này" (PLAN §4.5): run the next commit and/or push of a repo as another
 * account without touching its binding. In memory only (lost on restart, by design).
 *
 * ## Contract for the git commands (commit / push / fetch)
 *
 * The code that starts the operation — not the backend — consumes the override right before
 * invoking the command, and passes it as the optional "as account" parameter:
 *
 * ```ts
 * import { asAccountOf, settleOneShot, takeOneShot } from "@/features/accounts/switcher";
 *
 * const shot = takeOneShot(repoId, "commit"); // or "push" / "fetch"
 * try {
 *   await commands.commitCreate(repoId, message, options, asAccountOf(shot));
 *   settleOneShot(repoId, shot, true);
 * } catch (e) {
 *   settleOneShot(repoId, shot, false); // give it back so a retry uses it again
 *   throw e;
 * }
 * ```
 *
 * Wired in: `worktree/CommitBox.tsx` (commit), `remote/RemoteButtons.tsx` and
 * `command-palette/actions.ts` (fetch / pull / push, LFS upload + `git push` share one shot),
 * through `remote/transfers.ts#startTransfer` and `transfers/actions.ts` (`shot` parameter).
 *
 * - `takeOneShot(repoId, op)` returns the override and removes it when it applies to `op`, else
 *   returns `null` and leaves it alone. An override set for `"any"` applies to the next commit
 *   OR push (whichever comes first); `"fetch"` is only consumed by an override set for
 *   `"fetch"` (background/auto fetches must never eat a one-shot meant for a push).
 * - `identityId: null` → the account's default identity (same as a binding without identity).
 * - `peekOneShot(repoId, op)` reads without consuming (UI, e.g. the "Commit as" block).
 * - Pull = fetch + merge/rebase: pass `"fetch"` for its network part.
 */
import { create } from "zustand";
import type { AccountOverride } from "@/ipc";

export type OneShotOp = "commit" | "push" | "fetch" | "any";

/** Operation that may consume an override. */
export type TakeOp = Exclude<OneShotOp, "any">;

export interface OneShot {
  accountId: string;
  /** `null` = the account's default identity. */
  identityId: string | null;
  op: OneShotOp;
}

interface OneShotState {
  byRepo: Record<number, OneShot>;
}

export const useAsAccount = create<OneShotState>()(() => ({ byRepo: {} }));

/** Whether an override set for `shot.op` applies to the operation `op`. */
export const oneShotApplies = (shot: OneShot, op: TakeOp) =>
  shot.op === op || (shot.op === "any" && op !== "fetch");

/** Sets (replaces) the override of a repo; `null` clears it. */
export function setOneShot(repoId: number, shot: OneShot | null) {
  useAsAccount.setState((s) => {
    const byRepo = { ...s.byRepo };
    if (shot) byRepo[repoId] = shot;
    else delete byRepo[repoId];
    return { byRepo };
  });
}

export const clearOneShot = (repoId: number) => setOneShot(repoId, null);

/** The override that `op` would use, without consuming it. */
export function peekOneShot(repoId: number, op: TakeOp): OneShot | null {
  const shot = useAsAccount.getState().byRepo[repoId];
  return shot && oneShotApplies(shot, op) ? shot : null;
}

/** Returns and removes the override when it applies to `op` (see the contract above). */
export function takeOneShot(repoId: number, op: TakeOp): OneShot | null {
  const shot = peekOneShot(repoId, op);
  if (shot) clearOneShot(repoId);
  return shot;
}

/** The `asAccount` argument of the git commands for a taken override (`null` → none). */
export const asAccountOf = (shot: OneShot | null): AccountOverride | null =>
  shot ? { accountId: shot.accountId, identityId: shot.identityId } : null;

const sameShot = (a: OneShot, b: OneShot) =>
  a.accountId === b.accountId && a.identityId === b.identityId && a.op === b.op;

/**
 * Settles an override taken with {@link takeOneShot} once its operation finished:
 * - not run (`ok: false`: cancelled, or a local failure such as a commit hook) → given back so
 *   the next attempt uses it again, unless the user picked another override meanwhile;
 *   transfers refused by the server count as run (`ok: true`);
 * - succeeded → removed if it had been given back by an earlier failed attempt (a queue
 *   "Retry" re-runs with the account it captured).
 */
export function settleOneShot(repoId: number, shot: OneShot | null, ok: boolean) {
  if (!shot) return;
  const current = useAsAccount.getState().byRepo[repoId];
  if (ok) {
    if (current && sameShot(current, shot)) clearOneShot(repoId);
  } else if (!current) {
    setOneShot(repoId, shot);
  }
}

/** Pending override of a repo (any op), for the UI. */
export const useOneShot = (repoId: number | null | undefined) =>
  useAsAccount((s) => (repoId == null ? null : (s.byRepo[repoId] ?? null)));
