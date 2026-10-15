import { useState } from "react";
import { Check, Copy, LoaderCircle, LogIn, RefreshCw, Terminal, UserPlus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { commands, errorMessage, type CliAccountEntry, type CliImportItem } from "@/ipc";
import { cn } from "@/lib/utils";
import { useAccountDialogs } from "../dialogs";
import { copyText } from "../manager/openExternal";
import { Badge, Callout } from "../manager/ui";
import { invalidateAccounts } from "../queries";
import {
  defaultSelection,
  fixCommandOf,
  isAdded,
  newAccounts,
  resultTone,
  toRef,
  toolInstalled,
} from "./model";
import { rescanCliAccounts, useCliAccountsScan } from "./queries";

export interface CliAccountsPanelProps {
  /** Called after an import added at least one account (or refreshed its token). */
  onAdded?: () => void;
  /** No card header and tighter spacing (onboarding step, import wizard). */
  compact?: boolean;
}

/**
 * gh / glab accounts signed in on this machine that are not in the app yet, with a checkbox
 * each, "Add to app" and "Scan again" (PLAN 3B.1). Self-contained: runs its own scan query
 * and import, and refreshes the account queries so the pill / switcher / `Ctrl+Alt+1..9` see
 * the new accounts at once. Tokens are only read from the CLI when the user clicks Add.
 */
export function CliAccountsPanel({ onAdded, compact = false }: CliAccountsPanelProps) {
  const { t } = useTranslation();
  const scan = useCliAccountsScan();
  const [picked, setPicked] = useState<string[] | null>(null);
  const [results, setResults] = useState<Record<string, CliImportItem>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const data = scan.data;
  const fresh = newAccounts(data);
  const selected = picked ?? defaultSelection(data);
  // Accounts imported during this session stay listed with their result.
  const rows = data?.accounts.filter((a) => a.existingAccountId === null || results[a.key]) ?? [];
  const known = data?.accounts.filter((a) => a.existingAccountId !== null && !results[a.key]) ?? [];
  const chosen = fresh.filter((a) => selected.includes(a.key) && toolInstalled(data, a.tool));

  const toggle = (key: string, on: boolean) =>
    setPicked(on ? [...new Set([...selected, key])] : selected.filter((k) => k !== key));

  const run = async (entries: CliAccountEntry[]) => {
    if (entries.length === 0) return;
    setBusy(true);
    setError(null);
    try {
      const items = await commands.cliAccountsImport(entries.map(toRef));
      setResults((prev) => ({ ...prev, ...Object.fromEntries(items.map((i) => [i.key, i])) }));
      const done = new Set(items.filter((i) => isAdded(i.status)).map((i) => i.key));
      setPicked(selected.filter((k) => !done.has(k)));
      if (done.size > 0) {
        invalidateAccounts();
        onAdded?.();
      }
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const rescan = () => {
    setResults({});
    setPicked(null);
    setError(null);
    void rescanCliAccounts();
  };

  const rescanButton = (
    <Button
      size="xs"
      variant="outline"
      disabled={scan.isFetching || busy}
      onClick={rescan}
      aria-label={t("cliSync.rescan")}
    >
      <RefreshCw className={cn(scan.isFetching && "animate-spin")} />
      {t("cliSync.rescan")}
    </Button>
  );

  if (scan.isLoading)
    return (
      <p className="flex items-center gap-2 text-xs text-muted-foreground">
        <LoaderCircle className="size-3.5 animate-spin" />
        {t("cliSync.scanning")}
      </p>
    );
  if (scan.error)
    return (
      <Callout tone="danger" className="flex flex-wrap items-center gap-2">
        <span className="min-w-0 flex-1">
          {t("cliSync.scanFailed", { message: errorMessage(scan.error) })}
        </span>
        {rescanButton}
      </Callout>
    );
  if (!data) return null;

  if (rows.length === 0)
    return (
      <div
        className={cn(
          "flex flex-wrap items-center gap-2 text-xs text-muted-foreground",
          !compact && "rounded-md border border-dashed px-3 py-2",
        )}
      >
        <Terminal className="size-3.5 shrink-0" />
        <span className="min-w-0 flex-1">
          {data.accounts.length === 0
            ? t("cliSync.noneFound")
            : t("cliSync.allAdded", { count: data.accounts.length })}
        </span>
        {rescanButton}
      </div>
    );

  return (
    <section
      aria-label={t("cliSync.title", { count: fresh.length })}
      className={cn("grid gap-2 text-xs", !compact && "rounded-md border bg-muted/30 p-3")}
    >
      {!compact && (
        <div className="flex flex-wrap items-start gap-2">
          <Terminal className="mt-0.5 size-4 shrink-0 text-primary" />
          <div className="grid min-w-0 flex-1 gap-0.5">
            <span className="font-medium">
              {fresh.length > 0
                ? t("cliSync.title", { count: fresh.length })
                : t("cliSync.titleDone")}
            </span>
            <span className="text-muted-foreground">{t("cliSync.description")}</span>
          </div>
        </div>
      )}
      {compact && <p className="text-muted-foreground">{t("cliSync.description")}</p>}

      <ul className="grid gap-1.5">
        {rows.map((a) => (
          <AccountRow
            key={`cli-${a.key}`}
            entry={a}
            installed={toolInstalled(data, a.tool)}
            checked={selected.includes(a.key)}
            result={results[a.key]}
            busy={busy}
            onToggle={(on) => toggle(a.key, on)}
            onRetry={() => void run([a])}
          />
        ))}
      </ul>

      {known.length > 0 && (
        <details className="text-muted-foreground">
          <summary className="cursor-pointer select-none">
            {t("cliSync.known", { count: known.length })}
          </summary>
          <ul className="mt-1 grid gap-1">
            {known.map((a) => (
              <li key={`known-${a.key}`} className="flex flex-wrap items-center gap-2">
                <ToolBadge tool={a.tool} />
                <span>
                  {a.username ?? t("cliSync.unknownUser")} @ {a.host}
                </span>
                {a.canUpdate && toolInstalled(data, a.tool) && (
                  <Button
                    size="xs"
                    variant="ghost"
                    className="ml-auto"
                    disabled={busy}
                    onClick={() => void run([a])}
                  >
                    <RefreshCw />
                    {t("cliSync.updateToken", { tool: a.tool })}
                  </Button>
                )}
              </li>
            ))}
          </ul>
        </details>
      )}

      {error && (
        <p role="alert" className="text-destructive">
          {t("cliSync.importFailed", { message: error })}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <Button size="xs" disabled={busy || chosen.length === 0} onClick={() => void run(chosen)}>
          {busy ? <LoaderCircle className="animate-spin" /> : <UserPlus />}
          {busy ? t("cliSync.adding") : t("cliSync.add", { count: chosen.length })}
        </Button>
        {rescanButton}
        <span className="basis-full text-[11px] text-muted-foreground">{t("cliSync.consent")}</span>
      </div>
    </section>
  );
}

function ToolBadge({ tool }: { tool: CliAccountEntry["tool"] }) {
  return (
    <span className="rounded border px-1 font-mono text-[10px] text-muted-foreground uppercase">
      {tool}
    </span>
  );
}

function AccountRow({
  entry,
  installed,
  checked,
  result,
  busy,
  onToggle,
  onRetry,
}: {
  entry: CliAccountEntry;
  installed: boolean;
  checked: boolean;
  result: CliImportItem | undefined;
  busy: boolean;
  onToggle: (on: boolean) => void;
  onRetry: () => void;
}) {
  const { t } = useTranslation();
  const name = entry.username ?? t("cliSync.unknownUser");
  const done = result !== undefined && isAdded(result.status);
  return (
    <li className="grid gap-1 rounded-md border bg-background p-2">
      <label className="flex flex-wrap items-center gap-2">
        <input
          type="checkbox"
          aria-label={`${name} @ ${entry.host}`}
          checked={!done && installed && checked}
          disabled={done || !installed || busy}
          onChange={(e) => onToggle(e.target.checked)}
        />
        <ToolBadge tool={entry.tool} />
        <span className="font-medium">{name}</span>
        <span className="text-muted-foreground">@ {entry.host}</span>
        {entry.active && <Badge tone="ok">{t("cliSync.active", { tool: entry.tool })}</Badge>}
      </label>
      {!installed && !result && (
        <span className="pl-6 text-[11px] text-warning">
          {t("cliSync.notInstalled", { tool: entry.tool })}
        </span>
      )}
      {result && <ResultLine item={result} busy={busy} onRetry={onRetry} />}
    </li>
  );
}

const TONE_CLASS = {
  ok: "text-success",
  warn: "text-warning",
  danger: "text-destructive",
  muted: "text-muted-foreground",
} as const;

function ResultLine({
  item,
  busy,
  onRetry,
}: {
  item: CliImportItem;
  busy: boolean;
  onRetry: () => void;
}) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const vars = {
    tool: item.tool,
    host: item.host,
    user: item.username ?? t("cliSync.unknownUser"),
    actual: item.actualUsername ?? "",
    scopes: item.missingScopes.join(", "),
    message: item.message ?? "",
  };
  const command = fixCommandOf(item);
  const dialogs = useAccountDialogs.getState();
  const signInApp = (
    <Button size="xs" variant="outline" onClick={() => dialogs.openManager({ add: true })}>
      <LogIn />
      {t("cliSync.action.signInApp")}
    </Button>
  );
  return (
    <div className="grid gap-1 pl-6 text-[11px]" data-status={item.status}>
      <span
        className={cn("flex items-center gap-1 font-medium", TONE_CLASS[resultTone(item.status)])}
      >
        {isAdded(item.status) && <Check className="size-3" />}
        {t(`cliSync.status.${item.status}`, vars)}
      </span>
      {item.status !== "added" && (
        <span className="text-muted-foreground">
          {t(
            `cliSync.hint.${item.status === "missingScopes" && !command ? "missingScopesManual" : item.status}`,
            vars,
          )}
        </span>
      )}
      {command && (
        <span className="flex items-center gap-1">
          <code className="rounded bg-muted px-1 py-0.5 font-mono break-all">{command}</code>
          <Button
            size="icon-xs"
            variant="ghost"
            aria-label={t("cliSync.action.copy")}
            onClick={() => void copyText(command).then(setCopied)}
          >
            {copied ? <Check /> : <Copy />}
          </Button>
        </span>
      )}
      <span className="flex flex-wrap gap-1.5">
        {item.status === "missingScopes" && (
          <>
            <Button size="xs" variant="outline" disabled={busy} onClick={onRetry}>
              <RefreshCw />
              {t("cliSync.action.retry", { tool: item.tool })}
            </Button>
            {item.accountId && (
              <Button
                size="xs"
                variant="outline"
                onClick={() => dialogs.openManager({ accountId: item.accountId, reauth: true })}
              >
                <LogIn />
                {t("cliSync.action.reauth")}
              </Button>
            )}
          </>
        )}
        {(item.status === "invalidToken" ||
          item.status === "wrongUser" ||
          item.status === "noToken" ||
          item.status === "cliMissing") &&
          signInApp}
        {item.status === "failed" && (
          <Button size="xs" variant="outline" disabled={busy} onClick={onRetry}>
            <RefreshCw />
            {t("cliSync.action.tryAgain")}
          </Button>
        )}
      </span>
    </div>
  );
}
