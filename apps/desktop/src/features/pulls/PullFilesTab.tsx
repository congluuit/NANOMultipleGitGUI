import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { FileDiff, LoaderCircle, MessageSquare, MessageSquarePlus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { failureNotice } from "@/features/commits/outcome";
import {
  commands,
  type ProviderFamily,
  type PullComment,
  type PullDetail,
  type PullFile,
} from "@/ipc";
import { cn } from "@/lib/utils";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import { postComment } from "./actions";
import { Composer } from "./Composer";
import { describePullError } from "./errors";
import {
  lineKey,
  lineThreads,
  parsePatch,
  relativeTime,
  rowAnchor,
  threadCountByPath,
  threadsOfFile,
  type PatchRow,
  type Thread,
} from "./model";
import { PlainText, UserChip } from "./parts";
import { usePullComments, usePullFiles } from "./queries";

/** Rendering every line of a huge diff as DOM would freeze the view. */
const MAX_ROWS = 5000;

type Item = { accountId: string; fullName: string; number: number };

const STATUS_LETTER: Record<PullFile["status"], [string, string]> = {
  added: ["A", "text-success"],
  modified: ["M", "text-warning"],
  deleted: ["D", "text-danger"],
  renamed: ["R", "text-info"],
  copied: ["C", "text-info"],
  changed: ["M", "text-warning"],
};

function CommentLine({ c }: { c: PullComment }) {
  const language = useSettings((s) => s.language);
  return (
    <div className="flex flex-col gap-0.5 py-1">
      <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
        <UserChip user={c.author} />
        <span>{relativeTime(c.createdAt, language)}</span>
      </div>
      <PlainText text={c.body} className="text-xs" />
    </div>
  );
}

export function ThreadBox({
  thread,
  item,
  actor,
  onActor,
  canWrite,
}: {
  thread: Thread;
  item: Item;
  actor: string;
  onActor: (id: string) => void;
  canWrite: boolean;
}) {
  const { t } = useTranslation();
  const [replying, setReplying] = useState(false);
  return (
    <div className="my-1 ml-24 max-w-3xl rounded-md border bg-surface px-3 py-1 font-sans">
      {thread.resolved && (
        <div className="text-[10px] text-success">{t("pulls.files.resolved")}</div>
      )}
      {thread.comments.map((c) => (
        <CommentLine key={c.id} c={c} />
      ))}
      {canWrite &&
        (replying ? (
          <div className="pb-1">
            <Composer
              actor={actor}
              onActor={onActor}
              autoFocus
              placeholder={t("pulls.files.replyPlaceholder")}
              submitLabel={t("pulls.files.reply")}
              onCancel={() => setReplying(false)}
              onSubmit={async (text) => {
                const ok = await postComment(
                  item,
                  actor,
                  { kind: "reply", threadId: thread.id },
                  text,
                );
                if (ok) setReplying(false);
                return ok;
              }}
            />
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setReplying(true)}
            className="pb-1 text-[11px] text-primary hover:underline"
          >
            {t("pulls.files.reply")}
          </button>
        ))}
    </div>
  );
}

const ROW_CLASS: Record<PatchRow["kind"], string> = {
  add: "bg-success/10",
  del: "bg-danger/10",
  ctx: "",
  hunk: "bg-surface-2 text-muted-foreground",
  meta: "text-muted-foreground italic",
};

