import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import {
  CircleAlert,
  CircleCheck,
  CircleX,
  Download,
  ExternalLink,
  Info,
  LoaderCircle,
  RefreshCw,
  RotateCcw,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { repoKeys } from "@/features/repos/queries";
import {
  commands,
  errorMessage,
  events,
  type SetupInstallProgress,
  type SetupInstallResult,
  type SetupTool,
  type SetupToolId,
  type SetupToolsReport,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { cn } from "@/lib/utils";
import { canInstall, installSucceeded, toolLevel, type ToolLevel } from "./model";
import { setupKeys, useSetupTools } from "./queries";

const LEVEL_ICON: Record<ToolLevel, typeof CircleCheck> = {
  ok: CircleCheck,
  warn: CircleAlert,
  error: CircleX,
  info: Info,
};

const LEVEL_COLOR: Record<ToolLevel, string> = {
  ok: "text-success",
  warn: "text-warning",
  error: "text-danger",
  info: "text-muted-foreground",
};

/** Re-reads the tools everywhere (the shell's git banner uses `tool_status`). */
async function recheck() {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: setupKeys.tools }),
    queryClient.invalidateQueries({ queryKey: repoKeys.tools }),
  ]);
}

export function ToolsStep() {
  const { t } = useTranslation();
  const tools = useSetupTools(true);
  const [installing, setInstalling] = useState<SetupToolId | null>(null);
  const [results, setResults] = useState<Partial<Record<SetupToolId, SetupInstallResult>>>({});
  const [progress, setProgress] = useState<SetupInstallProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  const install = async (id: SetupToolId) => {
    setInstalling(id);
    setProgress(null);
    setError(null);
    let unlisten: (() => void) | undefined;
    try {
      unlisten = await events.setupInstallProgress.listen(({ payload }) => {
        if (payload.tool === id) setProgress(payload);
      });
      const r = await commands.setupInstall(id);
      setResults((old) => ({ ...old, [id]: r }));
      if (installSucceeded(r.outcome)) await recheck();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      unlisten?.();
      setInstalling(null);
      setProgress(null);
    }
  };

  if (!isTauri() && !tools.data) {
    return <p className="text-sm text-muted-foreground">{t("common.backendBrowser")}</p>;
  }
  if (tools.isPending) {
    return (
      <p className="flex items-center gap-2 text-sm text-muted-foreground">
        <LoaderCircle className="size-4 animate-spin" />
        {t("setup.tools.checking")}
      </p>
    );
  }
  if (tools.isError || !tools.data) {
    return (
      <div className="space-y-2 text-sm">
        <p className="text-danger">
          {t("setup.tools.checkFailed", { message: errorMessage(tools.error) })}
        </p>
        <Button size="sm" variant="outline" onClick={() => void recheck()}>
          <RefreshCw />
          {t("setup.tools.recheck")}
        </Button>
      </div>
    );
  }
  const report = tools.data;
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <p className="min-w-0 flex-1 text-sm text-muted-foreground">{t("setup.tools.intro")}</p>
        <Button
          size="sm"
          variant="outline"
          disabled={tools.isFetching || installing !== null}
          onClick={() => void recheck()}
        >
          <RefreshCw className={cn(tools.isFetching && "animate-spin")} />
          {t("setup.tools.recheck")}
        </Button>
      </div>
      <GateNotice report={report} />
      {report.packageManager && !report.packageManagerFound && (
        <p className="rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs">
          {t(`setup.tools.managerMissing.${report.packageManager}`)}
        </p>
      )}
      {error && <p className="text-xs text-danger">{error}</p>}
      <ul className="space-y-2">
        {report.tools.map((tool) => (
          <ToolRow
            key={`tool-${tool.id}`}
            tool={tool}
            installing={installing === tool.id}
            progress={installing === tool.id ? progress : null}
            busy={installing !== null}
            result={results[tool.id]}
            onInstall={() => void install(tool.id)}
          />
        ))}
      </ul>
    </div>
  );
}

function GateNotice({ report }: { report: SetupToolsReport }) {
  const { t } = useTranslation();
  const [restarting, setRestarting] = useState(false);
  const git = report.tools.find((x) => x.id === "git");
  if (report.restartNeeded) {
    return (
      <div className="flex flex-wrap items-center gap-2 rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs">
        <span className="min-w-0 flex-1">{t("setup.tools.restartNeeded")}</span>
        <Button
          size="xs"
          variant="outline"
          disabled={restarting}
          onClick={() => {
            setRestarting(true);
            commands.setupRestart().catch(() => setRestarting(false));
          }}
        >
          <RotateCcw />
          {t("setup.tools.restart")}
        </Button>
      </div>
    );
  }
  if (!report.canContinue) {
    return (
      <p className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-xs">
        {git?.state === "outdated"
          ? t("setup.tools.gitTooOld", { min: git.minimum ?? "" })
          : t("setup.tools.gitRequired")}
      </p>
    );
  }
  return null;
}

