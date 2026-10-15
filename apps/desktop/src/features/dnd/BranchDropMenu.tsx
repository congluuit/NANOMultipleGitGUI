import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { FastForward, GitBranch, GitMerge, GitPullRequestArrow } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { runOutcome } from "@/features/commits/outcome";
import { useRefDialog } from "@/features/commits/store";
import { openRebaseDialog } from "@/features/rebase/store";
import { commands } from "@/ipc";
import { dropChoices, type DropChoice } from "./choices";
import { useDnd, type BranchDrop } from "./store";

/** Label that follows the pointer while a branch is dragged. */
function DragGhost() {
  const { t } = useTranslation();
  const drag = useDnd((s) => s.drag);
  if (!drag) return null;
  return (
    <div
      aria-hidden
      className="pointer-events-none fixed z-50 flex items-center gap-1 rounded-full border bg-surface px-2 py-0.5 text-xs font-medium shadow-md"
      style={{ left: drag.x + 12, top: drag.y + 8 }}
    >
      <GitBranch className="size-3" />
      {drag.over ? t("dnd.ghost", { source: drag.source, target: drag.over }) : drag.source}
    </div>
  );
}

function DropMenu({ drop, onChoose }: { drop: BranchDrop; onChoose: (c: DropChoice) => void }) {
  const { t } = useTranslation();
  const { repoId, source, target } = drop;
  const relation = useQuery({
    queryKey: ["branch-relation", repoId, source, target],
    queryFn: () => commands.branchRelation(repoId, source, target),
    enabled: isTauri(),
    staleTime: 0,
    gcTime: 0,
  });
  const rel = relation.data?.status === "done" ? relation.data.value : null;
  const current = rel?.current ?? null;
  const close = () => useDnd.getState().setDrop(null);
  const note = (branch: string) =>
    current !== null && current !== branch ? (
      <span className="block text-[10px] text-muted-foreground">
        {t("dnd.needsCheckout", { branch })}
      </span>
    ) : null;

  return (
    <DropdownMenu open onOpenChange={(o) => !o && close()}>
      <DropdownMenuTrigger asChild>
        <span className="fixed size-0" style={{ left: drop.x, top: drop.y }} />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-72">
        <DropdownMenuLabel className="truncate">
          {t("dnd.menuTitle", { source, target })}
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        {relation.isPending && isTauri() && (
          <DropdownMenuLabel className="font-normal text-muted-foreground">
            {t("dnd.loading")}
          </DropdownMenuLabel>
        )}
        {rel && rel.ahead === 0 && (
          <DropdownMenuLabel className="font-normal text-muted-foreground">
            {t("dnd.upToDate", { source, target })}
          </DropdownMenuLabel>
        )}
        {dropChoices(rel).map((c) => (
          <DropdownMenuItem key={c} onSelect={() => onChoose(c)}>
            {c === "fastForward" ? (
              <FastForward />
            ) : c === "merge" ? (
              <GitMerge />
            ) : (
              <GitPullRequestArrow />
            )}
            <span className="min-w-0">
              {t(`dnd.${c}`, { source, target })}
              {c === "merge" && note(target)}
              {c === "rebase" && note(source)}
            </span>
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Ghost, drop menu and the "check out first?" confirmation. Mounted once (in the toolbar). */
export function BranchDropHost() {
  const { t } = useTranslation();
  const drop = useDnd((s) => s.drop);
  const [checkout, setCheckout] = useState<(BranchDrop & { current: string }) | null>(null);

  const choose = async (drop: BranchDrop, choice: DropChoice) => {
    useDnd.getState().setDrop(null);
    const { repoId, source, target } = drop;
    if (choice === "fastForward") {
      await runOutcome(
        repoId,
        () => commands.branchFastForward(repoId, target, source),
        () => t("dnd.fastForwarded", { source, target }),
      );
    } else if (choice === "rebase") {
      openRebaseDialog({ kind: "rebase", repoId, onto: target, ontoLabel: target, branch: source });
    } else {
      const refs = await commands.repoRefs(repoId).catch(() => null);
      const current = refs?.head.detached ? null : (refs?.head.branch ?? null);
      if (current === target) {
        useRefDialog.getState().open({ kind: "merge", repoId, rev: source, into: target });
      } else {
        setCheckout({ ...drop, current: current ?? "HEAD" });
      }
    }
  };

  const confirmCheckout = async () => {
    if (!checkout) return;
    const { repoId, source, target } = checkout;
    setCheckout(null);
    const ok = await runOutcome(
      repoId,
      () => commands.branchCheckout(repoId, target),
      () => t("ops.checkedOut", { name: target }),
    );
    if (ok !== undefined) {
      useRefDialog.getState().open({ kind: "merge", repoId, rev: source, into: target });
    }
  };

  return (
    <>
      <DragGhost />
      {drop && (
        <DropMenu
          key={`${drop.source}>${drop.target}@${drop.x},${drop.y}`}
          drop={drop}
          onChoose={(c) => void choose(drop, c)}
        />
      )}
      {checkout && (
        <ConfirmDialog
          open
          title={t("dnd.checkoutTitle", { branch: checkout.target })}
          description={t("dnd.checkoutBody", {
            source: checkout.source,
            branch: checkout.target,
            current: checkout.current,
          })}
          confirmLabel={t("dnd.checkoutConfirm")}
          onConfirm={() => void confirmCheckout()}
          onCancel={() => setCheckout(null)}
        />
      )}
    </>
  );
}
