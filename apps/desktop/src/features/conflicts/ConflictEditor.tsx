import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronUp, Save } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import { applyMonacoTheme, languageFor, monaco } from "@/features/diff/monaco";
import type { ConflictFile } from "@/ipc";
import { useUi } from "@/stores/ui";
import { saveResolution } from "./actions";
import type { SideLabels } from "./labels";
import {
  blockAt,
  blockEdit,
  locate,
  parseLines,
  replacement,
  stepBlock,
  type BlockChoice,
  type ConflictBlock,
} from "./markers";

const CSS = `
.nanomgg-cf-marker { background: color-mix(in srgb, var(--text-muted) 18%, transparent); }
.nanomgg-cf-ours { background: color-mix(in srgb, var(--success) 14%, transparent); }
.nanomgg-cf-base { background: color-mix(in srgb, var(--warning) 12%, transparent); }
.nanomgg-cf-theirs { background: color-mix(in srgb, var(--info) 14%, transparent); }
.nanomgg-cf-current { box-shadow: inset 3px 0 0 var(--primary); }
`;

const EDITOR_OPTIONS: monaco.editor.IStandaloneEditorConstructionOptions = {
  automaticLayout: true,
  minimap: { enabled: false },
  scrollBeyondLastLine: false,
  fontSize: 12,
  lineHeight: 18,
  contextmenu: false,
};

function decorations(blocks: ConflictBlock[], current: number) {
  const out: monaco.editor.IModelDeltaDecoration[] = [];
  const range = (from: number, to: number, className: string) => {
    if (to < from) return;
    out.push({
      range: new monaco.Range(from + 1, 1, to + 1, 1),
      options: { isWholeLine: true, className },
    });
  };
  blocks.forEach((b, i) => {
    const oursEnd = (b.baseAt ?? b.mid) - 1;
    for (const line of [b.start, b.mid, b.end, ...(b.baseAt === null ? [] : [b.baseAt])]) {
      range(line, line, "nanomgg-cf-marker");
    }
    range(b.start + 1, oursEnd, "nanomgg-cf-ours");
    if (b.baseAt !== null) range(b.baseAt + 1, b.mid - 1, "nanomgg-cf-base");
    range(b.mid + 1, b.end - 1, "nanomgg-cf-theirs");
    if (i === current) range(b.start, b.end, "nanomgg-cf-current");
  });
  return out;
}

function Pane({
  title,
  detail,
  tone,
  children,
}: {
  title: string;
  detail: string | null;
  tone: "ours" | "theirs" | "result";
  children: React.ReactNode;
}) {
  const bar = tone === "ours" ? "bg-success" : tone === "theirs" ? "bg-info" : "bg-primary";
  return (
    <section className="flex min-h-0 min-w-0 flex-1 flex-col" aria-label={title}>
      <header className="flex h-7 shrink-0 items-center gap-2 border-b bg-surface px-2 text-[11px]">
        <span className={`h-3 w-1 rounded ${bar}`} aria-hidden />
        <span className="truncate font-semibold">{title}</span>
        {detail && (
          <span className="truncate font-mono text-muted-foreground" title={detail}>
            {detail}
          </span>
        )}
      </header>
      {children}
    </section>
  );
}

