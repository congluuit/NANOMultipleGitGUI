import { useState } from "react";
import { LoaderCircle, Sparkles, Undo2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Button } from "@/components/ui/button";
import { useUi } from "@/stores/ui";
import { useDraft } from "@/stores/worktree";
import { type DraftText, hasText, sameText, sendsCodeAway, serviceName } from "./model";
import { useAiConfig } from "./queries";
import { useAiUi } from "./store";
import { applySuggestion, currentDraft, requestSuggestion, undoSuggestion } from "./suggest";

/**
 * ✨ next to the commit summary (only when AI suggestions are on): fills summary + body from
 * the staged changes. Text already typed is only replaced after asking; one "Hoàn tác" brings
 * it back.
 */
export function AiSuggestButton({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const config = useAiConfig();
  const busy = useAiUi((s) => s.generating[repoId] !== undefined);
  const undo = useAiUi((s) => s.undo[repoId]);
  const draft = useDraft(repoId);
  const [pending, setPending] = useState<DraftText | null>(null);

  if (!config?.enabled) return null;

  const fill = (next: DraftText, unstaged: boolean) => {
    applySuggestion(repoId, next);
    if (unstaged) {
      useUi.getState().showNotice({ kind: "info", message: t("ai.unstagedUsed") });
    }
  };

  const run = async (unstaged: boolean): Promise<void> => {
    const s = await requestSuggestion(t, repoId, unstaged, (u) => void run(u));
    if (!s) return;
    const next = { summary: s.summary, body: s.body };
    const now = currentDraft(repoId);
    if (hasText(now) && !sameText(now, next)) {
      setPending(next);
      if (s.source === "unstaged") {
        useUi.getState().showNotice({ kind: "info", message: t("ai.unstagedUsed") });
      }
      return;
    }
    fill(next, s.source === "unstaged");
  };

  const canUndo =
    undo !== undefined && sameText({ summary: draft.summary, body: draft.body }, undo.applied);
  const title = busy
    ? t("ai.generatingCancel")
    : sendsCodeAway(config)
      ? `${t("ai.suggest")}\n${t("ai.sendsTo", { service: serviceName(config) })}`
      : t("ai.suggest");

  return (
    <>
      {canUndo && (
        <Button
          variant="ghost"
          size="icon-sm"
          className="shrink-0"
          onClick={() => undoSuggestion(repoId)}
          title={t("ai.undo")}
          aria-label={t("ai.undo")}
        >
          <Undo2 />
        </Button>
      )}
      <Button
        variant="ghost"
        size="icon-sm"
        className="shrink-0 text-primary"
        onClick={() => void run(false)}
        title={title}
        aria-label={busy ? t("ai.generatingCancel") : t("ai.suggest")}
        aria-busy={busy}
        data-testid="ai-suggest"
      >
        {busy ? <LoaderCircle className="animate-spin" /> : <Sparkles />}
      </Button>
      <ConfirmDialog
        open={pending !== null}
        title={t("ai.replaceTitle")}
        description={t("ai.replaceBody")}
        confirmLabel={t("ai.replace")}
        onConfirm={() => {
          if (pending) applySuggestion(repoId, pending);
          setPending(null);
        }}
        onCancel={() => setPending(null)}
      >
        {pending && (
          <pre className="max-h-40 overflow-auto rounded-md border bg-muted/40 p-2 text-xs whitespace-pre-wrap">
            {pending.body ? `${pending.summary}\n\n${pending.body}` : pending.summary}
          </pre>
        )}
      </ConfirmDialog>
    </>
  );
}
