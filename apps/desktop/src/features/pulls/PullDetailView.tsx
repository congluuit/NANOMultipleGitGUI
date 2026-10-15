import { useState } from "react";
import {
  CircleAlert,
  Copy,
  ExternalLink,
  GitBranch,
  GitCommitHorizontal,
  GitMerge,
  LoaderCircle,
  LogIn,
  MessageSquare,
  RefreshCw,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { openExternal } from "@/features/accounts/manager/openExternal";
import type {
  ProviderFamily,
  PullComment,
  PullDetail,
  PullMergeMethod,
  PullMergeState,
  PullReviewEvent,
} from "@/ipc";
import { cn } from "@/lib/utils";
import { useGraphStore } from "@/stores/graph";
import { useHostingNav, type HostedItemRef } from "@/stores/hosting";
import { useSettings } from "@/stores/settings";
import { useWorkspace } from "@/stores/workspace";
import { checkoutPull, mergePull, postComment, setPullOpen, submitReview } from "./actions";
import { Composer } from "./Composer";
import { describePullError } from "./errors";
import { branchLabel, pullNumber, relativeTime, shortTerm, timeline } from "./model";
import {
  DecisionMark,
  PlainText,
  PullActingAs,
  ReviewerState,
  StateBadge,
  UserChip,
} from "./parts";
import { PullChecksSlot } from "./PullChecksSlot";
import { PullFilesTab } from "./PullFilesTab";
import {
  invalidatePull,
  useAccountFamily,
  usePullComments,
  usePullCommits,
  usePullDetail,
  usePullReviews,
} from "./queries";

type Tab = "overview" | "commits" | "files";

const MERGE_TONE: Record<PullMergeState, string> = {
  clean: "text-success",
  conflicts: "text-danger",
  blocked: "text-warning",
  behind: "text-warning",
  unstable: "text-warning",
  draft: "text-muted-foreground",
  checking: "text-muted-foreground",
  unknown: "text-muted-foreground",
};

function Box({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="flex flex-col gap-1.5 border-b px-3 py-2.5">
      <h3 className="text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
        {title}
      </h3>
      {children}
    </section>
  );
}

function TimelineEntry({
  c,
  onOpenFile,
  replies,
}: {
  c: PullComment;
  onOpenFile: (path: string) => void;
  replies: number;
}) {
  const { t } = useTranslation();
  const language = useSettings((s) => s.language);
  const when = relativeTime(c.createdAt, language);
  if (c.kind === "system") {
    return (
      <li className="flex items-center gap-2 px-1 text-[11px] text-muted-foreground">
        <GitCommitHorizontal className="size-3 shrink-0" />
        <span className="font-medium">{c.author?.username}</span>
        <PlainText text={c.body} className="min-w-0 truncate" />
        <span className="shrink-0">{when}</span>
      </li>
    );
  }
  return (
    <li className="rounded-md border">
      <div className="flex flex-wrap items-center gap-2 border-b bg-surface px-3 py-1 text-[11px] text-muted-foreground">
        <UserChip user={c.author} />
        {c.kind === "review" && c.reviewState && <ReviewerState state={c.reviewState} />}
        {c.kind === "line" && c.anchor && (
          <button
            type="button"
            onClick={() => onOpenFile(c.anchor!.path)}
            className="truncate text-primary hover:underline"
          >
            {t("pulls.overview.onLine", {
              path: c.anchor.path,
              line: c.anchor.line ?? c.anchor.originalLine ?? "?",
            })}
          </button>
        )}
        <span>{when}</span>
        {c.webUrl && (
          <button
            type="button"
            aria-label={t("pulls.openWeb")}
            title={t("pulls.openWeb")}
            onClick={() => void openExternal(c.webUrl!)}
            className="ml-auto hover:text-foreground"
          >
            <ExternalLink className="size-3" />
          </button>
        )}
      </div>
      {c.body.trim() ? (
        <div className="px-3 py-2 text-xs">
          <PlainText text={c.body} />
        </div>
      ) : null}
      {replies > 0 && (
        <button
          type="button"
          onClick={() => c.anchor && onOpenFile(c.anchor.path)}
          className="flex items-center gap-1 px-3 pb-2 text-[11px] text-primary hover:underline"
        >
          <MessageSquare className="size-3" />
          {t("pulls.overview.replies", { count: replies })}
        </button>
      )}
    </li>
  );
}

function OverviewTab({
  item,
  detail,
  actor,
  onActor,
  onOpenFile,
}: {
  item: HostedItemRef;
  detail: PullDetail;
  actor: string;
  onActor: (id: string) => void;
  onOpenFile: (path: string) => void;
}) {
  const { t } = useTranslation();
  const language = useSettings((s) => s.language);
  const comments = usePullComments(item);
  const all = comments.data ?? [];
  const entries = timeline(all);
  const replyCount = (c: PullComment) =>
    c.kind === "line" ? all.filter((x) => x.threadId === c.threadId && x.id !== c.id).length : 0;
  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-3 overflow-y-auto p-4">
      <div className="rounded-md border">
        <div className="flex items-center gap-2 border-b bg-surface px-3 py-1 text-[11px] text-muted-foreground">
          <UserChip user={detail.author} />
          <span>{relativeTime(detail.createdAt, language)}</span>
        </div>
        <div className="px-3 py-2 text-xs">
          {detail.body.trim() ? (
            <PlainText text={detail.body} />
          ) : (
            <span className="text-muted-foreground italic">{t("pulls.overview.noBody")}</span>
          )}
        </div>
      </div>
      {comments.isPending && (
        <p className="flex items-center gap-2 text-xs text-muted-foreground">
          <LoaderCircle className="size-3 animate-spin" />
          {t("pulls.loading")}
        </p>
      )}
      {comments.isError && (
        <p className="text-xs text-danger">{describePullError(comments.error).message}</p>
      )}
      <ul className="flex flex-col gap-2">
        {entries.map((c) => (
          <TimelineEntry key={c.id} c={c} onOpenFile={onOpenFile} replies={replyCount(c)} />
        ))}
      </ul>
      <Composer
        actor={actor}
        onActor={onActor}
        placeholder={t("pulls.overview.commentPlaceholder")}
        submitLabel={t("pulls.overview.comment")}
        onSubmit={(text) => postComment(item, actor, { kind: "general" }, text)}
      />
    </div>
  );
}

