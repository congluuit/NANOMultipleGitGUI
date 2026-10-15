/** Phase 6.1: "Chuyển nhánh" preferences (SQLite via `branch_switch_prefs_get/set`). */
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, errorMessage, type BranchSwitchPrefs, type BranchSwitchPrefsPatch } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { DEFAULT_PREFS } from "./model";

export const prefsKey = ["branchSwitchPrefs"] as const;

export function useBranchSwitchPrefs() {
  return useQuery({
    queryKey: prefsKey,
    queryFn: () => commands.branchSwitchPrefsGet(),
    enabled: isTauri(),
    staleTime: Infinity,
  });
}

/** Current preferences; defaults when they cannot be read. */
export async function getPrefs(): Promise<BranchSwitchPrefs> {
  try {
    return await queryClient.fetchQuery({
      queryKey: prefsKey,
      queryFn: () => commands.branchSwitchPrefsGet(),
      staleTime: Infinity,
    });
  } catch {
    return DEFAULT_PREFS;
  }
}

export async function savePrefs(patch: BranchSwitchPrefsPatch): Promise<void> {
  try {
    queryClient.setQueryData(prefsKey, await commands.branchSwitchPrefsSet(patch));
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
  }
}
