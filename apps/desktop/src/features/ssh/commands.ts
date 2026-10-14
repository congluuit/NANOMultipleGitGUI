/**
 * Command palette: "SSH keys…" → one entry per account (source `"ssh"`, separate from the
 * accounts feature's `"accounts"` commands). Accounts come from the query cache.
 */
import { KeyRound } from "lucide-react";
import {
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { accountKeys } from "@/features/accounts/queries";
import type { Account } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { accountName } from "./messages";

export function sshCommands(ctx: PaletteContext): PaletteCommand[] {
  const { t } = ctx;
  const accounts = queryClient.getQueryData<Account[]>(accountKeys.list) ?? [];
  const cmd: PaletteCommand = {
    id: "ssh.keys",
    title: t("ssh.cmd.open"),
    group: t("cmd.group.accounts"),
    icon: KeyRound,
    keywords: ["ssh keys", "khoa ssh", "public key", "ed25519", "deploy"],
    hint: accounts.length === 0 ? t("ssh.cmd.noAccounts") : undefined,
    when: () => accounts.length > 0,
    children: () =>
      accounts.map((a) => ({
        id: a.id,
        title: accountName(a),
        hint: a.username,
        group: "",
        keywords: [a.username],
        run: () => useAccountDialogs.getState().openSshKeys(a.id),
      })),
  };
  return [cmd];
}

registerCommands("ssh", sshCommands);
