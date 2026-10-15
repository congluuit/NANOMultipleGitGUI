import { useEffect, useRef } from "react";
import { useUi } from "@/stores/ui";
import { applyMonacoTheme, languageFor, monaco } from "./monaco";

interface Props {
  path: string;
  value: string;
}

/** Read-only Monaco editor for one file (file at a commit). Loaded lazily. */
export default function MonacoView({ path, value }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const editor = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const theme = useUi((s) => s.resolvedTheme);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const e = monaco.editor.create(el, {
      readOnly: true,
      automaticLayout: true,
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      fontSize: 12,
      lineHeight: 18,
      contextmenu: false,
    });
    editor.current = e;
    return () => {
      const model = e.getModel();
      e.dispose();
      model?.dispose();
      editor.current = null;
    };
  }, []);

  useEffect(() => {
    const e = editor.current;
    if (!e) return;
    const prev = e.getModel();
    e.setModel(monaco.editor.createModel(value, languageFor(path)));
    prev?.dispose();
  }, [path, value]);

  useEffect(() => {
    applyMonacoTheme(theme);
  }, [theme]);

  return <div ref={host} className="min-h-0 flex-1" data-testid="monaco-view" />;
}
