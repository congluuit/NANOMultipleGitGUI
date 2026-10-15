/**
 * Pure state of the interactive rebase dialog: rows (oldest first, the order git applies them),
 * reordering, actions, messages, validation, the preview of the result and the plan sent to
 * `rebase_interactive`.
 */
import type { RebaseCommit, RebasePreview, TodoAction, TodoStep } from "@/ipc";

export const ACTIONS: TodoAction[] = ["pick", "reword", "squash", "fixup", "drop", "edit"];

export interface TodoRow {
  oid: string;
  shortOid: string;
  subject: string;
  /** Original full message. */
  original: string;
  authorName: string;
  time: number;
  pushed: boolean;
  isMerge: boolean;
  action: TodoAction;
  /** Message typed by the user (`null` = the default for the action). */
  custom: string | null;
}

export function initialRows(commits: RebaseCommit[]): TodoRow[] {
  return commits.map((c) => ({
    oid: c.oid,
    shortOid: c.shortOid,
    subject: c.subject,
    original: c.message,
    authorName: c.authorName,
    time: c.time,
    pushed: c.pushed,
    isMerge: c.isMerge,
    action: "pick",
    custom: null,
  }));
}

const melds = (a: TodoAction) => a === "squash" || a === "fixup";

/** Moves row `from` to position `to` (clamped). */
export function moveRowTo(rows: TodoRow[], from: number, to: number): TodoRow[] {
  const target = Math.max(0, Math.min(rows.length - 1, to));
  if (from === target || from < 0 || from >= rows.length) return rows;
  const next = rows.slice();
  const [row] = next.splice(from, 1);
  next.splice(target, 0, row!);
  return next;
}

export const moveRow = (rows: TodoRow[], index: number, delta: number) =>
  moveRowTo(rows, index, index + delta);

export function setAction(rows: TodoRow[], index: number, action: TodoAction): TodoRow[] {
  return rows.map((r, i) =>
    i === index
      ? {
          ...r,
          action,
          // A message typed for another action does not carry over.
          custom: action === "reword" || action === "edit" ? r.custom : null,
        }
      : r,
  );
}

export function setMessage(rows: TodoRow[], index: number, message: string): TodoRow[] {
  return rows.map((r, i) => (i === index ? { ...r, custom: message } : r));
}

/** Row the commit at `index` melds into (squash / fixup), skipping dropped rows. */
export function groupHead(rows: TodoRow[], index: number): number | null {
  for (let j = index - 1; j >= 0; j--) {
    const a = rows[j]!.action;
    if (a === "drop" || melds(a)) continue;
    return j;
  }
  return null;
}

/** Last squash row of the group that ends at or after `index` (the one carrying the message). */
function isLastSquash(rows: TodoRow[], index: number): boolean {
  for (let k = index + 1; k < rows.length; k++) {
    const a = rows[k]!.action;
    if (a === "squash") return false;
    if (a === "fixup" || a === "drop") continue;
    return true;
  }
  return true;
}

/** Message of a kept (pick / reword / edit) row. */
function ownMessage(row: TodoRow): string {
  return row.action !== "pick" && row.custom !== null ? row.custom : row.original;
}

/** Default combined message for a squash row: its group head plus every squashed message. */
export function defaultSquashMessage(rows: TodoRow[], index: number): string {
  const head = groupHead(rows, index);
  const parts: string[] = [];
  if (head !== null) parts.push(ownMessage(rows[head]!));
  for (let k = (head ?? -1) + 1; k <= index; k++) {
    if (rows[k]!.action === "squash") parts.push(rows[k]!.original);
  }
  return parts.map((p) => p.trim()).join("\n\n");
}

/** The message shown in a row's editor. */
export function effectiveMessage(rows: TodoRow[], index: number): string {
  const row = rows[index]!;
  if (row.custom !== null) return row.custom;
  return row.action === "squash" ? defaultSquashMessage(rows, index) : row.original;
}

/** Whether a row shows a message editor: reword, edit, and the last squash of a group. */
export function hasEditor(rows: TodoRow[], index: number): boolean {
  const a = rows[index]!.action;
  return a === "reword" || a === "edit" || (a === "squash" && isLastSquash(rows, index));
}

export type TodoError =
  { kind: "merges" } | { kind: "squashFirst" } | { kind: "emptyMessage"; oid: string };

export function validate(rows: TodoRow[]): TodoError[] {
  const errors: TodoError[] = [];
  if (rows.some((r) => r.isMerge)) errors.push({ kind: "merges" });
  const first = rows.find((r) => r.action !== "drop");
  if (first && melds(first.action)) errors.push({ kind: "squashFirst" });
  rows.forEach((r, i) => {
    if (hasEditor(rows, i) && r.action !== "edit" && !effectiveMessage(rows, i).trim()) {
      errors.push({ kind: "emptyMessage", oid: r.shortOid });
    }
  });
  return errors;
}

/** Nothing would change: every row is a pick, in the original order. */
export function isUnchanged(rows: TodoRow[], original: RebaseCommit[]): boolean {
  return (
    rows.length === original.length &&
    rows.every((r, i) => r.action === "pick" && r.oid === original[i]!.oid)
  );
}

export interface PreviewCommit {
  /** Head row of the resulting commit. */
  oid: string;
  shortOid: string;
  subject: string;
  /** Commits melded into it. */
  folded: number;
  reworded: boolean;
  stops: boolean;
}

/** The commits the plan produces, oldest first, plus how many are dropped. */
export function previewResult(rows: TodoRow[]): { commits: PreviewCommit[]; dropped: number } {
  const commits: PreviewCommit[] = [];
  let dropped = 0;
  rows.forEach((r, i) => {
    if (r.action === "drop") {
      dropped++;
      return;
    }
    if (melds(r.action)) {
      const last = commits[commits.length - 1];
      if (!last) return; // invalid plan, reported by validate()
      last.folded++;
      if (r.action === "squash" && hasEditor(rows, i)) {
        last.subject = firstLine(effectiveMessage(rows, i));
        last.reworded = true;
      }
      return;
    }
    const message = ownMessage(r);
    commits.push({
      oid: r.oid,
      shortOid: r.shortOid,
      subject: firstLine(message),
      folded: 0,
      reworded: message.trim() !== r.original.trim(),
      stops: r.action === "edit",
    });
  });
  return { commits, dropped };
}

const firstLine = (m: string) => m.trim().split("\n")[0] ?? "";

/** The plan sent to the backend. */
export function toSteps(rows: TodoRow[]): TodoStep[] {
  return rows.map((r, i) => {
    let message: string | null = null;
    if (r.action === "reword") message = effectiveMessage(rows, i);
    else if (r.action === "squash" && hasEditor(rows, i)) message = effectiveMessage(rows, i);
    else if (r.action === "edit" && r.custom !== null && r.custom.trim() !== r.original.trim())
      message = r.custom;
    return { oid: r.oid, action: r.action, message };
  });
}

/** Whether the plain rebase may start with these options. */
export function canStartRebase(p: RebasePreview, allowPushed: boolean, autostash: boolean) {
  return (
    !p.upToDate && p.branch !== null && (p.pushed === 0 || allowPushed) && (!p.dirty || autostash)
  );
}
