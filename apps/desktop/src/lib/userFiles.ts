// Phase 6.2 / 6.5: pick a file to export to / import from (the backend reads and writes it).
import { open, save } from "@tauri-apps/plugin-dialog";
import type { TFunction } from "i18next";
import type { UserFileProblem } from "@/ipc";

/** Characters Windows / macOS do not allow in file names. */
export function safeFileName(name: string, fallback: string): string {
  // eslint-disable-next-line no-control-regex
  const cleaned = name.replace(/[<>:"/\\|?*\u0000-\u001f]+/g, " ").trim();
  return cleaned.replace(/\s+/g, " ").slice(0, 80) || fallback;
}

export async function pickSaveFile(opts: {
  title: string;
  defaultName: string;
  filterName: string;
  extension: string;
}): Promise<string | null> {
  const path = await save({
    title: opts.title,
    defaultPath: opts.defaultName,
    filters: [{ name: opts.filterName, extensions: [opts.extension] }],
  });
  return typeof path === "string" ? path : null;
}

export async function pickOpenFile(opts: {
  title: string;
  filterName: string;
  extension: string;
}): Promise<string | null> {
  const path = await open({
    title: opts.title,
    multiple: false,
    directory: false,
    filters: [{ name: opts.filterName, extensions: [opts.extension] }],
  });
  return typeof path === "string" ? path : null;
}

/** Why an import failed, in plain words. */
export function fileProblemText(t: TFunction, problem: UserFileProblem): string {
  return t(`userData.fileProblem.${problem}`);
}