function ToolRow({
  tool,
  installing,
  progress,
  busy,
  result,
  onInstall,
}: {
  tool: SetupTool;
  installing: boolean;
  progress: SetupInstallProgress | null;
  busy: boolean;
  result: SetupInstallResult | undefined;
  onInstall: () => void;
}) {
  const { t } = useTranslation();
  const level = toolLevel(tool);
  const Icon = LEVEL_ICON[level];
  const name = t(`setup.tool.${tool.id}.name`);
  return (
    <li className="rounded-md border px-3 py-2" aria-label={name}>
      <div className="flex items-start gap-2">
        <Icon
          className={cn("mt-0.5 size-4 shrink-0", LEVEL_COLOR[level])}
          aria-label={t(`setup.level.${level}`)}
        />
        <div className="min-w-0 flex-1 space-y-0.5">
          <div className="flex flex-wrap items-baseline gap-x-2">
            <span className="text-sm font-medium">{name}</span>
            <span className="text-[11px] text-muted-foreground">
              {t(`setup.importance.${tool.importance}`)}
            </span>
            {tool.version && <span className="font-mono text-xs">{tool.version}</span>}
          </div>
          <p className="text-xs text-muted-foreground">{t(`setup.tool.${tool.id}.purpose`)}</p>
          <p className={cn("text-xs", level !== "ok" && LEVEL_COLOR[level])}>
            <StateText tool={tool} />
          </p>
          {tool.path && (
            <p className="truncate font-mono text-[11px] text-muted-foreground" title={tool.path}>
              {tool.path}
            </p>
          )}
        </div>
      </div>
      {tool.state !== "ok" && tool.id !== "webview2" && (
        <div className="mt-2 flex flex-wrap items-center gap-2 pl-6">
          {canInstall(tool) && (
            <Button size="xs" disabled={busy} onClick={onInstall}>
              <Download />
              {t("setup.tools.install")}
            </Button>
          )}
          <Button size="xs" variant="outline" onClick={() => void openExternal(tool.downloadUrl)}>
            <ExternalLink />
            {t("setup.tools.download")}
          </Button>
        </div>
      )}
      {installing && <InstallRunning manager={tool.installer} progress={progress} />}
      {result && !installing && <InstallReport result={result} />}
    </li>
  );
}

function StateText({ tool }: { tool: SetupTool }) {
  const { t } = useTranslation();
  switch (tool.state) {
    case "ok":
      return t("setup.state.ok");
    case "outdated":
      return t("setup.state.outdated", { min: tool.minimum ?? "" });
    case "outsidePath":
      return t("setup.state.outsidePath");
    case "missing":
      return t(`setup.state.missing.${tool.importance}`);
  }
}

/** Seconds since the component mounted, ticking once a second. */
function useElapsed(): number {
  const [started] = useState(() => Date.now());
  const [now, setNow] = useState(started);
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);
  return Math.floor((now - started) / 1000);
}

function formatElapsed(seconds: number): string {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${s.toString().padStart(2, "0")}`;
}

function InstallRunning({
  manager,
  progress,
}: {
  manager: SetupTool["installer"];
  progress: SetupInstallProgress | null;
}) {
  const { t } = useTranslation();
  const [cancelling, setCancelling] = useState(false);
  const elapsed = useElapsed();
  const phase = progress?.phase ?? "preparing";
  const percent = progress?.percent ?? null;
  const recent = progress?.recent ?? [];
  return (
    <div className="mt-2 space-y-1.5 pl-6 text-xs" role="status">
      <div className="flex items-center gap-2">
        <LoaderCircle className="size-3.5 shrink-0 animate-spin" />
        <span className="min-w-0 flex-1 truncate font-medium">
          {cancelling
            ? t("setup.install.cancelling")
            : `${t(`setup.install.phase.${phase}`)}${percent !== null ? ` ${percent}%` : ""}`}
        </span>
        <span className="shrink-0 font-mono text-[11px] text-muted-foreground">
          {formatElapsed(elapsed)}
        </span>
      </div>
      <div
        role="progressbar"
        aria-label={t(`setup.install.phase.${phase}`)}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent ?? undefined}
        className="h-1.5 overflow-hidden rounded-full bg-surface-2"
      >
        <div
          className={cn(
            "h-full bg-primary transition-[width]",
            percent === null && "animate-pulse",
          )}
          style={{ width: `${percent ?? 100}%` }}
        />
      </div>
      <p className="text-muted-foreground">
        {t(manager === "winget" ? "setup.install.runningWinget" : "setup.install.running")}
      </p>
      {recent.length > 0 && (
        <pre
          className="max-h-24 overflow-hidden rounded bg-muted px-2 py-1 font-mono text-[11px] whitespace-pre-wrap break-all text-muted-foreground"
          aria-label={t("setup.install.liveOutput")}
        >
          {recent.join("\n")}
        </pre>
      )}
      <div className="flex justify-end">
        <Button
          size="xs"
          variant="outline"
          disabled={cancelling}
          onClick={() => {
            setCancelling(true);
            void commands.setupInstallCancel();
          }}
        >
          <X />
          {t("setup.install.cancel")}
        </Button>
      </div>
    </div>
  );
}

function InstallReport({ result }: { result: SetupInstallResult }) {
  const { t } = useTranslation();
  const ok = installSucceeded(result.outcome) || result.outcome === "started";
  return (
    <div className="mt-2 space-y-1 pl-6 text-xs">
      <p className={ok ? "text-success" : "text-danger"}>
        {t(`setup.install.outcome.${result.outcome}`, {
          manager: result.manager ? t(`setup.manager.${result.manager}`) : "",
        })}
      </p>
      {(result.command || result.output) && (
        <details className="text-muted-foreground">
          <summary className="cursor-pointer select-none">{t("setup.install.details")}</summary>
          {result.command && <p className="mt-1 font-mono break-all">{result.command}</p>}
          {result.exitCode !== null && (
            <p>{t("setup.install.exitCode", { code: result.exitCode })}</p>
          )}
          {result.output && (
            <pre className="mt-1 max-h-40 overflow-auto rounded bg-muted p-2 font-mono text-[11px] whitespace-pre-wrap">
              {result.output}
            </pre>
          )}
        </details>
      )}
    </div>
  );
}
