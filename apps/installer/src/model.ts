import type { MessageKey } from "./i18n";

export const STEPS = ["welcome", "license", "location", "install", "done"] as const;
export type Step = (typeof STEPS)[number];

export type Stage = "extracting" | "installing" | "finishing";

export interface Existing {
  dir: string;
  version: string | null;
  newer: boolean;
  same: boolean;
}

export interface SetupInfo {
  version: string;
  defaultDir: string;
  existing: Existing | null;
  hasPayload: boolean;
  appRunning: boolean;
  license: string;
}

/** Folder errors returned by `check_folder` / `install` that have their own message. */
const FOLDER_ERRORS = ["notAbsolute", "notWritable", "notEmpty"] as const;
export type FolderError = (typeof FOLDER_ERRORS)[number];

export function folderError(code: string | null): FolderError | null {
  return FOLDER_ERRORS.find((e) => e === code) ?? null;
}

export function folderErrorKey(code: FolderError): MessageKey {
  return `error.${code}`;
}

/** Progress bar value for a stage; the long "installing" stage creeps toward 90 %. */
export function stageProgress(stage: Stage | null, elapsedMs: number): number {
  switch (stage) {
    case null:
      return 0;
    case "extracting":
      return 8;
    case "installing":
      return 12 + 78 * (1 - Math.exp(-elapsedMs / 9000));
    case "finishing":
      return 96;
  }
}
