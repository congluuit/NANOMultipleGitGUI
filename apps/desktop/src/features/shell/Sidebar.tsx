import { useState, type ReactNode } from "react";
import {
  Archive,
  ChevronDown,
  ChevronRight,
  CircleDot,
  Cloud,
  FolderTree,
  GitBranch,
  GitCommitHorizontal,
  GitPullRequest,
  HardDrive,
  Layers,
  LoaderCircle,
  Package,
  PanelLeftClose,
  PanelLeftOpen,
  Server,
  Tag,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { ContextMenu, ContextMenuContent, ContextMenuTrigger } from "@/components/ui/context-menu";
import { LocalBranchItems, RemoteBranchItems, TagItems } from "@/features/commits/RefMenuItems";
import { switchBranch } from "@/features/branch-switch/actions";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useRepoRefs } from "@/features/repos/queries";
import { headState, matchesFilter, shortOid } from "@/features/repos/refsView";
import { startBranchDrag, useDnd } from "@/features/dnd/store";
import { cn } from "@/lib/utils";
import { useGraphStore } from "@/stores/graph";
import { useLayout } from "@/stores/layout";
import { useActiveRepo } from "@/stores/workspace";
import { LfsPanel } from "@/features/lfs/LfsPanel";
import { StashList } from "@/features/stash/StashList";
import { useLinkedWorktrees } from "@/features/linked-worktrees/queries";
import { WorktreeList } from "@/features/linked-worktrees/WorktreeList";
import { useSubmodules } from "@/features/submodules/queries";
import { SubmoduleList } from "@/features/submodules/SubmoduleList";
import { RemoteMenuItems } from "@/features/forks/RemoteMenuItems";
import { IssueSidebarList } from "@/features/issues/IssueSidebarList";
import { PullSidebarList } from "@/features/pulls/PullSidebarList";
import { usePullSidebarCount } from "@/features/pulls/sidebarData";

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
  onActivate,
  menu,
  drag,
}: {
  children: ReactNode;
  active?: boolean;
  indent?: number;
  title?: string;
  onClick?: () => void;
  /** Phase 6.1: double-click / Enter (switch to the branch); a single click keeps `onClick`. */
  onActivate?: () => void;
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
      onDoubleClick={onActivate}
      onKeyDown={
        onActivate &&
        ((e) => {
          // Enter would also "click" (jump to the commit); switching is what Enter means here.
          if (e.key === "Enter" && !e.repeat) {
            e.preventDefault();
            onActivate();
          }
        })
      }
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
          <ContextMenuContent className="max-h-[calc(100dvh-2rem)] w-80 overflow-y-auto">
            {menu}
          </ContextMenuContent>
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

interface SectionDef {
  id: string;
  icon: LucideIcon;
  title: string;
  count?: number;
  defaultOpen?: boolean;
  content: ReactNode;
}

/** Collapsed sidebar: one icon per section, each opening the section's list in a popover. */
function SidebarRail({ sections, loading }: { sections: SectionDef[]; loading: boolean }) {
  const { t } = useTranslation();
  const toggle = useLayout((s) => s.togglePanel);
  return (
    <nav className="flex w-full flex-col items-center gap-1" aria-label={t("layout.sidebar")}>
      <RailButton
        icon={PanelLeftOpen}
        label={t("layout.expandSidebar")}
        onClick={() => toggle("sidebar")}
      />
      <span className="my-1 h-px w-6 bg-border" />
      {loading && <LoaderCircle className="size-4 animate-spin text-muted-foreground" />}
      {sections.map((s) => (
        <Popover key={`rail-${s.id}`}>
          <Tooltip>
            <TooltipTrigger asChild>
              <PopoverTrigger asChild>
                <button
                  type="button"
                  aria-label={s.count !== undefined ? `${s.title} (${s.count})` : s.title}
                  className="relative flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-surface-2 hover:text-foreground data-[state=open]:bg-surface-2 data-[state=open]:text-foreground"
                >
                  <s.icon className="size-4" />
                  {!!s.count && (
                    <span className="absolute -top-0.5 -right-0.5 min-w-3.5 rounded-full bg-surface-2 px-0.5 text-center text-[9px] leading-3.5 font-medium text-foreground">
                      {s.count > 99 ? "99+" : s.count}
                    </span>
                  )}
                </button>
              </PopoverTrigger>
            </TooltipTrigger>
            <TooltipContent side="right">{s.title}</TooltipContent>
          </Tooltip>
          <PopoverContent
            side="right"
            align="start"
            className="flex max-h-[min(32rem,calc(100dvh-4rem))] w-72 flex-col p-0"
          >
            <div className="flex h-8 shrink-0 items-center gap-1.5 border-b px-3 text-[11px] font-semibold tracking-wide text-muted-foreground uppercase">
              <s.icon className="size-3.5" />
              <span className="flex-1">{s.title}</span>
              {s.count !== undefined && <span className="font-normal">{s.count}</span>}
            </div>
            <ul className="min-h-0 flex-1 overflow-y-auto py-1">{s.content}</ul>
          </PopoverContent>
        </Popover>
      ))}
    </nav>
  );
}

function RailButton({
  icon: Icon,
  label,
  onClick,
}: {
  icon: LucideIcon;
  label: string;
  onClick: () => void;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={label}
          onClick={onClick}
          className="flex size-8 items-center justify-center rounded-md text-muted-foreground hover:bg-surface-2 hover:text-foreground"
        >
          <Icon className="size-4" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="right">{label}</TooltipContent>
    </Tooltip>
  );
}

/** Branches, remotes, tags… of the active repo; `rail` renders the collapsed icon column. */
export function Sidebar({ rail = false }: { rail?: boolean }) {
  const { t } = useTranslation();
  const repo = useActiveRepo();
  const refs = useRepoRefs(repo?.id ?? null, !repo?.missing);
  const submodules = useSubmodules(repo?.id ?? -1, !!repo && !repo.missing);
  const worktrees = useLinkedWorktrees(repo?.id ?? -1, !!repo && !repo.missing);
  const [typed, setFilter] = useState("");
  // The rail has no filter box: its popovers list everything.
  const filter = rail ? "" : typed;
  const toggle = useLayout((s) => s.togglePanel);
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
  // Phase 4.1: pull / merge requests of the repo's hosted project.
  const pullCount = usePullSidebarCount(repo && !repo.missing ? repo.id : null);

  const sections: SectionDef[] = !data
    ? []
    : [
        {
          id: "local",
          icon: HardDrive,
          title: t("sidebar.local"),
          count: data.local.length,
          defaultOpen: true,
          content: (
            <>
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
                  onActivate={
                    repo
                      ? () => void switchBranch(repo.id, { kind: "local", name: b.name })
                      : undefined
                  }
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
                  title={[
                    b.upstreamGone
                      ? t("sidebar.upstreamGone")
                      : b.upstream
                        ? t("sidebar.tracking", { upstream: b.upstream })
                        : null,
                    b.isHead ? t("branchSwitch.tip.current") : t("branchSwitch.tip.local"),
                  ]
                    .filter(Boolean)
                    .join("\n")}
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
            </>
          ),
        },
        {
          id: "remote",
          icon: Cloud,
          title: t("sidebar.remote"),
          count: remoteBranchCount,
          defaultOpen: true,
          content: (
            <>
              {data.remotes.length === 0 && <Empty>{t("sidebar.noRemotes")}</Empty>}
              {data.remotes.map((r) => (
                <li key={r.name}>
                  {/* Phase 4.3: remote menu ("Fork repo này…"). */}
                  <ContextMenu>
                    <ContextMenuTrigger asChild>
                      <div
                        className="flex h-6 items-center gap-2 truncate pr-2 pl-6 text-xs font-medium"
                        title={r.url ?? undefined}
                      >
                        <Server className="size-3.5 shrink-0 text-muted-foreground" />
                        <span className="truncate">{r.name}</span>
                      </div>
                    </ContextMenuTrigger>
                    <ContextMenuContent className="w-64">
                      {repo && <RemoteMenuItems repoId={repo.id} remote={r.name} url={r.url} />}
                    </ContextMenuContent>
                  </ContextMenu>
                  <ul>
                    {r.branches
                      .filter((b) => matchesFilter(`${r.name}/${b.name}`, filter))
                      .map((b) => (
                        <Item
                          key={b.name}
                          indent={11}
                          onClick={jumpTo(b.oid)}
                          onActivate={
                            repo && b.name !== "HEAD"
                              ? () =>
                                  void switchBranch(repo.id, {
                                    kind: "remote",
                                    remote: r.name,
                                    branch: b.name,
                                  })
                              : undefined
                          }
                          menu={
                            repo && (
                              <RemoteBranchItems
                                repoId={repo.id}
                                current={current}
                                full={`${r.name}/${b.name}`}
                                oid={b.oid}
                              />
                            )
                          }
                          title={`${t("sidebar.jumpTo")}\n${t("branchSwitch.tip.remote")}`}
                        >
                          <GitBranch className="size-3.5 shrink-0 text-muted-foreground" />
                          <span className="truncate">{b.name}</span>
                        </Item>
                      ))}
                  </ul>
                </li>
              ))}
            </>
          ),
        },
        {
          id: "tags",
          icon: Tag,
          title: t("sidebar.tags"),
          count: data.tags.length,
          content: (
            <>
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
            </>
          ),
        },
        {
          id: "stashes",
          icon: Archive,
          title: t("sidebar.stashes"),
          count: data.stashes.length,
          content: (
            <>
              {data.stashes.length === 0 && <Empty>{t("sidebar.noStashes")}</Empty>}
              {repo && <StashList repoId={repo.id} stashes={data.stashes} />}
            </>
          ),
        },
        {
          id: "prs",
          icon: GitPullRequest,
          title: t("sidebar.prs"),
          count: pullCount,
          content: repo && <PullSidebarList repoId={repo.id} filter={filter} />,
        },
        {
          id: "issues",
          icon: CircleDot,
          title: t("issues.sidebar"),
          content: repo && <IssueSidebarList repoId={repo.id} filter={filter} />,
        },
        {
          id: "lfs",
          icon: Package,
          title: t("sidebar.lfs"),
          content: repo && <LfsPanel repoId={repo.id} />,
        },
        {
          id: "submodules",
          icon: Layers,
          title: t("sidebar.submodules"),
          count: submodules.data?.length,
          content: repo && <SubmoduleList repoId={repo.id} root={repo.path} filter={filter} />,
        },
        {
          id: "worktrees",
          icon: FolderTree,
          title: t("sidebar.worktrees"),
          count: worktrees.data?.length,
          content: repo && <WorktreeList repoId={repo.id} filter={filter} />,
        },
      ];

  const loading = !data && !!repo && !repo.missing;
  if (rail) return <SidebarRail sections={sections} loading={loading} />;

  return (
    <aside className="flex min-w-0 flex-1 flex-col bg-surface" aria-label={t("layout.sidebar")}>
      <div className="flex items-center gap-1 p-2">
        <input
          value={typed}
          onChange={(e) => setFilter(e.target.value)}
          placeholder={t("sidebar.filter")}
          className="h-7 min-w-0 flex-1 rounded-md border bg-background px-2 text-xs placeholder:text-muted-foreground focus:border-primary focus:outline-none"
        />
        <Tooltip>
          <TooltipTrigger asChild>
            <button
              type="button"
              aria-label={t("layout.collapseSidebar")}
              onClick={() => toggle("sidebar")}
              className="flex size-7 shrink-0 items-center justify-center rounded-md text-muted-foreground hover:bg-surface-2 hover:text-foreground"
            >
              <PanelLeftClose className="size-4" />
            </button>
          </TooltipTrigger>
          <TooltipContent>{t("layout.collapseSidebar")}</TooltipContent>
        </Tooltip>
      </div>
      <ScrollArea className="min-h-0 flex-1">
        {loading && (
          <p className="px-3 py-2 text-xs text-muted-foreground">{t("sidebar.loading")}</p>
        )}
        {sections.map((s) => (
          <Section
            key={`section-${s.id}`}
            icon={s.icon}
            title={s.title}
            count={s.count}
            defaultOpen={s.defaultOpen}
          >
            {s.content}
          </Section>
        ))}
      </ScrollArea>
    </aside>
  );
}
