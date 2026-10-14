import type { ChangeKind } from "@/ipc";

const LETTERS: Record<ChangeKind, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  copied: "C",
  typeChanged: "T",
  untracked: "U",
  conflicted: "!",
};

export const KIND_CLASS: Record<ChangeKind, string> = {
  added: "text-success",
  modified: "text-warning",
  deleted: "text-danger",
  renamed: "text-info",
  copied: "text-info",
  typeChanged: "text-warning",
  untracked: "text-success",
  conflicted: "text-danger",
};

export const kindLetter = (kind: ChangeKind) => LETTERS[kind];

/**
 * New selection after clicking `key` in `order`: plain click selects only it, Ctrl/Cmd toggles
 * it, Shift extends from `anchor`.
 */
export function clickSelect(
  order: string[],
  selected: ReadonlySet<string>,
  anchor: string | null,
  key: string,
  mods: { ctrl: boolean; shift: boolean },
): Set<string> {
  if (mods.shift && anchor !== null) {
    const a = order.indexOf(anchor);
    const b = order.indexOf(key);
    if (a >= 0 && b >= 0) {
      const [from, to] = a <= b ? [a, b] : [b, a];
      return new Set([...(mods.ctrl ? selected : []), ...order.slice(from, to + 1)]);
    }
  }
  if (mods.ctrl) {
    const next = new Set(selected);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    return next;
  }
  return new Set([key]);
}
