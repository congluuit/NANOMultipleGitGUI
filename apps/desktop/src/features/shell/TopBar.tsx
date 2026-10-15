import {
  Download,
  FolderGit2,
  FolderOpen,
  FolderPlus,
  Plus,
  Search,
  TriangleAlert,
  X,
} from "lucide-react";
import { useEffect, useRef, type WheelEvent } from "react";
import type { RepoTab } from "@/ipc";
import { useTranslation } from "react-i18next";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { AccountPill } from "@/features/accounts/AccountPill";
import { useRepoAccount } from "@/features/accounts/switcher";
import { openCloneDialog } from "@/features/clone/commands";
import { openCommandPalette } from "@/features/command-palette/store";
import {
  closeRepository,
  newRepositoryWithDialog,
  openRepository,
  openRepositoryWithDialog,
} from "@/features/repos/actions";
import { useRecentRepos } from "@/features/repos/queries";
import { cn } from "@/lib/utils";
import { useWorkspace } from "@/stores/workspace";

function NewTabMenu() {
  const { t } = useTranslation();
  const recent = useRecentRepos();
  const tabs = useWorkspace((s) => s.tabs);
  const openIds = new Set(tabs.map((tab) => tab.id));
  const candidates = (recent.data ?? []).filter((r) => r.exists && !openIds.has(r.id)).slice(0, 8);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label={t("tabs.newTab")}
        title={t("tabs.newTab")}
        className="mb-1 rounded p-1 text-muted-foreground hover:bg-surface hover:text-foreground"
      >
        <Plus className="size-4" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-72">
        <DropdownMenuItem onSelect={() => void openRepositoryWithDialog(t("repos.pickOpen"))}>
          <FolderOpen />
          {t("repos.open")}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => void newRepositoryWithDialog(t("repos.pickNew"))}>
          <FolderPlus />
          {t("repos.new")}
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => openCloneDialog()}>
          <Download />
          {t("clone.open")}
        </DropdownMenuItem>
        {candidates.length > 0 && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuLabel className="text-xs text-muted-foreground">
              {t("repos.recent")}
            </DropdownMenuLabel>
            {candidates.map((r) => (
              <DropdownMenuItem key={r.id} onSelect={() => void openRepository(r.path)}>
                <FolderGit2 />
                <span className="flex min-w-0 flex-col">
                  <span className="truncate">{r.name}</span>
                  <span className="truncate text-[11px] text-muted-foreground">{r.path}</span>
                </span>
              </DropdownMenuItem>
            ))}
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** One repo tab; its icon (and top edge when active) take the color of the repo's account. */
function RepoTabItem({ tab, active }: { tab: RepoTab; active: boolean }) {
  const { t } = useTranslation();
  const setActive = useWorkspace((s) => s.setActive);
  const { color, label } = useRepoAccount(tab.missing ? null : tab.id);
  const tip = tab.missing ? `${t("tabs.missing")}: ${tab.path}` : tab.path;
  return (
    <div
      role="tab"
      aria-selected={active}
      tabIndex={0}
      title={label ? `${tip} · ● ${label}` : tip}
      data-account-color={color ?? undefined}
      onClick={() => setActive(tab.id)}
      onKeyDown={(e) => e.key === "Enter" && setActive(tab.id)}
      onAuxClick={(e) => e.button === 1 && void closeRepository(tab.id)}
      className={cn(
        "group flex h-8 max-w-48 min-w-24 shrink cursor-default items-center gap-2 rounded-t-md border border-b-0 px-3 text-xs",
        active
          ? "bg-background font-medium"
          : "border-transparent text-muted-foreground hover:bg-surface",
      )}
      style={active && color ? { boxShadow: `inset 0 2px 0 ${color}` } : undefined}
    >
      {tab.missing ? (
        <TriangleAlert className="size-3.5 shrink-0 text-warning" />
      ) : (
        <FolderGit2 className="size-3.5 shrink-0" style={color ? { color } : undefined} />
      )}
      <span className={cn("truncate", tab.missing && "line-through opacity-70")}>{tab.name}</span>
      <button
        type="button"
        aria-label={t("tabs.close")}
        onClick={(e) => {
          e.stopPropagation();
          void closeRepository(tab.id);
        }}
        className="rounded p-0.5 opacity-0 group-hover:opacity-100 hover:bg-surface-2 focus-visible:opacity-100"
      >
        <X className="size-3" />
      </button>
    </div>
  );
}

export function TopBar() {
  const { t } = useTranslation();
  const tabs = useWorkspace((s) => s.tabs);
  const activeId = useWorkspace((s) => s.activeId);
  const isMac = navigator.userAgent.includes("Mac");
  const tabsRef = useRef<HTMLElement>(null);

  // Keep the active tab in view when it changes (or the strip scrolls).
  useEffect(() => {
    const el = tabsRef.current?.querySelector<HTMLElement>("[role=tab][aria-selected=true]");
    el?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [activeId, tabs.length]);

  // A mouse wheel scrolls the tab strip sideways.
  const onTabsWheel = (e: WheelEvent<HTMLElement>) => {
    if (e.deltaY !== 0 && Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
      e.currentTarget.scrollLeft += e.deltaY;
    }
  };

  return (
    <header className="flex h-10 shrink-0 items-center gap-3 border-b bg-surface-2 px-2">
      <div className="flex shrink-0">
        <AccountPill />
      </div>
      {/* Tabs shrink to a minimum, then scroll sideways; "+" and the search box stay visible. */}
      <div className="flex min-w-0 flex-1 items-end gap-1 self-end">
        <nav
          ref={tabsRef}
          role="tablist"
          onWheel={onTabsWheel}
          className="flex min-w-0 items-end gap-1 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
        >
          {tabs.map((tab) => (
            <RepoTabItem key={tab.id} tab={tab} active={tab.id === activeId} />
          ))}
        </nav>
        <div className="shrink-0">
          <NewTabMenu />
        </div>
      </div>
      <button
        type="button"
        onClick={() => openCommandPalette()}
        aria-label={t("toolbar.search")}
        className="flex h-7 w-40 shrink-0 items-center gap-2 rounded-md border bg-surface px-2 text-xs text-muted-foreground hover:border-primary/60 xl:w-56"
      >
        <Search className="size-3.5 shrink-0" />
        <span className="flex-1 truncate text-left">{t("toolbar.search")}</span>
        <kbd className="rounded border bg-surface-2 px-1 font-mono text-[10px]">
          {isMac ? "⌘K" : "Ctrl+K"}
        </kbd>
      </button>
    </header>
  );
}