function CommitsTab({ item, repoId }: { item: HostedItemRef; repoId: number | null }) {
  const { t } = useTranslation();
  const language = useSettings((s) => s.language);
  const commits = usePullCommits(item);
  const activeId = useWorkspace((s) => s.activeId);
  const close = useHostingNav((s) => s.closePullRequest);
  const requestJump = useGraphStore((s) => s.requestJump);
  const canJump = repoId !== null && repoId === activeId;
  if (commits.isPending) {
    return <p className="p-4 text-xs text-muted-foreground">{t("pulls.loading")}</p>;
  }
  if (commits.isError) {
    return <p className="p-4 text-xs text-danger">{describePullError(commits.error).message}</p>;
  }
  return (
    <ul className="min-h-0 flex-1 overflow-y-auto p-2 text-xs">
      {!canJump && (
        <li className="px-2 pb-2 text-[11px] text-muted-foreground">
          {t("pulls.commits.openRepoToJump")}
        </li>
      )}
      {commits.data.map((c) => (
        <li
          key={c.sha}
          className="group flex items-center gap-2 rounded px-2 py-1 hover:bg-surface-2"
        >
          <button
            type="button"
            disabled={!canJump}
            title={canJump ? t("pulls.commits.jump") : undefined}
            onClick={() => {
              if (repoId === null) return;
              close();
              requestJump(repoId, c.sha);
            }}
            className="flex min-w-0 flex-1 items-center gap-2 text-left disabled:cursor-default"
          >
            <span className="shrink-0 font-mono text-muted-foreground">{c.sha.slice(0, 7)}</span>
            <span className="min-w-0 flex-1 truncate" title={c.message}>
              {c.title}
            </span>
            <span className="shrink-0 text-muted-foreground">{c.authorLogin ?? c.authorName}</span>
            <span className="shrink-0 text-muted-foreground">
              {relativeTime(c.authoredAt, language)}
            </span>
          </button>
          <button
            type="button"
            aria-label={t("pulls.commits.copy")}
            title={t("pulls.commits.copy")}
            onClick={() => void navigator.clipboard.writeText(c.sha)}
            className="invisible shrink-0 text-muted-foreground group-hover:visible hover:text-foreground"
          >
            <Copy className="size-3" />
          </button>
        </li>
      ))}
    </ul>
  );
}

