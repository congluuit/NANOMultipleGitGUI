import { useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import {
  CircleCheck,
  Copy,
  Download,
  ExternalLink,
  LoaderCircle,
  Play,
  RefreshCw,
  ShieldCheck,
  Trash2,
  TriangleAlert,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import { copyText, openExternal } from "@/features/accounts/manager/openExternal";
import { InstallProgress } from "@/features/onboarding/setup/InstallProgress";
import {
  type AiInstallProgress,
  type AiInstallResult,
  commands,
  events,
  type OllamaModel,
  type OllamaStatus,
} from "@/ipc";
import { formatBytes } from "@/lib/format";
import { queryClient } from "@/lib/queryClient";
import { cn } from "@/lib/utils";
import { useUi } from "@/stores/ui";
import {
  type FullAiConfig,
  formatGb,
  isAiError,
  newRequestId,
  OLLAMA_DOWNLOAD_URL,
  OLLAMA_LINUX_COMMAND,
  OLLAMA_SUGGESTED,
  sameOllamaModel,
} from "./model";
import { aiKeys, refreshOllama, useOllamaStatus } from "./queries";
import { type AiPull, useAiUi } from "./store";
import { aiErrorText } from "./suggest";

/** Outcomes after which Ollama should now be found. */
const INSTALL_OK = new Set<AiInstallResult["outcome"]>(["installed", "alreadyInstalled"]);

/**
 * Ollama on this computer, step by step: install it (winget / Homebrew, or the download page),
 * start it, download a light model, pick it.
 */
export function OllamaPanel({
  config,
  save,
}: {
  config: FullAiConfig;
  save: (patch: Partial<FullAiConfig>) => void;
}) {
  const { t } = useTranslation();
  const status = useOllamaStatus(true);
  const [installing, setInstalling] = useState(false);
  const [install, setInstall] = useState<AiInstallResult | null>(null);
  const [progress, setProgress] = useState<AiInstallProgress | null>(null);
  const [starting, setStarting] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const runInstall = async () => {
    setInstalling(true);
    setInstall(null);
    setProgress(null);
    setError(null);
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await events.aiInstallProgress.listen(({ payload }) => setProgress(payload));
      const r = await commands.aiOllamaInstall();
      setInstall(r);
      if (r.outcome === "managerMissing") void openExternal(OLLAMA_DOWNLOAD_URL);
      if (INSTALL_OK.has(r.outcome)) await refreshOllama();
    } catch (e) {
      setError(aiErrorText(t, e));
    } finally {
      unlisten?.();
      setInstalling(false);
      setProgress(null);
    }
  };

  const runStart = async () => {
    if (starting) {
      void commands.aiCancel(starting);
      setStarting(null);
      return;
    }
    const id = newRequestId("start");
    setStarting(id);
    setError(null);
    try {
      queryClient.setQueryData(aiKeys.ollama, await commands.aiOllamaStart(id));
    } catch (e) {
      if (!(isAiError(e) && e.kind === "cancelled")) setError(aiErrorText(t, e));
    } finally {
      setStarting((cur) => (cur === id ? null : cur));
    }
  };

  const s = status.data;
  return (
    <section className="space-y-3">
      <p className="flex items-start gap-2 rounded-md border border-success/40 bg-success/10 px-3 py-2 text-xs">
        <ShieldCheck className="mt-0.5 size-3.5 shrink-0 text-success" />
        <span>{t("ai.ollama.privacy")}</span>
      </p>

      <div className="flex flex-wrap items-center gap-2">
        <div className="min-w-0 flex-1 text-xs font-semibold">{t("ai.ollama.title")}</div>
        <Button
          variant="outline"
          size="xs"
          disabled={status.isFetching || installing}
          onClick={() => void refreshOllama()}
        >
          <RefreshCw className={cn(status.isFetching && "animate-spin")} />
          {t("ai.ollama.recheck")}
        </Button>
      </div>

      {!isTauri() && !s ? (
        <p className="text-xs text-muted-foreground">{t("common.backendBrowser")}</p>
      ) : status.isPending ? (
        <p className="flex items-center gap-2 text-xs text-muted-foreground">
          <LoaderCircle className="size-3.5 animate-spin" />
          {t("ai.ollama.checking")}
        </p>
      ) : !s ? (
        <p className="text-xs text-danger">{aiErrorText(t, status.error)}</p>
      ) : !s.running && !s.installed ? (
        <NotInstalled
          status={s}
          installing={installing}
          progress={progress}
          result={install}
          onInstall={() => void runInstall()}
        />
      ) : !s.running ? (
        <div className="space-y-2 rounded-md border px-3 py-2 text-xs">
          <p>{t("ai.ollama.notRunning")}</p>
          <div className="flex flex-wrap gap-2">
            <Button size="sm" onClick={() => void runStart()}>
              {starting ? <LoaderCircle className="animate-spin" /> : <Play />}
              {starting ? t("ai.ollama.starting") : t("ai.ollama.start")}
            </Button>
            {starting && (
              <Button variant="outline" size="sm" onClick={() => void runStart()}>
                <X />
                {t("common.cancel")}
              </Button>
            )}
          </div>
          {starting && (
            <p className="text-[11px] text-muted-foreground">{t("ai.ollama.startingHint")}</p>
          )}
        </div>
      ) : (
        <Models status={s} config={config} save={save} />
      )}

      {error && <p className="text-xs text-danger">{error}</p>}

      <details className="text-xs">
        <summary className="cursor-pointer text-muted-foreground">
          {t("ai.ollama.advanced")}
        </summary>
        <div className="mt-2 space-y-1">
          <label htmlFor="ai-ollama-url" className="font-medium">
            {t("ai.ollama.url")}
          </label>
          <input
            id="ai-ollama-url"
            defaultValue={config.ollamaUrl}
            spellCheck={false}
            onBlur={(e) => {
              const v = e.target.value.trim();
              if (v !== config.ollamaUrl) {
                save({ ollamaUrl: v });
                void refreshOllama();
              }
            }}
            className="h-8 w-full rounded-md border bg-background px-2 text-xs focus:border-primary focus:outline-none"
          />
          <p className="text-[11px] text-muted-foreground">{t("ai.ollama.urlHint")}</p>
        </div>
      </details>
    </section>
  );
}

function NotInstalled({
  status,
  installing,
  progress,
  result,
  onInstall,
}: {
  status: OllamaStatus;
  installing: boolean;
  progress: AiInstallProgress | null;
  result: AiInstallResult | null;
  onInstall: () => void;
}) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  return (
    <div className="space-y-2 rounded-md border px-3 py-2 text-xs">
      <p>{t("ai.ollama.notInstalled")}</p>
      <div className="flex flex-wrap gap-2">
        {status.installer && (
          <Button size="sm" disabled={installing} onClick={onInstall}>
            {installing ? <LoaderCircle className="animate-spin" /> : <Download />}
            {t("ai.ollama.install")}
          </Button>
        )}
        <Button variant="outline" size="sm" onClick={() => void openExternal(status.downloadUrl)}>
          <ExternalLink />
          {t("ai.ollama.manual")}
        </Button>
      </div>
      {installing && (
        <InstallProgress
          manager={status.installer}
          progress={progress}
          onCancel={() => void commands.aiOllamaInstallCancel()}
        />
      )}
      {!status.installer && status.os === "macos" && (
        <p className="text-[11px] text-muted-foreground">{t("ai.ollama.noBrew")}</p>
      )}
      {!status.installer && status.os === "windows" && (
        <p className="text-[11px] text-muted-foreground">{t("ai.ollama.noWinget")}</p>
      )}
      {status.os === "linux" && (
        <div className="space-y-1">
          <p className="text-[11px] text-muted-foreground">{t("ai.ollama.linuxHint")}</p>
          <div className="flex items-center gap-1.5">
            <code className="min-w-0 flex-1 truncate rounded bg-muted px-2 py-1 font-mono text-[11px]">
              {OLLAMA_LINUX_COMMAND}
            </code>
            <Button
              variant="outline"
              size="xs"
              onClick={() => void copyText(OLLAMA_LINUX_COMMAND).then(setCopied)}
            >
              <Copy />
              {copied ? t("ai.copied") : t("ai.copy")}
            </Button>
          </div>
        </div>
      )}
      {result && <InstallOutcome result={result} />}
    </div>
  );
}

