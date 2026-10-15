import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { formatBytes } from "@/lib/format";
import { useUi } from "@/stores/ui";
import { warningText } from "./model";
import { useStorageStatus } from "./queries";
import { StorageDialog } from "./StorageDialog";
import { openStorageSettings, useStorageUi } from "./store";

/**
 * Mounted once in `App`: the Storage dialog, the startup warnings about the storage folders
 * (pointer unreadable, folder missing, data moved / move failed) and the "less than 1 GB free
 * on the data drive" notice, checked at start and every 30 minutes.
 */
export function StorageHost() {
  const { t } = useTranslation();
  const status = useStorageStatus().data;
  /** The low-space notice was shown for the current low period. */
  const lowNotified = useRef(false);

  useEffect(() => {
    if (!status) return;
    const ui = useUi.getState();
    const openAction = { label: t("storage.openSettings"), run: openStorageSettings };
    if (!useStorageUi.getState().noticesShown) {
      useStorageUi.getState().markNoticesShown();
      const texts = status.warnings.map((w) => warningText(t, w));
      const first = texts.find((w) => w.kind === "error") ?? texts[0];
      if (first) {
        const others = texts.filter((w) => w !== first).map((w) => w.message);
        ui.showNotice({
          kind: first.kind,
          message: first.message,
          hint: [first.hint, ...others].filter(Boolean).join("\n") || undefined,
          detail: first.detail,
          actions: [openAction],
        });
        return;
      }
    }
    if (!status.lowSpace) {
      lowNotified.current = false;
      return;
    }
    if (lowNotified.current || ui.notice) return;
    lowNotified.current = true;
    ui.showNotice({
      kind: "error",
      message: t("storage.lowSpace", { free: formatBytes(status.dataFreeBytes ?? 0) }),
      hint: t("storage.lowSpaceHint", { path: status.dataDir }),
      actions: [openAction],
    });
  }, [status, t]);

  return <StorageDialog />;
}
