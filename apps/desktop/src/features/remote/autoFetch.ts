import { useEffect } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { create } from "zustand";
import { commands } from "@/ipc";
import { selectRepoRunning, useTransferQueue } from "@/features/transfers/store";
import { invalidateRepo } from "@/features/worktree/queries";
import { newOpId } from "./plan";

/** Background fetch period while the repository is open. */
export const AUTO_FETCH_INTERVAL_MS = 5 * 60_000;
/** Returning to the window fetches again when the last fetch is older than this. */
const STALE_AFTER_MS = 2 * 60_000;
const FIRST_FETCH_DELAY_MS = 4_000;

interface LastFetchState {
  /** Unix ms of the last fetch (manual or background) by repository id. */
  at: Record<number, number>;
  mark: (repoId: number) => void;
}

export const useLastFetch = create<LastFetchState>((set) => ({
  at: {},
  mark: (repoId) => set((s) => ({ at: { ...s.at, [repoId]: Date.now() } })),
}));

/**
 * Keeps ahead / behind counts current so "pull before you push" is visible: fetches the
 * remotes of the open repository on open, every few minutes and when the window regains focus.
 *
 * Only remotes bound to an account with a stored token are fetched — their credentials come from
 * the app's helper, so a background fetch can never pop up a password prompt. Silent: no queue
 * item, no notice; a failure simply leaves the counts as they were.
 */
export function useAutoFetch(repoId: number) {
  useEffect(() => {
    if (!isTauri()) return;
    let stopped = false;
    const run = async () => {
      if (stopped || document.hidden) return;
      if (selectRepoRunning(useTransferQueue.getState(), repoId)) return;
      const last = useLastFetch.getState().at[repoId] ?? 0;
      if (Date.now() - last < STALE_AFTER_MS) return;
      useLastFetch.getState().mark(repoId);
      try {
        const remotes = (await commands.hostedRemoteList(repoId)).filter(
          (r) => r.accountId !== null && r.hasToken,
        );
        for (const r of remotes) {
          if (stopped) return;
          await commands.remoteFetch(repoId, newOpId("autofetch"), r.remote, false, true, null);
        }
        if (remotes.length > 0) invalidateRepo(repoId);
      } catch (e) {
        console.debug("background fetch skipped", e);
      }
    };
    const first = window.setTimeout(() => void run(), FIRST_FETCH_DELAY_MS);
    const timer = window.setInterval(() => void run(), AUTO_FETCH_INTERVAL_MS);
    const onFocus = () => void run();
    window.addEventListener("focus", onFocus);
    return () => {
      stopped = true;
      window.clearTimeout(first);
      window.clearInterval(timer);
      window.removeEventListener("focus", onFocus);
    };
  }, [repoId]);
}
