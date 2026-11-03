import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";
import {
  ChevronDown,
  ChevronUp,
  Clock,
  Command,
  Hash,
  ListMinus,
  Map as MinimapIcon,
  Pilcrow,
  Replace,
  RotateCcw,
  Save,
  Search,
  TriangleAlert,
  WrapText,
} from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { relativeTime } from "@/features/blame/blame";
import { applyMonacoTheme, languageFor, monaco, token, withAlpha } from "@/features/diff/monaco";
import { invalidateWorktree, worktreeKeys } from "@/features/worktree/queries";
import { commands, errorMessage, type BlameResult } from "@/ipc";
import { cn } from "@/lib/utils";
import { useUi } from "@/stores/ui";
import {
  acquireBuffer,
  bufferInfo,
  reloadFromDisk,
  releaseBuffer,
  saveBuffer,
  subscribeBuffers,
} from "./buffers";
import {
  applyChange,
  changeAt,
  changeKind,
  diffLines,
  originalLine,
  revertEdit,
  revertLines,
  splitLines,
  type LineChange,
} from "./lineDiff";
import { takeReveal, useReveal } from "./open";
import { bufferKey, useEditorPrefs, type EditorPrefs } from "./store";

const LINE_HEIGHT = 18;
const FONT = '"Cascadia Code", Consolas, "SF Mono", Menlo, "Courier New", monospace';

interface Props {
  repoId: number;
  path: string;
  /** Text the changes are measured against (index, HEAD, parent commit); `null`: no markers. */
  base: string | null;
  /** Name of `base` in the status bar ("index", "HEAD"...). */
  baseLabel: string | null;
  text: string;
  /** Version on disk of an editable working-tree file; `null` shows `text` read-only. */
  version: string | null;
  /** Shown above the editor (why it is read-only, for instance). */
  note?: ReactNode;
  /**
   * `stage`: `base` is the index and blocks can be staged; `unstage`: `text` is the index and
   * blocks can be unstaged (back to `base`, HEAD); `null`: neither.
   */
  stageMode?: "stage" | "unstage" | null;
  /** Blame of the cursor line: `null` the working tree, a commit id, `undefined` none. */
  blameRev?: string | null;
  /** Extra controls at the start of the toolbar (the compare picker). */
  toolbarExtra?: ReactNode;
  /** Offer to replace the whole text with `base` (comparing with another version). */
  canRestore?: boolean;
}

interface Status {
  line: number;
  column: number;
  selected: number;
  eol: "LF" | "CRLF";
  tabSize: number;
  insertSpaces: boolean;
  language: string;
}

interface EditorState {
  changes: LineChange[];
  status: Status | null;
}

const EMPTY_STATE: EditorState = { changes: [], status: null };

/** Cursor and scroll position per file, restored when the file is opened again. */
const viewStates = new Map<string, monaco.editor.ICodeEditorViewState>();

/**
 * Change markers and status bar data of an editor, recomputed from its events (content
 * changes re-run the line diff, cursor moves only the status).
 */
function editorStore(editor: monaco.editor.IStandaloneCodeEditor | null, base: string[] | null) {
  if (!editor) return { subscribe: () => () => {}, get: () => EMPTY_STATE };
  const readStatus = (): Status | null => {
    const model = editor.getModel();
    const pos = editor.getPosition();
    if (!model || !pos) return null;
    const opts = model.getOptions();
    return {
      line: pos.lineNumber,
      column: pos.column,
      selected: (editor.getSelections() ?? []).reduce(
        (n, s) => n + model.getValueLengthInRange(s),
        0,
      ),
      eol: model.getEOL() === "\r\n" ? "CRLF" : "LF",
      tabSize: opts.tabSize,
      insertSpaces: opts.insertSpaces,
      language: model.getLanguageId(),
    };
  };
  const readChanges = () => {
    const model = editor.getModel();
    return model && base ? diffLines(base, model.getLinesContent()) : [];
  };
  let state: EditorState = { changes: readChanges(), status: readStatus() };
  return {
    subscribe(notify: () => void) {
      const all = () => {
        state = { changes: readChanges(), status: readStatus() };
        notify();
      };
      const status = () => {
        state = { ...state, status: readStatus() };
        notify();
      };
      const subs = [
        editor.onDidChangeModel(all),
        editor.onDidChangeModelContent(all),
        editor.onDidChangeModelLanguage(status),
        editor.onDidChangeModelOptions(status),
        editor.onDidChangeCursorSelection(status),
      ];
      return () => subs.forEach((d) => d.dispose());
    },
    get: () => state,
  };
}

