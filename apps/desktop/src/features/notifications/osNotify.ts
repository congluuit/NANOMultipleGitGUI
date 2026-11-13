/**
 * Operating-system notifications: when a message appears while the window is in the
 * background (a push that finished, an error, a fetch summary), the same text is shown as a
 * toast, so work running behind another window does not end unnoticed.
 */
import { useEffect } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { commands } from "@/ipc";
import { useSettings } from "@/stores/settings";
import { useUi, type Notice } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";

/** The same text is not shown twice within this time. */
const REPEAT_MS = 10_000;

export interface OsMessage {
  title: string;
  body: string;
}

/** Title and body of the toast for a notice: the repository first when it names one. */
export function osMessage(notice: Notice, repoName: string | null): OsMessage {
  const body = notice.hint ?? "";
  return repoName
    ? { title: repoName, body: body ? `${notice.message}\n${body}` : notice.message }
    : { title: notice.message, body };
}

/** Whether the user can see the app right now (then the notice bar is enough). */
export const windowInBackground = () => document.hidden || !document.hasFocus();

export function useOsNotifications() {
  useEffect(() => {
    if (!isTauri()) return;
    let last = { key: "", at: 0 };
    return useUi.subscribe((state, prev) => {
      const notice = state.notice;
      if (!notice || notice === prev.notice) return;
      if (!useSettings.getState().osNotifications || !windowInBackground()) return;
      const repo =
        notice.repoId === undefined
          ? null
          : (useWorkspace.getState().tabs.find((t) => t.id === notice.repoId)?.name ?? null);
      const message = osMessage(notice, repo);
      const key = `${message.title}\n${message.body}`;
      const now = Date.now();
      if (key === last.key && now - last.at < REPEAT_MS) return;
      last = { key, at: now };
      commands
        .osNotify(message.title, message.body)
        .catch((e: unknown) => console.debug("OS notification skipped", e));
    });
  }, []);
}
