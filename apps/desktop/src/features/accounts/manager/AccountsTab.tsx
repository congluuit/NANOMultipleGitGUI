import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  ChevronDown,
  ChevronUp,
  FolderOpen,
  KeyRound,
  LoaderCircle,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import { AccountsBackupButtons } from "@/features/backup/AccountsBackupButtons";
import { commands, errorMessage, type Account, type AccountHealth, type Provider } from "@/ipc";
import { cn } from "@/lib/utils";
import { AccountAvatar } from "../AccountAvatar";
import { CliAccountsPanel } from "../cli-sync/CliAccountsPanel";
import { authMethodText } from "../cli-sync/labels";
import { useAccountDialogs } from "../dialogs";
import { moveRule, textOn } from "./forms";
import { expiryText, healthBadges } from "./health";
import { IdentitiesSection } from "./IdentitiesSection";
import { refreshAccounts, refreshHealth, useAccountColors } from "./queries";
import { Badge, Callout, Field, TextInput } from "./ui";

interface Props {
  accounts: readonly Account[];
  providers: readonly Provider[];
  health: readonly AccountHealth[];
  focusId: string | null;
  onAdd: () => void;
  onReauth: (account: Account) => void;
}

/** Account cards in switcher order (Ctrl/Cmd+Alt+1..9), with reorder and inline editing. */
export function AccountsTab({ accounts, providers, health, focusId, onAdd, onReauth }: Props) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState<string | null>(focusId);
  const [error, setError] = useState<string | null>(null);

  const move = async (index: number, delta: number) => {
    const ids = accounts.map((a) => a.id);
    const next = moveRule(ids, index, delta);
    if (next.join() === ids.join()) return;
    setError(null);
    try {
      await commands.accountReorder(next);
      refreshAccounts();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="grid gap-3">
      <div className="flex items-center justify-between gap-2">
        <p className="text-[11px] text-muted-foreground">{t("accounts.list.orderHint")}</p>
        <AccountsBackupButtons canExport={accounts.length > 0} />
        <Button size="sm" onClick={onAdd}>
          <Plus />
          {t("accounts.list.add")}
        </Button>
      </div>
      {accounts.length === 0 && (
        <Callout>
          <p>{t("accounts.list.empty")}</p>
        </Callout>
      )}
      <CliAccountsPanel />
      {error && <Callout tone="danger">{error}</Callout>}
      <ol className="grid gap-2">
        {accounts.map((a, index) => (
          <AccountCard
            key={a.id}
            account={a}
            index={index}
            last={index === accounts.length - 1}
            provider={providers.find((p) => p.id === a.providerId) ?? null}
            health={health.find((h) => h.accountId === a.id)}
            expanded={expanded === a.id}
            onToggle={() => setExpanded((cur) => (cur === a.id ? null : a.id))}
            onMove={(delta) => void move(index, delta)}
            onReauth={() => onReauth(a)}
          />
        ))}
      </ol>
    </div>
  );
}

function AccountCard({
  account,
  index,
  last,
  provider,
  health,
  expanded,
  onToggle,
  onMove,
  onReauth,
}: {
  account: Account;
  index: number;
  last: boolean;
  provider: Provider | null;
  health: AccountHealth | undefined;
  expanded: boolean;
  onToggle: () => void;
  onMove: (delta: number) => void;
  onReauth: () => void;
}) {
  const { t } = useTranslation();
  const badges = healthBadges(health, t);
  const name = account.label ?? account.displayName ?? account.username;
  return (
    <li className="rounded-md border" aria-label={name}>
      <div className="flex items-center gap-3 p-2.5">
        <div className="flex flex-col">
          <Button
            size="icon-xs"
            variant="ghost"
            aria-label={t("accounts.list.moveUp")}
            disabled={index === 0}
            onClick={() => onMove(-1)}
          >
            <ChevronUp />
          </Button>
          <Button
            size="icon-xs"
            variant="ghost"
            aria-label={t("accounts.list.moveDown")}
            disabled={last}
            onClick={() => onMove(1)}
          >
            <ChevronDown />
          </Button>
        </div>
        <AccountAvatar account={account} size={32} />
        <button
          type="button"
          className="grid min-w-0 flex-1 gap-0.5 text-left text-xs"
          aria-expanded={expanded}
          onClick={onToggle}
        >
          <span className="flex items-center gap-1.5 font-medium">
            <span
              aria-hidden
              className="size-2 shrink-0 rounded-full"
              style={{ background: account.color }}
            />
            <span className="truncate">{name}</span>
            {index < 9 && (
              <kbd className="rounded border px-1 font-mono text-[10px] text-muted-foreground">
                Ctrl+Alt+{index + 1}
              </kbd>
            )}
          </span>
          <span className="truncate text-muted-foreground">
            {provider?.name ?? account.providerId} · @{account.username} ·{" "}
            {authMethodText(t, account, provider?.kind)}
          </span>
          <span className="flex flex-wrap gap-1">
            {badges.map((b) => (
              <Badge key={b.text} tone={b.tone}>
                {b.text}
              </Badge>
            ))}
          </span>
        </button>
        {badges.some((b) => b.reauth) && (
          <Button size="xs" variant="outline" onClick={onReauth}>
            {t("accounts.health.reauth")}
          </Button>
        )}
      </div>
      {expanded && <AccountDetails account={account} health={health} onReauth={onReauth} />}
    </li>
  );
}

