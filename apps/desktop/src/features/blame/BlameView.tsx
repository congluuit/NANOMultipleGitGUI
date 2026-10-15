import { useLayoutEffect, useMemo, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { ArrowLeft, History, LoaderCircle, Undo2, UserRound, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { splitPath } from "@/features/commit/format";
import { formatDate } from "@/features/graph/draw";
import { commands, errorMessage, type BlameCommit, type BlameResult } from "@/ipc";
import { closeFileView, jumpToCommit, openFileView } from "@/stores/fileView";
import {
  ageBucket,
  ageOpacity,
  groupIndex,
  groupLines,
  relativeTime,
  timeRange,
  visibleRange,
} from "./blame";

const ROW = 18;
const GUTTER = 300;
const CHAR = 7.3;

interface Target {
  path: string;
  /** Hex id or `HEAD`; `null` = working tree. */
  rev: string | null;
}

const blameKeys = {
  blame: (repoId: number, t: Target) => ["blame", repoId, t.path, t.rev] as const,
};

/** Read-only blame of a file: who last changed each line, grouped by commit. */
export function BlameView({ repoId, path, rev }: { repoId: number } & Target) {
  const { t } = useTranslation();
  const [target, setTarget] = useState<Target>({ path, rev });
  const [back, setBack] = useState<Target[]>([]);
  const blame = useQuery({
    queryKey: blameKeys.blame(repoId, target),
    queryFn: () => commands.blameFile(repoId, target.path, target.rev),
    enabled: isTauri(),
    staleTime: target.rev === null ? 0 : Infinity,
  });
  const go = (next: Target) => {
    setBack((b) => [...b, target]);
    setTarget(next);
  };
  const { dir, name } = splitPath(target.path);
  const isCommit = target.rev !== null && target.rev !== "HEAD";

  return (
    <div
      className="flex min-w-0 flex-1 flex-col bg-background"
      aria-label={t("blame.title", { path: target.path })}
      data-testid="blame-view"
    >
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs">
        {back.length > 0 && (
          <Button
            variant="ghost"
            size="icon-xs"
            title={t("blame.back")}
            aria-label={t("blame.back")}
            onClick={() => {
              const prev = back[back.length - 1]!;
              setBack(back.slice(0, -1));
              setTarget(prev);
            }}
          >
            <ArrowLeft />
          </Button>
        )}
        <UserRound className="size-4 shrink-0 text-muted-foreground" />
        <span className="min-w-24 flex-1 truncate font-medium" title={target.path}>
          <span className="mr-1 text-muted-foreground">{t("blame.label")}</span>
          <span className="text-muted-foreground">{dir}</span>
          {name}
        </span>
        <span className="shrink-0 text-[11px] text-muted-foreground">{t("blame.revLabel")}</span>
        <ToggleGroup
          type="single"
          size="sm"
          variant="outline"
          value={target.rev === null ? "worktree" : target.rev === "HEAD" ? "head" : "commit"}
          onValueChange={(v) => {
            if (v === "worktree") go({ path: target.path, rev: null });
            else if (v === "head") go({ path: target.path, rev: "HEAD" });
          }}
        >
          <ToggleGroupItem value="worktree" className="h-6 px-2 text-[11px]">
            {t("blame.atWorktree")}
          </ToggleGroupItem>
          <ToggleGroupItem value="head" className="h-6 px-2 text-[11px]">
            HEAD
          </ToggleGroupItem>
          {isCommit && (
            <ToggleGroupItem value="commit" className="h-6 px-2 font-mono text-[11px]">
              {target.rev!.slice(0, 7)}
            </ToggleGroupItem>
          )}
        </ToggleGroup>
        <Button
          variant="outline"
          size="xs"
          onClick={() => openFileView(repoId, { kind: "history", path: target.path })}
        >
          <History />
          {t("blame.history")}
        </Button>
        <button
          type="button"
          onClick={() => closeFileView(repoId)}
          title={t("blame.close")}
          aria-label={t("blame.close")}
          className="rounded p-0.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <X className="size-4" />
        </button>
      </div>
      {blame.isError ? (
        <Centered>{t("blame.error", { message: errorMessage(blame.error) })}</Centered>
      ) : !blame.data ? (
        <Centered>
          <LoaderCircle className="mr-2 size-4 animate-spin" />
          {t("blame.loading")}
        </Centered>
      ) : blame.data.tooLarge ? (
        <Centered>{t("blame.tooLarge")}</Centered>
      ) : blame.data.binary ? (
        <Centered>{t("blame.binary")}</Centered>
      ) : blame.data.lines.length === 0 ? (
        <Centered>{t("blame.empty")}</Centered>
      ) : (
        <BlameLines
          repoId={repoId}
          data={blame.data}
          onParent={(c) =>
            c.previous && go({ path: c.previousPath ?? target.path, rev: c.previous })
          }
        />
      )}
    </div>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex flex-1 items-center justify-center p-6 text-center text-xs text-muted-foreground">
      {children}
    </div>
  );
}

function BlameLines({
  repoId,
  data,
  onParent,
}: {
  repoId: number;
  data: BlameResult;
  onParent: (c: BlameCommit) => void;
}) {
  const { t, i18n } = useTranslation();
  const scroller = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [height, setHeight] = useState(800);
  const groups = useMemo(() => groupLines(data.lineCommits), [data]);
  const byLine = useMemo(() => groupIndex(groups, data.lines.length), [groups, data]);
  const range = useMemo(() => timeRange(data.commits), [data]);
  const width = useMemo(
    () => GUTTER + 56 + data.lines.reduce((m, l) => Math.max(m, l.length), 10) * CHAR + 24,
    [data],
  );
  const [now] = useState(() => Math.floor(Date.now() / 1000));

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    setHeight(el.clientHeight || 800);
    const ro = new ResizeObserver(() => setHeight(el.clientHeight || 800));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const { first, last } = visibleRange(scrollTop, height, ROW, data.lines.length);
  const rows = [];
  for (let i = first; i < last; i++) {
    const g = groups[byLine[i]!]!;
    const c = data.commits[g.commit]!;
    const head = g.start === i;
    const bucket = c.uncommitted ? 0 : ageBucket(c.time, range.newest, range.oldest);
    const tip = c.uncommitted
      ? t("blame.uncommitted")
      : t("blame.tooltip", {
          oid: c.oid.slice(0, 10),
          author: c.author,
          date: formatDate(c.time, i18n.language),
          summary: c.summary,
        });
    rows.push(
      <div
        key={i}
        className="group absolute right-0 left-0 flex font-mono text-xs leading-[18px]"
        style={{ top: i * ROW, height: ROW }}
        data-line={i + 1}
      >
        <div
          className={`relative flex shrink-0 items-center gap-2 pr-2 pl-3 text-[11px] ${
            head ? "border-t border-border/60" : ""
          } ${c.uncommitted ? "" : "cursor-pointer hover:bg-surface-2"}`}
          style={{ width: GUTTER }}
          title={tip}
          onClick={() => !c.uncommitted && jumpToCommit(repoId, c.oid)}
        >
          <span
            className={`absolute inset-y-0 left-0 w-[3px] ${c.uncommitted ? "bg-warning" : "bg-primary"}`}
            style={{ opacity: c.uncommitted ? 1 : ageOpacity(bucket) }}
            aria-hidden
          />
          {head && (
            <>
              <span className="w-14 shrink-0 text-primary">
                {c.uncommitted ? "·······" : c.oid.slice(0, 7)}
              </span>
              <span className="min-w-0 flex-1 truncate font-sans">
                {c.uncommitted ? t("blame.uncommitted") : c.author}
              </span>
              {!c.uncommitted && (
                <span className="shrink-0 font-sans text-muted-foreground">
                  {relativeTime(c.time, now, i18n.language)}
                </span>
              )}
              {c.previous && (
                <button
                  type="button"
                  className="hidden shrink-0 rounded p-0.5 text-muted-foreground group-hover:block hover:bg-background hover:text-foreground"
                  title={t("blame.parentTitle", { oid: c.oid.slice(0, 7) })}
                  aria-label={t("blame.parent")}
                  onClick={(e) => {
                    e.stopPropagation();
                    onParent(c);
                  }}
                >
                  <Undo2 className="size-3" />
                </button>
              )}
            </>
          )}
        </div>
        <span className="w-14 shrink-0 pr-3 text-right text-muted-foreground select-none">
          {i + 1}
        </span>
        <span className="pr-6 whitespace-pre">{data.lines[i]}</span>
      </div>,
    );
  }

  return (
    <div
      ref={scroller}
      className="min-h-0 flex-1 overflow-auto"
      onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}
      data-testid="blame-lines"
    >
      <div className="relative" style={{ height: data.lines.length * ROW, minWidth: width }}>
        {rows}
      </div>
    </div>
  );
}