function InstallOutcome({ result }: { result: AiInstallResult }) {
  const { t } = useTranslation();
  const ok = INSTALL_OK.has(result.outcome);
  const manager = result.manager ?? "winget";
  return (
    <div className="space-y-1">
      <p className={cn("text-[11px]", ok ? "text-success" : "text-danger")}>
        {ok
          ? t("ai.ollama.installed")
          : result.outcome === "managerMissing"
            ? t("ai.ollama.managerMissing")
            : t(`setup.install.outcome.${result.outcome}`, { manager })}
      </p>
      {result.output && (
        <details className="text-[11px]">
          <summary className="cursor-pointer text-muted-foreground">
            {t("setup.install.details")}
          </summary>
          <pre className="mt-1 max-h-40 overflow-auto rounded bg-muted p-2 whitespace-pre-wrap">
            {result.command ? `$ ${result.command}\n` : ""}
            {result.output}
          </pre>
        </details>
      )}
    </div>
  );
}

function Models({
  status,
  config,
  save,
}: {
  status: OllamaStatus;
  config: FullAiConfig;
  save: (patch: Partial<FullAiConfig>) => void;
}) {
  const { t } = useTranslation();
  const pull = useAiUi((s) => s.pull);
  const [error, setError] = useState<string | null>(null);
  const [custom, setCustom] = useState("");
  const [toDelete, setToDelete] = useState<string | null>(null);
  const installed = (name: string) => status.models.some((m) => sameOllamaModel(m.name, name));
  const chosen = config.models.ollama;
  const choose = (name: string) => save({ models: { ...config.models, ollama: name } });

  const download = async (model: string) => {
    const name = model.trim();
    if (!name || useAiUi.getState().pull) return;
    const id = newRequestId("pull");
    useAiUi.getState().setPull({ requestId: id, model: name, progress: null });
    setError(null);
    try {
      await commands.aiOllamaPull(name, id);
      useUi.getState().showNotice({
        kind: "success",
        message: t("ai.ollama.pulled", { model: name }),
      });
      // Nothing usable chosen yet: take the model just downloaded.
      if (!chosen || !installed(chosen)) choose(name);
      setCustom("");
    } catch (e) {
      if (!(isAiError(e) && e.kind === "cancelled")) setError(aiErrorText(t, e));
    } finally {
      if (useAiUi.getState().pull?.requestId === id) useAiUi.getState().setPull(null);
      await refreshOllama();
    }
  };

  const remove = async (name: string) => {
    setToDelete(null);
    setError(null);
    try {
      await commands.aiOllamaDelete(name);
    } catch (e) {
      setError(aiErrorText(t, e));
    } finally {
      await refreshOllama();
    }
  };

  const others = status.models.filter(
    (m) => !OLLAMA_SUGGESTED.some((s) => sameOllamaModel(s.name, m.name)),
  );

  return (
    <div className="space-y-3">
      <p className="flex items-center gap-1.5 text-xs text-success">
        <CircleCheck className="size-3.5" />
        {t("ai.ollama.running", { version: status.version ?? "?" })}
      </p>
      {chosen && !installed(chosen) && (
        <p className="flex items-start gap-1.5 rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs">
          <TriangleAlert className="mt-0.5 size-3.5 shrink-0 text-warning" />
          {t("ai.ollama.chosenMissing", { model: chosen })}
        </p>
      )}

      <div className="space-y-1.5">
        <div className="text-xs font-medium">{t("ai.ollama.suggested")}</div>
        <ul className="space-y-1.5">
          {OLLAMA_SUGGESTED.map((m) => (
            <ModelRow
              key={`ollama-suggested-${m.name}`}
              name={m.name}
              detail={t("ai.ollama.size", { size: formatGb(m.sizeGb), ram: m.ramGb })}
              note={t(`ai.ollama.note.${m.note}`)}
              recommended={m.recommended}
              installed={installed(m.name)}
              chosen={sameOllamaModel(chosen, m.name)}
              pullBusy={pull !== null}
              pulling={pull?.model === m.name ? pull : null}
              onUse={() => choose(m.name)}
              onPull={() => void download(m.name)}
              onDelete={() => setToDelete(m.name)}
            />
          ))}
        </ul>
      </div>

      {others.length > 0 && (
        <div className="space-y-1.5">
          <div className="text-xs font-medium">{t("ai.ollama.otherModels")}</div>
          <ul className="space-y-1.5">
            {others.map((m: OllamaModel) => (
              <ModelRow
                key={`ollama-other-${m.name}`}
                name={m.name}
                detail={[formatBytes(m.size), m.parameterSize].filter(Boolean).join(" · ")}
                installed
                chosen={sameOllamaModel(chosen, m.name)}
                pullBusy={pull !== null}
                pulling={null}
                onUse={() => choose(m.name)}
                onPull={() => {}}
                onDelete={() => setToDelete(m.name)}
              />
            ))}
          </ul>
        </div>
      )}

      <div className="space-y-1">
        <label htmlFor="ai-ollama-custom" className="text-xs font-medium">
          {t("ai.ollama.custom")}
        </label>
        <div className="flex gap-1.5">
          <input
            id="ai-ollama-custom"
            value={custom}
            spellCheck={false}
            placeholder="qwen2.5-coder:7b"
            onChange={(e) => setCustom(e.target.value)}
            className="h-8 min-w-0 flex-1 rounded-md border bg-background px-2 font-mono text-xs focus:border-primary focus:outline-none"
          />
          <Button
            variant="outline"
            size="sm"
            disabled={!custom.trim() || pull !== null}
            onClick={() => void download(custom)}
          >
            <Download />
            {t("ai.ollama.pull")}
          </Button>
        </div>
        {pull && !OLLAMA_SUGGESTED.some((m) => m.name === pull.model) && (
          <PullProgress pull={pull} />
        )}
      </div>

      {error && <p className="text-xs text-danger">{error}</p>}

      <ConfirmDialog
        open={toDelete !== null}
        title={t("ai.ollama.deleteTitle")}
        description={t("ai.ollama.deleteBody", { model: toDelete ?? "" })}
        confirmLabel={t("ai.ollama.delete")}
        destructive
        onConfirm={() => toDelete && void remove(toDelete)}
        onCancel={() => setToDelete(null)}
      />
    </div>
  );
}

