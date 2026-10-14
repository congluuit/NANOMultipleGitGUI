import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";

export function useAppInfo() {
  return useQuery({
    queryKey: ["app-info"],
    queryFn: () => commands.appInfo(),
    enabled: isTauri(),
    staleTime: Infinity,
  });
}
