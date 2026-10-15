import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { FileCode2, LoaderCircle, Minus, Plus, Undo2, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group";
import { splitPath } from "@/features/commit/format";
import { runOp, worktreeKeys } from "@/features/worktree/queries";
import {
  commands,
  errorMessage,
  type DiffRequest,
  type FileDiff,
  type LineSelection,
  type PatchAction,
} from "@/ipc";
import { useWorktreeStore, type DiffMode } from "@/stores/worktree";
import { HunkList } from "./HunkList";
import { ImageDiff } from "./ImageDiff";
import { isImagePath } from "./imageModes";
import { countSelected, EMPTY_SEL, toSelections, type LineSel } from "./selection";

const MonacoDiff = lazy(() => import("./MonacoDiff"));

interface PendingDiscard {
  kind: "file" | "lines";
  selection: LineSelection[];
}

interface DiffViewProps {
  repoId: number;
  request: DiffRequest;
  /** Embedded use (file history): replaces closing the repository's open diff. */
  onClose?: () => void;
}

export function DiffView({ repoId, request, onClose }: DiffViewProps) {
  const { t } = useTranslation();
  const mode = useWorktreeStore((s) => s.diffMode);
  const setMode = useWorktreeStore((s) => s.setDiffMode);
  const ignoreWs = useWorktreeStore((s) => s.ignoreWhitespace);
  const setIgnoreWs = useWorktreeStore((s) => s.setIgnoreWhitespace);
  const openDiff = useWorktreeStore((s) => s.openDiff);
  const root = useRef<HTMLDivElement>(null);
  const [sel, setSel] = useState<{ fp: string | null; lines: LineSel }>({
    fp: null,
    lines: EMPTY_SEL,
  });
  const [pending, setPending] = useState<PendingDiscard | null>(null);
  const [busy, setBusy] = useState(false);
  // Images open as pictures; the text views stay one click away (SVG is text too).
  const image = isImagePath(request.path);
  const [textView, setTextView] = useState(false);
  const showImage = image && !textView;

  const diff = useQuery({
    queryKey: worktreeKeys.diff(repoId, request),
    queryFn: () => commands.fileDiff(repoId, request),
    enabled: isTauri() && mode === "hunk" && !showImage,
  });
  const contents = useQuery({
    queryKey: worktreeKeys.contents(repoId, request),
    queryFn: () => commands.fileContents(repoId, request),
    enabled: isTauri() && mode !== "hunk" && !showImage,
  });

  const d = diff.data;
  // A new version of the diff invalidates any selection made on the previous one.
  const lines = d && sel.fp === d.fingerprint ? sel.lines : EMPTY_SEL;
  const setLines = (next: LineSel) => setSel({ fp: d?.fingerprint ?? null, lines: next });
  const selectedCount = countSelected(lines);

  useEffect(() => {
    root.current?.focus({ preventScroll: true });
  }, [request]);

  const close = onClose ?? (() => openDiff(repoId, null));
  const { dir, name } = splitPath(request.path);
  const side = request.side;
  const editable = side === "unstaged" || side === "staged";
  const canLines = editable && !!d?.partial;

  const applyLines = async (action: PatchAction, selection: LineSelection[]) => {
    if (!d || selection.length === 0) return;
    setBusy(true);
    await runOp(repoId, () =>
      commands.applyLines(repoId, action, request.path, d.fingerprint, selection),
    );
    setLines(EMPTY_SEL);
    setBusy(false);
  };

  const stageFile = async () => {
    await runOp(repoId, () => commands.stageFiles(repoId, [request.path]));
    openDiff(repoId, { side: "staged", path: request.path });
  };
  const unstageFile = async () => {
    const paths = request.oldPath ? [request.oldPath, request.path] : [request.path];
    await runOp(repoId, () => commands.unstageFiles(repoId, paths));
    openDiff(repoId, { side: "unstaged", path: request.path });
  };
  const discard = async () => {
    const p = pending;
    setPending(null);
    if (!p) return;
    if (p.kind === "lines") {
      await applyLines("discard", p.selection);
      return;
    }
    const untracked = side === "untracked";
    await runOp(repoId, () =>
      commands.discardFiles(
        repoId,
        untracked ? [] : [request.path],
        untracked ? [request.path] : [],
      ),
    );
    close();
  };

  return (
    <div
      ref={root}
      tabIndex={-1}
      onKeyDown={(e) => {
        if (e.key === "Escape" && !e.defaultPrevented) {
          if (selectedCount) setLines(EMPTY_SEL);
          else close();
        }
      }}
      className="flex min-w-0 flex-1 flex-col bg-background outline-none"
      aria-label={t("diff.title")}
    >
      <div className="flex h-9 shrink-0 items-center gap-2 border-b px-3 text-xs">
        <FileCode2 className="size-4 shrink-0 text-muted-foreground" />
        <span className="min-w-24 flex-1 truncate font-medium" title={request.path}>
          <span className="text-muted-foreground">{dir}</span>
          {name}
        </span>
        <span className="shrink-0 rounded bg-surface-2 px-1.5 py-0.5 text-[11px] text-muted-foreground">
          {side === "commit"
            ? t("diff.commit", { oid: (request.commit ?? "").slice(0, 7) })
            : t(`diff.${side}`)}
        </span>
        {d && (
          <span className="shrink-0 font-mono text-[11px] tabular-nums">
            <span className="text-success">+{d.additions}</span>{" "}
            <span className="text-danger">−{d.deletions}</span>
          </span>
        )}
        {(side === "unstaged" || side === "untracked") && (
          <>
            <Button
              variant="outline"
              size="xs"
              onClick={() => setPending({ kind: "file", selection: [] })}
            >
              <Undo2 />
              {t("diff.discardFile")}
            </Button>
            <Button size="xs" onClick={() => void stageFile()}>
              <Plus />
              {t("diff.stageFile")}
            </Button>
          </>
        )}
        {side === "staged" && (
          <Button variant="outline" size="xs" onClick={() => void unstageFile()}>
            <Minus />
            {t("diff.unstageFile")}
          </Button>
        )}
        <ToggleGroup
          type="single"
          size="sm"
          variant="outline"
          value={showImage ? "image" : mode}
          onValueChange={(v) => {
            if (!v) return;
            setTextView(v !== "image");
            if (v !== "image") setMode(v as DiffMode);
          }}
          className="shrink-0"
        >
          {image && (
            <ToggleGroupItem value="image" className="h-6 px-2 text-[11px]">
              {t("imageDiff.modeImage")}
            </ToggleGroupItem>
          )}
          <ToggleGroupItem value="hunk" className="h-6 px-2 text-[11px]">
            {t("diff.modeHunk")}
          </ToggleGroupItem>
          <ToggleGroupItem value="inline" className="h-6 px-2 text-[11px]">
            {t("diff.modeInline")}
          </ToggleGroupItem>
          <ToggleGroupItem value="split" className="h-6 px-2 text-[11px]">
            {t("diff.modeSplit")}
          </ToggleGroupItem>
        </ToggleGroup>
        {mode !== "hunk" && !showImage && (
          <label className="flex shrink-0 items-center gap-1 text-[11px] text-muted-foreground">
            <input
              type="checkbox"
              checked={ignoreWs}
              onChange={(e) => setIgnoreWs(e.target.checked)}
              className="accent-primary"
            />
            {t("diff.ignoreWs")}
          </label>
        )}
        <button
          type="button"
          onClick={close}
          title={t("diff.close")}
          aria-label={t("diff.close")}
          className="rounded p-0.5 text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <X className="size-4" />
        </button>
      </div>

      {showImage ? (
        <ImageDiff repoId={repoId} request={request} />
      ) : mode === "hunk" ? (
        <HunkBody
          query={diff}
          canLines={canLines}
          lines={lines}
          onLines={setLines}
          side={side}
          onHunk={(action, selection) =>
            action === "discard"
              ? setPending({ kind: "lines", selection })
              : void applyLines(action, selection)
          }
        />
      ) : (
        <EditorBody
          query={contents}
          path={request.path}
          split={mode === "split"}
          ignoreWs={ignoreWs}
        />
      )}

      {mode === "hunk" && !showImage && canLines && selectedCount > 0 && (
        <div className="flex shrink-0 items-center gap-2 border-t bg-surface px-3 py-2 text-xs">
          <span className="text-muted-foreground">
            {t("diff.linesSelected", { count: selectedCount })}
          </span>
          <span className="flex-1" />
          <Button variant="ghost" size="xs" onClick={() => setLines(EMPTY_SEL)}>
            {t("diff.clearSelection")}
          </Button>
          {side === "unstaged" ? (
            <>
              <Button
                variant="outline"
                size="xs"
                disabled={busy}
                onClick={() => setPending({ kind: "lines", selection: toSelections(lines) })}
              >
                <Undo2 />
                {t("diff.discardLines", { count: selectedCount })}
              </Button>
              <Button
                size="xs"
                disabled={busy}
                onClick={() => void applyLines("stage", toSelections(lines))}
              >
                <Plus />
                {t("diff.stageLines", { count: selectedCount })}
              </Button>
            </>
          ) : (
            <Button
              size="xs"
              disabled={busy}
              onClick={() => void applyLines("unstage", toSelections(lines))}
            >
              <Minus />
              {t("diff.unstageLines", { count: selectedCount })}
            </Button>
          )}
        </div>
      )}

      <ConfirmDialog
        open={pending !== null}
        title={pending?.kind === "lines" ? t("diff.discardLinesTitle") : t("wip.discardTitle")}
        description={
          pending?.kind === "lines"
            ? t("diff.discardLinesBody")
            : t("wip.discardBody", { count: 1 })
        }
        confirmLabel={t("wip.discardConfirm")}
        destructive
        onConfirm={() => void discard()}
        onCancel={() => setPending(null)}
      />
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

function Loading() {
  const { t } = useTranslation();
  return (
    <Centered>
      <LoaderCircle className="mr-2 size-4 animate-spin" />
      {t("diff.loading")}
    </Centered>
  );
}

interface HunkBodyProps {
  query: { data?: FileDiff; isPending: boolean; isError: boolean; error: unknown };
  canLines: boolean;
  lines: LineSel;
  onLines: (sel: LineSel) => void;
  side: DiffRequest["side"];
  onHunk: (action: PatchAction, selection: LineSelection[]) => void;
}

function HunkBody({ query, canLines, lines, onLines, side, onHunk }: HunkBodyProps) {
  const { t } = useTranslation();
  const d = query.data;
  if (query.isError)
    return <Centered>{t("diff.error", { message: errorMessage(query.error) })}</Centered>;
  if (!d) return <Loading />;
  const notes: string[] = [];
  if (d.newFile) notes.push(t("diff.newFile"));
  if (d.deletedFile) notes.push(t("diff.deletedFile"));
  if (d.oldPath && d.oldPath !== d.path) notes.push(t("diff.renamed", { path: d.oldPath }));
  if (d.oldMode && d.newMode) notes.push(t("diff.modeChange", { from: d.oldMode, to: d.newMode }));
  const editable = side === "unstaged" || side === "staged";
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {(notes.length > 0 || (editable && !d.partial && d.hunks.length > 0)) && (
        <div className="shrink-0 border-b bg-surface px-3 py-1.5 text-[11px] text-muted-foreground">
          {notes.join(" · ")}
          {editable && !d.partial && d.hunks.length > 0 && (
            <span className="block">{t("diff.wholeFileOnly")}</span>
          )}
        </div>
      )}
      {d.binary ? (
        <Centered>{t("diff.binary")}</Centered>
      ) : d.tooLarge ? (
        <Centered>{t("diff.tooLarge")}</Centered>
      ) : d.hunks.length === 0 ? (
        <Centered>{t("diff.noChanges")}</Centered>
      ) : (
        <HunkList
          hunks={d.hunks}
          selectable={canLines}
          selection={lines}
          onSelection={onLines}
          side={side}
          onHunk={onHunk}
        />
      )}
    </div>
  );
}

interface EditorBodyProps {
  query: {
    data?: { original: string | null; modified: string | null; binary: boolean; tooLarge: boolean };
    isError: boolean;
    error: unknown;
  };
  path: string;
  split: boolean;
  ignoreWs: boolean;
}

function EditorBody({ query, path, split, ignoreWs }: EditorBodyProps) {
  const { t } = useTranslation();
  const c = query.data;
  if (query.isError)
    return <Centered>{t("diff.error", { message: errorMessage(query.error) })}</Centered>;
  if (!c) return <Loading />;
  if (c.binary) return <Centered>{t("diff.binary")}</Centered>;
  if (c.tooLarge) return <Centered>{t("diff.tooLarge")}</Centered>;
  return (
    <Suspense fallback={<Loading />}>
      <MonacoDiff
        path={path}
        original={c.original ?? ""}
        modified={c.modified ?? ""}
        sideBySide={split}
        ignoreWhitespace={ignoreWs}
      />
    </Suspense>
  );
}