function ReviewersBox({ item, family }: { item: HostedItemRef; family: ProviderFamily | null }) {
  const { t } = useTranslation();
  const reviews = usePullReviews(item);
  const r = reviews.data;
  return (
    <Box title={t("pulls.side.reviewers")}>
      {reviews.isPending && <span className="text-xs text-muted-foreground">…</span>}
      {reviews.isError && (
        <span className="text-[11px] text-danger">{describePullError(reviews.error).message}</span>
      )}
      {r && (
        <>
          <DecisionMark decision={r.decision} withLabel />
          {r.approvalsLeft !== null && r.approvalsLeft > 0 && (
            <span className="text-[11px] text-warning">
              {t("pulls.side.approvalsLeft", { count: r.approvalsLeft })}
            </span>
          )}
          {r.approvalsUnavailable && family === "gitlab" && (
            <span className="text-[11px] text-muted-foreground">
              {t("pulls.side.approvalsUnavailable")}
            </span>
          )}
          {r.reviewers.length === 0 && r.teams.length === 0 && (
            <span className="text-xs text-muted-foreground">{t("pulls.side.noReviewers")}</span>
          )}
          <ul className="flex flex-col gap-1 text-xs">
            {r.reviewers.map((rv) => (
              <li key={rv.user.username} className="flex items-center gap-2">
                <UserChip user={rv.user} />
                <span className="flex-1" />
                <ReviewerState state={rv.state} />
              </li>
            ))}
            {r.teams.map((team) => (
              <li key={`team-${team}`} className="flex items-center gap-2 text-muted-foreground">
                <span className="truncate">{team}</span>
                <span className="flex-1" />
                <ReviewerState state="requested" />
              </li>
            ))}
          </ul>
        </>
      )}
    </Box>
  );
}

