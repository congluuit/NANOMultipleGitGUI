import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";

export const storageKeys = {
  overview: ["storage", "overview"] as const,
  status: ["storage", "status"] as const,
};

/** Re-checks the free space of the data drive this often (cheap, no folder walk). */
export const STATUS_POLL_MS = 30 * 60 * 1000;

/** Every folder with its size; measured only while the Storage dialog is open. */
export function useStorageOverview(enabled: boolean) {
  return useQuery({
    queryKey: storageKeys.overview,
    queryFn: () => commands.storageOverview(),
    enabled: enabled && isTauri(),
    staleTime: 0,
  });
}

export function useStorageStatus() {
  return useQuery({
    queryKey: storageKeys.status,
    queryFn: () => commands.storageStatus(),
    enabled: isTauri(),
    refetchInterval: STATUS_POLL_MS,
  });
}
