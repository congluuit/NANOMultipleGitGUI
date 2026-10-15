import { GitBranch, Tag } from "lucide-react";
import {
  ContextMenuContent,
  ContextMenuLabel,
  ContextMenuSeparator,
} from "@/components/ui/context-menu";
import type { RefLabel } from "@/features/graph/draw";
import { useRepoRefs } from "@/features/repos/queries";
import type { GraphRow } from "@/ipc";
import { LocalBranchItems, RemoteBranchItems, TagItems } from "./RefMenuItems";

/** Right-click on a branch / tag label of the graph: the menu of that ref at its commit. */
export function RefMenuContent({
  repoId,
  row,
  label,
}: {
  repoId: number;
  row: GraphRow;
  label: RefLabel;
}) {
  const head = useRepoRefs(repoId).data?.head;
  const current = head?.detached ? null : (head?.branch ?? null);
  const Icon = label.kind === "tag" ? Tag : GitBranch;
  return (
    <ContextMenuContent className="w-80">
      <ContextMenuLabel className="flex items-center gap-1.5" title={row.subject}>
        <Icon className="size-3.5 shrink-0 text-muted-foreground" />
        <span className="truncate">{label.text}</span>
      </ContextMenuLabel>
      <ContextMenuSeparator />
      {label.kind === "tag" ? (
        <TagItems repoId={repoId} name={label.text} oid={row.hash} />
      ) : label.kind === "remote" ? (
        <RemoteBranchItems repoId={repoId} current={current} full={label.text} oid={row.hash} />
      ) : (
        <LocalBranchItems
          repoId={repoId}
          current={current}
          name={label.text}
          oid={row.hash}
          isMerge={row.isMerge}
        />
      )}
    </ContextMenuContent>
  );
}