/** First line of a change marker (deletions sit on the line above the removed lines). */
function markerLine(c: LineChange): number {
  return c.modifiedCount > 0 ? c.modifiedStart + 1 : Math.max(1, c.modifiedStart);
}

const blockKey = (c: LineChange) => `${c.originalStart}:${c.originalCount}`;

function decorations(changes: LineChange[]): monaco.editor.IModelDeltaDecoration[] {
  const colors = {
    added: token("--success"),
    modified: token("--info"),
    deleted: token("--danger"),
  };
  return changes.map((c) => {
    const kind = changeKind(c);
    const ruler = {
      overviewRuler: { color: colors[kind], position: monaco.editor.OverviewRulerLane.Left },
      minimap: {
        color: withAlpha(colors[kind], 0.6),
        position: monaco.editor.MinimapPosition.Gutter,
      },
    };
    if (kind === "deleted") {
      const line = markerLine(c);
      return {
        range: new monaco.Range(line, 1, line, 1),
        options: {
          linesDecorationsClassName: cn(
            "nm-gutter nm-gutter-deleted",
            c.modifiedStart === 0 && "nm-gutter-deleted-top",
          ),
          ...ruler,
        },
      };
    }
    return {
      range: new monaco.Range(c.modifiedStart + 1, 1, c.modifiedStart + c.modifiedCount, 1),
      options: {
        isWholeLine: true,
        className: `nm-line-${kind}`,
        linesDecorationsClassName: `nm-gutter nm-gutter-${kind}`,
        ...ruler,
      },
    };
  });
}

/**
 * The removed lines of a change, shown between the editor lines. Monaco's text layer covers
 * view zones, so the revert button gets no clicks of its own: the editor's mouse handler
 * hit-tests it (see the zones effect).
 */
function zoneNode(
  c: LineChange,
  base: string[],
  language: string,
  actions: { label: string; run: () => void }[],
): { root: HTMLElement; buttons: { el: HTMLElement; run: () => void }[] } {
  const root = document.createElement("div");
  root.className = "nm-zone";
  root.style.fontFamily = FONT;
  const head = document.createElement("div");
  head.className = "nm-zone-head";
  const stats = document.createElement("span");
  stats.textContent = `\u2212${c.originalCount} +${c.modifiedCount}`;
  head.append(stats);
  const buttons = actions.map((a) => {
    const el = document.createElement("span");
    el.className = "nm-zone-action";
    el.textContent = a.label;
    head.append(el);
    return { el, run: a.run };
  });
  root.append(head);
  if (c.originalCount > 0) {
    const body = document.createElement("div");
    body.className = "nm-zone-lines";
    const text = base.slice(c.originalStart, c.originalStart + c.originalCount).join("\n");
    body.textContent = text;
    void monaco.editor.colorize(text, language, {}).then((html) => {
      body.innerHTML = html;
    });
    root.append(body);
  }
  return { root, buttons };
}