function AccountDetails({
  account,
  health,
  onReauth,
}: {
  account: Account;
  health: AccountHealth | undefined;
  onReauth: () => void;
}) {
  const { t } = useTranslation();
  const colors = useAccountColors();
  const openSshKeys = useAccountDialogs((s) => s.openSshKeys);
  const [label, setLabel] = useState(account.label ?? "");
  const [cloneDir, setCloneDir] = useState(account.defaultCloneDir ?? "");
  const [busy, setBusy] = useState<"save" | "refresh" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const palette = colors.data ?? [];

  const save = async (patch: { color?: string; label?: string; defaultCloneDir?: string }) => {
    setBusy("save");
    setError(null);
    try {
      await commands.accountSave({
        id: account.id,
        providerId: account.providerId,
        username: account.username,
        displayName: account.displayName,
        avatarUrl: account.avatarUrl,
        authMethod: account.authMethod,
        scopes: account.scopes,
        tokenExpiresAt: account.tokenExpiresAt,
        color: patch.color ?? account.color,
        label: patch.label ?? label,
        defaultCloneDir: patch.defaultCloneDir ?? cloneDir,
      });
      refreshAccounts();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  const pickDir = async () => {
    const dir = await openDialog({
      directory: true,
      multiple: false,
      title: t("accounts.field.cloneDir"),
    });
    if (typeof dir === "string") {
      setCloneDir(dir);
      void save({ defaultCloneDir: dir });
    }
  };

  const recheck = async () => {
    setBusy("refresh");
    setError(null);
    try {
      await commands.accountRefresh(account.id);
      refreshHealth();
      refreshAccounts();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  const remove = async () => {
    setConfirmDelete(false);
    try {
      await commands.accountDelete(account.id);
      refreshAccounts();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  return (
    <div className="grid gap-4 border-t p-3">
      <div className="grid grid-cols-2 gap-3">
        <Field label={t("accounts.field.label")} hint={t("accounts.field.labelHint")}>
          <TextInput
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            onBlur={() => label !== (account.label ?? "") && void save({ label })}
          />
        </Field>
        <Field label={t("accounts.field.cloneDir")}>
          <div className="flex gap-1">
            <TextInput
              value={cloneDir}
              placeholder={t("accounts.field.cloneDirPlaceholder")}
              onChange={(e) => setCloneDir(e.target.value)}
              onBlur={() =>
                cloneDir !== (account.defaultCloneDir ?? "") &&
                void save({ defaultCloneDir: cloneDir })
              }
            />
            <Button
              type="button"
              size="icon-sm"
              variant="outline"
              aria-label={t("accounts.field.browse")}
              onClick={() => void pickDir()}
            >
              <FolderOpen />
            </Button>
          </div>
        </Field>
      </div>

      <div className="grid gap-1.5 text-xs">
        <span className="text-muted-foreground">{t("accounts.field.color")}</span>
        <div
          role="radiogroup"
          aria-label={t("accounts.field.color")}
          className="flex flex-wrap gap-1.5"
        >
          {palette.map((c) => (
            <button
              key={c}
              type="button"
              role="radio"
              aria-checked={c.toLowerCase() === account.color.toLowerCase()}
              aria-label={c}
              title={c}
              onClick={() => void save({ color: c })}
              className={cn(
                "flex size-6 items-center justify-center rounded-full text-[10px] font-bold ring-offset-2 ring-offset-background",
                c.toLowerCase() === account.color.toLowerCase() && "ring-2 ring-foreground",
              )}
              style={{ background: c, color: textOn(c) }}
            >
              {(account.label ?? account.username).charAt(0).toUpperCase()}
            </button>
          ))}
        </div>
      </div>

      <div className="grid gap-1 text-xs">
        <span className="text-muted-foreground">{t("accounts.field.token")}</span>
        <div className="flex flex-wrap items-center gap-1">
          {health && <span>{expiryText(health.expiresAt, health.autoRefresh, t)}</span>}
          {account.scopes.map((s) => (
            <Badge key={s} className="font-mono">
              {s}
            </Badge>
          ))}
          {health && !health.scopesKnown && health.hasToken && (
            <span className="text-muted-foreground">{t("accounts.health.scopesUnknown")}</span>
          )}
        </div>
        {health?.optionalMissing.length ? (
          <span className="text-[11px] text-muted-foreground">
            {t("accounts.health.optionalMissing", {
              scopes: health.optionalMissing.map((m) => m.anyOf.join("/")).join(", "),
            })}
          </span>
        ) : null}
        {health?.lastError && (
          <span className="text-[11px] text-destructive">{health.lastError}</span>
        )}
      </div>

      <IdentitiesSection accountId={account.id} hasToken={health?.hasToken ?? false} />

      {error && <Callout tone="danger">{error}</Callout>}
      <div className="flex flex-wrap gap-2">
        <Button size="xs" variant="outline" onClick={onReauth}>
          {t("accounts.health.reauth")}
        </Button>
        <Button
          size="xs"
          variant="outline"
          disabled={busy !== null || !health?.hasToken}
          onClick={() => void recheck()}
        >
          {busy === "refresh" ? <LoaderCircle className="animate-spin" /> : <RefreshCw />}
          {t("accounts.list.recheck")}
        </Button>
        <Button size="xs" variant="outline" onClick={() => openSshKeys(account.id)}>
          <KeyRound />
          {t("accounts.list.sshKeys")}
        </Button>
        <Button
          size="xs"
          variant="ghost"
          className="ml-auto text-destructive"
          onClick={() => setConfirmDelete(true)}
        >
          <Trash2 />
          {t("accounts.list.delete")}
        </Button>
      </div>
      <ConfirmDialog
        open={confirmDelete}
        title={t("accounts.list.deleteTitle", { name: account.label ?? account.username })}
        description={t("accounts.list.deleteBody")}
        confirmLabel={t("accounts.list.delete")}
        destructive
        onCancel={() => setConfirmDelete(false)}
        onConfirm={() => void remove()}
      />
    </div>
  );
}
