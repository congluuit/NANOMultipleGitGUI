import { useEffect, useState } from "react";
import { Check, Circle, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { formatEta } from "@/features/transfers/format";
import { formatBytes } from "@/lib/format";
import { cn } from "@/lib/utils";
import { cancelLfsTrack, progressText, type TrackProgress } from "./trackJob";

type StepState = "done" | "running" | "pending";

function Step({ state, label, detail }: { state: StepState; label: string; detail?: string }) {
  return (
    <li className="flex items-start gap-2">
      {state === "done" ? (
        <Check className="mt-px size-3.5 shrink-0 text-success" />
      ) : state === "running" ? (
        <LoaderCircle className="mt-px size-3.5 shrink-0 animate-spin text-primary" />
      ) : (
        <Circle className="mt-px size-3.5 shrink-0 text-muted-foreground/60" />
      )}
      <span className={cn("min-w-0 flex-1", state === "pending" && "text-muted-foreground")}>
        {label}
      </span>
      {detail && (
        <span className="shrink-0 font-mono text-muted-foreground tabular-nums">{detail}</span>
      )}
    </li>
  );
}

function Bar({ percent }: { percent: number }) {
  return (
    <span className="block h-1.5 w-full overflow-hidden rounded bg-border">
      <span
        className="block h-full bg-primary transition-[width] duration-300"
        style={{ width: `${Math.max(2, percent)}%` }}
      />
    </span>
  );
}

/** Seconds since `since`, re-rendered every second. */
function useElapsed(since: number): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  return Math.max(0, (now - since) / 1000);
}

/** Steps, bar, file being converted and time of a running "track with LFS" job. */
export function TrackProgressView({ job }: { job: TrackProgress }) {
  const { t } = useTranslation();
  const elapsed = useElapsed(job.startedAt);
  const converting = job.phase === "convert";
  return (
    <div
      role="status"
      aria-label={progressText(job)}
      className="space-y-3 rounded-md border bg-background p-3 text-xs"
    >
      <ol className="space-y-1.5">
        {job.patterns > 0 && (
          <Step
            state={converting ? "done" : "running"}
            label={t("lfsTrack.stepPatterns")}
            detail={`${job.donePatterns}/${job.patterns}`}
          />
        )}
        <Step
          state={converting ? "running" : "pending"}
          label={t("lfsTrack.stepConvert")}
          detail={`${job.doneFiles}/${job.files}`}
        />
        <Step state="pending" label={t("lfsTrack.stepCommit")} />
      </ol>
      <div className="space-y-1">
        <div className="flex items-center gap-2">
          <span className="min-w-0 flex-1 truncate text-muted-foreground">
            {converting
              ? t("transfers.bytes", {
                  done: formatBytes(job.doneBytes),
                  total: formatBytes(job.bytes),
                })
              : t("lfsTrack.phasePatterns", { done: job.donePatterns, total: job.patterns })}
          </span>
          <span className="shrink-0 font-medium tabular-nums">{job.percent}%</span>
        </div>
        <Bar percent={job.percent} />
        <p className="truncate font-mono text-muted-foreground" title={job.current ?? undefined}>
          {job.current ?? " "}
        </p>
      </div>
      <p className="text-muted-foreground tabular-nums">
        {t("lfsTrack.elapsed", { time: formatEta(elapsed) })}
        {job.etaSecs !== null && ` · ${t("transfers.eta", { time: formatEta(job.etaSecs) })}`}
      </p>
    </div>
  );
}

/** Compact progress for the commit box: the commit waits for the job. */
export function TrackProgressStrip({ repoId, job }: { repoId: number; job: TrackProgress }) {
  const { t } = useTranslation();
  return (
    <div
      role="status"
      className="space-y-1 rounded-md border border-primary/30 bg-primary/5 p-2 text-[11px]"
    >
      <div className="flex items-center gap-2">
        <LoaderCircle className="size-3.5 shrink-0 animate-spin text-primary" />
        <span className="min-w-0 flex-1 truncate font-medium" title={progressText(job)}>
          {progressText(job)}
        </span>
        <span className="shrink-0 text-muted-foreground tabular-nums">{job.percent}%</span>
        <button
          type="button"
          disabled={job.cancelling}
          onClick={() => cancelLfsTrack(repoId)}
          className="shrink-0 rounded px-1.5 py-0.5 text-danger hover:bg-danger/10 disabled:opacity-50"
        >
          {job.cancelling ? t("lfsTrack.stopping") : t("lfsTrack.stop")}
        </button>
      </div>
      <Bar percent={job.percent} />
      <p className="text-muted-foreground">{t("lfsTrack.commitAfter")}</p>
    </div>
  );
}
