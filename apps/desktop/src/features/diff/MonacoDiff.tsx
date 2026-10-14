import { useEffect, useRef } from "react";
import { useUi } from "@/stores/ui";
import { applyMonacoTheme, languageFor, monaco } from "./monaco";

interface Props {
  path: string;
  original: string;
  modified: string;
  sideBySide: boolean;
  ignoreWhitespace: boolean;
}

/** Read-only Monaco diff editor. Loaded lazily: Monaco is a large chunk. */
export default function MonacoDiff({
  path,
  original,
  modified,
  sideBySide,
  ignoreWhitespace,
}: Props) {
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<monaco.editor.IStandaloneDiffEditor | null>(null);
  const theme = useUi((s) => s.resolvedTheme);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const e = monaco.editor.createDiffEditor(el, {
      readOnly: true,
      originalEditable: false,
      automaticLayout: true,
      renderSideBySide: true,
      useInlineViewWhenSpaceIsLimited: false,
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      fontSize: 12,
      lineHeight: 18,
      renderOverviewRuler: true,
      hideUnchangedRegions: { enabled: true, contextLineCount: 3, minimumLineCount: 5 },
      diffAlgorithm: "advanced",
      contextmenu: false,
    });
    editor.current = e;
    return () => {
      const model = e.getModel();
      e.dispose();
      model?.original.dispose();
      model?.modified.dispose();
      editor.current = null;
    };
  }, []);

  useEffect(() => {
    const e = editor.current;
    if (!e) return;
    const lang = languageFor(path);
    const prev = e.getModel();
    e.setModel({
      original: monaco.editor.createModel(original, lang),
      modified: monaco.editor.createModel(modified, lang),
    });
    prev?.original.dispose();
    prev?.modified.dispose();
  }, [path, original, modified]);

  useEffect(() => {
    editor.current?.updateOptions({
      renderSideBySide: sideBySide,
      ignoreTrimWhitespace: ignoreWhitespace,
    });
  }, [sideBySide, ignoreWhitespace]);

  useEffect(() => {
    applyMonacoTheme(theme);
  }, [theme]);

  return <div ref={host} className="min-h-0 flex-1" data-testid="monaco-diff" />;
}
