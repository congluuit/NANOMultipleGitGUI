import { useEffect, useState } from "react";
import { LoaderCircle, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import type { SetupInstallPhase, SetupPackageManager } from "@/ipc";
import { cn } from "@/lib/utils";

/** What an installer is doing, as sent by `setup-install-progress` / `ai-install-progress`. */
export interface InstallProgressState {
  phase: SetupInstallPhase;
  percent: number | null;
  recent: string[];
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

/**
 * A running "Install it for me": current step, progress bar (pulsing when the installer prints
 * no percent), the last installer lines, elapsed time and a Cancel button.
 */
export function InstallProgress({
  manager,
  progress,
  onCancel,
  className,
}: {
  manager: SetupPackageManager | null;
  progress: InstallProgressState | null;
  onCancel: () => void;
  className?: string;
}) {
  const { t } = useTranslation();
  const [cancelling, setCancelling] = useState(false);
  const elapsed = useElapsed();
  const phase = progress?.phase ?? "preparing";
  const percent = progress?.percent ?? null;
  const recent = progress?.recent ?? [];
  return (
    <div className={cn("space-y-1.5 text-xs", className)} role="status">
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
            onCancel();
          }}
        >
          <X />
          {t("setup.install.cancel")}
        </Button>
      </div>
    </div>
  );
}