function ModelRow({
  name,
  detail,
  note,
  recommended,
  installed,
  chosen,
  pullBusy,
  pulling,
  onUse,
  onPull,
  onDelete,
}: {
  name: string;
  detail: string;
  note?: string;
  recommended?: boolean;
  installed: boolean;
  chosen: boolean;
  pullBusy: boolean;
  pulling: AiPull | null;
  onUse: () => void;
  onPull: () => void;
  onDelete: () => void;
}) {
  const { t } = useTranslation();
  return (
    <li
      className={cn("rounded-md border px-3 py-2 text-xs", chosen && installed && "border-primary")}
      aria-label={name}
    >
      <div className="flex flex-wrap items-center gap-2">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-baseline gap-x-2">
            <span className="font-mono font-medium">{name}</span>
            {recommended && (
              <span className="rounded bg-primary/10 px-1.5 text-[10px] text-primary">
                {t("ai.ollama.recommended")}
              </span>
            )}
            <span className="text-[11px] text-muted-foreground">{detail}</span>
          </div>
          {note && <div className="text-[11px] text-muted-foreground">{note}</div>}
        </div>
        {installed ? (
          <>
            {chosen ? (
              <span className="flex items-center gap-1 text-[11px] text-primary">
                <CircleCheck className="size-3.5" />
                {t("ai.ollama.inUse")}
              </span>
            ) : (
              <Button variant="outline" size="xs" onClick={onUse}>
                {t("ai.ollama.use")}
              </Button>
            )}
            <Button
              variant="ghost"
              size="icon-xs"
              onClick={onDelete}
              title={t("ai.ollama.delete")}
              aria-label={t("ai.ollama.delete")}
            >
              <Trash2 />
            </Button>
          </>
        ) : !pulling ? (
          <Button variant="outline" size="xs" disabled={pullBusy} onClick={onPull}>
            <Download />
            {t("ai.ollama.pull")}
          </Button>
        ) : null}
      </div>
      {pulling && <PullProgress pull={pulling} />}
    </li>
  );
}

function PullProgress({ pull }: { pull: AiPull }) {
  const { t } = useTranslation();
  const p = pull.progress;
  const percent = p?.percent ?? null;
  return (
    <div className="mt-2 space-y-1">
      <div className="h-1.5 overflow-hidden rounded bg-muted">
        <div
          className={cn(
            "h-full bg-primary transition-[width]",
            percent === null && "animate-pulse",
          )}
          style={{ width: `${percent ?? 100}%` }}
        />
      </div>
      <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
        <span className="min-w-0 flex-1 truncate">
          {p && p.total > 0
            ? t("ai.ollama.pullProgress", {
                percent: percent ?? 0,
                done: formatBytes(p.completed),
                total: formatBytes(p.total),
              })
            : t("ai.ollama.pullStarting")}
        </span>
        <Button
          variant="ghost"
          size="xs"
          onClick={() => {
            void commands.aiCancel(pull.requestId);
            useAiUi.getState().setPull(null);
          }}
        >
          <X />
          {t("common.cancel")}
        </Button>
      </div>
    </div>
  );
}
