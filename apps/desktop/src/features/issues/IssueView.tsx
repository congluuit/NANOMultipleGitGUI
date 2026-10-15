import { useState } from "react";
import {
  CircleCheck,
  CircleDot,
  ExternalLink,
  GitBranchPlus,
  LoaderCircle,
  MessageSquare,
  Pencil,
  RefreshCw,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { ScrollArea } from "@/components/ui/scroll-area";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { accountLabel, useSortedAccounts } from "@/features/accounts/switcher";
import { relativeTime } from "@/features/clone/model";
import {
  commands,
  type IssueDetail,
  type IssueUpdate,
  type IssueUser,
  type ProviderFamily,
} from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { cn } from "@/lib/utils";
import { useHostingNav, type HostedItemRef } from "@/stores/hosting";
import { useUi } from "@/stores/ui";
import { useAccountById, useAccountFamily } from "./accountHooks";
import { ActingAs, AccountDot } from "./ActingAs";
import { createIssueBranch, issueErrorNotice, openRef, problemActions } from "./actions";
import { describeIssueError } from "./errors";
import { LabelDot } from "./IssueSidebarList";
import {
  invalidateIssueLists,
  issueKeys,
  useAssignableUsers,
  useHostedRemotes,
  useIssue,
  useIssueComments,
  useIssueLabels,
} from "./queries";
import { SafeText } from "./SafeText";

function Person({ user }: { user: IssueUser | null }) {
  if (!user) return null;
  return (
    <span className="inline-flex min-w-0 items-center gap-1">
      {user.avatarUrl?.startsWith("https://") && (
        <img
          src={user.avatarUrl}
          alt=""
          className="size-4 shrink-0 rounded-full"
          referrerPolicy="no-referrer"
        />
      )}
      <span className="truncate font-medium">{user.username}</span>
    </span>
  );
}

/** Center view of one issue (opened through `useHostingNav().openIssue`). */
export function IssueView({ issueRef }: { issueRef: HostedItemRef }) {
  const { t, i18n } = useTranslation();
  const close = useHostingNav((s) => s.closeIssue);
  const detail = useIssue(issueRef);
  const comments = useIssueComments(issueRef);
  const account = useAccountById(issueRef.accountId);
  const family = useAccountFamily(issueRef.accountId);
  const name = account ? accountLabel(account) : issueRef.accountId;
  const d = detail.data;
  const problem = detail.isError ? describeIssueError(t, detail.error, name, i18n.language) : null;
  const refCtx = {
    accountId: issueRef.accountId,
    fullName: issueRef.fullName,
    family,
    repoId: issueRef.repoId ?? null,
  };

  return (
    <section
      className="flex min-w-0 flex-1 flex-col bg-background"
      aria-label={t("issues.view.title", { number: issueRef.number })}
    >
      <header className="flex h-10 shrink-0 items-center gap-2 border-b px-3 text-xs">
        {d && <IssueStateIcon state={d.issue.state} className="size-4" />}
        <span className="truncate text-muted-foreground">{issueRef.fullName}</span>
        <span className="font-medium tabular-nums">#{issueRef.number}</span>
        <span className="text-muted-foreground">·</span>
        <AccountDot account={account} />
        <span className="flex-1" />
        <button
          type="button"
          aria-label={t("issues.refresh")}
          title={t("issues.refresh")}
          onClick={() => {
            void detail.refetch();
            void comments.refetch();
          }}
          className="rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <RefreshCw className={cn("size-3.5", detail.isFetching && "animate-spin")} />
        </button>
        {d?.issue.webUrl.startsWith("https://") && (
          <button
            type="button"
            aria-label={t("issues.openInBrowser")}
            title={t("issues.openInBrowser")}
            onClick={() => void openExternal(d.issue.webUrl)}
            className="rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
          >
            <ExternalLink className="size-3.5" />
          </button>
        )}
        <button
          type="button"
          aria-label={t("issues.view.close")}
          title={t("issues.view.close")}
          onClick={close}
          className="rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <X className="size-3.5" />
        </button>
      </header>
      {detail.isPending && (
        <div className="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground">
          <LoaderCircle className="size-4 animate-spin" />
          {t("issues.loading")}
        </div>
      )}
      {problem && (
        <div className="space-y-2 p-4 text-xs">
          <p className="text-danger">{problem.message}</p>
          {problem.hint && <p className="text-muted-foreground">{problem.hint}</p>}
          <div className="flex flex-wrap gap-2">
            {problemActions(problem, issueRef.accountId).map((a) => (
              <Button key={`fix-${a.label}`} size="xs" variant="outline" onClick={a.run}>
                {a.label}
              </Button>
            ))}
            {problem.fix === "switchAccount" && <TryOtherAccount issueRef={issueRef} />}
          </div>
        </div>
      )}
      {d && (
        <ScrollArea className="min-h-0 flex-1">
          <div className="mx-auto max-w-3xl space-y-4 p-4 text-xs">
            {d.issue.isPullRequest && (
              <p className="rounded border border-info/40 bg-info/10 px-2 py-1">
                {t("issues.view.isPullRequest")}{" "}
                <button
                  type="button"
                  className="text-primary underline-offset-2 hover:underline"
                  onClick={() => useHostingNav.getState().openPullRequest(issueRef)}
                >
                  {t("issues.view.openPullRequest")}
                </button>
              </p>
            )}
            <h1 className="text-base leading-snug font-semibold break-words">
              {d.issue.title}{" "}
              <span className="font-normal text-muted-foreground">#{d.issue.number}</span>
            </h1>
            <Meta detail={d} />
            <Editors issueRef={issueRef} detail={d} />
            <div className="rounded-md border p-3 leading-relaxed">
              {d.body.trim() ? (
                <SafeText text={d.body} family={family} onRef={(p) => void openRef(refCtx, p)} />
              ) : (
                <span className="text-muted-foreground italic">{t("issues.view.noBody")}</span>
              )}
            </div>
            {issueRef.repoId != null && <BranchFromIssue repoId={issueRef.repoId} detail={d} />}
            <Comments issueRef={issueRef} family={family} />
            <Composer issueRef={issueRef} detail={d} />
          </div>
        </ScrollArea>
      )}
    </section>
  );
}

export function IssueStateIcon({
  state,
  className,
}: {
  state: "open" | "closed";
  className?: string;
}) {
  return state === "open" ? (
    <CircleDot className={cn("shrink-0 text-success", className)} />
  ) : (
    <CircleCheck className={cn("shrink-0 text-muted-foreground", className)} />
  );
}

function Meta({ detail: d }: { detail: IssueDetail }) {
  const { t, i18n } = useTranslation();
  const [now] = useState(() => Date.now());
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-muted-foreground">
      <span
        className={cn(
          "rounded-full px-2 py-0.5 font-medium",
          d.issue.state === "open" ? "bg-success/15 text-success" : "bg-surface-2",
        )}
      >
        {t(`issues.state.${d.issue.state}`)}
      </span>
      {d.issue.author && (
        <span className="inline-flex items-center gap-1">
          <Person user={d.issue.author} />
          {d.issue.createdAt != null &&
            t("issues.view.opened", { when: relativeTime(d.issue.createdAt, now, i18n.language) })}
        </span>
      )}
      {d.issue.state === "closed" && d.closedAt != null && (
        <span>
          {t("issues.view.closed", { when: relativeTime(d.closedAt, now, i18n.language) })}
        </span>
      )}
      <span className="inline-flex items-center gap-1">
        <MessageSquare className="size-3" />
        {d.issue.comments}
      </span>
      {d.locked && <span>{t("issues.view.locked")}</span>}
    </div>
  );
}

