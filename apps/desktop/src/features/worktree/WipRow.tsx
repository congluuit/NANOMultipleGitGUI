import { PencilLine } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { useGraphStore, useSelection } from "@/stores/graph";
import { useWorktreeStatus } from "./queries";

/** "Work in progress" row above the history: uncommitted changes, click to show staging. */
export function WipRow({ repoId }: { repoId: number }) {
  const { t } = useTranslation();
  const status = useWorktreeStatus(repoId);
  const selection = useSelection(repoId);
  const select = useGraphStore((s) => s.select);
  const st = status.data;
  if (!st) return null;
  const total = st.staged.length + st.unstaged.length + st.conflicted.length;
  if (total === 0) return null;
  const active = selection === null;
  return (
    <button
      type="button"
      onClick={() => select(repoId, null)}
      title={t("wip.rowTitle")}
      className={cn(
        "flex h-7 shrink-0 items-center gap-2 border-b px-3 text-left text-xs hover:bg-surface-2",
        active && "bg-primary/10 shadow-[inset_3px_0_0_var(--primary)]",
      )}
    >
      <PencilLine className="size-3.5 text-primary" />
      <span className="font-semibold">{t("wip.row")}</span>
      <span className="text-muted-foreground">{t("wip.count", { count: total })}</span>
      {st.staged.length > 0 && (
        <span className="rounded bg-success/15 px-1 text-[11px] text-success">
          {t("wip.stagedCount", { count: st.staged.length })}
        </span>
      )}
      {st.conflicted.length > 0 && (
        <span className="rounded bg-danger/15 px-1 text-[11px] text-danger">
          {t("wip.conflictCount", { count: st.conflicted.length })}
        </span>
      )}
    </button>
  );
}