/** The blame text shown after the cursor line, and its hover. */
function blameText(
  blame: BlameResult,
  line: number | null,
  t: TFunction,
  locale: string,
): { text: string; hover: string | null } {
  if (line === null) return { text: t("editor.blameUnsaved"), hover: null };
  const commit = blame.commits[blame.lineCommits[line] ?? -1];
  if (!commit) return { text: "", hover: null };
  if (commit.uncommitted) return { text: t("editor.blameUncommitted"), hover: null };
  const when = relativeTime(commit.time, Date.now() / 1000, locale);
  const date = new Date(commit.time * 1000).toLocaleString(locale);
  return {
    text: t("editor.blameLine", { author: commit.author, when, summary: commit.summary }),
    hover: [
      `**${commit.summary}**`,
      `${commit.author} <${commit.email}>, ${date}`,
      `\`${commit.oid.slice(0, 10)}\``,
    ].join("\n\n"),
  };
}

export default function FileEditor({
  repoId,
  path,
  base,
  baseLabel,
  text,
  version,
  note,
  stageMode = null,
  blameRev,
  toolbarExtra,
  canRestore = false,
}: Props) {
  const { t, i18n } = useTranslation();
  const editable = version !== null;
  const key = bufferKey(repoId, path);
  const prefs = useEditorPrefs();
  const theme = useUi((s) => s.resolvedTheme);
  const themeVersion = useUi((s) => s.themeVersion);
  const [editor, setEditor] = useState<monaco.editor.IStandaloneCodeEditor | null>(null);
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(() => new Set());
  const [saved, setSaved] = useState(false);
  const info = useSyncExternalStore(subscribeBuffers, () => (editable ? bufferInfo(key) : null));
  const model = info?.model ?? null;

  const baseLines = useMemo(() => (base === null ? null : splitLines(base)), [base]);
  const store = useMemo(() => editorStore(editor, baseLines), [editor, baseLines]);
  const { changes, status } = useSyncExternalStore(store.subscribe, store.get);
  const live = useRef({ changes, baseLines });
  useEffect(() => {
    live.current = { changes, baseLines };
  });

  const mount = useCallback(
    (el: HTMLDivElement | null) => {
      if (!el) return;
      const p = useEditorPrefs.getState();
      const e = monaco.editor.create(el, {
        model: null,
        readOnly: !editable,
        readOnlyMessage: { value: t("editor.readOnlyMessage") },
        automaticLayout: true,
        fontFamily: FONT,
        fontSize: 12,
        lineHeight: LINE_HEIGHT,
        scrollBeyondLastLine: false,
        glyphMargin: false,
        lineDecorationsWidth: 12,
        minimap: { enabled: p.minimap, renderCharacters: false, showSlider: "mouseover" },
        wordWrap: p.wordWrap ? "on" : "off",
        renderWhitespace: p.whitespace ? "all" : "selection",
        mouseWheelZoom: true,
        wordBasedSuggestions: "currentDocument",
        contextmenu: true,
        fixedOverflowWidgets: true,
      });
      setEditor(e);
      return () => {
        const state = e.getModel() && editable ? e.saveViewState() : null;
        if (state) viewStates.set(key, state);
        e.dispose();
        setEditor(null);
      };
    },
    [editable, key, t],
  );

  // Editable: the file's buffer (kept across views while it has unsaved edits).
  useEffect(() => {
    if (version !== null) acquireBuffer(repoId, path, text, version);
  }, [repoId, path, text, version]);

  useEffect(() => {
    if (!editor || !model || model.isDisposed()) return;
    editor.setModel(model);
    const state = viewStates.get(key);
    if (state) editor.restoreViewState(state);
    return () => {
      if (editor.getModel() === model) {
        const s = editor.saveViewState();
        if (s) viewStates.set(key, s);
        editor.setModel(null);
      }
      // Later, so an effect that runs again at once (StrictMode, Fast Refresh) can take the
      // buffer back before a clean one is disposed.
      window.setTimeout(() => releaseBuffer(key), 0);
    };
  }, [editor, model, key]);

  // Read-only: a throwaway model of the text.
  useEffect(() => {
    if (!editor || editable) return;
    const m = monaco.editor.createModel(text, languageFor(path));
    editor.setModel(m);
    return () => {
      if (editor.getModel() === m) editor.setModel(null);
      m.dispose();
    };
  }, [editor, editable, text, path]);

  useEffect(() => {
    editor?.updateOptions({
      wordWrap: prefs.wordWrap ? "on" : "off",
      renderWhitespace: prefs.whitespace ? "all" : "selection",
      minimap: { enabled: prefs.minimap },
    });
  }, [editor, prefs.wordWrap, prefs.whitespace, prefs.minimap]);

  useEffect(() => {
    applyMonacoTheme(theme);
  }, [theme, themeVersion]);

  useEffect(() => {
    if (!editor) return;
    const collection = editor.createDecorationsCollection(decorations(changes));
    return () => collection.clear();
  }, [editor, changes, theme, themeVersion]);

  const revert = useCallback(
    (c: LineChange) => {
      const m = editor?.getModel();
      const lines = live.current.baseLines;
      if (!editor || !m || !lines || !editable) return;
      const e = revertEdit(lines, m.getLinesContent(), c, m.getEOL());
      editor.pushUndoStop();
      editor.executeEdits("nanomgg.revertChange", [
        {
          range: new monaco.Range(e.startLine, e.startColumn, e.endLine, e.endColumn),
          text: e.text,
        },
      ]);
      editor.pushUndoStop();
      editor.setPosition({ lineNumber: c.modifiedStart + 1, column: 1 });
      editor.focus();
    },
    [editor, editable],
  );

  /** Stages (or unstages) one block: the index gets this block's lines and nothing else. */
  const stageBlock = useCallback(
    async (c: LineChange) => {
      const m = editor?.getModel();
      const lines = live.current.baseLines;
      if (!m || !lines || !stageMode || base === null) return;
      // What gets staged is what is on screen: unsaved edits go to disk first.
      if (editable && !(await saveBuffer(key))) return;
      const current = m.getLinesContent();
      const index = stageMode === "stage" ? base : text;
      const next =
        stageMode === "stage" ? applyChange(lines, current, c) : revertLines(lines, current, c);
      const eol = index.includes("\r\n") ? "\r\n" : "\n";
      const bom = index.charCodeAt(0) === 0xfeff ? index.charAt(0) : "";
      try {
        const ok = await commands.indexStageText(repoId, path, bom + next.join(eol), index);
        if (!ok) {
          useUi.getState().showNotice({ kind: "error", message: t("editor.indexChanged"), repoId });
        }
      } catch (e) {
        useUi.getState().showNotice({ kind: "error", message: errorMessage(e), repoId });
      } finally {
        invalidateWorktree(repoId);
      }
    },
    [editor, stageMode, base, text, editable, key, repoId, path, t],
  );

  useEffect(() => {
    const m = editor?.getModel();
    if (!editor || !m || !baseLines) return;
    const ids: string[] = [];
    const buttons: { zone: string; el: HTMLElement; run: () => void }[] = [];
    const language = m.getLanguageId();
    editor.changeViewZones((acc) => {
      for (const c of changes) {
        const show = (prefs.showRemoved && c.originalCount > 0) || expanded.has(blockKey(c));
        if (!show) continue;
        const actions: { label: string; run: () => void }[] = [];
        if (editable) actions.push({ label: t("editor.revert"), run: () => revert(c) });
        if (stageMode) {
          actions.push({
            label: t(stageMode === "stage" ? "editor.stageBlock" : "editor.unstageBlock"),
            run: () => void stageBlock(c),
          });
        }
        const node = zoneNode(c, baseLines, language, actions);
        const id = acc.addZone({
          afterLineNumber: c.modifiedStart,
          heightInPx: LINE_HEIGHT * (c.originalCount + 1),
          domNode: node.root,
        });
        ids.push(id);
        for (const b of node.buttons) buttons.push({ zone: id, ...b });
      }
    });
    const click = editor.onMouseDown((e) => {
      if (e.target.type !== monaco.editor.MouseTargetType.CONTENT_VIEW_ZONE) return;
      const zone = e.target.detail.viewZoneId;
      const { clientX: x, clientY: y } = e.event.browserEvent;
      const hit = buttons.find((b) => {
        if (b.zone !== zone) return false;
        const r = b.el.getBoundingClientRect();
        return x >= r.left && x <= r.right && y >= r.top && y <= r.bottom;
      });
      if (!hit) return;
      e.event.preventDefault();
      hit.run();
    });
    return () => {
      click.dispose();
      editor.changeViewZones((acc) => ids.forEach((id) => acc.removeZone(id)));
    };
  }, [
    editor,
    changes,
    baseLines,
    prefs.showRemoved,
    expanded,
    editable,
    stageMode,
    revert,
    stageBlock,
    t,
  ]);

  // Jump to a line asked for by quick open or a search result.
  const reveal = useReveal((s) => s.reveal);
  const hasModel = status !== null;
  useEffect(() => {
    if (!editor || !hasModel || !reveal) return;
    const r = takeReveal(repoId, path);
    if (!r) return;
    if (r.line > 0) {
      editor.setPosition({ lineNumber: r.line, column: r.column });
      editor.revealLineInCenter(r.line);
    }
    editor.focus();
  }, [editor, hasModel, reveal, repoId, path]);

  // Blame of the cursor line, shown after its end.
  const blame = useQuery({
    queryKey: worktreeKeys.blame(repoId, path, blameRev ?? null),
    queryFn: () => commands.blameFile(repoId, path, blameRev ?? null),
    enabled: isTauri() && prefs.blame && blameRev !== undefined,
    staleTime: 30_000,
  });
  const blameMap = useRef<{ data: BlameResult; changes: LineChange[]; alt: number } | null>(null);
  useEffect(() => {
    const m = editor?.getModel();
    const data = blame.data;
    if (!editor || !m || !status || !prefs.blame || !data || data.binary || data.tooLarge) return;
    // The blamed text is the file on disk (or at the commit): map the buffer's lines onto it.
    const alt = m.getAlternativeVersionId();
    let cached = blameMap.current;
    if (!cached || cached.data !== data || cached.alt !== alt) {
      cached = { data, changes: diffLines(data.lines, m.getLinesContent()), alt };
      blameMap.current = cached;
    }
    const line = originalLine(cached.changes, status.line - 1);
    const { text: label, hover } = blameText(data, line, t, i18n.language);
    if (!label) return;
    const end = m.getLineMaxColumn(status.line);
    const collection = editor.createDecorationsCollection([
      {
        range: new monaco.Range(status.line, end, status.line, end),
        options: {
          showIfCollapsed: true,
          after: { content: `    ${label}`, inlineClassName: "nm-blame" },
          hoverMessage: hover ? { value: hover } : undefined,
        },
      },
    ]);
    return () => collection.clear();
  }, [editor, status, prefs.blame, blame.data, t, i18n.language]);

  const restoreAll = () => {
    const m = editor?.getModel();
    if (!editor || !m || base === null || !editable) return;
    editor.pushUndoStop();
    // A BOM stays out of the text, Monaco keeps it apart.
    const text = base.charCodeAt(0) === 0xfeff ? base.slice(1) : base;
    editor.executeEdits("nanomgg.restoreFile", [{ range: m.getFullModelRange(), text }]);
    editor.pushUndoStop();
    editor.focus();
  };

  const save = useCallback(
    async (overwrite = false) => {
      if (!editable) return;
      if (await saveBuffer(key, overwrite)) {
        setSaved(true);
        window.setTimeout(() => setSaved(false), 2000);
      }
    },
    [editable, key],
  );

  const goTo = useCallback(
    (step: 1 | -1) => {
      const list = live.current.changes;
      if (!editor || list.length === 0) return;
      const line = editor.getPosition()?.lineNumber ?? 0;
      const starts = list.map(markerLine);
      let index = -1;
      for (let i = 0; i < starts.length; i++) {
        const s = starts[i] ?? 0;
        if (step > 0 && s > line) {
          index = i;
          break;
        }
        if (step < 0 && s < line) index = i;
      }
      if (index < 0) index = step > 0 ? 0 : starts.length - 1;
      const target = starts[index] ?? 1;
      editor.setPosition({ lineNumber: target, column: 1 });
      editor.revealLineInCenter(target);
      editor.focus();
    },
    [editor],
  );

  useEffect(() => {
    if (!editor) return;
    const subs: monaco.IDisposable[] = [
      editor.addAction({
        id: "nanomgg.nextChange",
        label: t("editor.nextChange"),
        keybindings: [monaco.KeyMod.Alt | monaco.KeyCode.F5],
        run: () => goTo(1),
      }),
      editor.addAction({
        id: "nanomgg.previousChange",
        label: t("editor.prevChange"),
        keybindings: [monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.F5],
        run: () => goTo(-1),
      }),
      // A click on a change marker shows or hides what the lines were before.
      editor.onMouseDown((e) => {
        if (e.target.type !== monaco.editor.MouseTargetType.GUTTER_LINE_DECORATIONS) return;
        const line = e.target.position?.lineNumber;
        const c = line ? live.current.changes[changeAt(live.current.changes, line - 1)] : null;
        if (!c) return;
        setExpanded((prev) => {
          const next = new Set(prev);
          if (!next.delete(blockKey(c))) next.add(blockKey(c));
          return next;
        });
      }),
    ];
    if (editable) {
      subs.push(
        editor.addAction({
          id: "nanomgg.save",
          label: t("editor.save"),
          keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS],
          contextMenuGroupId: "1_nanomgg",
          contextMenuOrder: 1,
          run: () => void save(),
        }),
        editor.addAction({
          id: "nanomgg.revertChange",
          label: t("editor.revertChange"),
          contextMenuGroupId: "1_nanomgg",
          contextMenuOrder: 2,
          run: (ed) => {
            const line = (ed.getPosition()?.lineNumber ?? 1) - 1;
            const c = live.current.changes[changeAt(live.current.changes, line)];
            if (c) revert(c);
          },
        }),
      );
    }
    if (stageMode) {
      subs.push(
        editor.addAction({
          id: "nanomgg.stageChange",
          label: t(stageMode === "stage" ? "editor.stageChange" : "editor.unstageChange"),
          contextMenuGroupId: "1_nanomgg",
          contextMenuOrder: 3,
          run: (ed) => {
            const line = (ed.getPosition()?.lineNumber ?? 1) - 1;
            const c = live.current.changes[changeAt(live.current.changes, line)];
            if (c) void stageBlock(c);
          },
        }),
      );
    }
    return () => subs.forEach((d) => d.dispose());
  }, [editor, editable, stageMode, goTo, revert, stageBlock, save, t]);

  const run = (id: string) => {
    editor?.focus();
    void editor?.getAction(id)?.run();
  };

  const added = changes.reduce((n, c) => n + c.modifiedCount, 0);
  const removed = changes.reduce((n, c) => n + c.originalCount, 0);
  const current = status ? changeAt(changes, status.line - 1) : -1;
  const mixedEol = editable && /\r\n/.test(text) && /(^|[^\r])\n/.test(text);
  const bom = text.startsWith("\uFEFF");

  return (
    <div
      className="flex min-h-0 flex-1 flex-col"
      onKeyDown={(e) => {
        // Escape belongs to the editor (find widget, suggestions...), not to closing the view.
        if (e.key === "Escape" && e.target instanceof Element && e.target.closest(".monaco-editor"))
          e.stopPropagation();
        if (
          editable &&
          (e.ctrlKey || e.metaKey) &&
          e.key.toLowerCase() === "s" &&
          !e.defaultPrevented
        ) {
          e.preventDefault();
          void save();
        }
      }}
    >
      <div className="flex h-8 shrink-0 items-center gap-1 overflow-x-auto border-b px-2 text-xs [&>*]:shrink-0">
        {editable && (
          <>
            <Button
              size="xs"
              variant={info?.dirty ? "default" : "outline"}
              disabled={!info?.dirty}
              onClick={() => void save()}
              title={t("editor.saveHint")}
            >
              <Save />
              {t("editor.save")}
            </Button>
            {/* "Unsaved" is in the diff header; here only the confirmation of a save. */}
            {saved && !info?.dirty && (
              <span className="px-1 text-[11px] whitespace-nowrap text-success">
                {t("editor.saved")}
              </span>
            )}
          </>
        )}
        {toolbarExtra}
        {canRestore && editable && (
          <Button
            size="xs"
            variant="outline"
            onClick={restoreAll}
            title={t("editor.restoreAllHint", { base: baseLabel ?? "" })}
          >
            <RotateCcw />
            {t("editor.restoreAll")}
          </Button>
        )}
        <span className="flex-1" />
        {baseLines && (
          <>
            <ToolButton
              label={t("editor.prevChange")}
              disabled={changes.length === 0}
              onClick={() => goTo(-1)}
            >
              <ChevronUp />
            </ToolButton>
            <ToolButton
              label={t("editor.nextChange")}
              disabled={changes.length === 0}
              onClick={() => goTo(1)}
            >
              <ChevronDown />
            </ToolButton>
            <span className="px-1 text-[11px] whitespace-nowrap text-muted-foreground tabular-nums">
              {changes.length === 0
                ? t("editor.noChanges")
                : current >= 0
                  ? t("editor.changeOf", { index: current + 1, count: changes.length })
                  : t("editor.changes", { count: changes.length })}
            </span>
            <PrefToggle pref="showRemoved" label={t("editor.showRemoved")}>
              <ListMinus />
            </PrefToggle>
            <span className="mx-1 h-4 w-px bg-border" />
          </>
        )}
        {blameRev !== undefined && (
          <PrefToggle pref="blame" label={t("editor.blame")}>
            <Clock />
          </PrefToggle>
        )}
        <PrefToggle pref="wordWrap" label={t("editor.wordWrap")}>
          <WrapText />
        </PrefToggle>
        <PrefToggle pref="whitespace" label={t("editor.whitespace")}>
          <Pilcrow />
        </PrefToggle>
        <PrefToggle pref="minimap" label={t("editor.minimap")}>
          <MinimapIcon />
        </PrefToggle>
        <span className="mx-1 h-4 w-px bg-border" />
        <ToolButton label={t("editor.find")} onClick={() => run("actions.find")}>
          <Search />
        </ToolButton>
        {editable && (
          <ToolButton
            label={t("editor.replace")}
            onClick={() => run("editor.action.startFindReplaceAction")}
          >
            <Replace />
          </ToolButton>
        )}
        <ToolButton label={t("editor.gotoLine")} onClick={() => run("editor.action.gotoLine")}>
          <Hash />
        </ToolButton>
        <ToolButton label={t("editor.commands")} onClick={() => run("editor.action.quickCommand")}>
          <Command />
        </ToolButton>
      </div>

      {note && (
        <div className="shrink-0 border-b bg-surface px-3 py-1.5 text-[11px] text-muted-foreground">
          {note}
        </div>
      )}
      {info?.conflict && (
        <div className="flex shrink-0 items-center gap-2 border-b bg-warning/10 px-3 py-1.5 text-xs">
          <TriangleAlert className="size-4 shrink-0 text-warning" />
          <span className="flex-1">{t("editor.conflict")}</span>
          <Button size="xs" variant="outline" onClick={() => void reloadFromDisk(key)}>
            {t("editor.reload")}
          </Button>
          <Button size="xs" onClick={() => void save(true)}>
            {t("editor.overwrite")}
          </Button>
        </div>
      )}

      <div ref={mount} className="min-h-0 flex-1" data-testid="file-editor" />

      {status && (
        <div className="flex h-6 shrink-0 items-center gap-3 border-t bg-surface px-3 text-[11px] text-muted-foreground">
          <span className="tabular-nums">
            {t("editor.position", { line: status.line, column: status.column })}
            {status.selected > 0 && ` (${t("editor.selected", { count: status.selected })})`}
          </span>
          {baseLines && baseLabel && (
            <span className="tabular-nums">
              <span className="text-success">+{added}</span>{" "}
              <span className="text-danger">
                {"\u2212"}
                {removed}
              </span>{" "}
              {t("editor.vsBase", { base: baseLabel })}
            </span>
          )}
          <span className="flex-1" />
          {mixedEol && (
            <span
              className="flex items-center gap-1 text-warning"
              title={t("editor.mixedEolTitle", { eol: status.eol })}
            >
              <TriangleAlert className="size-3" />
              {t("editor.mixedEol")}
            </span>
          )}
          <select
            value={`${status.insertSpaces ? "s" : "t"}${status.tabSize}`}
            onChange={(e) => {
              const v = e.target.value;
              editor?.getModel()?.updateOptions({
                insertSpaces: v.startsWith("s"),
                tabSize: Number(v.slice(1)),
              });
            }}
            className="bg-transparent text-[11px] outline-none hover:text-foreground"
            aria-label={t("editor.indentation")}
          >
            {[2, 4, 8].map((n) => (
              <option key={`s${n}`} value={`s${n}`}>
                {t("editor.spaces", { size: n })}
              </option>
            ))}
            {[2, 4, 8].map((n) => (
              <option key={`t${n}`} value={`t${n}`}>
                {t("editor.tabs", { size: n })}
              </option>
            ))}
          </select>
          <span>{bom ? t("editor.utf8Bom") : "UTF-8"}</span>
          <button
            type="button"
            disabled={!editable}
            onClick={() =>
              editor
                ?.getModel()
                ?.pushEOL(
                  status.eol === "LF"
                    ? monaco.editor.EndOfLineSequence.CRLF
                    : monaco.editor.EndOfLineSequence.LF,
                )
            }
            title={editable ? t("editor.eolTitle") : undefined}
            className="hover:text-foreground disabled:hover:text-muted-foreground"
          >
            {status.eol}
          </button>
          <LanguagePicker
            value={status.language}
            onChange={(id) => {
              const m = editor?.getModel();
              if (m) monaco.editor.setModelLanguage(m, id);
            }}
          />
        </div>
      )}
    </div>
  );
}

