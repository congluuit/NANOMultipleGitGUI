// Monaco is bundled locally (no CDN: the app's CSP only allows its own origin). Only the core
// editor, the diff editor, find and the syntax tokenizers are loaded; there are no language
// services, so a single editor worker is enough.
import * as monaco from "monaco-editor/editor/editor.api";
import "monaco-editor/features/codicon/register";
import "monaco-editor/features/diffEditor/register";
import "monaco-editor/features/find/register";
import "monaco-editor/languages/definitions/register.all";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import type { ResolvedTheme } from "@/theme/theme";
import { expandHex } from "./color";

self.MonacoEnvironment = { getWorker: () => new EditorWorker() };

function token(name: string): string {
  return expandHex(getComputedStyle(document.documentElement).getPropertyValue(name).trim());
}

/** `#rrggbb` + alpha (0–1) → `#rrggbbaa`, the format Monaco theme colors accept. */
export function withAlpha(hex: string, alpha: number): string {
  const a = Math.round(Math.min(1, Math.max(0, alpha)) * 255)
    .toString(16)
    .padStart(2, "0");
  return /^#[0-9a-f]{6}$/i.test(hex) ? `${hex}${a}` : hex;
}

/** (Re)defines the app theme from the current design tokens and activates it. */
export function applyMonacoTheme(theme: ResolvedTheme): string {
  const name = `nanomgg-${theme}`;
  const bg = token("--surface");
  const text = token("--text");
  const muted = token("--text-muted");
  const border = token("--border");
  const success = token("--success");
  const danger = token("--danger");
  const primary = token("--primary");
  monaco.editor.defineTheme(name, {
    base: theme === "dark" ? "vs-dark" : "vs",
    inherit: true,
    rules: [],
    colors: {
      "editor.background": bg,
      "editor.foreground": text,
      "editorLineNumber.foreground": muted,
      "editorLineNumber.activeForeground": text,
      "editorGutter.background": bg,
      "editorCursor.foreground": primary,
      "editor.selectionBackground": withAlpha(primary, 0.25),
      "editorWidget.background": token("--surface-2"),
      "editorWidget.border": border,
      "diffEditor.insertedTextBackground": withAlpha(success, 0.22),
      "diffEditor.removedTextBackground": withAlpha(danger, 0.22),
      "diffEditor.insertedLineBackground": withAlpha(success, 0.1),
      "diffEditor.removedLineBackground": withAlpha(danger, 0.1),
      "diffEditor.border": border,
      "diffEditor.diagonalFill": withAlpha(border, 0.6),
      "scrollbarSlider.background": withAlpha(muted, 0.25),
      "scrollbarSlider.hoverBackground": withAlpha(muted, 0.4),
    },
  });
  monaco.editor.setTheme(name);
  return name;
}

/** Monaco language id for a file path, from the registered languages' extensions/filenames. */
export function languageFor(path: string): string {
  const name = path.split("/").pop()?.toLowerCase() ?? "";
  for (const lang of monaco.languages.getLanguages()) {
    if (lang.filenames?.some((f) => f.toLowerCase() === name)) return lang.id;
  }
  let best: { id: string; len: number } | null = null;
  for (const lang of monaco.languages.getLanguages()) {
    for (const ext of lang.extensions ?? []) {
      if (name.endsWith(ext.toLowerCase()) && (!best || ext.length > best.len)) {
        best = { id: lang.id, len: ext.length };
      }
    }
  }
  return best?.id ?? "plaintext";
}

export { monaco };
