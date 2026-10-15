import { useState, type ReactNode } from "react";
import {
  Archive,
  ChevronDown,
  ChevronRight,
  Cloud,
  FolderTree,
  GitBranch,
  GitCommitHorizontal,
  GitPullRequest,
  HardDrive,
  Layers,
  Package,
  Server,
  Tag,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import { LocalBranchItems, RemoteBranchItems, TagItems } from "@/features/commits/RefMenuItems";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useRepoRefs } from "@/features/repos/queries";
import { headState, matchesFilter, shortOid } from "@/features/repos/refsView";
import { startBranchDrag, useDnd } from "@/features/dnd/store";
import { cn } from "@/lib/utils";
import { useGraphStore } from "@/stores/graph";
import { useActiveRepo } from "@/stores/workspace";
import { LfsPanel } from "@/features/lfs/LfsPanel";
import { LfsManagerHost } from "@/features/lfs/LfsManagerDialog";
import { StashList } from "@/features/stash/StashList";
import { useLinkedWorktrees } from "@/features/linked-worktrees/queries";
import { WorktreeList } from "@/features/linked-worktrees/WorktreeList";
import { useSubmodules } from "@/features/submodules/queries";
import { SubmoduleList } from "@/features/submodules/SubmoduleList";

function Section({
  icon: Icon,
  title,
  count,
  defaultOpen = false,
  children,
}: {
  icon: LucideIcon;
  title: string;
  count?: number;
  defaultOpen?: boolean;
  children?: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <section>
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex h-7 w-full items-center gap-1.5 px-2 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase hover:text-foreground"
      >
        <Chevron className="size-3.5" />
        <Icon className="size-3.5" />
        <span className="flex-1 text-left">{title}</span>
        {count !== undefined && <span className="font-normal">{count}</span>}
      </button>
      {open && <ul className="pb-1">{children}</ul>}
    </section>
  );
}

function Item({
  children,
  active,
  indent = 8,
  title,
  onClick,
  menu,
  drag,
}: {
  children: ReactNode;
  active?: boolean;
  indent?: number;
  title?: string;
  onClick?: () => void;
  /** Context menu content (items) for the row. */
  menu?: ReactNode;
  /** Local branch row: drag it onto another branch to merge / rebase (step 3.1). */
  drag?: { repoId: number; branch: string };
}) {
  const dropOver = useDnd((s) => !!drag && s.drag?.over === drag.branch);
  const className = cn(
    "flex h-6 w-full items-center gap-2 truncate pr-2 text-left text-xs hover:bg-surface-2",
    active && "bg-surface-2 font-semibold",
    dropOver && "ring-1 ring-primary ring-inset",
  );
  const row = onClick ? (
    <button
      type="button"
      data-drop-branch={drag?.branch}
      onPointerDown={drag && ((e) => startBranchDrag(e, drag.repoId, drag.branch))}
      onClick={onClick}
      style={{ paddingLeft: indent * 4 }}
      className={className}
    >
      {children}
    </button>
  ) : (
    <div style={{ paddingLeft: indent * 4 }} className={cn(className, "cursor-default")}>
      {children}
    </div>
  );
  if (menu) {
    return (
      <li title={title}>
        <ContextMenu>
          <ContextMenuTrigger asChild>{row}</ContextMenuTrigger>
          <ContextMenuContent className="w-64">{menu}</ContextMenuContent>
        </ContextMenu>
      </li>
    );
  }
  return (
    <li title={title}>
      {onClick ? (
        <button
          type="button"
          onClick={onClick}
          style={{ paddingLeft: indent * 4 }}
          className={className}
        >
          {children}
        </button>
      ) : (
        <div style={{ paddingLeft: indent * 4 }} className={cn(className, "cursor-default")}>
          {children}
        </div>
      )}
    </li>
  );
}

function Empty({ children }: { children: ReactNode }) {
  return <li className="py-1 pr-2 pl-8 text-[11px] text-muted-foreground italic">{children}</li>;
}

