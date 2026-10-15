import { useEffect } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { events } from "@/ipc";
import { checkForUpdates } from "./actions";
import { useUpdater } from "./store";

/** Delay before the startup check, so it does not compete with opening the last repos. */
export const STARTUP_DELAY_MS = 5_000;
/** While another dialog (onboarding, import wizard…) is open, the update dialog waits. */
export const BUSY_RETRY_MS = 3_000;

let startupCheckDone = false;

/** Test hook: lets the startup check run again. */
export function resetStartupCheck() {
  startupCheckDone = false;
}

function anotherDialogOpen(): boolean {
  return document.querySelector('[role="dialog"], [role="alertdialog"]') !== null;
}

/** Once per app session; the backend limits it further to once a day. */
export function useStartupCheck() {
  useEffect(() => {
    if (startupCheckDone || !isTauri()) return;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const run = () => {
      if (startupCheckDone) return;
      if (anotherDialogOpen()) {
        timer = setTimeout(run, BUSY_RETRY_MS);
        return;
      }
      startupCheckDone = true;
      void checkForUpdates({ startup: true });
    };
    timer = setTimeout(run, STARTUP_DELAY_MS);
    return () => clearTimeout(timer);
  }, []);
}

export function useUpdateProgressEvents() {
  useEffect(() => {
    if (!isTauri()) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void events.updateProgress
      .listen(({ payload }) => useUpdater.getState().setProgress(payload))
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);
}
