import type { TFunction } from "i18next";
import { type AiSuggestion, commands, errorMessage } from "@/ipc";
import { type Notice, useUi } from "@/stores/ui";
import { useSettings } from "@/stores/settings";
import { useWorktreeStore, EMPTY_DRAFT } from "@/stores/worktree";
import { type DraftText, errorDetail, errorFix, isAiError, newRequestId } from "./model";
import { openAiSettings, useAiUi } from "./store";

/** Plain-words text of an AI error (settings dialog, commit panel). */
export function aiErrorText(t: TFunction, e: unknown): string {
  if (!isAiError(e)) return errorMessage(e);
  return t(`ai.error.${e.kind}`, {
    model: "model" in e ? e.model : "",
    status: "status" in e ? e.status : "",
    message: e.message,
  });
}

/** Plain-words notice for a failed suggestion, with the quick fix the error allows. */
export function aiErrorNotice(
  t: TFunction,
  e: unknown,
  retry: (unstaged: boolean) => void,
  unstaged: boolean,
): Notice | null {
  if (!isAiError(e)) {
    return { kind: "error", message: t("ai.error.internal"), detail: errorMessage(e) };
  }
  if (e.kind === "cancelled") return null;
  const fix = errorFix(e);
  const actions: Notice["actions"] = [];
  if (fix === "settings") actions.push({ label: t("ai.openSettings"), run: openAiSettings });
  if (fix === "unstaged") actions.push({ label: t("ai.useUnstaged"), run: () => retry(true) });
  if (fix === "retry") actions.push({ label: t("ai.retry"), run: () => retry(unstaged) });
  return {
    kind: e.kind === "nothingStaged" ? "info" : "error",
    message: aiErrorText(t, e),
    hint: e.kind === "nothingStaged" && e.hasUnstaged ? t("ai.unstagedHint") : undefined,
    detail: errorDetail(e),
    actions: actions.length ? actions : undefined,
  };
}

export const currentDraft = (repoId: number): DraftText => {
  const d = useWorktreeStore.getState().drafts[repoId] ?? EMPTY_DRAFT;
  return { summary: d.summary, body: d.body };
};

/** Fills the draft with `next`, remembering the old text for one "Hoàn tác". */
export function applySuggestion(repoId: number, next: DraftText) {
  const previous = currentDraft(repoId);
  useWorktreeStore.getState().updateDraft(repoId, next);
  useAiUi.getState().setUndo(repoId, { previous, applied: next });
}

export function undoSuggestion(repoId: number) {
  const undo = useAiUi.getState().undo[repoId];
  if (!undo) return;
  useWorktreeStore.getState().updateDraft(repoId, undo.previous);
  useAiUi.getState().setUndo(repoId, null);
}

/**
 * Asks the backend for a suggestion. Returns `null` when it failed (a notice is shown) or was
 * cancelled. A second call while one is running cancels it.
 */
export async function requestSuggestion(
  t: TFunction,
  repoId: number,
  unstaged: boolean,
  retry: (unstaged: boolean) => void,
): Promise<AiSuggestion | null> {
  const ui = useAiUi.getState();
  const running = ui.generating[repoId];
  if (running) {
    ui.setGenerating(repoId, null);
    await commands.aiCancel(running).catch(() => false);
    return null;
  }
  const id = newRequestId();
  ui.setGenerating(repoId, id);
  try {
    const s = await commands.aiGenerate(repoId, id, useSettings.getState().language, unstaged);
    return useAiUi.getState().generating[repoId] === id ? s : null;
  } catch (e) {
    if (useAiUi.getState().generating[repoId] === id) {
      const notice = aiErrorNotice(t, e, retry, unstaged);
      if (notice) useUi.getState().showNotice(notice);
    }
    return null;
  } finally {
    if (useAiUi.getState().generating[repoId] === id) {
      useAiUi.getState().setGenerating(repoId, null);
    }
  }
}
