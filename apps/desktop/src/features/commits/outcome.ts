import i18n from "i18next";
import { errorMessage, type FailureKind, type OpOutcome, type Outcome } from "@/ipc";
import { invalidateRepo } from "@/features/worktree/queries";
import { useUi } from "@/stores/ui";

/** User-facing text for a rejected operation: our explanation, then git's own message. */
export function failureText(reason: FailureKind, message: string): string {
  const label = i18n.t(`failure.${reason}`);
  const detail = message.trim();
  return detail && detail !== label ? `${label} — ${detail}` : label;
}

/** Notice for an operation that stopped on conflicts (empty string when there are none). */
export function conflictText(outcome: Pick<OpOutcome, "conflicts">): string {
  return outcome.conflicts.length
    ? i18n.t("ops.conflicts", { count: outcome.conflicts.length })
    : "";
}

/**
 * Runs a write command returning an `Outcome`, reports rejections/cancellation/errors in the
 * notice bar, refreshes the repository and returns the value when it went through.
 */
export async function runOutcome<T>(
  repoId: number,
  call: () => Promise<Outcome<T>>,
  success?: (value: T) => string | null,
): Promise<T | undefined> {
  const notify = useUi.getState().showNotice;
  try {
    const out = await call();
    if (out.status === "done") {
      const text = success?.(out.value);
      if (text) notify({ kind: "info", message: text });
      return out.value;
    }
    if (out.status === "rejected") {
      notify({ kind: "error", message: failureText(out.reason, out.message) });
    } else {
      notify({ kind: "info", message: i18n.t("ops.cancelled") });
    }
  } catch (e) {
    notify({ kind: "error", message: errorMessage(e) });
  } finally {
    invalidateRepo(repoId);
  }
  return undefined;
}
