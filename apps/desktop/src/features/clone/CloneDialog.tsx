import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { open as openFolderDialog } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Pencil, Search, TriangleAlert, X } from "lucide-react";
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { accountKeys, useAccounts, useProviders } from "@/features/accounts/queries";
import { formatBytes } from "@/features/transfers/format";
import { commands, errorMessage, type CloneProgress, type CloneRepo } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { cn } from "@/lib/utils";
import { cancelClone, cloneFailureText, runClone } from "./actions";
import {
  accountTitle,
  baseName,
  filterRepos,
  flattenRepos,
  lfsMode,
  neededBytes,
  renderTemplate,
  spaceWarning,
  templateVars,
  urlForProtocol,
  validateForm,
  type CloneForm,
  type Protocol,
} from "./model";
import {
  cloneKeys,
  refreshCloneList,
  useCloneDestination,
  useCloneGuess,
  useCloneListEvents,
  useCloneRepoList,
  useDebounced,
} from "./queries";
import { RepoList } from "./RepoList";

const ALL = "__all__";
const NONE = "__none__";

const repoKey = (r: CloneRepo) => `${r.accountId}/${r.id}`;

const inputClass =
  "h-8 w-full rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none aria-invalid:border-danger";

/** Mounted once (App.tsx); opens through `useAccountDialogs().openClone({ accountId?, url? })`. */
export function CloneDialogHost() {
  const preset = useAccountDialogs((s) => s.clone);
  const close = useAccountDialogs((s) => s.close);
  if (!preset) return null;
  return (
    <CloneDialog
      key={`${preset.accountId ?? ""}|${preset.url ?? ""}`}
      presetAccountId={preset.accountId}
      presetUrl={preset.url}
      onClose={() => close("clone")}
    />
  );
}

interface Running {
  opId: string;
  progress: CloneProgress | null;
  cancelling: boolean;
}

function ProgressView({ running, onCancel }: { running: Running; onCancel: () => void }) {
  const { t } = useTranslation();
  const p = running.progress;
  const percent = p?.percent ?? null;
  return (
    <div className="space-y-1.5 rounded-md border bg-surface p-2" aria-live="polite">
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="truncate">
          {running.cancelling
            ? t("clone.cancelling")
            : p
              ? `${p.label}${percent != null ? ` ${percent}%` : ""}`
              : t("clone.starting")}
        </span>
        <span className="shrink-0 font-mono text-[11px] text-muted-foreground">
          {[p?.bytes, p?.throughput].filter(Boolean).join(" · ")}
        </span>
      </div>
      <div
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent ?? undefined}
        className="h-1.5 overflow-hidden rounded-full bg-surface-2"
      >
        <div
          className={cn("h-full bg-primary transition-[width]", percent == null && "animate-pulse")}
          style={{ width: `${percent ?? 100}%` }}
        />
      </div>
      <div className="flex justify-end">
        <Button variant="outline" size="xs" disabled={running.cancelling} onClick={onCancel}>
          <X />
          {t("clone.cancelClone")}
        </Button>
      </div>
    </div>
  );
}

