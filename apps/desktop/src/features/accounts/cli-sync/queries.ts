import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

/** Under `accountKeys.all`: adding an account any other way re-checks which CLI ones are new. */
export const cliSyncKeys = {
  scan: ["accounts", "cliSync", "scan"] as const,
};

/** gh / glab accounts on this machine (config files only; no token is read). */
export function useCliAccountsScan(enabled = true) {
  return useQuery({
    queryKey: cliSyncKeys.scan,
    queryFn: () => commands.cliAccountsScan(),
    enabled: isTauri() && enabled,
    staleTime: 60_000,
  });
}

export function rescanCliAccounts() {
  return queryClient.refetchQueries({ queryKey: cliSyncKeys.scan });
}
