import type { TFunction } from "i18next";
import type { StorageKind, StorageMoveProgress, StorageWarning, TargetProblem } from "@/ipc";
import { formatBytes } from "@/lib/format";

export const STORAGE_KINDS: StorageKind[] = ["log", "cache", "data"];

/** Same threshold as the backend (`LOW_SPACE_BYTES`). */
export const LOW_SPACE_BYTES = 1024 ** 3;

/** Why a folder cannot be used, with what to do about it. */
export function problemText(t: TFunction, problem: TargetProblem): string {
  switch (problem.kind) {
    case "notEmpty":
      return t("storage.problem.notEmpty", { path: problem.path });
    case "notWritable":
      return t("storage.problem.notWritable", { message: problem.message });
    case "notEnoughSpace":
      return t("storage.problem.notEnoughSpace", {
        needed: formatBytes(problem.needed),
        free: formatBytes(problem.free),
      });
    default:
      return t(`storage.problem.${problem.kind}`);
  }
}

export interface WarningText {
  kind: "error" | "info";
  message: string;
  hint?: string;
  detail?: string;
}

export function warningText(t: TFunction, w: StorageWarning): WarningText {
  switch (w.kind) {
    case "pointerUnreadable":
      return {
        kind: "error",
        message: t("storage.warning.pointerUnreadable"),
        hint: t("storage.warning.pointerUnreadableHint"),
        detail: `${w.path}\n${w.message}`,
      };
    case "missingFolder":
      return {
        kind: "error",
        message: t("storage.warning.missingFolder", {
          kind: t(`storage.kind.${w.storage}`),
          path: w.path,
        }),
        hint: t("storage.warning.missingFolderHint", { fallback: w.fallback }),
      };
    case "dataMoved":
      return { kind: "info", message: t("storage.warning.dataMoved", { to: w.to }) };
    case "dataMoveFailed":
      return {
        kind: "error",
        message: t("storage.warning.dataMoveFailed", { to: w.to }),
        hint: t("storage.warning.dataMoveFailedHint", { from: w.from }),
        detail: w.message,
      };
    case "oldFilesLeft":
      return {
        kind: "info",
        message: t("storage.warning.oldFilesLeft", { count: w.count, path: w.path }),
      };
  }
}

/** 0–100 for the progress bar (copying and verifying each take half). */
export function progressPercent(p: StorageMoveProgress): number {
  if (p.phase === "cleaning") return 100;
  const part =
    p.totalBytes > 0
      ? p.doneBytes / p.totalBytes
      : p.totalFiles > 0
        ? p.doneFiles / p.totalFiles
        : 1;
  const base = p.phase === "verifying" ? 50 : 0;
  return Math.min(100, Math.round(base + part * 50));
}
