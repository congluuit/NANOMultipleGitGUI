import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";

/** How many conflicting files are named before "and N more". */
export const LISTED_FILES = 5;

/**
 * Forecast of merging `rev` into `base` (`null` = HEAD), computed in the background. Fresh on
 * every dialog open: the branches may have moved since the last look.
 */
export function useMergeForecast(repoId: number, rev: string, base: string | null) {
  return useQuery({
    queryKey: ["merge-forecast", repoId, rev, base],
    queryFn: () => commands.mergeForecast(repoId, rev, base),
    enabled: isTauri(),
    staleTime: 0,
    gcTime: 0,
    retry: false,
  });
}
