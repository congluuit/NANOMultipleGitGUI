import type { Notice } from "@/stores/ui";

/**
 * How long a notice stays before hiding itself (paused while the pointer or focus is on it):
 * a quick confirmation ("Switched", "Copied") goes after 3 s; an error, or anything with a
 * hint / details / actions to read, stays 5 s.
 */
export const NOTICE_SHORT_MS = 3_000;
export const NOTICE_LONG_MS = 5_000;

export function noticeDuration(notice: Notice): number {
  const rich = Boolean(notice.hint || notice.detail || notice.actions?.length);
  return notice.kind === "error" || rich ? NOTICE_LONG_MS : NOTICE_SHORT_MS;
}