/** Re-opens the issue with another account of the same provider (404 on a private project). */
function TryOtherAccount({ issueRef }: { issueRef: HostedItemRef }) {
  const { t } = useTranslation();
  const accounts = useSortedAccounts();
  const current = accounts.find((a) => a.id === issueRef.accountId);
  const others = accounts.filter(
    (a) =>
      a.id !== issueRef.accountId &&
      a.secrets.includes("token") &&
      (!current || a.providerId === current.providerId),
  );
  if (others.length === 0) return null;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="xs" variant="outline">
          {t("issues.view.tryOtherAccount")}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent className="text-xs">
        {others.map((a) => (
          <DropdownMenuItem
            key={a.id}
            onSelect={() => useHostingNav.getState().openIssue({ ...issueRef, accountId: a.id })}
          >
            <AccountDot account={a} />
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Applies a change as `actor` and refreshes the caches; `false` when it failed. */
async function applyUpdate(
  issueRef: HostedItemRef,
  actor: string,
  change: IssueUpdate,
): Promise<boolean> {
  try {
    const next = await commands.issueUpdate(actor, issueRef.fullName, issueRef.number, change);
    queryClient.setQueryData(issueKeys.detail(issueRef), next);
    void queryClient.invalidateQueries({ queryKey: issueKeys.comments(issueRef) });
    invalidateIssueLists();
    return true;
  } catch (e) {
    useUi.getState().showNotice(issueErrorNotice(e, actor));
    return false;
  }
}

function Editors({ issueRef, detail: d }: { issueRef: HostedItemRef; detail: IssueDetail }) {
  const { t } = useTranslation();
  const [actor, setActor] = useState(issueRef.accountId);
  const [busy, setBusy] = useState(false);
  const labels = useIssueLabels(issueRef.accountId, issueRef.fullName);
  const users = useAssignableUsers(issueRef.accountId, issueRef.fullName);
  const current = d.issue.labels.map((l) => l.name);
  const assigned = d.issue.assignees.map((a) => a.username);

  const run = async (change: IssueUpdate) => {
    setBusy(true);
    await applyUpdate(issueRef, actor, change);
    setBusy(false);
    setActor(issueRef.accountId);
  };
  const toggle = (list: string[], item: string) =>
    list.includes(item) ? list.filter((x) => x !== item) : [...list, item];

  return (
    <div className="grid grid-cols-[96px_1fr] items-start gap-x-2 gap-y-2">
      <span className="pt-0.5 text-muted-foreground">{t("issues.view.labels")}</span>
      <div className="flex flex-wrap items-center gap-1">
        {d.issue.labels.length === 0 && (
          <span className="text-muted-foreground">{t("issues.view.noLabels")}</span>
        )}
        {d.issue.labels.map((l) => (
          <span
            key={`l-${l.name}`}
            title={l.description ?? undefined}
            className="inline-flex items-center gap-1 rounded-full border px-2 py-0.5"
          >
            <LabelDot color={l.color} />
            {l.name}
          </span>
        ))}
        <DropdownMenu>
          <DropdownMenuTrigger
            disabled={busy}
            aria-label={t("issues.view.editLabels")}
            title={t("issues.view.editLabels")}
            className="rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
          >
            <Pencil className="size-3" />
          </DropdownMenuTrigger>
          <DropdownMenuContent className="max-h-80 w-56 overflow-y-auto text-xs">
            <DropdownMenuLabel className="text-[11px] text-muted-foreground">
              {t("issues.view.editLabels")}
            </DropdownMenuLabel>
            {labels.isPending && (
              <DropdownMenuItem disabled>{t("issues.loading")}</DropdownMenuItem>
            )}
            {labels.isError && (
              <DropdownMenuItem disabled>{t("issues.view.cannotLoad")}</DropdownMenuItem>
            )}
            {labels.data?.length === 0 && (
              <DropdownMenuItem disabled>{t("issues.filter.noLabels")}</DropdownMenuItem>
            )}
            {(labels.data ?? []).map((l) => (
              <DropdownMenuCheckboxItem
                key={`le-${l.name}`}
                checked={current.includes(l.name)}
                onSelect={(e) => e.preventDefault()}
                onCheckedChange={() => void run({ labels: toggle(current, l.name) })}
              >
                <LabelDot color={l.color} />
                <span className="truncate">{l.name}</span>
              </DropdownMenuCheckboxItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <span className="pt-0.5 text-muted-foreground">{t("issues.view.assignees")}</span>
      <div className="flex flex-wrap items-center gap-2">
        {d.issue.assignees.length === 0 && (
          <span className="text-muted-foreground">{t("issues.view.noAssignees")}</span>
        )}
        {d.issue.assignees.map((u) => (
          <Person key={`a-${u.username}`} user={u} />
        ))}
        <DropdownMenu>
          <DropdownMenuTrigger
            disabled={busy}
            aria-label={t("issues.view.editAssignees")}
            title={t("issues.view.editAssignees")}
            className="rounded p-1 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
          >
            <Pencil className="size-3" />
          </DropdownMenuTrigger>
          <DropdownMenuContent className="max-h-80 w-56 overflow-y-auto text-xs">
            <DropdownMenuLabel className="text-[11px] text-muted-foreground">
              {t("issues.view.editAssignees")}
            </DropdownMenuLabel>
            {users.isPending && <DropdownMenuItem disabled>{t("issues.loading")}</DropdownMenuItem>}
            {users.isError && (
              <DropdownMenuItem disabled>{t("issues.view.cannotLoad")}</DropdownMenuItem>
            )}
            {(users.data ?? []).map((u) => (
              <DropdownMenuCheckboxItem
                key={`ae-${u.username}`}
                checked={assigned.includes(u.username)}
                onSelect={(e) => e.preventDefault()}
                onCheckedChange={() => void run({ assignees: toggle(assigned, u.username) })}
              >
                <Person user={u} />
              </DropdownMenuCheckboxItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
        {busy && <LoaderCircle className="size-3 animate-spin text-muted-foreground" />}
      </div>
      <span />
      <div>
        <ActingAs accountId={actor} onChange={setActor} disabled={busy} />
      </div>
    </div>
  );
}

function Comments({
  issueRef,
  family,
}: {
  issueRef: HostedItemRef;
  family: ProviderFamily | null;
}) {
  const { t, i18n } = useTranslation();
  const comments = useIssueComments(issueRef);
  const [now] = useState(() => Date.now());
  const refCtx = {
    accountId: issueRef.accountId,
    fullName: issueRef.fullName,
    family,
    repoId: issueRef.repoId ?? null,
  };
  if (comments.isPending) {
    return (
      <p className="flex items-center gap-1.5 text-muted-foreground">
        <LoaderCircle className="size-3 animate-spin" />
        {t("issues.view.loadingComments")}
      </p>
    );
  }
  if (comments.isError) return <p className="text-danger">{t("issues.view.cannotLoad")}</p>;
  const list = comments.data.comments;
  return (
    <div className="space-y-2">
      <h2 className="font-semibold">
        {t("issues.view.comments", { count: list.filter((c) => !c.system).length })}
      </h2>
      {list.map((c) =>
        c.system ? (
          <p key={`c-${c.id}`} className="pl-2 text-[11px] text-muted-foreground">
            <span className="font-medium">{c.author?.username}</span> {c.body}
          </p>
        ) : (
          <article key={`c-${c.id}`} className="rounded-md border">
            <header className="flex items-center gap-2 border-b bg-surface px-3 py-1.5 text-muted-foreground">
              <Person user={c.author} />
              {c.createdAt != null && <span>{relativeTime(c.createdAt, now, i18n.language)}</span>}
            </header>
            <div className="p-3 leading-relaxed">
              <SafeText text={c.body} family={family} onRef={(p) => void openRef(refCtx, p)} />
            </div>
          </article>
        ),
      )}
      {comments.data.truncated && (
        <p className="text-muted-foreground">{t("issues.view.moreComments")}</p>
      )}
    </div>
  );
}

function Composer({ issueRef, detail: d }: { issueRef: HostedItemRef; detail: IssueDetail }) {
  const { t } = useTranslation();
  const [body, setBody] = useState("");
  const [actor, setActor] = useState(issueRef.accountId);
  const [busy, setBusy] = useState<"comment" | "state" | null>(null);
  const open = d.issue.state === "open";

  const comment = async () => {
    if (!body.trim()) return;
    setBusy("comment");
    try {
      await commands.issueCommentCreate(actor, issueRef.fullName, issueRef.number, body);
      setBody("");
      setActor(issueRef.accountId);
      void queryClient.invalidateQueries({ queryKey: issueKeys.comments(issueRef) });
      void queryClient.invalidateQueries({ queryKey: issueKeys.detail(issueRef) });
    } catch (e) {
      useUi.getState().showNotice(issueErrorNotice(e, actor));
    } finally {
      setBusy(null);
    }
  };

  const toggleState = async () => {
    setBusy("state");
    // A typed comment goes in first, as on the website ("Close with comment").
    if (body.trim()) {
      try {
        await commands.issueCommentCreate(actor, issueRef.fullName, issueRef.number, body);
        setBody("");
      } catch (e) {
        useUi.getState().showNotice(issueErrorNotice(e, actor));
        setBusy(null);
        return;
      }
    }
    if (await applyUpdate(issueRef, actor, { state: open ? "closed" : "open" })) {
      setActor(issueRef.accountId);
    }
    setBusy(null);
  };

  return (
    <div className="space-y-2 rounded-md border p-3">
      <textarea
        value={body}
        onChange={(e) => setBody(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            void comment();
          }
        }}
        placeholder={
          d.locked ? t("issues.view.lockedPlaceholder") : t("issues.view.commentPlaceholder")
        }
        aria-label={t("issues.view.commentPlaceholder")}
        rows={4}
        className="w-full resize-y rounded-md border bg-background px-2 py-1.5 text-xs focus:border-primary focus:outline-none"
      />
      <div className="flex flex-wrap items-center gap-2">
        <ActingAs accountId={actor} onChange={setActor} disabled={busy !== null} />
        <span className="flex-1" />
        <Button
          size="sm"
          variant="outline"
          disabled={busy !== null}
          onClick={() => void toggleState()}
        >
          {busy === "state" && <LoaderCircle className="animate-spin" />}
          {open
            ? body.trim()
              ? t("issues.view.closeWithComment")
              : t("issues.view.closeIssue")
            : t("issues.view.reopen")}
        </Button>
        <Button size="sm" disabled={busy !== null || !body.trim()} onClick={() => void comment()}>
          {busy === "comment" && <LoaderCircle className="animate-spin" />}
          {t("issues.view.comment")}
        </Button>
      </div>
    </div>
  );
}

/** "Create a branch for this issue" (`feature/<n>-<slug>`), optionally switching to it. */
function BranchFromIssue({ repoId, detail: d }: { repoId: number; detail: IssueDetail }) {
  const { t } = useTranslation();
  const remotes = useHostedRemotes(repoId);
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  const [checkout, setCheckout] = useState(true);
  const [busy, setBusy] = useState(false);
  if (!remotes.data?.length) return null;

  const start = async () => {
    setOpen(true);
    const suggested = await commands
      .issueBranchName(d.issue.number, d.issue.title)
      .catch(() => `feature/${d.issue.number}`);
    setName(suggested);
  };
  const create = async () => {
    setBusy(true);
    const made = await createIssueBranch(repoId, d.issue.number, d.issue.title, name, checkout);
    setBusy(false);
    if (made) setOpen(false);
  };

  if (!open) {
    return (
      <Button size="sm" variant="outline" onClick={() => void start()}>
        <GitBranchPlus />
        {t("issues.branch.open")}
      </Button>
    );
  }
  return (
    <form
      className="space-y-2 rounded-md border p-3"
      onSubmit={(e) => {
        e.preventDefault();
        void create();
      }}
    >
      <label className="block space-y-1">
        <span className="font-medium">{t("issues.branch.name")}</span>
        <input
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="h-8 w-full rounded-md border bg-background px-2 font-mono text-xs focus:border-primary focus:outline-none"
          spellCheck={false}
        />
      </label>
      <p className="text-[11px] text-muted-foreground">{t("issues.branch.hint")}</p>
      <label className="flex items-center gap-1.5">
        <input
          type="checkbox"
          checked={checkout}
          onChange={(e) => setCheckout(e.target.checked)}
          className="accent-primary"
        />
        {t("issues.branch.checkout")}
      </label>
      <div className="flex justify-end gap-2">
        <Button type="button" size="sm" variant="ghost" onClick={() => setOpen(false)}>
          {t("common.cancel")}
        </Button>
        <Button type="submit" size="sm" disabled={busy || !name.trim()}>
          {busy && <LoaderCircle className="animate-spin" />}
          {t("issues.branch.create")}
        </Button>
      </div>
    </form>
  );
}
