import { useTranslation } from "react-i18next";
import { useGraphStore } from "@/stores/graph";
import { useActiveRepo } from "@/stores/workspace";
import { useRepoRefs } from "@/features/repos/queries";
import { CiBadge } from "./CiBadge";
import { CiEntryView, CiProblemHint } from "./CiSummary";
import { useCiStatus } from "./queries";

/**
 * "CI" row of the commit details: badge + summary, expandable to jobs and log links. Nothing
 * for repositories without a hosted remote or commits without CI.
 */
export function CommitCiRow({ repoId, hash }: { repoId: number; hash: string }) {
  const { t } = useTranslation();
  const { entry, problem } = useCiStatus(repoId, hash);
  const hint = problem && problem.kind !== "noHostedRemote" && !entry;
  if (!hint && (!entry || entry.status.state === "none")) return null;
  return (
    <div className="grid grid-cols-[84px_1fr] items-start gap-2">
      <span className="text-muted-foreground">{t("ci.title")}</span>
      <div className="min-w-0">
        {entry && entry.status.state !== "none" ? (
          <CiEntryView entry={entry} defaultOpen={entry.status.state === "failure"} />
        ) : (
          <CiProblemHint problem={problem} />
        )}
      </div>
    </div>
  );
}

/**
 * `✓ CI` of HEAD for the status bar. Clicking jumps to HEAD in the graph (its details show the
 * jobs). Nothing when there is no CI to show.
 */
export function HeadCiIndicator() {
  const { t } = useTranslation();
  const repo = useActiveRepo();
  const repoId = repo && !repo.missing ? repo.id : null;
  const head = useRepoRefs(repoId).data?.head;
  const oid = head && !head.unborn ? head.oid : null;
  const { entry } = useCiStatus(repoId, oid);
  const requestJump = useGraphStore((s) => s.requestJump);
  if (!repoId || !oid || !entry || entry.status.state === "none") return null;
  return (
    <button
      type="button"
      className="flex shrink-0 items-center rounded px-1 hover:bg-surface"
      onClick={() => requestJump(repoId, oid)}
      data-testid="status-ci"
    >
      <CiBadge
        state={entry.status.state}
        jobs={entry.status.jobs}
        label={t("ci.short")}
        size={12}
      />
    </button>
  );
}
