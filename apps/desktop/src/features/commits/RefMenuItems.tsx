import { Copy, GitBranchPlus, GitMerge, LogIn, PencilLine, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ContextMenuItem, ContextMenuSeparator } from "@/components/ui/context-menu";
import { RebaseBranchItems } from "@/features/rebase/MenuItems";
import { commands } from "@/ipc";
import { runOutcome } from "./outcome";
import { useRefDialog } from "./store";

interface Common {
  repoId: number;
  /** Checked-out branch, `null` when HEAD is detached. */
  current: string | null;
}

/** Items for a local branch (commit menu submenu and sidebar row menu). */
export function LocalBranchItems({
  repoId,
  current,
  name,
  oid,
}: Common & { name: string; oid?: string }) {
  const { t } = useTranslation();
  const open = useRefDialog((s) => s.open);
  const isCurrent = name === current;
  return (
    <>
      <ContextMenuItem
        disabled={isCurrent}
        onSelect={() =>
          void runOutcome(
            repoId,
            () => commands.branchCheckout(repoId, name),
            () => t("ops.checkedOut", { name }),
          )
        }
      >
        <LogIn />
        {t("menu.checkout")}
      </ContextMenuItem>
      <ContextMenuItem
        disabled={isCurrent || !current}
        onSelect={() => open({ kind: "merge", repoId, rev: name, into: current })}
      >
        <GitMerge />
        {t("menu.mergeInto", { branch: current ?? "HEAD" })}
      </ContextMenuItem>
      <RebaseBranchItems repoId={repoId} name={name} current={current} />
      {oid && (
        <ContextMenuItem
          onSelect={() => open({ kind: "createBranch", repoId, start: oid, label: name })}
        >
          <GitBranchPlus />
          {t("menu.branchFrom", { name })}
        </ContextMenuItem>
      )}
      <ContextMenuItem onSelect={() => open({ kind: "renameBranch", repoId, name })}>
        <PencilLine />
        {t("menu.rename")}
      </ContextMenuItem>
      <ContextMenuItem
        variant="destructive"
        disabled={isCurrent}
        onSelect={() => open({ kind: "deleteBranch", repoId, name, force: false })}
      >
        <Trash2 />
        {t("menu.deleteBranch")}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => void navigator.clipboard.writeText(name)}>
        <Copy />
        {t("menu.copyName")}
      </ContextMenuItem>
    </>
  );
}

/** Items for a remote-tracking branch, `full` being `origin/feature/x`. */
export function RemoteBranchItems({ repoId, current, full }: Common & { full: string }) {
  const { t } = useTranslation();
  const open = useRefDialog((s) => s.open);
  const slash = full.indexOf("/");
  const remote = full.slice(0, slash);
  const branch = full.slice(slash + 1);
  return (
    <>
      <ContextMenuItem
        onSelect={() =>
          void runOutcome(
            repoId,
            () => commands.branchCheckoutRemote(repoId, remote, branch, null),
            (name) => t("ops.checkedOut", { name }),
          )
        }
      >
        <LogIn />
        {t("menu.checkoutRemote")}
      </ContextMenuItem>
      <ContextMenuItem
        disabled={!current}
        onSelect={() => open({ kind: "merge", repoId, rev: full, into: current })}
      >
        <GitMerge />
        {t("menu.mergeInto", { branch: current ?? "HEAD" })}
      </ContextMenuItem>
      <RebaseBranchItems repoId={repoId} name={full} current={current} />
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => void navigator.clipboard.writeText(full)}>
        <Copy />
        {t("menu.copyName")}
      </ContextMenuItem>
    </>
  );
}

export function TagItems({ repoId, name, oid }: { repoId: number; name: string; oid?: string }) {
  const { t } = useTranslation();
  const open = useRefDialog((s) => s.open);
  return (
    <>
      {oid && (
        <ContextMenuItem
          onSelect={() => open({ kind: "createBranch", repoId, start: oid, label: name })}
        >
          <GitBranchPlus />
          {t("menu.branchFrom", { name })}
        </ContextMenuItem>
      )}
      <ContextMenuItem
        variant="destructive"
        onSelect={() => open({ kind: "deleteTag", repoId, name })}
      >
        <Trash2 />
        {t("menu.deleteTag")}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={() => void navigator.clipboard.writeText(name)}>
        <Copy />
        {t("menu.copyName")}
      </ContextMenuItem>
    </>
  );
}