function MergeBox({
  item,
  detail,
  family,
  actor,
  onActor,
}: {
  item: HostedItemRef;
  detail: PullDetail;
  family: ProviderFamily | null;
  actor: string;
  onActor: (id: string) => void;
}) {
  const { t } = useTranslation();
  const term = shortTerm(family);
  const methods = detail.mergeMethods;
  const [method, setMethod] = useState<PullMergeMethod>(methods[0] ?? "merge");
  const [deleteBranch, setDeleteBranch] = useState(detail.deleteSourceDefault);
  const [confirm, setConfirm] = useState<"merge" | "close" | null>(null);
  const [busy, setBusy] = useState(false);
  const open = detail.state === "open";
  const blocked = detail.mergeState === "conflicts" || detail.mergeState === "draft";

  const run = async (what: "merge" | "close" | "reopen") => {
    setBusy(true);
    setConfirm(null);
    if (what === "merge") {
      await mergePull(item, actor, family, {
        method,
        deleteSourceBranch: deleteBranch,
        expectedHeadSha: detail.headSha || null,
      });
    } else {
      await setPullOpen(item, actor, what === "reopen");
    }
    setBusy(false);
  };

  if (!open) {
    return (
      <Box title={t("pulls.side.status")}>
        <span className="text-xs">
          {detail.state === "merged"
            ? t("pulls.side.mergedBy", { user: detail.mergedBy?.username ?? "?" })
            : t("pulls.side.closedState")}
        </span>
        {detail.state === "closed" && (
          <div className="flex items-center gap-2">
            <ActingAsLine actor={actor} onActor={onActor} busy={busy} />
            <Button size="xs" variant="outline" disabled={busy} onClick={() => void run("reopen")}>
              {t("pulls.side.reopen")}
            </Button>
          </div>
        )}
      </Box>
    );
  }

  return (
    <Box title={t("pulls.side.merge")}>
      <span className={cn("flex items-start gap-1 text-xs", MERGE_TONE[detail.mergeState])}>
        <CircleAlert className="mt-0.5 size-3 shrink-0" />
        <span>
          {t(`pulls.mergeState.${detail.mergeState}`, { target: detail.targetBranch })}
          {detail.mergeStateRaw && detail.mergeState === "blocked" && (
            <span className="text-muted-foreground"> ({detail.mergeStateRaw})</span>
          )}
        </span>
      </span>
      {detail.mergeState === "conflicts" && (
        <span className="text-[11px] text-muted-foreground">{t("pulls.side.conflictHint")}</span>
      )}
      <label className="flex flex-col gap-0.5 text-[11px] text-muted-foreground">
        {t("pulls.side.method")}
        <select
          value={method}
          onChange={(e) => setMethod(e.target.value as PullMergeMethod)}
          className="h-7 rounded-md border bg-background px-1 text-xs text-foreground"
        >
          {methods.map((m) => (
            <option key={m} value={m}>
              {t(`pulls.method.${m}`)}
            </option>
          ))}
        </select>
      </label>
      <label className="flex items-center gap-1.5 text-xs">
        <input
          type="checkbox"
          checked={deleteBranch}
          onChange={(e) => setDeleteBranch(e.target.checked)}
        />
        {t("pulls.side.deleteBranch", { branch: detail.sourceBranch })}
      </label>
      <ActingAsLine actor={actor} onActor={onActor} busy={busy} />
      <div className="flex gap-2">
        <Button size="sm" disabled={busy || blocked} onClick={() => setConfirm("merge")}>
          <GitMerge />
          {t("pulls.side.mergeButton")}
        </Button>
        <Button size="sm" variant="outline" disabled={busy} onClick={() => setConfirm("close")}>
          {t("pulls.side.close", { term })}
        </Button>
      </div>
      <ConfirmDialog
        open={confirm !== null}
        title={
          confirm === "merge"
            ? t("pulls.side.confirmMerge", { number: pullNumber(family, detail.number) })
            : t("pulls.side.confirmClose", { number: pullNumber(family, detail.number) })
        }
        description={
          confirm === "merge"
            ? t("pulls.side.confirmMergeBody", {
                source: detail.sourceBranch,
                target: detail.targetBranch,
                method: t(`pulls.method.${method}`),
              })
            : t("pulls.side.confirmCloseBody")
        }
        confirmLabel={
          confirm === "merge" ? t("pulls.side.mergeButton") : t("pulls.side.close", { term })
        }
        destructive={confirm === "close"}
        busy={busy}
        onConfirm={() => confirm && void run(confirm)}
        onCancel={() => setConfirm(null)}
      />
    </Box>
  );
}

function ActingAsLine({
  actor,
  onActor,
  busy,
}: {
  actor: string;
  onActor: (id: string) => void;
  busy: boolean;
}) {
  return <PullActingAs accountId={actor} onChange={onActor} disabled={busy} />;
}

