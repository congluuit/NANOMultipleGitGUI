import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";

export const updaterKeys = {
  status: ["updater", "status"] as const,
};

export function useUpdaterStatus() {
  return useQuery({
    queryKey: updaterKeys.status,
    queryFn: () => commands.updaterStatus(),
    enabled: isTauri(),
  });
}
