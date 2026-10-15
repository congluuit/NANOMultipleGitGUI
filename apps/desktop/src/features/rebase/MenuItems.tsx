import { GitPullRequestArrow, ListOrdered } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ContextMenuItem } from "@/components/ui/context-menu";
import { openRebaseDialog } from "./store";

const short = (oid: string) => oid.slice(0, 7);

/** "Rebase <current> onto here" + "Interactive rebase from here" for a commit row. */
export function RebaseCommitItems({
  repoId,
  oid,
  current,
  isHead,
}: {
  repoId: number;
  oid: string;
  /** Checked-out branch, `null` when HEAD is detached. */
  current: string | null;
  isHead: boolean;
}) {
  const { t } = useTranslation();
  return (
    <>
      <ContextMenuItem
        disabled={!current || isHead}
        onSelect={() =>
          openRebaseDialog({
            kind: "rebase",
            repoId,
            onto: oid,
            ontoLabel: short(oid),
            branch: null,
          })
        }
      >
        <GitPullRequestArrow />
        {t("rebase.menu.rebaseOnto", { branch: current ?? "HEAD" })}
      </ContextMenuItem>
      <ContextMenuItem
        disabled={!current || isHead}
        onSelect={() =>
          openRebaseDialog({ kind: "interactive", repoId, base: oid, baseLabel: short(oid) })
        }
      >
        <ListOrdered />
        {t("rebase.menu.interactive")}
      </ContextMenuItem>
    </>
  );
}

/** The same two entries for a branch (local or remote) of the sidebar / commit menu. */
export function RebaseBranchItems({
  repoId,
  name,
  current,
}: {
  repoId: number;
  /** `main` or `origin/main`. */
  name: string;
  current: string | null;
}) {
  const { t } = useTranslation();
  const isCurrent = name === current;
  return (
    <>
      <ContextMenuItem
        disabled={!current || isCurrent}
        onSelect={() =>
          openRebaseDialog({ kind: "rebase", repoId, onto: name, ontoLabel: name, branch: null })
        }
      >
        <GitPullRequestArrow />
        {t("rebase.menu.rebaseOntoBranch", { branch: current ?? "HEAD", target: name })}
      </ContextMenuItem>
      <ContextMenuItem
        disabled={!current || isCurrent}
        onSelect={() =>
          openRebaseDialog({ kind: "interactive", repoId, base: name, baseLabel: name })
        }
      >
        <ListOrdered />
        {t("rebase.menu.interactive")}
      </ContextMenuItem>
    </>
  );
}