function ReviewPopover({
  item,
  family,
  actor,
  onActor,
}: {
  item: HostedItemRef;
  family: ProviderFamily | null;
  actor: string;
  onActor: (id: string) => void;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [event, setEvent] = useState<PullReviewEvent>("comment");
  const reviews = usePullReviews(item);
  const events: PullReviewEvent[] = ["comment", "approve", "requestChanges"];
  if (family === "gitlab" && reviews.data?.approvedByMe) events.push("unapprove");
  const needsText = event === "comment" || (event === "requestChanges" && family === "gitlab");
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button size="sm" variant="outline">
          {t("pulls.review.button")}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-96 p-3">
        <Composer
          actor={actor}
          onActor={onActor}
          autoFocus
          allowEmpty={!needsText}
          placeholder={t("pulls.review.placeholder")}
          submitLabel={t("pulls.review.submit")}
          onSubmit={async (text) => {
            const ok = await submitReview(item, actor, event, text);
            if (ok) setOpen(false);
            return ok;
          }}
          extra={
            <div className="flex flex-col gap-1 text-xs">
              {events.map((ev) => (
                <label key={ev} className="flex items-center gap-1.5">
                  <input
                    type="radio"
                    name="pull-review-event"
                    checked={event === ev}
                    onChange={() => setEvent(ev)}
                  />
                  {t(`pulls.review.event.${ev}`)}
                </label>
              ))}
              {family === "gitlab" && event === "requestChanges" && (
                <span className="text-[11px] text-muted-foreground">
                  {t("pulls.review.gitlabRequestChanges")}
                </span>
              )}
            </div>
          }
        />
      </PopoverContent>
    </Popover>
  );
}

/**
 * Pull / merge request opened through `useHostingNav().pullRequest` (center area): overview
 * with conversation, reviewers and merge box, commits, files with line comments.
 */
