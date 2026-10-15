/** Small building blocks of the pull request views. */
import { Fragment, useMemo, useState } from "react";
import {
  ChevronDown,
  CircleCheck,
  CircleDot,
  CircleX,
  GitMerge,
  GitPullRequestDraft,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { AccountAvatar } from "@/features/accounts/AccountAvatar";
import { openExternal } from "@/features/accounts/manager/openExternal";
import { accountLabel, useSortedAccounts } from "@/features/accounts/switcher";
import type { PullReviewDecision, PullReviewerState, PullState, PullUser } from "@/ipc";
import { cn } from "@/lib/utils";
import { splitLinks } from "./model";

/**
 * Untrusted provider text (description, comment, commit message) as plain text: line breaks
 * kept, `https://` links opened in the browser. Markdown / HTML is never interpreted.
 */
export function PlainText({ text, className }: { text: string; className?: string }) {
  const parts = useMemo(() => splitLinks(text), [text]);
  return (
    <span className={cn("break-words whitespace-pre-wrap", className)}>
      {parts.map((p, i) =>
        p.url ? (
          <a
            key={`u-${i}`}
            href={p.text}
            onClick={(e) => {
              e.preventDefault();
              void openExternal(p.text);
            }}
            className="text-primary underline-offset-2 hover:underline"
          >
            {p.text}
          </a>
        ) : (
          <Fragment key={`t-${i}`}>{p.text}</Fragment>
        ),
      )}
    </span>
  );
}

/** Avatar (https only) + login of a provider user. */
export function UserChip({ user, size = 16 }: { user: PullUser | null; size?: number }) {
  const [failed, setFailed] = useState(false);
  if (!user) return <span className="text-muted-foreground">?</span>;
  const url = user.avatarUrl?.startsWith("https://") ? user.avatarUrl : null;
  return (
    <span className="inline-flex min-w-0 items-center gap-1" title={user.displayName ?? undefined}>
      {url && !failed ? (
        <img
          src={url}
          alt=""
          width={size}
          height={size}
          referrerPolicy="no-referrer"
          onError={() => setFailed(true)}
          className="shrink-0 rounded-full"
        />
      ) : (
        <span
          aria-hidden
          className="inline-flex shrink-0 items-center justify-center rounded-full bg-surface-2 text-[9px] font-semibold"
          style={{ width: size, height: size }}
        >
          {user.username.slice(0, 1).toUpperCase()}
        </span>
      )}
      <span className="truncate font-medium">{user.username}</span>
    </span>
  );
}

/** Open / Draft / Merged / Closed badge. */
export function StateBadge({ state, draft }: { state: PullState; draft: boolean }) {
  const { t } = useTranslation();
  const [Icon, cls, label] =
    state === "merged"
      ? [GitMerge, "bg-purple-500/15 text-purple-600 dark:text-purple-400", t("pulls.state.merged")]
      : state === "closed"
        ? [CircleX, "bg-danger/15 text-danger", t("pulls.state.closed")]
        : draft
          ? [GitPullRequestDraft, "bg-surface-2 text-muted-foreground", t("pulls.state.draft")]
          : [CircleDot, "bg-success/15 text-success", t("pulls.state.open")];
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-1 rounded-full px-1.5 py-0.5 text-[10px] font-medium",
        cls,
      )}
    >
      <Icon className="size-3" />
      {label}
    </span>
  );
}

const DECISION_CLASS: Record<PullReviewDecision, string> = {
  approved: "text-success",
  changesRequested: "text-danger",
  reviewRequired: "text-warning",
  none: "text-muted-foreground",
};

/** Small review decision icon + label (sidebar rows, header). */
export function DecisionMark({
  decision,
  withLabel = false,
}: {
  decision: PullReviewDecision;
  withLabel?: boolean;
}) {
  const { t } = useTranslation();
  if (decision === "none" && !withLabel) return null;
  const Icon =
    decision === "approved" ? CircleCheck : decision === "changesRequested" ? CircleX : CircleDot;
  const label = t(`pulls.decision.${decision}`);
  return (
    <span
      className={cn("inline-flex shrink-0 items-center gap-1", DECISION_CLASS[decision])}
      title={label}
    >
      <Icon className="size-3" aria-label={withLabel ? undefined : label} />
      {withLabel && <span className="text-[11px]">{label}</span>}
    </span>
  );
}

const REVIEWER_CLASS: Record<PullReviewerState, string> = {
  approved: "text-success",
  changesRequested: "text-danger",
  requested: "text-warning",
  commented: "text-muted-foreground",
  dismissed: "text-muted-foreground line-through",
};

export function ReviewerState({ state }: { state: PullReviewerState }) {
  const { t } = useTranslation();
  return (
    <span className={cn("text-[11px]", REVIEWER_CLASS[state])}>{t(`pulls.reviewer.${state}`)}</span>
  );
}

/**
 * "With ● account ▾" shown before every write (PLAN §4.5): the account that will act, which
 * can be changed for this action only among accounts of the same provider that have a token.
 */
export function PullActingAs({
  accountId,
  onChange,
  disabled,
}: {
  accountId: string;
  onChange: (accountId: string) => void;
  disabled?: boolean;
}) {
  const { t } = useTranslation();
  const accounts = useSortedAccounts();
  const current = accounts.find((a) => a.id === accountId) ?? null;
  const choices = accounts.filter(
    (a) => a.secrets.includes("token") && (!current || a.providerId === current.providerId),
  );
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        disabled={disabled || choices.length < 2}
        data-testid="pull-acting-as"
        title={t("pulls.actingAsHint")}
        className="inline-flex max-w-full min-w-0 items-center gap-1 rounded px-1 text-[11px] text-muted-foreground hover:bg-surface-2 hover:text-foreground disabled:hover:bg-transparent"
      >
        <span className="shrink-0">{t("pulls.actingAs")}</span>
        {current && (
          <span
            aria-hidden
            className="size-2 shrink-0 rounded-full"
            style={{ background: current.color }}
          />
        )}
        <span className="truncate font-medium text-foreground">
          {current ? accountLabel(current) : accountId}
        </span>
        {choices.length >= 2 && <ChevronDown className="size-3 shrink-0" />}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-60 text-xs">
        <DropdownMenuLabel className="text-[11px] text-muted-foreground">
          {t("pulls.actingAsOnce")}
        </DropdownMenuLabel>
        {choices.map((a) => (
          <DropdownMenuItem key={a.id} onSelect={() => onChange(a.id)}>
            <AccountAvatar account={a} size={16} />
            <span className="truncate">{accountLabel(a)}</span>
            {a.id === accountId && <span className="ml-auto text-muted-foreground">✓</span>}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
