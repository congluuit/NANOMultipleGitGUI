import { useEffect, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { dirtyCount } from "./store";

/** Asks before the window closes while files edited in the app have unsaved changes. */
export function UnsavedQuitGuard() {
  const { t } = useTranslation();
  const [count, setCount] = useState(0);

  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void getCurrentWindow()
      .onCloseRequested((event) => {
        const n = dirtyCount();
        if (n === 0) return;
        event.preventDefault();
        setCount(n);
      })
      .then((fn) => (disposed ? fn() : (unlisten = fn)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  return (
    <ConfirmDialog
      open={count > 0}
      title={t("editor.quitTitle")}
      description={t("editor.quitBody", { count })}
      confirmLabel={t("editor.quitConfirm")}
      destructive
      onConfirm={() => void getCurrentWindow().destroy()}
      onCancel={() => setCount(0)}
    />
  );
}