export function Sidebar() {
  const { t } = useTranslation();
  const repo = useActiveRepo();
  const refs = useRepoRefs(repo?.id ?? null, !repo?.missing);
  const submodules = useSubmodules(repo?.id ?? -1, !!repo && !repo.missing);
  const worktrees = useLinkedWorktrees(repo?.id ?? -1, !!repo && !repo.missing);
  const [filter, setFilter] = useState("");
  const requestJump = useGraphStore((s) => s.requestJump);
  const jumpTo = (oid: string | null | undefined) => () => {
    if (repo && oid) requestJump(repo.id, oid);
  };
  const data = refs.data;

  const local = (data?.local ?? []).filter((b) => matchesFilter(b.name, filter));
  const tags = (data?.tags ?? []).filter((tag) => matchesFilter(tag.name, filter));
  const head = data ? headState(data.head) : null;
  const current = head?.kind === "branch" ? head.branch : null;
  const remoteBranchCount = (data?.remotes ?? []).reduce((n, r) => n + r.branches.length, 0);

  return (
    <aside className="flex w-60 shrink-0 flex-col border-r bg-surface">
      <div className="p-2">
        <input
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          placeholder={t("sidebar.filter")}
          className="h-7 w-full rounded-md border bg-background px-2 text-xs placeholder:text-muted-foreground focus:border-primary focus:outline-none"
        />
      </div>
      <ScrollArea className="min-h-0 flex-1">
        {!data && repo && !repo.missing && (
          <p className="px-3 py-2 text-xs text-muted-foreground">{t("sidebar.loading")}</p>
        )}
        {data && (
          <>
            <Section
              icon={HardDrive}
              title={t("sidebar.local")}
              count={data.local.length}
              defaultOpen
            >
              {head?.kind === "unborn" && (
                <Item active>
                  <GitBranch className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="truncate">{t("sidebar.unborn", { branch: head.branch })}</span>
                </Item>
              )}
              {head?.kind === "detached" && (
                <Item active onClick={jumpTo(data.head.oid)} title={t("sidebar.jumpTo")}>
                  <GitCommitHorizontal className="size-3.5 shrink-0 text-warning" />
                  <span className="truncate">{t("sidebar.detached", { oid: head.oid })}</span>
                </Item>
              )}
              {local.map((b) => (
                <Item
                  key={b.name}
                  active={b.isHead}
                  onClick={jumpTo(b.oid)}
                  drag={repo ? { repoId: repo.id, branch: b.name } : undefined}
                  menu={
                    repo && (
                      <LocalBranchItems
                        repoId={repo.id}
                        current={current}
                        name={b.name}
                        oid={b.oid}
                      />
                    )
                  }
                  title={
                    b.upstreamGone
                      ? t("sidebar.upstreamGone")
                      : b.upstream
                        ? t("sidebar.tracking", { upstream: b.upstream })
                        : undefined
                  }
                >
                  <GitBranch className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className={cn("flex-1 truncate", b.upstreamGone && "line-through")}>
                    {b.name}
                  </span>
                  {b.ahead > 0 && <span className="text-success">↑{b.ahead}</span>}
                  {b.behind > 0 && <span className="text-info">↓{b.behind}</span>}
                </Item>
              ))}
              {data.local.length === 0 && head?.kind !== "unborn" && (
                <Empty>{t("sidebar.noBranches")}</Empty>
              )}
            </Section>

            <Section icon={Cloud} title={t("sidebar.remote")} count={remoteBranchCount} defaultOpen>
              {data.remotes.length === 0 && <Empty>{t("sidebar.noRemotes")}</Empty>}
              {data.remotes.map((r) => (
                <li key={r.name}>
                  <div
                    className="flex h-6 items-center gap-2 truncate pr-2 pl-6 text-xs font-medium"
                    title={r.url ?? undefined}
                  >
                    <Server className="size-3.5 shrink-0 text-muted-foreground" />
                    <span className="truncate">{r.name}</span>
                  </div>
                  <ul>
                    {r.branches
                      .filter((b) => matchesFilter(`${r.name}/${b.name}`, filter))
                      .map((b) => (
                        <Item
                          key={b.name}
                          indent={11}
                          onClick={jumpTo(b.oid)}
                          menu={
                            repo && (
                              <RemoteBranchItems
                                repoId={repo.id}
                                current={current}
                                full={`${r.name}/${b.name}`}
                              />
                            )
                          }
                          title={t("sidebar.jumpTo")}
                        >
                          <GitBranch className="size-3.5 shrink-0 text-muted-foreground" />
                          <span className="truncate">{b.name}</span>
                        </Item>
                      ))}
                  </ul>
                </li>
              ))}
            </Section>

            <Section icon={Tag} title={t("sidebar.tags")} count={data.tags.length}>
              {data.tags.length === 0 && <Empty>{t("sidebar.noTags")}</Empty>}
              {tags.map((tag) => (
                <Item
                  key={tag.name}
                  title={shortOid(tag.oid)}
                  onClick={jumpTo(tag.oid)}
                  menu={repo && <TagItems repoId={repo.id} name={tag.name} oid={tag.oid} />}
                >
                  <Tag className="size-3.5 shrink-0 text-muted-foreground" />
                  <span className="truncate">{tag.name}</span>
                </Item>
              ))}
            </Section>

            <Section icon={Archive} title={t("sidebar.stashes")} count={data.stashes.length}>
              {data.stashes.length === 0 && <Empty>{t("sidebar.noStashes")}</Empty>}
              {repo && <StashList repoId={repo.id} stashes={data.stashes} />}
            </Section>

            <Section icon={GitPullRequest} title={t("sidebar.prs")}>
              <Empty>{t("sidebar.comingSoon")}</Empty>
            </Section>
            <Section icon={Package} title={t("sidebar.lfs")}>
              {repo && <LfsPanel repoId={repo.id} />}
            </Section>
            <Section icon={Layers} title={t("sidebar.submodules")} count={submodules.data?.length}>
              {repo && <SubmoduleList repoId={repo.id} root={repo.path} filter={filter} />}
            </Section>
            <Section
              icon={FolderTree}
              title={t("sidebar.worktrees")}
              count={worktrees.data?.length}
            >
              {repo && <WorktreeList repoId={repo.id} filter={filter} />}
            </Section>
          </>
        )}
      </ScrollArea>
      <LfsManagerHost />
    </aside>
  );
}
