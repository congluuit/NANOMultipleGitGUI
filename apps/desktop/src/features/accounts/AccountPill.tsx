import { useState } from "react";
import { Check, ChevronDown, Lock, Plus, Settings2, Unlink, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { Account, Identity, RemoteAccountContext } from "@/ipc";
import { cn } from "@/lib/utils";
import { useActiveRepo } from "@/stores/workspace";
import { AccountAvatar } from "./AccountAvatar";
import { useAccountDialogs } from "./dialogs";
import { bindRepo, setAccountOnce, unbindRepo } from "./switcher/actions";
import {
  useAllIdentities,
  useProviderName,
  useRepoAccount,
  useSortedAccounts,
  useSourceText,
} from "./switcher/hooks";
import {
  accountLabel,
  accountShortcutHint,
  hasRepoBinding,
  hasSplitRemotes,
  identitiesOf,
  type RepoAccount,
} from "./switcher/model";
import { clearOneShot, useOneShot } from "./switcher/oneShot";

type Mode = "repo" | "once";

const keepOpen = (e: Event) => e.preventDefault();

function IdentityLine({ identity }: { identity: Identity }) {
  return (
    <span className="truncate text-xs text-muted-foreground">
      {identity.name} &lt;{identity.email}&gt;
    </span>
  );
}

/** One account (and its identities when it has several) of the switcher list. */
function AccountRows({
  account,
  index,
  identities,
  current,
  providerName,
  disabled,
  isMac,
  onChoose,
}: {
  account: Account;
  index: number;
  identities: Identity[];
  current: RepoAccount;
  providerName: string | null;
  disabled: boolean;
  isMac: boolean;
  onChoose: (accountId: string, identityId: string | null) => void;
}) {
  const { t } = useTranslation();
  const isCurrent = current.account?.id === account.id;
  const single = identities.length <= 1;
  const shortcut = accountShortcutHint(index, isMac);
  return (
    <DropdownMenuGroup>
      <DropdownMenuItem
        disabled={disabled}
        data-current={isCurrent && single ? "true" : undefined}
        onSelect={() => onChoose(account.id, null)}
        className="gap-2 py-1.5"
      >
        <AccountAvatar account={account} size={22} />
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate text-sm">
            {accountLabel(account)}
            {providerName && <span className="text-muted-foreground"> ({providerName})</span>}
          </span>
          {single && identities[0] ? (
            <IdentityLine identity={identities[0]} />
          ) : (
            <span className="truncate text-xs text-muted-foreground">
              @{account.username}
              {identities.length === 0 && ` · ${t("switcher.noIdentity")}`}
            </span>
          )}
        </span>
        {isCurrent && single && <Check className="size-4 text-primary" />}
        {shortcut && <DropdownMenuShortcut>{shortcut}</DropdownMenuShortcut>}
      </DropdownMenuItem>
      {!single &&
        identities.map((i) => {
          const on = isCurrent && current.identity?.id === i.id;
          return (
            <DropdownMenuItem
              key={i.id}
              disabled={disabled}
              data-current={on ? "true" : undefined}
              onSelect={() => onChoose(account.id, i.id)}
              className="py-1 pl-10"
            >
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-xs">
                  {i.name}
                  {i.isDefault && (
                    <span className="text-muted-foreground">
                      {" "}
                      · {t("switcher.defaultIdentity")}
                    </span>
                  )}
                </span>
                <span className="truncate text-xs text-muted-foreground">{i.email}</span>
              </span>
              {on && <Check className="size-4 text-primary" />}
            </DropdownMenuItem>
          );
        })}
    </DropdownMenuGroup>
  );
}

/** A remote bound differently from the repo: bind it to another account or clear its binding. */
function RemoteRow({
  repoId,
  remote,
  accounts,
}: {
  repoId: number;
  remote: RemoteAccountContext;
  accounts: Account[];
}) {
  const { t } = useTranslation();
  const sourceText = useSourceText();
  return (
    <DropdownMenuSub>
      <DropdownMenuSubTrigger className="gap-2 text-xs" title={remote.url}>
        <span className="font-mono">{remote.remote}</span>
        <span className="text-muted-foreground">→</span>
        {remote.account && <AccountAvatar account={remote.account} size={14} />}
        <span className="min-w-0 flex-1 truncate">
          {remote.account ? accountLabel(remote.account) : t("switcher.unboundShort")}
          <span className="text-muted-foreground">
            {" · "}
            {remote.inherited ? t("switcher.inherited") : sourceText(remote)}
          </span>
        </span>
      </DropdownMenuSubTrigger>
      <DropdownMenuSubContent className="w-60">
        {accounts.map((a) => (
          <DropdownMenuItem
            key={a.id}
            onSelect={() => void bindRepo(repoId, { accountId: a.id, remote: remote.remote })}
          >
            <AccountAvatar account={a} size={16} />
            <span className="flex-1 truncate">{accountLabel(a)}</span>
            {!remote.inherited && remote.account?.id === a.id && (
              <Check className="size-4 text-primary" />
            )}
          </DropdownMenuItem>
        ))}
        {!remote.inherited && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem onSelect={() => void unbindRepo(repoId, remote.remote)}>
              <Unlink />
              {t("switcher.clearRemote", { remote: remote.remote })}
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuSubContent>
    </DropdownMenuSub>
  );
}

/**
 * Account pill of the top bar (PLAN §4.5): avatar + label of the account the active repo uses.
 * The menu binds the repo ("Dùng cho repo này", optionally locked with "Ghi nhớ") or sets a
 * one-time override ("Chỉ cho thao tác này"), per account and identity.
 */
export function AccountPill() {
  const { t } = useTranslation();
  const tab = useActiveRepo();
  const repoId = tab && !tab.missing ? tab.id : null;
  const current = useRepoAccount(repoId);
  const accounts = useSortedAccounts();
  const identities = useAllIdentities();
  const providerName = useProviderName();
  const oneShot = useOneShot(repoId);
  const openManager = useAccountDialogs((s) => s.openManager);
  const sourceText = useSourceText();
  const [mode, setMode] = useState<Mode>("repo");
  const [remember, setRemember] = useState<boolean | null>(null);
  const locked = remember ?? current.source === "locked";
  const isMac = navigator.userAgent.includes("Mac");
  const shotAccount = oneShot ? accounts.find((a) => a.id === oneShot.accountId) : undefined;

  const choose = (accountId: string, identityId: string | null) => {
    if (repoId === null) return;
    if (mode === "once") setAccountOnce(repoId, accountId, identityId, "any");
    else void bindRepo(repoId, { accountId, identityId, locked });
  };

  const onOpenChange = (open: boolean) => {
    if (open) return;
    setMode("repo");
    setRemember(null);
  };

  const title =
    repoId === null ? t("switcher.noRepo") : `${t("switcher.pillTitle")} · ${sourceText(current)}`;

  return (
    <DropdownMenu onOpenChange={onOpenChange}>
      <DropdownMenuTrigger
        data-testid="account-pill"
        className="flex h-7 max-w-56 items-center gap-2 rounded-full border bg-surface px-1.5 pr-2.5 text-xs font-medium hover:bg-surface-2"
        style={current.color ? { borderColor: current.color } : undefined}
        title={title}
        aria-label={`${t("account.switch")}: ${current.label ?? t("switcher.unboundShort")}`}
      >
        <AccountAvatar account={current.account} size={20} />
        <span className={cn("truncate", !current.account && "text-muted-foreground")}>
          {current.label ?? t("switcher.unboundShort")}
        </span>
        {current.source === "locked" && <Lock className="size-3 text-muted-foreground" />}
        {oneShot && (
          <span
            className="rounded-full px-1 text-[10px] text-white"
            style={{ background: shotAccount?.color ?? "var(--primary)" }}
            title={t("switcher.pending", {
              op: t(`switcher.op.${oneShot.op}`),
              label: shotAccount ? accountLabel(shotAccount) : oneShot.accountId,
            })}
          >
            1×
          </span>
        )}
        <ChevronDown className="size-3.5 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-80">
        <DropdownMenuLabel className="flex flex-col gap-0.5 text-xs font-normal">
          {repoId === null || !tab ? (
            <span className="text-muted-foreground">{t("switcher.noRepo")}</span>
          ) : (
            <>
              <span className="truncate font-medium">{tab.name}</span>
              <span className="truncate text-muted-foreground" data-testid="account-source">
                {current.label ? `${current.label} · ` : ""}
                {sourceText(current)}
                {current.identity && ` · ${current.identity.email}`}
              </span>
            </>
          )}
        </DropdownMenuLabel>
        {repoId !== null && oneShot && (
          <DropdownMenuItem onSelect={() => clearOneShot(repoId)} className="text-xs">
            <X />
            <span className="min-w-0 flex-1 truncate">
              {t("switcher.pending", {
                op: t(`switcher.op.${oneShot.op}`),
                label: shotAccount ? accountLabel(shotAccount) : oneShot.accountId,
              })}
            </span>
            <span className="text-muted-foreground">{t("switcher.cancelPending")}</span>
          </DropdownMenuItem>
        )}
        {repoId !== null && accounts.length > 0 && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuRadioGroup value={mode} onValueChange={(v) => setMode(v as Mode)}>
              <DropdownMenuRadioItem value="repo" onSelect={keepOpen} className="text-xs">
                {t("account.useForRepo")}
              </DropdownMenuRadioItem>
              <DropdownMenuRadioItem value="once" onSelect={keepOpen} className="text-xs">
                {t("account.onlyThisAction")}
              </DropdownMenuRadioItem>
            </DropdownMenuRadioGroup>
            {mode === "repo" ? (
              <DropdownMenuCheckboxItem
                checked={locked}
                onCheckedChange={(v) => setRemember(v === true)}
                onSelect={keepOpen}
                className="text-xs"
              >
                {t("switcher.remember")}
              </DropdownMenuCheckboxItem>
            ) : (
              <p className="px-2 py-1 text-[11px] text-muted-foreground">
                {t("switcher.onceHint")}
              </p>
            )}
          </>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuLabel className="text-xs text-muted-foreground">
          {t("switcher.accounts")}
        </DropdownMenuLabel>
        {accounts.length === 0 && (
          <p className="px-2 pb-1 text-xs text-muted-foreground">{t("switcher.noAccounts")}</p>
        )}
        {accounts.map((a, i) => (
          <AccountRows
            key={a.id}
            account={a}
            index={i}
            identities={identitiesOf(identities, a.id)}
            current={current}
            providerName={providerName(a.providerId)}
            disabled={repoId === null}
            isMac={isMac}
            onChoose={choose}
          />
        ))}
        {repoId !== null && current.context && hasSplitRemotes(current.context) && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuLabel className="text-xs text-muted-foreground">
              {t("switcher.perRemote")}
            </DropdownMenuLabel>
            {current.context.perRemote.map((r) => (
              <RemoteRow key={r.remote} repoId={repoId} remote={r} accounts={accounts} />
            ))}
          </>
        )}
        <DropdownMenuSeparator />
        {repoId !== null && hasRepoBinding(current.context ?? undefined) && (
          <DropdownMenuItem onSelect={() => void unbindRepo(repoId)}>
            <Unlink />
            {t("switcher.unbind")}
          </DropdownMenuItem>
        )}
        <DropdownMenuItem onSelect={() => openManager()}>
          <Settings2 />
          {t("account.manage")}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => openManager({ add: true })}>
          <Plus />
          {t("switcher.add")}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
