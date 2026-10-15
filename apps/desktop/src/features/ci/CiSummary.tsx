import { useState } from "react";
import { ChevronDown, ChevronRight, LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { CiEntry, CiProblem } from "@/ipc";
import { CiBadge } from "./CiBadge";
import { useAccountLabelOf } from "./hooks";
import { CiChecksList } from "./CiChecksList";
import { describeCiProblem } from "./errors";
import { countJobs } from "./model";
import { useCiStatusFor, useCiEnabled } from "./queries";

/** Quiet hint for a CI problem with its fix button; nothing when there is nothing to say. */
export function CiProblemHint({ problem }: { problem: CiProblem | null }) {
  const { t, i18n } = useTranslation();
  const labelOf = useAccountLabelOf();
  const described = describeCiProblem(t, problem, labelOf, i18n.language);
  if (!described) return null;
  return (
    <p className="text-[11px] text-muted-foreground" role="note">
      {described.text}{" "}
      {described.action && (
        <button type="button" className="text-primary underline" onClick={described.action.run}>
          {described.action.label}
        </button>
      )}
    </p>
  );
}

/**
 * Badge + one-line summary of a CI entry, expandable to the job list. Shared by the commit
 * details and {@link CiSummary}.
 */
export function CiEntryView({
  entry,
  defaultOpen = false,
}: {
  entry: CiEntry;
  defaultOpen?: boolean;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(defaultOpen);
  const { status } = entry;
  const counts = countJobs(status.jobs);
  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <div className="space-y-1">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-1.5 rounded text-left text-xs hover:bg-surface-2"
      >
        <Chevron className="size-3 shrink-0 text-muted-foreground" />
        <CiBadge state={status.state} jobs={status.jobs} />
        <span className="font-medium">{t(`ci.state.${status.state}`)}</span>
        {counts.total > 0 && (
          <span className="text-muted-foreground">
            · {t("ci.jobCounts", { passed: counts.success, total: counts.total })}
          </span>
        )}
        {entry.remote && entry.remote !== "origin" && (
          <span className="ml-auto truncate text-[11px] text-muted-foreground">
            {t("ci.onRemote", { remote: entry.remote })}
          </span>
        )}
      </button>
      {open && <CiChecksList status={status} accountId={entry.accountId} className="pl-4" />}
    </div>
  );
}

/**
 * CI summary of commit `sha` of project `fullName`, read with `accountId` — for PR / MR pages
 * (agent 4.1 embeds it). Shows nothing when the commit has no CI.
 */
export function CiSummary({
  accountId,
  fullName,
  sha,
  defaultOpen = false,
}: {
  accountId: string;
  /** `owner/repo` or `group/sub/project`. */
  fullName: string;
  /** Head commit of the PR / MR. */
  sha: string;
  defaultOpen?: boolean;
}) {
  const { t } = useTranslation();
  const on = useCiEnabled();
  const { entry, problem, isLoading } = useCiStatusFor(accountId, fullName, sha);
  if (!on) return null;
  if (isLoading) {
    return (
      <p className="flex items-center gap-1 text-[11px] text-muted-foreground">
        <LoaderCircle className="size-3 animate-spin" />
        {t("ci.loading")}
      </p>
    );
  }
  if (problem) return <CiProblemHint problem={problem} />;
  if (!entry || entry.status.state === "none") return null;
  return <CiEntryView entry={entry} defaultOpen={defaultOpen} />;
}
