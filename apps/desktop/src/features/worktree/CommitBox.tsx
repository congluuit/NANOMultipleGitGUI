import { useState } from "react";
import { LoaderCircle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { CommitAs, asAccountOf, settleOneShot, takeOneShot } from "@/features/accounts/switcher";
import { peekOneShot } from "@/features/accounts/switcher/oneShot";
import { runCommitGuard } from "@/features/guards";
import { useLargeFileGuard } from "@/features/lfs/useLargeFileGuard";
import { commitErrorNotice } from "@/features/signing/commitError";
import { commands } from "@/ipc";
import { cn } from "@/lib/utils";
import { useUi } from "@/stores/ui";
import { useDraft, useWorktreeStore } from "@/stores/worktree";
import { invalidateRepo } from "./queries";

const SUMMARY_LIMIT = 72;

interface Props {
  repoId: number;
  stagedCount: number;
  conflictCount: number;
  /** HEAD exists (amend is possible). */
  hasHead: boolean;
  /** Staged paths, checked by the Large File Guard before committing. */
  stagedPaths?: string[];
}

export function CommitBox({
  repoId,
  stagedCount,
  conflictCount,
  hasHead,
  stagedPaths = [],
}: Props) {
  const { t } = useTranslation();
  const draft = useDraft(repoId);
  const update = useWorktreeStore((s) => s.updateDraft);
  const clear = useWorktreeStore((s) => s.clearDraft);
  const guard = useLargeFileGuard(repoId);
  const [busy, setBusy] = useState(false);

  const message = draft.body.trim()
    ? `${draft.summary.trim()}\n\n${draft.body.trim()}`
    : draft.summary.trim();
  const blocked =
    conflictCount > 0
      ? t("commit.hasConflicts")
      : stagedCount === 0 && !draft.amend
        ? t("commit.nothingStaged")
        : null;
  const canCommit = !busy && !blocked && draft.summary.trim().length > 0;

  const toggleAmend = async (amend: boolean) => {
    update(repoId, { amend });
    if (amend && !draft.summary.trim() && !draft.body.trim()) {
      const last = await commands.headMessage(repoId).catch(() => null);
      if (last) {
        const [summary = "", ...rest] = last.split("\n");
        const body = rest.join("\n").trim();
        update(repoId, { summary, body, prefill: last });
      }
    } else if (!amend && draft.prefill !== null && message === draft.prefill.trim()) {
      update(repoId, { summary: "", body: "", prefill: null });
    }
  };

  const submit = async () => {
    if (!canCommit) return;
    setBusy(true);
    // Commit Guard (PLAN §4.6) first, unless the user explicitly picked "Commit as…".
    if (!peekOneShot(repoId, "commit") && (await runCommitGuard(repoId)) === "cancel") {
      setBusy(false);
      return;
    }
    // Large File Guard (PLAN §6.2): big or binary files may be sent to LFS first.
    const held = await guard.intercept(stagedPaths, () => void commit());
    if (held) {
      setBusy(false);
      return;
    }
    await commit();
  };

  const commit = async () => {
    setBusy(true);
    // "Commit as…" (switcher): consumed right before the command, given back if it fails.
    const shot = takeOneShot(repoId, "commit");
    try {
      const result = await commands.commitCreate(
        repoId,
        message,
        { amend: draft.amend, signoff: draft.signoff, allowEmpty: false },
        asAccountOf(shot),
      );
      settleOneShot(repoId, shot, true);
      clear(repoId);
      useUi.getState().showNotice({
        kind: "info",
        message: t(draft.amend ? "commit.amended" : "commit.done", {
          hash: result.hash.slice(0, 7),
          summary: result.summary,
        }),
      });
    } catch (e) {
      settleOneShot(repoId, shot, false);
      useUi.getState().showNotice(commitErrorNotice(e, repoId));
    } finally {
      setBusy(false);
      invalidateRepo(repoId);
    }
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void submit();
    }
  };

  const over = draft.summary.length > SUMMARY_LIMIT;

  return (
    <div className="space-y-2 border-t p-3">
      <div className="relative">
        <input
          value={draft.summary}
          onChange={(e) => update(repoId, { summary: e.target.value })}
          onKeyDown={onKeyDown}
          placeholder={t("commit.summary")}
          aria-label={t("commit.summary")}
          className="h-8 w-full rounded-md border bg-background pr-12 pl-2 text-xs focus:border-primary focus:outline-none"
        />
        <span
          className={cn(
            "absolute top-1/2 right-2 -translate-y-1/2 font-mono text-[10px] tabular-nums",
            over ? "text-warning" : "text-muted-foreground",
          )}
          title={t("commit.summaryHint", { limit: SUMMARY_LIMIT })}
        >
          {SUMMARY_LIMIT - draft.summary.length}
        </span>
      </div>
      <textarea
        value={draft.body}
        onChange={(e) => update(repoId, { body: e.target.value })}
        onKeyDown={onKeyDown}
        placeholder={t("commit.body")}
        aria-label={t("commit.body")}
        rows={4}
        className="w-full resize-y rounded-md border bg-background px-2 py-1.5 font-mono text-xs focus:border-primary focus:outline-none"
      />
      <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs">
        <label className="flex items-center gap-1.5">
          <input
            type="checkbox"
            checked={draft.amend}
            disabled={!hasHead}
            onChange={(e) => void toggleAmend(e.target.checked)}
            className="accent-primary"
          />
          {t("commit.amend")}
        </label>
        <label className="flex items-center gap-1.5" title={t("commit.signoffHint")}>
          <input
            type="checkbox"
            checked={draft.signoff}
            onChange={(e) => update(repoId, { signoff: e.target.checked })}
            className="accent-primary"
          />
          {t("commit.signoff")}
        </label>
      </div>
      <CommitAs repoId={repoId} />
      <Button
        className="w-full"
        size="sm"
        disabled={!canCommit}
        onClick={() => void submit()}
        title={blocked ?? t("commit.shortcut")}
      >
        {busy && <LoaderCircle className="animate-spin" />}
        {draft.amend ? t("commit.submitAmend") : t("commit.submit", { count: stagedCount })}
      </Button>
      {blocked && <p className="text-center text-[11px] text-muted-foreground">{blocked}</p>}
      {guard.dialog}
    </div>
  );
}