export function PullDetailView({ pullRef }: { pullRef: HostedItemRef }) {
  const { t } = useTranslation();
  const close = useHostingNav((s) => s.closePullRequest);
  const family = useAccountFamily(pullRef.accountId);
  const detail = usePullDetail(pullRef);
  const commits = usePullCommits(pullRef);
  const [tab, setTab] = useState<Tab>("overview");
  const [file, setFile] = useState<string | null>(null);
  // "With ● account" for the writes of this view only (reset when another PR opens).
  const [actor, setActor] = useState(pullRef.accountId);
  const [checkingOut, setCheckingOut] = useState(false);
  const repoId = pullRef.repoId ?? null;
  const term = shortTerm(family);
  const d = detail.data;

  const openFile = (path: string) => {
    setFile(path);
    setTab("files");
  };

  return (
    <section
      className="flex min-w-0 flex-1 flex-col bg-background"
      aria-label={t("pulls.detailLabel", { term })}
      data-testid="pull-detail"
    >
      <header className="flex shrink-0 flex-col gap-1 border-b px-4 py-2">
        <div className="flex items-center gap-2">
          {d && <StateBadge state={d.state} draft={d.draft} />}
          <h2 className="min-w-0 flex-1 truncate text-sm font-semibold" title={d?.title}>
            {d ? d.title : t("pulls.loading")}
            <span className="ml-2 font-normal text-muted-foreground">
              {pullNumber(family, pullRef.number)}
            </span>
          </h2>
          <button
            type="button"
            aria-label={t("pulls.refresh")}
            title={t("pulls.refresh")}
            onClick={() => invalidatePull(pullRef)}
            className="rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
          >
            <RefreshCw className={cn("size-3.5", detail.isFetching && "animate-spin")} />
          </button>
          {d?.webUrl && (
            <button
              type="button"
              aria-label={t("pulls.openWeb")}
              title={t("pulls.openWeb")}
              onClick={() => void openExternal(d.webUrl)}
              className="rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
            >
              <ExternalLink className="size-3.5" />
            </button>
          )}
          <button
            type="button"
            aria-label={t("pulls.close")}
            title={t("pulls.close")}
            onClick={close}
            className="rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
          >
            <X className="size-4" />
          </button>
        </div>
        {d && (
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-muted-foreground">
            <UserChip user={d.author} />
            <span className="inline-flex min-w-0 items-center gap-1">
              <GitBranch className="size-3 shrink-0" />
              <span className="truncate font-mono">
                {branchLabel(d.sourceBranch, d.sourceRepo, d.targetRepo)}
              </span>
              →<span className="truncate font-mono">{d.targetBranch}</span>
            </span>
            {d.fromFork && <span>{t("pulls.fromFork", { repo: d.sourceRepo ?? "?" })}</span>}
            {d.labels.map((l) => (
              <span key={`label-${l}`} className="rounded-full border px-1.5 text-[10px]">
                {l}
              </span>
            ))}
            <span className="flex-1" />
            {repoId !== null && family && (
              <Button
                size="sm"
                variant="outline"
                disabled={checkingOut}
                title={t("pulls.checkout.tip")}
                onClick={async () => {
                  setCheckingOut(true);
                  await checkoutPull(repoId, d, family);
                  setCheckingOut(false);
                }}
              >
                {checkingOut ? <LoaderCircle className="animate-spin" /> : <LogIn />}
                {t("pulls.checkout.button")}
              </Button>
            )}
            {d.state === "open" && (
              <ReviewPopover item={pullRef} family={family} actor={actor} onActor={setActor} />
            )}
          </div>
        )}
        <nav className="-mb-2 flex gap-1" role="tablist">
          {(["overview", "commits", "files"] as const).map((id) => {
            const count =
              id === "commits"
                ? (commits.data?.length ?? d?.commits)
                : id === "files"
                  ? d?.changedFiles
                  : undefined;
            return (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={tab === id}
                onClick={() => setTab(id)}
                className={cn(
                  "border-b-2 px-2 py-1 text-xs",
                  tab === id
                    ? "border-primary font-medium text-foreground"
                    : "border-transparent text-muted-foreground hover:text-foreground",
                )}
              >
                {t(`pulls.tab.${id}`)}
                {count != null && <span className="ml-1 text-muted-foreground">{count}</span>}
              </button>
            );
          })}
        </nav>
      </header>
      {detail.isPending ? (
        <div className="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground">
          <LoaderCircle className="size-4 animate-spin" />
          {t("pulls.loading")}
        </div>
      ) : detail.isError ? (
        <DetailError error={detail.error} onRetry={() => void detail.refetch()} />
      ) : d ? (
        <div className="flex min-h-0 flex-1">
          {tab === "overview" && (
            <>
              <OverviewTab
                item={pullRef}
                detail={d}
                actor={actor}
                onActor={setActor}
                onOpenFile={openFile}
              />
              <aside className="w-72 shrink-0 overflow-y-auto border-l">
                <ReviewersBox item={pullRef} family={family} />
                <PullChecksSlot
                  repoId={repoId}
                  accountId={pullRef.accountId}
                  fullName={pullRef.fullName}
                  sha={d.headSha}
                />
                <MergeBox
                  key={`merge-${d.headSha}-${d.state}`}
                  item={pullRef}
                  detail={d}
                  family={family}
                  actor={actor}
                  onActor={setActor}
                />
                {d.assignees.length > 0 && (
                  <Box title={t("pulls.side.assignees")}>
                    {d.assignees.map((a) => (
                      <UserChip key={a.username} user={a} />
                    ))}
                  </Box>
                )}
              </aside>
            </>
          )}
          {tab === "commits" && <CommitsTab item={pullRef} repoId={repoId} />}
          {tab === "files" && (
            <PullFilesTab
              item={pullRef}
              detail={d}
              family={family}
              repoId={repoId}
              actor={actor}
              onActor={setActor}
              selected={file}
              onSelect={setFile}
            />
          )}
        </div>
      ) : null}
    </section>
  );
}

function DetailError({ error, onRetry }: { error: unknown; onRetry: () => void }) {
  const { t } = useTranslation();
  const d = describePullError(error);
  return (
    <div
      role="alert"
      className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-xs"
    >
      <p className="font-medium text-danger">{d.message}</p>
      {d.hint && <p className="max-w-md text-muted-foreground">{d.hint}</p>}
      <div className="flex gap-2">
        {d.actions.map((a) => (
          <Button key={a.label} size="sm" variant="outline" onClick={a.run}>
            {a.label}
          </Button>
        ))}
        <Button size="sm" variant="ghost" onClick={onRetry}>
          {t("pulls.list.retry")}
        </Button>
      </div>
    </div>
  );
}