function ToolButton({
  label,
  disabled,
  pressed,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  pressed?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <Button
      variant="ghost"
      size="icon-xs"
      title={label}
      aria-label={label}
      aria-pressed={pressed}
      disabled={disabled}
      onClick={onClick}
      className={cn(pressed && "bg-surface-2 text-foreground")}
    >
      {children}
    </Button>
  );
}

function PrefToggle({
  pref,
  label,
  children,
}: {
  pref: keyof EditorPrefs;
  label: string;
  children: ReactNode;
}) {
  const value = useEditorPrefs((s) => s[pref]);
  const setPref = useEditorPrefs((s) => s.setPref);
  return (
    <ToolButton label={label} pressed={value} onClick={() => setPref(pref, !value)}>
      {children}
    </ToolButton>
  );
}

function LanguagePicker({ value, onChange }: { value: string; onChange: (id: string) => void }) {
  const { t } = useTranslation();
  const languages = useMemo(
    () =>
      monaco.languages
        .getLanguages()
        .map((l) => ({ id: l.id, name: l.aliases?.[0] ?? l.id }))
        .sort((a, b) => a.name.localeCompare(b.name)),
    [],
  );
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="max-w-32 bg-transparent text-[11px] outline-none hover:text-foreground"
      aria-label={t("editor.language")}
      title={t("editor.language")}
    >
      {languages.map((l) => (
        <option key={l.id} value={l.id}>
          {l.name}
        </option>
      ))}
    </select>
  );
}
