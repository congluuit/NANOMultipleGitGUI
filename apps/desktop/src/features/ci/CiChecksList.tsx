import { useState } from "react";
import { ExternalLink, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { openExternal } from "@/features/accounts/manager/openExternal";
import type { CiStatus } from "@/ipc";
import { cn } from "@/lib/utils";
import { jobIcon, useAccountLabelOf } from "./hooks";
import { describeProviderError } from "./errors";
import { ciTextClass, jobDuration, safeUrl, sortJobs } from "./model";

/** Jobs shown before "Show all". */
const COLLAPSED_JOBS = 8;

/**
 * Jobs of a commit's CI (name, stage / app, state, duration, log link). Failed jobs first.
 * Names and descriptions come from the provider and are rendered as plain text.
 */
export function CiChecksList({
  status,
  accountId,
  className,
}: {
  status: CiStatus;
  /** Account the status was read with (for the "missing permission" hint). */
  accountId?: string | null;
  className?: string;
}) {
  const { t, i18n } = useTranslation();
  const labelOf = useAccountLabelOf();
  const [expanded, setExpanded] = useState(false);
  const jobs = sortJobs(status.jobs);
  const visible = expanded ? jobs : jobs.slice(0, COLLAPSED_JOBS);
  // Durations of running jobs count up to when the list was mounted (re-read every ~20 s).
  const [now] = useState(() => Math.floor(Date.now() / 1000));
  const pipeline = safeUrl(status.pipelineUrl);
  const checksHint =
    status.checksError && accountId
      ? describeProviderError(t, status.checksError, accountId, labelOf(accountId), i18n.language)
      : null;

  return (
    <div className={cn("space-y-1 text-xs", className)}>
      {checksHint && (
        <p className="flex items-start gap-1 text-[11px] text-warning" role="note">
          <TriangleAlert className="mt-0.5 size-3 shrink-0" />
          <span>
            {t("ci.checksUnavailable")} {checksHint.text}{" "}
            {checksHint.action && (
              <button type="button" className="underline" onClick={checksHint.action.run}>
                {checksHint.action.label}
              </button>
            )}
          </span>
        </p>
      )}
      {jobs.length === 0 ? (
        <p className="text-[11px] text-muted-foreground">{t("ci.noJobs")}</p>
      ) : (
        <ul className="space-y-0.5" aria-label={t("ci.jobs")}>
          {visible.map((job, i) => {
            const Icon = jobIcon(job.state);
            const url = safeUrl(job.url);
            const duration = jobDuration(job, now);
            return (
              <li
                key={`${job.kind}:${job.group ?? ""}:${job.name}:${i}`}
                className="flex items-center gap-1.5"
              >
                <Icon
                  className={cn(
                    "size-3.5 shrink-0",
                    ciTextClass(job.state),
                    job.state === "running" && "animate-pulse",
                  )}
                  aria-label={t(`ci.jobState.${job.state}`)}
                />
                <span className="min-w-0 flex-1 truncate" title={job.description ?? job.name}>
                  {job.group && <span className="text-muted-foreground">{job.group} · </span>}
                  {job.name}
                  {job.allowFailure && job.state === "failure" && (
                    <span className="text-muted-foreground"> ({t("ci.allowedToFail")})</span>
                  )}
                </span>
                <span className="shrink-0 text-[11px] text-muted-foreground tabular-nums">
                  {duration ?? t(`ci.jobState.${job.state}`)}
                </span>
                {url && (
                  <button
                    type="button"
                    className="shrink-0 rounded p-0.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
                    title={t("ci.openLog")}
                    aria-label={t("ci.openLogOf", { name: job.name })}
                    onClick={() => void openExternal(url)}
                  >
                    <ExternalLink className="size-3" />
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}
      <div className="flex flex-wrap gap-x-3 text-[11px]">
        {jobs.length > COLLAPSED_JOBS && (
          <button
            type="button"
            className="text-primary hover:underline"
            onClick={() => setExpanded(!expanded)}
          >
            {expanded ? t("ci.showLess") : t("ci.showAll", { count: jobs.length })}
          </button>
        )}
        {pipeline && (
          <button
            type="button"
            className="text-primary hover:underline"
            onClick={() => void openExternal(pipeline)}
          >
            {t("ci.openPipeline")}
          </button>
        )}
        {status.truncated && <span className="text-muted-foreground">{t("ci.truncated")}</span>}
      </div>
    </div>
  );
}
