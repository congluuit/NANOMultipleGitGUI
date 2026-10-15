import type { TFunction } from "i18next";
import type { UndoKind, UndoStep } from "@/ipc";

const MAX_SUBJECT = 60;

const clip = (s: string) => (s.length > MAX_SUBJECT ? `${s.slice(0, MAX_SUBJECT - 1)}…` : s);

/** "commit “abc”", "switch from main to topic"… */
export function describeOp(t: TFunction, op: UndoKind): string {
  const { kind, ...rest } = op;
  const values = Object.fromEntries(
    Object.entries(rest).map(([k, v]) => [k, clip(String(v).trim())]),
  );
  // The variables differ per kind; the i18n test checks both languages use the same ones.
  const translate = t as unknown as (key: string, options: Record<string, string>) => string;
  return translate(`undo.op.${kind}`, values);
}

/** Tooltip of the Undo (`forward = false`) or Redo button. */
export function stepTooltip(t: TFunction, step: UndoStep | null, forward: boolean): string {
  if (!step) return t(forward ? "undo.nothingRedo" : "undo.nothingUndo");
  const what = describeOp(t, step.op);
  const parts = [t(forward ? "undo.redoTip" : "undo.undoTip", { what })];
  if (step.blocked) parts.push(t(`undo.blocked.${step.blocked}`));
  else if (step.pushed) parts.push(t("undo.pushedNote"));
  return parts.join("\n");
}

/** A step can run now. */
export const isRunnable = (step: UndoStep | null | undefined): step is UndoStep =>
  !!step && step.blocked === null;
