import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { continueRebase, skipRebaseCommit } from "@/features/rebase/actions";
import { commands, type OperationState } from "@/ipc";
import { conflictText, runOutcome } from "./outcome";

const opStateKey = (repoId: number) => ["opstate", repoId] as const;

/** Shown while a merge / rebase / cherry-pick / revert is stopped, with abort and continue. */
export function OperationBanner({ repoId }: { repoId: number }) {
  const state = useQuery({
    queryKey: opStateKey(repoId),
    queryFn: () => commands.repoOperationState(repoId),
    enabled: isTauri(),
  });
  const s = state.data;
  if (!s?.operation || s.operation === "bisect") return null;
  return <OperationBannerView repoId={repoId} state={s} />;
}

/** The banner for a known state (tested on its own). */
export function OperationBannerView({
  repoId,
  state: s,
}: {
  repoId: number;
  state: OperationState;
}) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const op = s.operation;
  if (!op) return null;
  const rebase = op === "rebase";
  const hasStep = s.step !== null && s.totalSteps !== null;

  const run = async (kind: "abort" | "continue" | "skip") => {
    setBusy(true);
    if (kind === "abort") {
      await runOutcome(
        repoId,
        () => commands.operationAbort(repoId),
        (o) => t("ops.aborted", { op: t(`ops.kind.${o}`) }),
      );
    } else if (rebase) {
      await (kind === "skip" ? skipRebaseCommit(repoId) : continueRebase(repoId));
    } else {
      await runOutcome(
        repoId,
        () => commands.operationContinue(repoId),
        (o) => conflictText(o) || t("ops.continued", { op: t(`ops.kind.${op}`) }),
      );
    }
    setBusy(false);
  };

  return (
    <div
      role="status"
      className="flex shrink-0 items-center gap-2 border-b bg-warning/10 px-3 py-1.5 text-xs"
    >
      <TriangleAlert className="size-4 shrink-0 text-warning" />
      <span className="min-w-0 flex-1">
        {rebase ? (
          <span className="font-semibold">
            {hasStep
              ? t("rebase.banner.title", { step: s.step, total: s.totalSteps })
              : t("rebase.banner.titleNoStep")}
          </span>
        ) : (
          <>
            <span className="font-semibold">
              {t("ops.inProgress", { op: t(`ops.kind.${op}`) })}
            </span>
            {hasStep && (
              <span className="ml-1 text-muted-foreground">
                {t("ops.step", { step: s.step, total: s.totalSteps })}
              </span>
            )}
          </>
        )}
        <span className="ml-2 text-muted-foreground">
          {s.conflicts.length
            ? t("ops.conflictsLeft", { count: s.conflicts.length })
            : rebase
              ? t("rebase.banner.stoppedEdit")
              : t("ops.readyToContinue")}
        </span>
      </span>
      <Button variant="outline" size="xs" disabled={busy} onClick={() => void run("abort")}>
        {t("ops.abort")}
      </Button>
      {rebase && (
        <Button
          variant="outline"
          size="xs"
          disabled={busy}
          title={t("rebase.banner.skipTip")}
          onClick={() => void run("skip")}
        >
          {t("rebase.banner.skip")}
        </Button>
      )}
      <Button
        size="xs"
        disabled={busy || s.conflicts.length > 0}
        onClick={() => void run("continue")}
      >
        {t("ops.continue")}
      </Button>
    </div>
  );
}
