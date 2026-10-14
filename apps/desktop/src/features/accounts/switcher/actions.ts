/** Switcher actions shared by the pill, the banner, the shortcuts and the command palette. */
import i18n from "i18next";
import { isTauri } from "@tauri-apps/api/core";
import { worktreeKeys } from "@/features/worktree/queries";
import { commands, errorMessage, type Account, type RepoAccountContext } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { accountKeys } from "../queries";
import { accountLabel, sortAccounts } from "./model";
import { setOneShot, type OneShotOp } from "./oneShot";

const notify = (message: string, kind: "info" | "error" = "info") =>
  useUi.getState().showNotice({ kind, message });

function applyContext(repoId: number, ctx: RepoAccountContext) {
  queryClient.setQueryData(accountKeys.repoContext(repoId), ctx);
  // Binding may rewrite `user.name/email` in the repo config ("write identity to repo").
  void queryClient.invalidateQueries({ queryKey: worktreeKeys.identity(repoId) });
}

export interface BindOptions {
  accountId: string;
  /** `null` = the account's default identity. */
  identityId?: string | null;
  /** "Ghi nhớ": lock the binding above clone bindings and rules. */
  locked?: boolean;
  /** Bind one remote instead of the repo default. */
  remote?: string | null;
}

/** "Dùng cho repo này": saves the binding and shows "Đã chuyển repo sang ● <label>". */
export async function bindRepo(repoId: number, opts: BindOptions): Promise<boolean> {
  try {
    const ctx = await commands.repoBindingSet(repoId, {
      remote: opts.remote ?? null,
      accountId: opts.accountId,
      identityId: opts.identityId ?? null,
      locked: opts.locked ?? false,
      source: "manual",
    });
    applyContext(repoId, ctx);
    const account = ctx.account ?? cachedAccount(opts.accountId);
    const label = account ? accountLabel(account) : opts.accountId;
    notify(i18n.t(opts.locked ? "switcher.switchedLocked" : "switcher.switched", { label }));
    return true;
  } catch (e) {
    notify(errorMessage(e), "error");
    return false;
  }
}

/** "Bỏ gắn": removes the repo-level binding (or one remote's). */
export async function unbindRepo(repoId: number, remote: string | null = null): Promise<boolean> {
  try {
    applyContext(repoId, await commands.repoBindingClear(repoId, remote));
    notify(remote ? i18n.t("switcher.remoteUnbound", { remote }) : i18n.t("switcher.unbound"));
    return true;
  } catch (e) {
    notify(errorMessage(e), "error");
    return false;
  }
}

/** "Chỉ cho thao tác này": next `op` of the repo runs as this account (see `oneShot.ts`). */
export function setAccountOnce(
  repoId: number,
  accountId: string,
  identityId: string | null,
  op: OneShotOp,
) {
  setOneShot(repoId, { accountId, identityId, op });
  const account = cachedAccount(accountId);
  notify(
    i18n.t("switcher.oneShotSet", {
      op: i18n.t(`switcher.op.${op}`),
      label: account ? accountLabel(account) : accountId,
    }),
  );
}

function cachedAccount(id: string): Account | null {
  return queryClient.getQueryData<Account[]>(accountKeys.list)?.find((a) => a.id === id) ?? null;
}

/** Accounts in switcher order, loading them if no component has yet. */
async function loadAccounts(): Promise<Account[]> {
  const cached = queryClient.getQueryData<Account[]>(accountKeys.list);
  if (cached || !isTauri()) return sortAccounts(cached ?? []);
  const list = await queryClient.ensureQueryData({
    queryKey: accountKeys.list,
    queryFn: () => commands.accountList(),
  });
  return sortAccounts(list);
}

/**
 * `Ctrl/Cmd+Alt+N`: binds the active repo to the account at `index` (0-based, switcher order).
 * No-op (returns `false`) without a usable repo or when there is no such account.
 */
export async function bindNthAccount(index: number): Promise<boolean> {
  const { tabs, activeId } = useWorkspace.getState();
  const repo = tabs.find((t) => t.id === activeId);
  if (!repo || repo.missing) return false;
  const account = (await loadAccounts().catch(() => []))[index];
  if (!account) return false;
  return bindRepo(repo.id, { accountId: account.id });
}
