/**
 * Command palette contributions of the accounts feature (PLAN §4.5): Switch account… /
 * Commit as… / Push as… / Clone as… / Manage / Add / Import. Replaces the built-in disabled
 * placeholders registered under the same source. Data comes from the query cache (providers
 * are plain functions); the account pill keeps accounts, identities and the active repo's
 * context loaded.
 */
import {
  ArrowUpFromLine,
  Download,
  FileInput,
  GitCommitHorizontal,
  Settings2,
  Terminal,
  Unlink,
  UserPlus,
  UserRound,
} from "lucide-react";
import { focusCommitSummary, push } from "@/features/command-palette/actions";
import {
  registerCommands,
  type PaletteCommand,
  type PaletteContext,
} from "@/features/command-palette/registry";
import { pushPlan } from "@/features/remote/plan";
import type { Account, Identity, Provider } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { useAccountDialogs } from "./dialogs";
import { rescanCliAccounts } from "./cli-sync/queries";
import { accountKeys } from "./queries";
import { bindRepo, setAccountOnce, unbindRepo } from "./switcher/actions";
import { cachedAccounts, cachedRepoAccount } from "./switcher/hooks";
import {
  accountLabel,
  accountShortcutHint,
  hasRepoBinding,
  identitiesOf,
  providerNameOf,
} from "./switcher/model";

/** One sub-list entry per (account, identity); accounts without identities get one entry. */
interface Choice {
  account: Account;
  identity: Identity | null;
  /** First entry of its account (carries the `Ctrl/Cmd+Alt+N` hint). */
  first: boolean;
  index: number;
}

function choices(accounts: Account[], identities: Identity[]): Choice[] {
  return accounts.flatMap((account, index): Choice[] => {
    const ids = identitiesOf(identities, account.id);
    return ids.length === 0
      ? [{ account, identity: null, first: true, index }]
      : ids.map((identity, i) => ({ account, identity, first: i === 0, index }));
  });
}

