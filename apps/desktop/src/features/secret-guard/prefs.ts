/** Secret Guard preferences (SQLite via `secret_guard_prefs` / `secret_guard_set_enabled`). */
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, errorMessage, type SecretGuardPrefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";

export const prefsKey = ["secretGuardPrefs"] as const;

export const DEFAULT_PREFS: SecretGuardPrefs = { enabled: true, ignoredCount: 0 };

export function useSecretGuardPrefs() {
  return useQuery({
    queryKey: prefsKey,
    queryFn: () => commands.secretGuardPrefs(),
    enabled: isTauri(),
    staleTime: Infinity,
  });
}

async function save(run: () => Promise<SecretGuardPrefs>): Promise<boolean> {
  try {
    queryClient.setQueryData(prefsKey, await run());
    return true;
  } catch (e) {
    useUi.getState().showNotice({ kind: "error", message: errorMessage(e) });
    return false;
  }
}

export const setSecretGuardEnabled = (enabled: boolean) =>
  save(() => commands.secretGuardSetEnabled(enabled));

export const clearIgnoredSecrets = () => save(() => commands.secretGuardClearIgnored());