export function CloneDialog({
  presetAccountId,
  presetUrl,
  onClose,
}: {
  presetAccountId: string | null;
  presetUrl: string | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const accounts = useAccounts();
  const providers = useProviders();
  const settings = useQuery({
    queryKey: accountKeys.settings,
    queryFn: () => commands.accountSettingsGet(),
    enabled: isTauri(),
  });
  const list = useCloneRepoList(true);
  useCloneListEvents(true);

  const [query, setQuery] = useState("");
  const [filterAccount, setFilterAccount] = useState<string | null>(presetAccountId);
  const [selected, setSelected] = useState<CloneRepo | null>(null);
  const [url, setUrl] = useState(presetUrl ?? "");
  /** `undefined` = automatic (selected repo / best guess). */
  const [accountChoice, setAccountChoice] = useState<string | null | undefined>(
    presetAccountId ?? undefined,
  );
  const [protocolChoice, setProtocolChoice] = useState<Protocol | null>(null);
  const [destOverride, setDestOverride] = useState<string | null>(null);
  const [templateEdit, setTemplateEdit] = useState<string | null>(null);
  const [writeIdentityChoice, setWriteIdentity] = useState<boolean | null>(null);
  const [submodules, setSubmodules] = useState(false);
  const [shallow, setShallow] = useState(false);
  const [lfs, setLfs] = useState<CloneForm["lfs"]>("all");
  const [lfsInclude, setLfsInclude] = useState("");
  const [running, setRunning] = useState<Running | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  const accountList = useMemo(() => accounts.data ?? [], [accounts.data]);
  const byId = useMemo(() => new Map(accountList.map((a) => [a.id, a])), [accountList]);
  const allRepos = useMemo(() => flattenRepos(list.data), [list.data]);
  const debouncedQuery = useDebounced(query, 120);
  const visible = useMemo(
    () => filterRepos(allRepos, { query: debouncedQuery, accountId: filterAccount }),
    [allRepos, debouncedQuery, filterAccount],
  );

  const typedUrl = useDebounced(url, 250);
  const guess = useCloneGuess(selected ? "" : typedUrl);
  const guessData = selected ? undefined : guess.data;
  const protocol: Protocol = protocolChoice ?? (guessData?.transport === "ssh" ? "ssh" : "https");
  const effectiveUrl = selected
    ? protocol === "ssh"
      ? selected.cloneSsh
      : selected.cloneHttps
    : urlForProtocol(url, guessData, protocol);
  const accountId: string | null =
    accountChoice !== undefined
      ? accountChoice
      : (selected?.accountId ?? guessData?.candidates[0]?.accountId ?? null);
  const candidates = guessData?.candidates ?? [];

  const suggestion = useCloneDestination(accountId, effectiveUrl, null);
  const template = templateEdit ?? suggestion.data?.template ?? "";
  const preview =
    suggestion.data && templateEdit !== null
      ? renderTemplate(templateEdit, templateVars(suggestion.data))
      : (suggestion.data?.path ?? "");
  const destPath = destOverride ?? preview;
  const checkedPath = useDebounced(destPath, 300);
  const check = useCloneDestination(accountId, effectiveUrl, checkedPath || null);
  const destInfo = check.data ?? suggestion.data;
  const destUsable =
    check.data && checkedPath === destPath ? check.data.usable : (suggestion.data?.usable ?? true);

  const needed = destInfo ? neededBytes(destInfo, lfs) : null;
  const space = spaceWarning(destInfo?.freeBytes ?? null, needed);
  const writeIdentity =
    accountId !== null && (writeIdentityChoice ?? settings.data?.identityWriteToRepoConfig ?? true);
  const urlValid = selected !== null || (guessData?.valid ?? url.trim().length > 0);
  const errors = validateForm(
    { url: effectiveUrl, dest: destPath, lfs, lfsInclude },
    { urlValid, destUsable },
  );

  const selectRepo = (repo: CloneRepo) => {
    setSelected(repo);
    setUrl(protocol === "ssh" ? repo.cloneSsh : repo.cloneHttps);
    setAccountChoice(repo.accountId);
    setDestOverride(null);
    setError(null);
  };

  const pickFolder = async () => {
    if (!isTauri()) return;
    const dir = await openFolderDialog({
      directory: true,
      multiple: false,
      title: t("clone.pickFolder"),
    });
    if (typeof dir !== "string") return;
    const name = destInfo?.repo || baseName(effectiveUrl).replace(/\.git$/, "");
    const sep = dir.includes("\\") ? "\\" : "/";
    setDestOverride(name ? `${dir.replace(/[/\\]+$/, "")}${sep}${name}` : dir);
  };

  const saveTemplate = async () => {
    if (templateEdit === null) return;
    try {
      await commands.cloneSetTemplate(templateEdit);
      setTemplateEdit(null);
      void queryClient.invalidateQueries({ queryKey: cloneKeys.all });
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const refresh = async () => {
    setRefreshing(true);
    try {
      await refreshCloneList();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setRefreshing(false);
    }
  };

  const submit = async () => {
    if (errors.length > 0 || running) return;
    setError(null);
    const run = await runClone(
      {
        url: effectiveUrl,
        accountId,
        identityId: null,
        dest: destPath.trim(),
        branch: null,
        depth: shallow ? 1 : null,
        recurseSubmodules: submodules,
        lfs: lfsMode({ lfs, lfsInclude }),
        writeIdentity,
      },
      (opId) => setRunning({ opId, progress: null, cancelling: false }),
      (progress) => setRunning((r) => (r ? { ...r, progress } : r)),
    );
    setRunning(null);
    if (run.status === "done") {
      onClose();
      return;
    }
    if (run.status === "cancelled") {
      setError(t("clone.cancelled"));
      return;
    }
    setError(cloneFailureText(run));
  };

  const cancel = () => {
    if (!running) return;
    setRunning({ ...running, cancelling: true });
    void cancelClone(running.opId);
  };

  const filterItems = [
    { value: ALL, label: t("clone.allAccounts") },
    ...accountList.map((a) => ({ value: a.id, label: accountTitle(a, providers.data) })),
  ];

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !running) onClose();
      }}
    >
      <DialogContent
        className="sm:max-w-3xl"
        onEscapeKeyDown={(e) => running && e.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>{t("clone.title")}</DialogTitle>
          <DialogDescription>{t("clone.description")}</DialogDescription>
        </DialogHeader>

        <div className="space-y-3">
          <div className="flex gap-2">
            <label className="relative flex-1">
              <Search className="pointer-events-none absolute top-2 left-2 size-4 text-muted-foreground" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("clone.searchPlaceholder")}
                aria-label={t("clone.searchPlaceholder")}
                className={cn(inputClass, "pl-8")}
              />
            </label>
            <Select
              value={filterAccount ?? ALL}
              onValueChange={(v) => setFilterAccount(v === ALL ? null : v)}
            >
              <SelectTrigger className="w-56 text-xs" aria-label={t("clone.accountFilter")}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {filterItems.map((item) => (
                  <SelectItem key={item.value} value={item.value} className="text-xs">
                    {item.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <RepoList
            repos={visible}
            total={allRepos.length}
            accounts={list.data?.accounts ?? []}
            byId={byId}
            providers={providers.data}
            selectedKey={selected ? repoKey(selected) : null}
            loading={list.isLoading}
            refreshing={refreshing || (list.data?.accounts.some((a) => a.refreshing) ?? false)}
            onRefresh={() => void refresh()}
            onSelect={selectRepo}
            now={list.dataUpdatedAt}
          />

          <div className="grid grid-cols-[7rem_1fr] items-center gap-x-3 gap-y-2 text-xs">
            <label htmlFor="clone-url" className="font-semibold">
              {t("clone.url")}
            </label>
            <div className="space-y-1">
              <input
                id="clone-url"
                value={url}
                onChange={(e) => {
                  setUrl(e.target.value);
                  setSelected(null);
                  setDestOverride(null);
                }}
                placeholder={t("clone.urlPlaceholder")}
                aria-invalid={url.trim().length > 0 && !urlValid}
                className={cn(inputClass, "font-mono")}
              />
              {!selected && candidates.length > 0 && (
                <div className="text-[11px] text-muted-foreground">
                  {candidates.length === 1
                    ? t("clone.guessOne", {
                        account: accountTitle(
                          byId.get(candidates[0]?.accountId ?? ""),
                          providers.data,
                        ),
                      })
                    : t("clone.guessMany", { count: candidates.length })}
                </div>
              )}
              {!selected && guessData?.valid && !guessData.local && candidates.length === 0 && (
                <div className="text-[11px] text-warning">{t("clone.guessNone")}</div>
              )}
            </div>

            <span className="font-semibold">{t("clone.account")}</span>
            <div className="flex flex-wrap items-center gap-3">
              <Select
                value={accountId ?? NONE}
                onValueChange={(v) => setAccountChoice(v === NONE ? null : v)}
              >
                <SelectTrigger className="w-64 text-xs" aria-label={t("clone.account")}>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={NONE} className="text-xs">
                    {t("clone.noAccount")}
                  </SelectItem>
                  {accountList.map((a) => (
                    <SelectItem key={a.id} value={a.id} className="text-xs">
                      <span
                        aria-hidden
                        className="mr-1 inline-block size-2 rounded-full"
                        style={{ backgroundColor: a.color }}
                      />
                      {accountTitle(a, providers.data)}
                      {candidates.some((c) => c.accountId === a.id) && (
                        <span className="ml-1 text-muted-foreground">· {t("clone.suggested")}</span>
                      )}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <fieldset className="flex items-center gap-3" aria-label={t("clone.protocol")}>
                <span className="text-muted-foreground">{t("clone.protocol")}:</span>
                {(["https", "ssh"] as const).map((p) => (
                  <label key={p} className="flex items-center gap-1">
                    <input
                      type="radio"
                      name="clone-protocol"
                      checked={protocol === p}
                      disabled={guessData?.local}
                      onChange={() => {
                        setProtocolChoice(p);
                        if (selected) setUrl(p === "ssh" ? selected.cloneSsh : selected.cloneHttps);
                      }}
                    />
                    {p.toUpperCase()}
                  </label>
                ))}
              </fieldset>
            </div>

            <label htmlFor="clone-dest" className="font-semibold">
              {t("clone.destination")}
            </label>
            <div className="space-y-1">
              <div className="flex gap-2">
                <input
                  id="clone-dest"
                  value={destPath}
                  onChange={(e) => setDestOverride(e.target.value)}
                  aria-invalid={destPath.length > 0 && !destUsable}
                  className={cn(inputClass, "font-mono")}
                />
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => void pickFolder()}
                  aria-label={t("clone.pickFolder")}
                  title={t("clone.pickFolder")}
                >
                  <FolderOpen />
                </Button>
              </div>
              <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[11px] text-muted-foreground">
                {templateEdit === null ? (
                  <>
                    <span>
                      {t("clone.template")}: <code className="font-mono">{template}</code>
                    </span>
                    <button
                      type="button"
                      className="rounded p-0.5 hover:bg-surface hover:text-foreground"
                      onClick={() => {
                        setTemplateEdit(template);
                        setDestOverride(null);
                      }}
                      aria-label={t("clone.editTemplate")}
                      title={t("clone.editTemplate")}
                    >
                      <Pencil className="size-3" />
                    </button>
                    {destOverride !== null && (
                      <button
                        type="button"
                        className="underline hover:text-foreground"
                        onClick={() => setDestOverride(null)}
                      >
                        {t("clone.useSuggestion")}
                      </button>
                    )}
                  </>
                ) : (
                  <>
                    <input
                      value={templateEdit}
                      onChange={(e) => setTemplateEdit(e.target.value)}
                      aria-label={t("clone.template")}
                      className="h-6 w-64 rounded border bg-background px-1 font-mono"
                    />
                    <button
                      type="button"
                      className="underline hover:text-foreground"
                      disabled={!templateEdit.includes("{repo}")}
                      onClick={() => void saveTemplate()}
                    >
                      {t("clone.saveTemplate")}
                    </button>
                    <span>{t("clone.templateHelp")}</span>
                  </>
                )}
              </div>
              <div className="flex flex-wrap gap-x-3 text-[11px] text-muted-foreground">
                {destInfo?.freeBytes != null && (
                  <span>{t("clone.freeSpace", { size: formatBytes(destInfo.freeBytes) })}</span>
                )}
                {destInfo?.sizeBytes != null && (
                  <span>{t("clone.repoSize", { size: formatBytes(destInfo.sizeBytes) })}</span>
                )}
                {destInfo?.lfsSizeBytes != null && destInfo.lfsSizeBytes > 0 && (
                  <span>{t("clone.lfsSize", { size: formatBytes(destInfo.lfsSizeBytes) })}</span>
                )}
              </div>
              {space && (
                <div className="flex items-center gap-1 text-[11px] text-warning" role="alert">
                  <TriangleAlert className="size-3" />
                  {space === "insufficient"
                    ? t("clone.spaceInsufficient", { size: formatBytes(needed ?? 0) })
                    : t("clone.spaceLow")}
                </div>
              )}
              {destPath.length > 0 && !destUsable && (
                <div className="text-[11px] text-danger">{t("clone.error.destNotEmpty")}</div>
              )}
            </div>

            <span className="font-semibold">{t("clone.options")}</span>
            <div className="flex flex-wrap gap-x-4 gap-y-1">
              <label className="flex items-center gap-1.5">
                <input
                  type="checkbox"
                  checked={writeIdentity}
                  disabled={accountId === null}
                  onChange={(e) => setWriteIdentity(e.target.checked)}
                />
                {t("clone.writeIdentity")}
              </label>
              <label className="flex items-center gap-1.5">
                <input
                  type="checkbox"
                  checked={submodules}
                  onChange={(e) => setSubmodules(e.target.checked)}
                />
                {t("clone.submodules")}
              </label>
              <label className="flex items-center gap-1.5">
                <input
                  type="checkbox"
                  checked={shallow}
                  onChange={(e) => setShallow(e.target.checked)}
                />
                {t("clone.shallow")}
              </label>
            </div>

            <span className="font-semibold">{t("clone.lfs")}</span>
            <fieldset
              className="flex flex-wrap items-center gap-x-4 gap-y-1"
              aria-label={t("clone.lfs")}
            >
              {(["all", "later", "partial"] as const).map((mode) => (
                <label key={mode} className="flex items-center gap-1.5">
                  <input
                    type="radio"
                    name="clone-lfs"
                    checked={lfs === mode}
                    onChange={() => setLfs(mode)}
                  />
                  {t(`clone.lfsMode.${mode}`)}
                </label>
              ))}
              <input
                value={lfsInclude}
                onChange={(e) => {
                  setLfsInclude(e.target.value);
                  setLfs("partial");
                }}
                placeholder="Assets/UI/**"
                aria-label={t("clone.lfsPattern")}
                aria-invalid={lfs === "partial" && errors.includes("patternInvalid")}
                className="h-7 w-48 rounded-md border bg-background px-2 font-mono focus:border-primary focus:outline-none aria-invalid:border-danger"
              />
            </fieldset>
          </div>

          {running && <ProgressView running={running} onCancel={cancel} />}
          {error && (
            <div
              className="rounded-md border border-danger/40 bg-danger/10 p-2 text-xs text-danger"
              role="alert"
            >
              {error}
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" size="sm" disabled={running !== null} onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            size="sm"
            disabled={errors.length > 0 || running !== null}
            title={errors.map((e) => t(`clone.error.${e}`)).join("\n") || undefined}
            onClick={() => void submit()}
          >
            {t("clone.submit")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
