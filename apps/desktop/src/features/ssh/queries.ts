import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";

export const sshKeys = {
  all: ["ssh"] as const,
  list: (accountId: string | null) => ["ssh", "keys", accountId] as const,
  permissions: (keyId: string) => ["ssh", "permissions", keyId] as const,
};

/** SSH keys of one account (or every key when `accountId` is `null`). */
export function useSshKeys(accountId: string | null, enabled = true) {
  return useQuery({
    queryKey: sshKeys.list(accountId),
    queryFn: () => commands.sshKeyList(accountId),
    enabled: enabled && isTauri(),
  });
}

/** Whether the private key file is readable by the current user only (runs `icacls` on Windows). */
export function useSshKeyPermissions(keyId: string, enabled: boolean) {
  return useQuery({
    queryKey: sshKeys.permissions(keyId),
    queryFn: () => commands.sshKeyCheckPermissions(keyId),
    enabled: enabled && isTauri(),
    staleTime: 60_000,
    retry: false,
  });
}

export function invalidateSshKeys() {
  void queryClient.invalidateQueries({ queryKey: sshKeys.all });
}
