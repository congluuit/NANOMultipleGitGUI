import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, type WorkOverview } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export const myworkKeys = { all: ["mywork"] as const };

/** Background refresh while the app is open (badge). */
export const REFRESH_MS = 5 * 60_000;

/** A cancelled run keeps the previous answer instead of showing a half-empty panel. */
async function load(refresh: boolean): Promise<WorkOverview> {
  const data = await commands.myworkList(refresh);
  const previous = queryClient.getQueryData<WorkOverview>(myworkKeys.all);
  return data.cancelled && previous ? previous : data;
}

/** "My work" of every account with a token; refetched every 5 minutes. */
export function useMyWork() {
  return useQuery({
    queryKey: myworkKeys.all,
    queryFn: () => load(false),
    enabled: isTauri(),
    staleTime: 60_000,
    refetchInterval: REFRESH_MS,
    retry: false,
  });
}

/** Reloads every account, bypassing the backend's one-minute cache. */
export async function refreshMyWork() {
  await queryClient.fetchQuery({
    queryKey: myworkKeys.all,
    queryFn: () => load(true),
    staleTime: 0,
  });
}

export async function cancelMyWork() {
  await commands.myworkCancel();
}
