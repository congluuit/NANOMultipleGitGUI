import type { TFunction } from "i18next";
import type { AccountPreview, AccountsImportReport, BackupKind, BackupProblem } from "@/ipc";

/** Same rule as the backend (`nanomgg_backup::crypto::MIN_PASSWORD_CHARS`). */
export const MIN_PASSWORD_CHARS = 8;

export const ACCOUNTS_EXTENSION = "nanomgg-accounts";
export const FULL_EXTENSION = "nanomgg-backup";

/** Why a new file password cannot be used yet (`null` = fine). Counts characters, not bytes. */
export function passwordProblem(password: string, confirm: string): "tooShort" | "mismatch" | null {
  if ([...password].length < MIN_PASSWORD_CHARS) return "tooShort";
  if (password !== confirm) return "mismatch";
  return null;
}

export function passwordProblemText(t: TFunction, problem: "tooShort" | "mismatch"): string {
  return problem === "tooShort"
    ? t("backup.password.tooShort", { min: MIN_PASSWORD_CHARS })
    : t("backup.password.mismatch");
}

/** What went wrong with a chosen file and how to fix it. `expected` = the dialog's kind. */
export function problemText(
  t: TFunction,
  problem: BackupProblem,
  expected: BackupKind,
): { message: string; hint?: string } {
  switch (problem) {
    case "wrongPassword":
      return {
        message: t("backup.problem.wrongPassword"),
        hint: t("backup.problem.wrongPasswordHint"),
      };
    case "wrongKind":
      return {
        message: t(
          expected === "accounts"
            ? "backup.problem.wrongKindAccounts"
            : "backup.problem.wrongKindFull",
        ),
      };
    case "notABackup":
    case "newerVersion":
    case "tooLarge":
    case "corrupt":
    case "weakPassword":
      return { message: t(`backup.problem.${problem}`) };
  }
}

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

/** `nanomgg-accounts-2026-10-15.nanomgg-accounts` / `nanomgg-backup-2026-10-15.nanomgg-backup`. */
export function defaultFileName(kind: BackupKind, now: Date = new Date()): string {
  const day = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
  const ext = kind === "accounts" ? ACCOUNTS_EXTENSION : FULL_EXTENSION;
  return `${ext}-${day}.${ext}`;
}

/** Ticked by default when opening a file: accounts that are not on this machine yet. */
export function defaultSelection(accounts: readonly AccountPreview[]): string[] {
  return accounts.filter((a) => !a.existingAccountId).map((a) => a.key);
}

/** Toggles `key` in a selection, keeping the file order. */
export function toggleKey(
  selection: readonly string[],
  key: string,
  order: readonly string[],
): string[] {
  const set = new Set(selection);
  if (set.has(key)) set.delete(key);
  else set.add(key);
  return order.filter((k) => set.has(k));
}

/** "Đã thêm X tài khoản, bỏ qua Y" plus follow-up lines. */
export function importSummary(
  t: TFunction,
  report: AccountsImportReport,
): { message: string; lines: string[] } {
  let message = t("backup.accountsImport.result", {
    added: report.added,
    skipped: report.skipped,
  });
  if (report.updated > 0) {
    message += ` · ${t("backup.accountsImport.resultUpdated", { count: report.updated })}`;
  }
  const lines: string[] = [];
  if (report.needSignIn.length > 0) {
    lines.push(t("backup.accountsImport.needSignIn", { count: report.needSignIn.length }));
  }
  if (report.signingDisabled > 0) {
    lines.push(t("backup.accountsImport.signingDisabled", { count: report.signingDisabled }));
  }
  return { message, lines };
}