/** Unified diff with line numbers, line threads under their line and "comment on line". */
export function PatchView({
  patch,
  file,
  threads,
  item,
  actor,
  onActor,
  canWrite,
}: {
  patch: string;
  file: PullFile;
  threads: readonly Thread[];
  item: Item;
  actor: string;
  onActor: (id: string) => void;
  canWrite: boolean;
}) {
  const { t } = useTranslation();
  const rows = useMemo(() => parsePatch(patch), [patch]);
  const { byLine, outdated } = useMemo(() => threadsOfFile(threads, file.path), [threads, file]);
  const [composing, setComposing] = useState<string | null>(null);
  const shown = rows.length > MAX_ROWS ? rows.slice(0, MAX_ROWS) : rows;

  return (
    <div className="min-h-0 flex-1 overflow-auto font-mono text-[12px] leading-[18px]">
      {outdated.length > 0 && (
        <div className="border-b p-2 font-sans">
          <div className="mb-1 text-[11px] font-medium text-muted-foreground">
            {t("pulls.files.outdated", { count: outdated.length })}
          </div>
          {outdated.map((th) => (
            <ThreadBox
              key={th.id}
              thread={th}
              item={item}
              actor={actor}
              onActor={onActor}
              canWrite={canWrite}
            />
          ))}
        </div>
      )}
      <div className="min-w-max">
        {shown.map((row, i) => {
          if (row.kind === "hunk" || row.kind === "meta") {
            return (
              <div key={`r-${i}`} className={cn("px-3", ROW_CLASS[row.kind])}>
                {row.text}
              </div>
            );
          }
          const anchor = rowAnchor(row);
          const key = anchor ? lineKey(anchor.side, anchor.line) : null;
          const lineThreadsHere = key ? (byLine.get(key) ?? []) : [];
          return (
            <div key={`r-${i}`}>
              <div className={cn("group flex", ROW_CLASS[row.kind])}>
                <span className="w-12 shrink-0 pr-2 text-right text-muted-foreground select-none">
                  {row.oldLine ?? ""}
                </span>
                <span className="w-12 shrink-0 pr-2 text-right text-muted-foreground select-none">
                  {row.newLine ?? ""}
                </span>
                <span className="relative w-5 shrink-0 text-center text-muted-foreground select-none">
                  {canWrite && key ? (
                    <button
                      type="button"
                      aria-label={t("pulls.files.commentLine")}
                      title={t("pulls.files.commentLine")}
                      onClick={() => setComposing(composing === key ? null : key)}
                      className="invisible absolute inset-0 flex items-center justify-center rounded bg-primary text-primary-foreground group-hover:visible"
                    >
                      <MessageSquarePlus className="size-3" />
                    </button>
                  ) : null}
                  {row.kind === "add" ? "+" : row.kind === "del" ? "−" : " "}
                </span>
                <span className="pr-4 whitespace-pre">{row.text}</span>
              </div>
              {lineThreadsHere.map((th) => (
                <ThreadBox
                  key={th.id}
                  thread={th}
                  item={item}
                  actor={actor}
                  onActor={onActor}
                  canWrite={canWrite}
                />
              ))}
              {composing === key && anchor && (
                <div className="my-1 ml-24 max-w-3xl font-sans">
                  <Composer
                    actor={actor}
                    onActor={onActor}
                    autoFocus
                    placeholder={t("pulls.files.commentPlaceholder")}
                    submitLabel={t("pulls.files.comment")}
                    onCancel={() => setComposing(null)}
                    onSubmit={async (text) => {
                      const ok = await postComment(
                        item,
                        actor,
                        {
                          kind: "line",
                          path: file.path,
                          oldPath: file.oldPath,
                          line: anchor.line,
                          side: anchor.side,
                          pairedLine: anchor.pairedLine,
                        },
                        text,
                      );
                      if (ok) setComposing(null);
                      return ok;
                    }}
                  />
                </div>
              )}
            </div>
          );
        })}
      </div>
      {rows.length > MAX_ROWS && (
        <p className="p-3 font-sans text-xs text-muted-foreground">
          {t("pulls.files.truncatedRows", { count: MAX_ROWS })}
        </p>
      )}
    </div>
  );
}

/** Diff of a file the provider sent without a patch: computed with local git on request. */
function LocalDiff({
  repoId,
  detail,
  family,
  file,
  ...rest
}: {
  repoId: number;
  detail: PullDetail;
  family: ProviderFamily;
  file: PullFile;
  threads: readonly Thread[];
  item: Item;
  actor: string;
  onActor: (id: string) => void;
  canWrite: boolean;
}) {
  const { t } = useTranslation();
  const [asked, setAsked] = useState(false);
  const q = useQuery({
    queryKey: ["pulls", "localDiff", repoId, detail.headSha, detail.baseSha, file.path],
    queryFn: async () => {
      const out = await commands.pullLocalDiff(repoId, {
        fullName: detail.targetRepo,
        number: detail.number,
        family,
        baseSha: detail.baseSha ?? "",
        headSha: detail.headSha,
        targetBranch: detail.targetBranch,
        path: file.path,
        oldPath: file.oldPath,
      });
      if (out.status === "rejected") {
        useUi.getState().showNotice(failureNotice(out.reason, out.message, repoId));
        return null;
      }
      return out.status === "done" ? out.value : null;
    },
    enabled: asked && !!detail.baseSha,
    staleTime: Infinity,
    retry: false,
  });
  if (!asked || !detail.baseSha) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-xs text-muted-foreground">
        <p>{t("pulls.files.noPatch")}</p>
        {detail.baseSha && (
          <Button size="sm" variant="outline" onClick={() => setAsked(true)}>
            {t("pulls.files.localDiff")}
          </Button>
        )}
      </div>
    );
  }
  if (q.isPending) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground">
        <LoaderCircle className="size-4 animate-spin" />
        {t("pulls.files.localDiffLoading")}
      </div>
    );
  }
  if (q.isError) {
    return <p className="p-4 text-xs text-danger">{describePullError(q.error).message}</p>;
  }
  const d = q.data;
  if (!d?.patch) {
    return (
      <p className="p-4 text-xs text-muted-foreground">
        {d?.binary
          ? t("pulls.files.binary")
          : d?.tooLarge
            ? t("pulls.files.tooLarge")
            : t("pulls.files.noChanges")}
      </p>
    );
  }
  return <PatchView patch={d.patch} file={file} {...rest} />;
}