export function accountCommands(c: PaletteContext): PaletteCommand[] {
  const { t, repo } = c;
  const group = t("cmd.group.accounts");
  const { accounts, identities } = cachedAccounts();
  const providers = queryClient.getQueryData<Provider[]>(accountKeys.providers);
  const current = repo ? cachedRepoAccount(repo.id) : null;
  const hasAccounts = accounts.length > 0;
  const noAccountsHint = hasAccounts ? undefined : t("switcher.cmd.noAccounts");
  const plan = c.refs ? pushPlan(c.refs) : null;

  const choiceItem = (ch: Choice, run: () => unknown, withShortcut: boolean): PaletteCommand => {
    const label = accountLabel(ch.account);
    const provider = providerNameOf(ch.account, providers);
    return {
      id: `${ch.account.id}:${ch.identity?.id ?? "-"}`,
      title: ch.identity ? `${label} · ${ch.identity.name} <${ch.identity.email}>` : label,
      group: "",
      icon: UserRound,
      hint: provider ?? undefined,
      keywords: [ch.account.username, ...(provider ? [provider] : [])],
      shortcut:
        withShortcut && ch.first
          ? (accountShortcutHint(ch.index, c.isMac) ?? undefined)
          : undefined,
      checked:
        !!current?.account &&
        current.account.id === ch.account.id &&
        (ch.identity === null || current.identity?.id === ch.identity.id),
      run,
    };
  };

  return [
    {
      id: "accounts.switch",
      title: t("cmd.accounts.switch"),
      group,
      icon: UserRound,
      hint:
        noAccountsHint ??
        (current?.label ? `${t("switcher.cmd.current")}: ${current.label}` : undefined),
      keywords: ["switch account", "chuyen tai khoan", "use for repo"],
      when: (x) => x.repo !== null && hasAccounts,
      children: (x) =>
        choices(accounts, identities).map((ch) =>
          choiceItem(
            ch,
            () =>
              x.repo &&
              bindRepo(x.repo.id, {
                accountId: ch.account.id,
                identityId: ch.identity?.id ?? null,
              }),
            true,
          ),
        ),
    },
    {
      id: "accounts.commitAs",
      title: t("cmd.accounts.commitAs"),
      group,
      icon: GitCommitHorizontal,
      hint: noAccountsHint,
      keywords: ["commit as", "commit voi tu cach"],
      when: (x) => x.repo !== null && hasAccounts,
      children: (x) =>
        choices(accounts, identities).map((ch) =>
          choiceItem(
            ch,
            () => {
              if (!x.repo) return;
              setAccountOnce(x.repo.id, ch.account.id, ch.identity?.id ?? null, "commit");
              return focusCommitSummary(x.repo.id);
            },
            false,
          ),
        ),
    },
    {
      id: "accounts.pushAs",
      title: t("cmd.accounts.pushAs"),
      group,
      icon: ArrowUpFromLine,
      hint: noAccountsHint ?? (plan ? `${plan.remote}/${plan.branch}` : undefined),
      keywords: ["push as", "push voi tu cach"],
      when: (x) => x.repo !== null && hasAccounts && !x.busy,
      children: (x) =>
        choices(accounts, identities).map((ch) =>
          choiceItem(
            ch,
            () => {
              if (!x.repo) return;
              setAccountOnce(x.repo.id, ch.account.id, ch.identity?.id ?? null, "push");
              if (x.refs && pushPlan(x.refs)) return push(x.repo.id, x.refs, false);
              useUi.getState().showNotice({ kind: "info", message: t("switcher.cmd.pushLater") });
            },
            false,
          ),
        ),
    },
    {
      id: "accounts.cloneAs",
      title: t("cmd.accounts.cloneAs"),
      group,
      icon: Download,
      hint: noAccountsHint,
      keywords: ["clone as", "clone voi tu cach"],
      when: () => hasAccounts,
      children: () =>
        accounts.map((a) => ({
          id: a.id,
          title: accountLabel(a),
          group: "",
          icon: UserRound,
          hint: providerNameOf(a, providers) ?? undefined,
          keywords: [a.username],
          run: () => useAccountDialogs.getState().openClone({ accountId: a.id }),
        })),
    },
    {
      id: "accounts.unbind",
      title: t("switcher.cmd.unbind"),
      group,
      icon: Unlink,
      hint: current?.label ?? undefined,
      keywords: ["unbind account", "bo gan tai khoan"],
      when: (x) => x.repo !== null && hasRepoBinding(current?.context ?? undefined),
      run: (x) => x.repo && unbindRepo(x.repo.id),
    },
    {
      id: "accounts.manage",
      title: t("switcher.cmd.manage"),
      group,
      icon: Settings2,
      keywords: ["manage accounts", "quan ly tai khoan", "rules"],
      run: () => useAccountDialogs.getState().openManager(),
    },
    {
      id: "accounts.add",
      title: t("switcher.cmd.add"),
      group,
      icon: UserPlus,
      keywords: ["add account", "sign in", "login", "them tai khoan", "dang nhap"],
      run: () => useAccountDialogs.getState().openManager({ add: true }),
    },
    {
      id: "accounts.import",
      title: t("switcher.cmd.import"),
      group,
      icon: FileInput,
      keywords: ["import", "gitconfig", "ssh config", "gh", "glab", "nhap cau hinh"],
      run: () => useAccountDialogs.getState().openImportWizard(),
    },
    {
      // Phase 3B.1: the CLI panel sits at the top of the account manager's Accounts tab.
      id: "accounts.importCli",
      title: t("cliSync.cmd.import"),
      group,
      icon: Terminal,
      keywords: ["gh", "glab", "github cli", "gitlab cli", "import", "nhap tai khoan", "cli"],
      run: () => {
        void rescanCliAccounts();
        useAccountDialogs.getState().openManager();
      },
    },
  ];
}

registerCommands("accounts", accountCommands);