/** Three-way conflict editor: ours / theirs on top (read-only), editable result below. */
export default function ConflictEditor({
  repoId,
  file,
  labels,
}: {
  repoId: number;
  file: ConflictFile;
  labels: SideLabels;
}) {
  const { t } = useTranslation();
  const theme = useUi((s) => s.resolvedTheme);
  const oursHost = useRef<HTMLDivElement>(null);
  const theirsHost = useRef<HTMLDivElement>(null);
  const resultHost = useRef<HTMLDivElement>(null);
  const ours = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const theirs = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const result = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const [blocks, setBlocks] = useState<ConflictBlock[]>([]);
  const [current, setCurrent] = useState(-1);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);

  const currentBlocks = () => parseLines(result.current?.getModel()?.getLinesContent() ?? []);

  /** Moves the cursor to a block and shows the matching lines in the side panes. */
  const goto = useCallback((index: number) => {
    const e = result.current;
    const list = parseLines(e?.getModel()?.getLinesContent() ?? []);
    const b = list[index];
    if (!e || !b) return;
    e.setPosition({ lineNumber: b.start + 1, column: 1 });
    e.revealLineInCenter(b.start + 1);
    setCurrent(index);
    for (const [side, lines] of [
      [ours.current, b.ours],
      [theirs.current, b.theirs],
    ] as const) {
      const at = locate(side?.getModel()?.getLinesContent() ?? [], lines);
      if (side && at >= 0) {
        side.revealLinesInCenter(at + 1, at + lines.length);
        side.setSelection(new monaco.Range(at + 1, 1, at + lines.length + 1, 1));
      }
    }
  }, []);

  useEffect(() => {
    const lang = languageFor(file.path);
    const make = (
      el: HTMLDivElement | null,
      value: string,
      readOnly: boolean,
    ): monaco.editor.IStandaloneCodeEditor | null =>
      el
        ? monaco.editor.create(el, {
            ...EDITOR_OPTIONS,
            readOnly,
            model: monaco.editor.createModel(value, lang),
          })
        : null;
    ours.current = make(oursHost.current, file.ours?.text ?? "", true);
    theirs.current = make(theirsHost.current, file.theirs?.text ?? "", true);
    const r = make(
      resultHost.current,
      file.working ?? file.ours?.text ?? file.theirs?.text ?? "",
      false,
    );
    result.current = r;
    const editors = [ours.current, theirs.current, r];
    if (!r) return;
    const collection = r.createDecorationsCollection();
    let cur = -1;
    const refresh = () => {
      const list = parseLines(r.getModel()?.getLinesContent() ?? []);
      const line = (r.getPosition()?.lineNumber ?? 1) - 1;
      cur = blockAt(list, line);
      if (cur >= 0 && !(line >= list[cur]!.start && line <= list[cur]!.end)) cur = -1;
      setBlocks(list);
      setCurrent(cur);
      collection.set(decorations(list, cur));
    };
    const subs = [r.onDidChangeModelContent(refresh), r.onDidChangeCursorPosition(refresh)];
    refresh();
    // Start at the first conflict.
    const first = parseLines(r.getModel()?.getLinesContent() ?? [])[0];
    if (first) {
      r.setPosition({ lineNumber: first.start + 1, column: 1 });
      r.revealLineInCenter(first.start + 1);
    }
    r.focus();
    return () => {
      subs.forEach((s) => s.dispose());
      for (const e of editors) {
        const model = e?.getModel();
        e?.dispose();
        model?.dispose();
      }
      ours.current = theirs.current = result.current = null;
    };
  }, [file]);

  useEffect(() => {
    applyMonacoTheme(theme);
  }, [theme]);

  const apply = (choice: BlockChoice) => {
    const e = result.current;
    const model = e?.getModel();
    const list = currentBlocks();
    const index = current >= 0 ? current : 0;
    const b = list[index];
    if (!e || !model || !b) return;
    const edit = blockEdit(
      b,
      replacement(b, choice),
      model.getLineCount(),
      (n) => model.getLineMaxColumn(n) - 1,
    );
    e.pushUndoStop();
    e.executeEdits("conflict", [
      {
        range: new monaco.Range(edit.startLine, edit.startColumn, edit.endLine, edit.endColumn),
        text: edit.lines.join(model.getEOL()),
        forceMoveMarkers: true,
      },
    ]);
    e.pushUndoStop();
    // The block is gone: the next one now has this index (or wrap to the first).
    const left = currentBlocks();
    if (left.length > 0) goto(Math.min(index, left.length - 1));
    else e.setPosition({ lineNumber: Math.max(1, edit.startLine), column: 1 });
    e.focus();
  };

  const step = (dir: 1 | -1) => {
    const line = (result.current?.getPosition()?.lineNumber ?? 1) - 1;
    const i = stepBlock(currentBlocks(), line, dir);
    if (i >= 0) goto(i);
  };

  const save = async () => {
    setConfirm(false);
    const value = result.current?.getModel()?.getValue();
    if (value === undefined) return;
    setBusy(true);
    await saveResolution(repoId, file.path, value);
    setBusy(false);
  };

  const b = current >= 0 ? blocks[current] : undefined;
  const choiceButtons: { choice: BlockChoice; label: string }[] = [
    { choice: "ours", label: t("conflict.take", { side: labels.oursShort }) },
    { choice: "theirs", label: t("conflict.take", { side: labels.theirsShort }) },
    { choice: "oursThenTheirs", label: t("conflict.takeBoth", { first: labels.oursShort }) },
    { choice: "theirsThenOurs", label: t("conflict.takeBoth", { first: labels.theirsShort }) },
  ];

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <style>{CSS}</style>
      <div className="flex h-[38%] min-h-32 shrink-0 border-b">
        <Pane title={labels.ours} detail={labels.oursDetail} tone="ours">
          <div ref={oursHost} className="min-h-0 flex-1" data-testid="conflict-ours" />
        </Pane>
        <div className="w-px bg-border" />
        <Pane title={labels.theirs} detail={labels.theirsDetail} tone="theirs">
          <div ref={theirsHost} className="min-h-0 flex-1" data-testid="conflict-theirs" />
        </Pane>
      </div>
      <div
        className="flex shrink-0 flex-wrap items-center gap-1.5 border-b bg-surface px-2 py-1.5 text-xs"
        role="toolbar"
        aria-label={t("conflict.blockToolbar")}
      >
        <Button
          variant="ghost"
          size="icon-xs"
          title={t("conflict.prev")}
          aria-label={t("conflict.prev")}
          disabled={blocks.length === 0}
          onClick={() => step(-1)}
        >
          <ChevronUp />
        </Button>
        <Button
          variant="ghost"
          size="icon-xs"
          title={t("conflict.next")}
          aria-label={t("conflict.next")}
          disabled={blocks.length === 0}
          onClick={() => step(1)}
        >
          <ChevronDown />
        </Button>
        <span className="min-w-24 font-medium tabular-nums" data-testid="conflict-counter">
          {blocks.length === 0
            ? t("conflict.noBlocks")
            : b
              ? `${t("conflict.block", { index: current + 1, total: blocks.length })} · ${t("conflict.blocksLeft", { count: blocks.length })}`
              : t("conflict.blocksLeft", { count: blocks.length })}
        </span>
        {blocks.length > 0 && (
          <>
            {choiceButtons.map((c) => (
              <Button
                key={c.choice}
                variant="outline"
                size="xs"
                disabled={!b}
                onClick={() => apply(c.choice)}
              >
                {c.label}
              </Button>
            ))}
            {b?.base && (
              <Button
                variant="ghost"
                size="xs"
                title={t("conflict.takeBaseTitle")}
                onClick={() => apply("base")}
              >
                {t("conflict.takeBase")}
              </Button>
            )}
            {!b && (
              <span className="text-[11px] text-muted-foreground">{t("conflict.noBlockHere")}</span>
            )}
          </>
        )}
      </div>
      <Pane title={t("conflict.result")} detail={t("conflict.resultHint")} tone="result">
        <div ref={resultHost} className="min-h-0 flex-1" data-testid="conflict-result" />
      </Pane>
      <div className="flex shrink-0 items-center gap-2 border-t bg-surface px-3 py-2 text-xs">
        <span className="text-[11px] text-muted-foreground">
          {file.crlf ? "CRLF" : "LF"}
          {file.bom ? " · BOM" : ""}
        </span>
        <span className="flex-1" />
        <Button
          size="xs"
          disabled={busy}
          onClick={() => (blocks.length > 0 ? setConfirm(true) : void save())}
        >
          <Save />
          {t("conflict.save")}
        </Button>
      </div>
      <ConfirmDialog
        open={confirm}
        title={t("conflict.saveConfirmTitle")}
        description={t("conflict.saveConfirmBody", { count: blocks.length })}
        confirmLabel={t("conflict.saveAnyway")}
        onConfirm={() => void save()}
        onCancel={() => setConfirm(false)}
      />
    </div>
  );
}
