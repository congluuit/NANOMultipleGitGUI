import { Lock } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";
import { useActiveRepo } from "@/stores/workspace";
import { lockBadge } from "./lfsModel";
import { useLfsLocks, useLfsStatus } from "./queries";

/**
 * "🔒 owner" for a working-tree file locked with Git LFS. A file locked by someone else that
 * you have changed is shown as a warning: your change cannot be pushed until they unlock it.
 */
export function LfsLockBadge({ path, changed = true }: { path: string; changed?: boolean }) {
  const { t } = useTranslation();
  const repo = useActiveRepo();
  const status = useLfsStatus(repo?.id ?? -1, !!repo && !repo.missing);
  const locks = useLfsLocks(repo?.id, status.data?.usesLfs === true);
  const lock =
    locks.data?.status === "done" ? locks.data.value.locks.find((l) => l.path === path) : undefined;
  const badge = lockBadge(lock);
  if (!badge) return null;
  const warn = !badge.ours && changed;
  return (
    <span
      className={cn(
        "flex shrink-0 items-center gap-0.5 rounded px-1 text-[10px] font-semibold",
        badge.ours
          ? "bg-info/15 text-info"
          : warn
            ? "bg-danger/15 text-danger"
            : "bg-warning/15 text-warning",
      )}
      title={
        badge.ours
          ? t("lfsLocks.byYou")
          : warn
            ? t("lfsLocks.editingLocked", { owner: badge.owner })
            : t("lfsLocks.byOther", { owner: badge.owner })
      }
    >
      <Lock className="size-2.5" />
      {badge.ours ? t("lfsLocks.you") : badge.owner}
    </span>
  );
}
