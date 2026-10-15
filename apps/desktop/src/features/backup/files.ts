import { open, save } from "@tauri-apps/plugin-dialog";
import type { TFunction } from "i18next";
import type { BackupKind } from "@/ipc";
import { ACCOUNTS_EXTENSION, FULL_EXTENSION, defaultFileName } from "./model";

function filters(t: TFunction, kind: BackupKind) {
  return kind === "accounts"
    ? [{ name: t("backup.file.filterAccounts"), extensions: [ACCOUNTS_EXTENSION] }]
    : [{ name: t("backup.file.filterFull"), extensions: [FULL_EXTENSION] }];
}

/** Save dialog for a new file; `null` when cancelled. The backend writes it. */
export async function pickSavePath(t: TFunction, kind: BackupKind): Promise<string | null> {
  const path = await save({
    title: t(kind === "accounts" ? "backup.accountsExport.saveTitle" : "backup.full.saveTitle"),
    defaultPath: defaultFileName(kind),
    filters: filters(t, kind),
  });
  return path ?? null;
}

/** Open dialog for an existing file; `null` when cancelled. The backend reads it. */
export async function pickOpenPath(t: TFunction, kind: BackupKind): Promise<string | null> {
  const path = await open({
    title: t(kind === "accounts" ? "backup.accountsImport.openTitle" : "backup.full.openTitle"),
    multiple: false,
    directory: false,
    filters: [...filters(t, kind), { name: t("backup.file.filterAll"), extensions: ["*"] }],
  });
  return typeof path === "string" ? path : null;
}
