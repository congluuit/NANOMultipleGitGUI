/**
 * Encodings and line endings of files in the built-in editor: their names in the status bar,
 * the choices of its menus, and the encoding a file is read in when the user picked one
 * ("Reopen with encoding"). The bytes themselves are decoded and encoded by the backend
 * (`edit.rs`), which writes a file back byte for byte as it was read.
 */
import type { TFunction } from "i18next";
import { create } from "zustand";
import type { LineEnding, TextEncoding } from "@/ipc";
import { bufferKey } from "./store";

/** Encodings a file can be read in ("Reopen with encoding"); `null` detects it again. */
export const REOPEN_ENCODINGS: (TextEncoding | null)[] = [
  null,
  "utf8",
  "utf16Le",
  "utf16Be",
  "windows1252",
];

/** Encodings a file can be written in ("Save with encoding"). */
export const SAVE_ENCODINGS: TextEncoding[] = [
  "utf8",
  "utf8Bom",
  "utf16Le",
  "utf16Be",
  "windows1252",
];

export function encodingLabel(encoding: TextEncoding, t: TFunction): string {
  switch (encoding) {
    case "utf8":
      return "UTF-8";
    case "utf8Bom":
      return t("editor.utf8Bom");
    case "utf16Le":
      return "UTF-16 LE";
    case "utf16Be":
      return "UTF-16 BE";
    case "windows1252":
      return "Windows-1252";
  }
}

/** Reading as UTF-8 keeps a BOM that is there, so both are the same choice when reopening. */
export function sameReadEncoding(a: TextEncoding, b: TextEncoding): boolean {
  const read = (e: TextEncoding) => (e === "utf8Bom" ? "utf8" : e);
  return read(a) === read(b);
}

/** UTF-8 with or without BOM: the index and other versions of the file read the same way. */
export const isUtf8 = (encoding: TextEncoding | null | undefined) =>
  !encoding || encoding === "utf8" || encoding === "utf8Bom";

/** The two line endings a file can be converted to. */
export type Eol = Extract<LineEnding, "lf" | "crlf">;

export function eolLabel(eol: LineEnding, t: TFunction): string {
  switch (eol) {
    case "lf":
      return "LF";
    case "crlf":
      return "CRLF";
    case "mixed":
      return t("editor.eol.mixed");
    case "none":
      return t("editor.eol.none");
  }
}

interface EncodingOverrides {
  /** Buffer key to the encoding its file is read in, when the user picked one. */
  byKey: Record<string, TextEncoding>;
  set: (repoId: number, path: string, encoding: TextEncoding | null) => void;
}

/** Encodings picked for files during this session (not kept across restarts). */
export const useEncodingOverrides = create<EncodingOverrides>()((set) => ({
  byKey: {},
  set: (repoId, path, encoding) =>
    set((s) => {
      const key = bufferKey(repoId, path);
      if ((s.byKey[key] ?? null) === encoding) return s;
      const byKey = { ...s.byKey };
      if (encoding) byKey[key] = encoding;
      else delete byKey[key];
      return { byKey };
    }),
}));

export const encodingOverride = (repoId: number, path: string): TextEncoding | null =>
  useEncodingOverrides.getState().byKey[bufferKey(repoId, path)] ?? null;
