import { useState, type ReactNode } from "react";
import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import {
  CheckCircle2,
  CircleAlert,
  CircleDashed,
  FileInput,
  KeyRound,
  LoaderCircle,
  LogIn,
  RefreshCw,
  UserPlus,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { CliAccountsPanel } from "@/features/accounts/cli-sync";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { invalidateAccounts, useAccounts, useProviders } from "@/features/accounts/queries";
import { invalidateSshKeys } from "@/features/ssh/queries";
import { accountName } from "@/features/ssh/messages";
import {
  commands,
  errorMessage,
  type Account,
  type ApplyItem,
  type ImportApplyResult,
  type ImportScan,
  type SourceRef,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { cn } from "@/lib/utils";
import {
  buildSelection,
  fillMissingAccounts,
  initialState,
  isEmptyScan,
  keyCandidates,
  needsAccount,
  selectionSize,
  setIdentityAccount,
  type Choice,
  type WizardState,
} from "./model";
import { onboardingKeys } from "./queries";

/** Closing the wizard in any way (Skip, Done, ×, Esc) marks the import as handled. */
async function markDone() {
  if (!isTauri()) return;
  try {
    const status = await commands.onboardingSetImportDone(true);
    queryClient.setQueryData(onboardingKeys.status, status);
  } catch (e) {
    console.error("could not save onboarding.importDone", e);
  }
}

/**
 * Mounted once in `App.tsx`. It no longer opens by itself: the first-run setup
 * (`setup/SetupWizard.tsx`) replaced that and opens it from its accounts step.
 */
export function ImportWizardHost() {
  const open = useAccountDialogs((s) => s.importWizard);
  const close = () => {
    useAccountDialogs.getState().close("importWizard");
    void markDone();
  };
  return (
    <Dialog open={open} onOpenChange={(o) => !o && close()}>
      {open && <ImportWizard onClose={close} />}
    </Dialog>
  );
}

type Step = "scan" | "choose" | "apply";

export function ImportWizard({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const [step, setStep] = useState<Step>("scan");
  const scan = useQuery({
    queryKey: onboardingKeys.scan,
    queryFn: () => commands.importScan(),
    enabled: isTauri(),
    staleTime: 0,
    gcTime: 0,
  });
  const accounts = useAccounts().data ?? [];
  const providers = useProviders().data ?? [];
  const [state, setState] = useState<WizardState | null>(null);
  const [result, setResult] = useState<ImportApplyResult | null>(null);
  const [applying, setApplying] = useState(false);
  const [applyError, setApplyError] = useState<string | null>(null);

  // New accounts (signed in from the wizard) fill items that had none.
  const accountsKey = accounts.map((a) => a.id).join(",");
  const [seenAccounts, setSeenAccounts] = useState(accountsKey);
  if (seenAccounts !== accountsKey) {
    setSeenAccounts(accountsKey);
    if (state && scan.data) setState(fillMissingAccounts(state, scan.data, accounts, providers));
  }

  const toChoose = (data: ImportScan) => {
    setState(initialState(data, accounts, providers));
    setStep("choose");
  };

  const apply = async (data: ImportScan, s: WizardState) => {
    setApplying(true);
    setApplyError(null);
    try {
      const r = await commands.importApply(buildSelection(data, s, accounts));
      setResult(r);
      setStep("apply");
      queryClient.setQueryData(onboardingKeys.status, (old: unknown) =>
        old && typeof old === "object"
          ? { ...old, importDone: true, shouldOpenWizard: false }
          : old,
      );
      invalidateAccounts();
      invalidateSshKeys();
    } catch (e) {
      setApplyError(errorMessage(e));
    } finally {
      setApplying(false);
    }
  };

  const steps: Step[] = ["scan", "choose", "apply"];
  const selection = scan.data && state ? buildSelection(scan.data, state, accounts) : null;
  const count = selection ? selectionSize(selection) : 0;

  return (
    <DialogContent className="flex max-h-[88vh] flex-col sm:max-w-3xl">
      <DialogHeader>
        <DialogTitle className="flex items-center gap-2">
          <FileInput className="size-4" />
          {t("onboarding.title")}
        </DialogTitle>
        <DialogDescription>{t("onboarding.subtitle")}</DialogDescription>
        <ol className="flex gap-3 pt-1 text-[11px]">
          {steps.map((s, i) => (
            <li
              key={s}
              aria-current={s === step ? "step" : undefined}
              className={cn(
                "flex items-center gap-1",
                s === step ? "font-semibold text-foreground" : "text-muted-foreground",
              )}
            >
              <span
                className={cn(
                  "grid size-4 place-items-center rounded-full border text-[10px]",
                  s === step && "border-primary bg-primary text-primary-foreground",
                )}
              >
                {i + 1}
              </span>
              {t(`onboarding.step.${s}`)}
            </li>
          ))}
        </ol>
      </DialogHeader>

      <div className="-mx-1 min-h-0 flex-1 overflow-y-auto px-1 text-xs">
        {step === "scan" && <ScanStep scan={scan} />}
        {step === "choose" && scan.data && state && (
          <ChooseStep scan={scan.data} state={state} setState={setState} accounts={accounts} />
        )}
        {step === "apply" && result && <ResultStep result={result} />}
        {applyError && (
          <p role="alert" className="mt-2 text-destructive">
            {applyError}
          </p>
        )}
      </div>

      <DialogFooter className="gap-2 sm:justify-between">
        {step !== "apply" ? (
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("onboarding.skip")}
          </Button>
        ) : (
          <span />
        )}
        <div className="flex gap-2">
          {step === "scan" && (
            <>
              <Button
                variant="outline"
                size="sm"
                disabled={scan.isFetching}
                onClick={() => void scan.refetch()}
              >
                <RefreshCw />
                {t("onboarding.rescan")}
              </Button>
              {scan.data && isEmptyScan(scan.data) ? (
                <Button
                  size="sm"
                  onClick={() => {
                    onClose();
                    useAccountDialogs.getState().openManager({ add: true });
                  }}
                >
                  <UserPlus />
                  {t("onboarding.addAccount")}
                </Button>
              ) : (
                <Button
                  size="sm"
                  disabled={!scan.data}
                  onClick={() => scan.data && toChoose(scan.data)}
                >
                  {t("onboarding.next")}
                </Button>
              )}
            </>
          )}
          {step === "choose" && (
            <>
              <Button variant="outline" size="sm" onClick={() => setStep("scan")}>
                {t("onboarding.back")}
              </Button>
              <Button
                size="sm"
                disabled={count === 0 || applying}
                onClick={() => scan.data && state && void apply(scan.data, state)}
              >
                {applying && <LoaderCircle className="animate-spin" />}
                {applying ? t("onboarding.applying") : t("onboarding.apply", { count })}
              </Button>
            </>
          )}
          {step === "apply" && (
            <Button size="sm" onClick={onClose}>
              {t("onboarding.done")}
            </Button>
          )}
        </div>
      </DialogFooter>
    </DialogContent>
  );
}

function sourceText(s: SourceRef) {
  return s.line != null ? `${s.file}:${s.line}` : s.file;
}

function ScanStep({ scan }: { scan: UseQueryResult<ImportScan> }) {
  const { t } = useTranslation();
  if (scan.isLoading || (scan.isFetching && !scan.data))
    return (
      <p className="flex items-center gap-2 py-8 text-muted-foreground">
        <LoaderCircle className="size-4 animate-spin" />
        {t("onboarding.scanning")}
      </p>
    );
  if (scan.error)
    return (
      <p role="alert" className="py-4 text-destructive">
        {t("onboarding.scanFailed", { message: errorMessage(scan.error) })}
      </p>
    );
  const data = scan.data;
  if (!data) return null;
  const r = data.report;
  const tiles: [string, number][] = [
    [t("onboarding.summary.identities"), r.identities.length],
    [t("onboarding.summary.rules"), r.rules.length],
    [t("onboarding.summary.sshKeys"), keyCandidates(data).length],
    [t("onboarding.summary.accounts"), r.accounts.length],
    [t("onboarding.summary.warnings"), r.warnings.length],
  ];
  return (
    <div className="space-y-3">
      <div className="grid grid-cols-2 gap-2 sm:grid-cols-5">
        {tiles.map(([label, n]) => (
          <div key={label} className="rounded-md border p-2">
            <div className="text-lg font-semibold tabular-nums">{n}</div>
            <div className="text-[11px] text-muted-foreground">{label}</div>
          </div>
        ))}
      </div>
      {isEmptyScan(data) && <p className="text-muted-foreground">{t("onboarding.nothingFound")}</p>}
      {r.warnings.length > 0 && (
        <ul className="space-y-1">
          {r.warnings.map((w, i) => (
            <li key={i} className="flex gap-1.5 text-warning">
              <CircleAlert className="mt-px size-3.5 shrink-0" />
              <span>
                {t(`onboarding.warning.${w.kind}`)} —{" "}
                <span className="font-mono text-[11px] break-all">{sourceText(w.source)}</span>
              </span>
            </li>
          ))}
        </ul>
      )}
      {r.scannedFiles.length > 0 && (
        <details>
          <summary className="cursor-pointer text-muted-foreground">
            {t("onboarding.files", { count: r.scannedFiles.length })}
          </summary>
          <ul className="mt-1 space-y-0.5 font-mono text-[11px] text-muted-foreground">
            {r.scannedFiles.map((f) => (
              <li key={f} className="break-all">
                {f}
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="space-y-1.5">
      <h3 className="text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
        {title}
      </h3>
      <ul className="space-y-1.5">{children}</ul>
    </section>
  );
}

function AccountSelect({
  value,
  accounts,
  allowNone,
  invalid,
  onChange,
  label,
}: {
  value: string | null;
  accounts: readonly Account[];
  allowNone?: boolean;
  invalid?: boolean;
  onChange: (id: string | null) => void;
  label: string;
}) {
  const { t } = useTranslation();
  return (
    <select
      aria-label={label}
      aria-invalid={invalid}
      value={value ?? ""}
      onChange={(e) => onChange(e.target.value || null)}
      className="h-7 max-w-48 rounded-md border bg-background px-1.5 text-xs aria-invalid:border-destructive"
    >
      <option value="">
        {allowNone ? t("onboarding.unassigned") : t("onboarding.chooseAccount")}
      </option>
      {accounts.map((a) => (
        <option key={a.id} value={a.id}>
          {accountName(a)}
          {a.label || a.displayName ? ` (${a.username})` : ""}
        </option>
      ))}
    </select>
  );
}

function Row({
  choice,
  onToggle,
  label,
  children,
  disabled,
}: {
  choice: Choice;
  onToggle: (checked: boolean) => void;
  label: string;
  children: ReactNode;
  disabled?: boolean;
}) {
  return (
    <li
      className={cn(
        "flex flex-wrap items-start gap-2 rounded-md border p-2",
        !choice.checked && "opacity-70",
      )}
    >
      <input
        type="checkbox"
        aria-label={label}
        className="mt-0.5"
        checked={choice.checked}
        disabled={disabled}
        onChange={(e) => onToggle(e.target.checked)}
      />
      <div className="flex min-w-0 flex-1 flex-wrap items-start gap-2">{children}</div>
    </li>
  );
}

function ChooseStep({
  scan,
  state,
  setState,
  accounts,
}: {
  scan: ImportScan;
  state: WizardState;
  setState: (s: WizardState) => void;
  accounts: readonly Account[];
}) {
  const { t } = useTranslation();
  const r = scan.report;
  const keys = keyCandidates(scan);
  const update = <K extends keyof WizardState>(
    kind: K,
    index: number,
    patch: Partial<WizardState[K][number]>,
  ) =>
    setState({
      ...state,
      [kind]: state[kind].map((c, i) => (i === index ? { ...c, ...patch } : c)),
    });
  const signIn = () => useAccountDialogs.getState().openManager({ add: true });

  return (
    <div className="space-y-4">
      {accounts.length === 0 && (
        <div className="flex flex-wrap items-center gap-2 rounded-md border border-warning/50 bg-warning/10 px-3 py-2">
          <span className="min-w-0 flex-1">{t("onboarding.noAccountsHint")}</span>
          <Button size="xs" variant="outline" onClick={signIn}>
            <UserPlus />
            {t("onboarding.addAccount")}
          </Button>
        </div>
      )}

      {r.accounts.length > 0 && (
        // Phase 3B.1: CLI accounts are added with their token here (no sign-in needed).
        <section className="space-y-1.5">
          <h3 className="text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
            {t("onboarding.section.accounts")}
          </h3>
          <CliAccountsPanel compact />
        </section>
      )}

      {r.identities.length > 0 && (
        <Section title={t("onboarding.section.identities")}>
          {r.identities.map((idn, i) => {
            const c = state.identities[i];
            if (!c) return null;
            return (
              <Row
                key={i}
                choice={c}
                label={idn.email ?? idn.name ?? String(i)}
                disabled={!idn.email}
                onToggle={(checked) => update("identities", i, { checked })}
              >
                <div className="min-w-0 flex-1">
                  <div className="font-medium">
                    {idn.name ?? "—"}{" "}
                    <span className="text-muted-foreground">
                      &lt;{idn.email ?? t("onboarding.noEmail")}&gt;
                    </span>
                  </div>
                  <div className="text-[11px] text-muted-foreground">
                    {idn.scope === "global"
                      ? t("onboarding.global")
                      : t("onboarding.conditional", { condition: idn.condition ?? "" })}{" "}
                    · <span className="font-mono break-all">{sourceText(idn.source)}</span>
                  </div>
                  {needsAccount("identity", c) && (
                    <div className="text-[11px] text-destructive">
                      {t("onboarding.needsAccount")}
                    </div>
                  )}
                </div>
                <AccountSelect
                  label={t("onboarding.account")}
                  value={c.accountId}
                  accounts={accounts}
                  invalid={needsAccount("identity", c)}
                  onChange={(id) => setState(setIdentityAccount(state, scan, i, id))}
                />
                <label className="flex items-center gap-1 text-[11px] text-muted-foreground">
                  <input
                    type="checkbox"
                    checked={c.makeDefault}
                    onChange={(e) => update("identities", i, { makeDefault: e.target.checked })}
                  />
                  {t("onboarding.makeDefault")}
                </label>
              </Row>
            );
          })}
        </Section>
      )}

      {r.rules.length > 0 && (
        <Section title={t("onboarding.section.rules")}>
          {r.rules.map((rule, i) => {
            const c = state.rules[i];
            if (!c) return null;
            const ident = rule.identityIndex != null ? r.identities[rule.identityIndex] : undefined;
            const target =
              rule.pathGlob ??
              (rule.host
                ? `${rule.host}${rule.ownerOrGroup ? `/${rule.ownerOrGroup}` : ""}`
                : t("onboarding.ruleRemote", {
                    glob: rule.remoteUrlGlob ?? rule.remoteRegex ?? "",
                  }));
            return (
              <Row
                key={i}
                choice={c}
                label={rule.condition}
                onToggle={(checked) => update("rules", i, { checked })}
              >
                <div className="min-w-0 flex-1">
                  <div className="font-mono text-[11px] break-all">
                    {rule.condition} → <span className="font-semibold">{target}</span>
                  </div>
                  <div className="text-[11px] text-muted-foreground">
                    {ident?.email && `${t("onboarding.ruleIdentity", { email: ident.email })} · `}
                    <span className="font-mono break-all">{sourceText(rule.source)}</span>
                  </div>
                  {needsAccount("rule", c) && (
                    <div className="text-[11px] text-destructive">
                      {t("onboarding.needsAccount")}
                    </div>
                  )}
                </div>
                <AccountSelect
                  label={t("onboarding.account")}
                  value={c.accountId}
                  accounts={accounts}
                  invalid={needsAccount("rule", c)}
                  onChange={(id) => update("rules", i, { accountId: id })}
                />
              </Row>
            );
          })}
        </Section>
      )}

      {keys.length > 0 && (
        <Section title={t("onboarding.section.keys")}>
          {keys.map((k, i) => {
            const c = state.keys[i];
            if (!c) return null;
            const aliases = k.aliases.filter((a) => a.isAlias);
            return (
              <Row
                key={k.path}
                choice={c}
                label={k.path}
                disabled={!k.exists}
                onToggle={(checked) => update("keys", i, { checked })}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5 font-mono text-[11px] break-all">
                    <KeyRound className="size-3 shrink-0" />
                    {k.path}
                  </div>
                  <div className="text-[11px] text-muted-foreground">
                    {k.aliases.map((a) => a.alias).join(", ")} · {k.hostName}
                    {k.comment && ` · ${k.comment}`}
                  </div>
                  {!k.exists && (
                    <div className="flex items-center gap-1 text-[11px] text-destructive">
                      <CircleDashed className="size-3" />
                      {t("onboarding.keyMissing")}
                    </div>
                  )}
                  {k.known && (
                    <div className="text-[11px] text-success">{t("onboarding.keyKnown")}</div>
                  )}
                  {aliases.map((a) => (
                    <div key={a.alias} className="text-[11px] text-muted-foreground">
                      {t("onboarding.aliasUnnecessary", { alias: a.alias, host: a.hostName })}
                    </div>
                  ))}
                  {c.checked && k.exists && (
                    <input
                      type="password"
                      autoComplete="off"
                      placeholder={t("onboarding.passphrase")}
                      aria-label={t("onboarding.passphrase")}
                      value={c.passphrase}
                      onChange={(e) => update("keys", i, { passphrase: e.target.value })}
                      className="mt-1 h-7 w-56 rounded-md border bg-background px-2 text-xs"
                    />
                  )}
                </div>
                <AccountSelect
                  label={t("onboarding.account")}
                  value={c.accountId}
                  accounts={accounts}
                  allowNone
                  onChange={(id) => update("keys", i, { accountId: id })}
                />
              </Row>
            );
          })}
        </Section>
      )}
    </div>
  );
}

function StatusIcon({ item }: { item: ApplyItem }) {
  if (item.status === "created") return <CheckCircle2 className="size-3.5 text-success" />;
  if (item.status === "existing")
    return <CheckCircle2 className="size-3.5 text-muted-foreground" />;
  return <CircleAlert className="size-3.5 text-destructive" />;
}

function ResultStep({ result }: { result: ImportApplyResult }) {
  const { t } = useTranslation();
  const by = (s: ApplyItem["status"]) => result.items.filter((i) => i.status === s).length;
  return (
    <div className="space-y-3">
      <p className="font-medium">
        {t("onboarding.resultSummary", {
          created: by("created"),
          existing: by("existing"),
          failed: by("failed"),
        })}
      </p>
      <ul className="space-y-1">
        {result.items.map((item, i) => (
          <li key={i} className="flex items-start gap-2 rounded-md border px-2 py-1.5">
            <StatusIcon item={item} />
            <span className="w-20 shrink-0 text-muted-foreground">
              {t(`onboarding.kind.${item.kind}`)}
            </span>
            <span className="min-w-0 flex-1 font-mono text-[11px] break-all">{item.label}</span>
            <span
              className={cn(
                "shrink-0 text-[11px]",
                item.status === "failed" ? "text-destructive" : "text-muted-foreground",
              )}
              title={item.message ?? undefined}
            >
              {item.status === "failed"
                ? t(`onboarding.code.${codeKey(item.code)}`)
                : t(`onboarding.status.${item.status}`)}
            </span>
          </li>
        ))}
      </ul>
      {result.signIn.length > 0 && (
        <div className="space-y-1">
          <p>{t("onboarding.signInPending")}</p>
          {result.signIn.map((a, i) => (
            <div key={i} className="flex items-center gap-2">
              <span>
                {a.username ?? t("onboarding.unknownUser")} @ {a.host}
              </span>
              <Button
                size="xs"
                variant="outline"
                onClick={() => useAccountDialogs.getState().openManager({ add: true })}
              >
                <LogIn />
                {t("onboarding.signInAgain")}
              </Button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

const CODES = [
  "passphraseRequired",
  "wrongPassphrase",
  "keyOfAnotherAccount",
  "identityNotFound",
] as const;

function codeKey(code: string | null): (typeof CODES)[number] | "error" {
  return (CODES as readonly string[]).includes(code ?? "")
    ? (code as (typeof CODES)[number])
    : "error";
}
