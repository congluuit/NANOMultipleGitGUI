import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands, type SetupProgress, type SetupStatus } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export const setupKeys = {
  status: ["setup", "status"] as const,
  tools: ["setup", "tools"] as const,
};

export function useSetupStatus() {
  return useQuery({
    queryKey: setupKeys.status,
    queryFn: () => commands.setupStatus(),
    enabled: isTauri(),
    staleTime: Infinity,
  });
}

export function useSetupTools(enabled: boolean) {
  return useQuery({
    queryKey: setupKeys.tools,
    queryFn: () => commands.setupTools(),
    enabled: enabled && isTauri(),
    staleTime: 0,
    retry: false,
  });
}

/** Stores the progress; the cache is updated first so the UI never waits for the database. */
export async function saveProgress(progress: SetupProgress): Promise<void> {
  queryClient.setQueryData<SetupStatus>(setupKeys.status, (old) =>
    old ? { ...old, progress, shouldAutoOpen: false } : old,
  );
  if (!isTauri()) return;
  try {
    queryClient.setQueryData(setupKeys.status, await commands.setupSave(progress));
  } catch (e) {
    console.error("could not save onboarding.setup", e);
  }
}