/** "Files" tab: changed files and the diff of the selected one with its line comments. */
export function PullFilesTab({
  item,
  detail,
  family,
  repoId,
  actor,
  onActor,
  selected,
  onSelect,
}: {
  item: Item;
  detail: PullDetail;
  family: ProviderFamily | null;
  repoId: number | null;
  actor: string;
  onActor: (id: string) => void;
  selected: string | null;
  onSelect: (path: string) => void;
}) {
  const { t } = useTranslation();
  const files = usePullFiles(item);
  const comments = usePullComments(item);
  const threads = useMemo(() => lineThreads(comments.data ?? []), [comments.data]);
  const counts = useMemo(() => threadCountByPath(threads), [threads]);
  const list = files.data?.files ?? [];
  const file = list.find((f) => f.path === selected) ?? list[0] ?? null;
  const canWrite = detail.state === "open";

  if (files.isPending) {
    return (
      <div className="flex flex-1 items-center justify-center gap-2 text-xs text-muted-foreground">
        <LoaderCircle className="size-4 animate-spin" />
        {t("pulls.loading")}
      </div>
    );
  }
  if (files.isError) {
    return <p className="p-4 text-xs text-danger">{describePullError(files.error).message}</p>;
  }
  return (
    <div className="flex min-h-0 flex-1">
      <ul
        className="w-72 shrink-0 overflow-y-auto border-r py-1 text-xs"
        aria-label={t("pulls.tab.files")}
      >
        {files.data?.truncated && (
          <li className="px-3 py-1 text-[11px] text-warning">{t("pulls.files.truncated")}</li>
        )}
        {list.map((f) => {
          const [letter, cls] = STATUS_LETTER[f.status];
          const n = counts.get(f.path) ?? 0;
          return (
            <li key={`file-${f.path}`}>
              <button
                type="button"
                onClick={() => onSelect(f.path)}
                title={f.oldPath ? `${f.oldPath} → ${f.path}` : f.path}
                className={cn(
                  "flex h-6 w-full items-center gap-2 px-3 text-left hover:bg-surface-2",
                  file?.path === f.path && "bg-surface-2 font-medium",
                )}
              >
                <span className={cn("w-3 shrink-0 font-mono font-semibold", cls)}>{letter}</span>
                <span className="min-w-0 flex-1 truncate" dir="rtl">
                  {f.path}
                </span>
                {n > 0 && (
                  <span className="flex shrink-0 items-center gap-0.5 text-muted-foreground">
                    <MessageSquare className="size-3" />
                    {n}
                  </span>
                )}
                <span className="shrink-0 text-success">+{f.additions}</span>
                <span className="shrink-0 text-danger">−{f.deletions}</span>
              </button>
            </li>
          );
        })}
      </ul>
      <div className="flex min-w-0 flex-1 flex-col">
        {file ? (
          <>
            <div className="flex h-8 shrink-0 items-center gap-2 border-b px-3 text-xs">
              <FileDiff className="size-3.5 text-muted-foreground" />
              <span className="truncate font-medium">{file.path}</span>
              {file.oldPath && (
                <span className="truncate text-muted-foreground">← {file.oldPath}</span>
              )}
              {canWrite && file.patch && (
                <span className="ml-auto shrink-0 text-[11px] text-muted-foreground">
                  {t("pulls.files.commentHint")}
                </span>
              )}
            </div>
            {file.patch ? (
              <PatchView
                key={`patch-${file.path}`}
                patch={file.patch}
                file={file}
                threads={threads}
                item={item}
                actor={actor}
                onActor={onActor}
                canWrite={canWrite}
              />
            ) : repoId !== null && family ? (
              <LocalDiff
                key={`local-${file.path}`}
                repoId={repoId}
                detail={detail}
                family={family}
                file={file}
                threads={threads}
                item={item}
                actor={actor}
                onActor={onActor}
                canWrite={false}
              />
            ) : (
              <p className="p-4 text-xs text-muted-foreground">{t("pulls.files.noPatchNoRepo")}</p>
            )}
          </>
        ) : (
          <p className="p-4 text-xs text-muted-foreground">{t("pulls.files.none")}</p>
        )}
      </div>
    </div>
  );
}
