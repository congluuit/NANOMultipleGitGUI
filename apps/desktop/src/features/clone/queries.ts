import { useEffect, useState } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, events } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export const cloneKeys = {
  all: ["clone"] as const,
  list: ["clone", "list"] as const,
  guess: (url: string) => ["clone", "guess", url] as const,
  destination: (accountId: string | null, url: string, path: string | null) =>
    ["clone", "destination", accountId, url, path] as const,
};

/** Unified repository list of every account (cached by the backend; stale lists refresh). */
export function useCloneRepoList(enabled: boolean) {
  return useQuery({
    queryKey: cloneKeys.list,
    queryFn: () => commands.cloneRepoList(null, false),
    enabled: enabled && isTauri(),
    staleTime: 30_000,
  });
}

/** Refetches every account's list now (the ↻ button). */
export async function refreshCloneList() {
  const list = await commands.cloneRepoList(null, true);
  queryClient.setQueryData(cloneKeys.list, list);
  return list;
}

/** Re-reads the list when a background refresh finished. Mount while the dialog is open. */
export function useCloneListEvents(enabled: boolean) {
  useEffect(() => {
    if (!enabled || !isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void events.cloneListUpdated
      .listen(() => void queryClient.invalidateQueries({ queryKey: cloneKeys.list }))
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [enabled]);
}

/** `value` after it stopped changing for `ms`. */
export function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(id);
  }, [value, ms]);
  return debounced;
}

/** Account candidates and HTTPS/SSH forms of a pasted URL. */
export function useCloneGuess(url: string) {
  const trimmed = url.trim();
  return useQuery({
    queryKey: cloneKeys.guess(trimmed),
    queryFn: () => commands.cloneGuess(trimmed),
    enabled: isTauri() && trimmed.length > 0,
    placeholderData: keepPreviousData,
  });
}

/** Suggested destination (or the check of `path`), with free disk space. */
export function useCloneDestination(accountId: string | null, url: string, path: string | null) {
  const trimmed = url.trim();
  return useQuery({
    queryKey: cloneKeys.destination(accountId, trimmed, path),
    queryFn: () => commands.cloneDestination(accountId, trimmed, path),
    enabled: isTauri() && trimmed.length > 0,
    placeholderData: keepPreviousData,
  });
}
