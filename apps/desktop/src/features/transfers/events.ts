import { useEffect } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { commands, events } from "@/ipc";
import { useTransferQueue } from "./store";

/**
 * Feeds the transfer queue from backend events: `transferProgress` (fetch / pull / push) and
 * `lfsTransferProgress` (LFS up/downloads). Also picks up LFS transfers still running after a
 * window reload. Mount once.
 */
export function useTransferQueueEvents() {
  useEffect(() => {
    if (!isTauri()) return;
    const unlisten: (() => void)[] = [];
    let disposed = false;
    const keep = (fn: () => void) => (disposed ? fn() : unlisten.push(fn));
    void events.transferProgress
      .listen(({ payload }) => useTransferQueue.getState().gitProgress(payload))
      .then(keep);
    void events.lfsTransferProgress
      .listen(({ payload }) => useTransferQueue.getState().lfsProgress(payload))
      .then(keep);
    void commands
      .lfsTransferQueue()
      .then((list) => {
        if (disposed) return;
        for (const p of list) useTransferQueue.getState().lfsProgress(p);
      })
      .catch(() => undefined);
    return () => {
      disposed = true;
      for (const fn of unlisten) fn();
    };
  }, []);
}
