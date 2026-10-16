import {
  Check,
  ChevronDown,
  Copy,
  Download,
  FolderGit2,
  FolderOpen,
  FolderPlus,
  History,
  Pin,
  PinOff,
  Plus,
  Search,
  Settings,
  TriangleAlert,
  Undo2,
  X,
} from "lucide-react";
import {
  Fragment,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
  type WheelEvent,
} from "react";
import type { RepoTab } from "@/ipc";
import { useTranslation } from "react-i18next";
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { AccountPill } from "@/features/accounts/AccountPill";
import { useRepoAccount } from "@/features/accounts/switcher";
import { openCloneDialog } from "@/features/clone/commands";
import { openCommandPalette } from "@/features/command-palette/store";
import {
  closeAllRepositories,
  closeOtherRepositories,
  closeRepositoriesToTheRight,
  closeRepository,
  newRepositoryWithDialog,
  openRepository,
  openRepositoryWithDialog,
  reopenLastClosed,
} from "@/features/repos/actions";
import { useRecentRepos } from "@/features/repos/queries";
import { recentCandidates } from "@/features/repos/recentCandidates";
import { versionLabel } from "@/features/updater/notes";
import { useUpdater } from "@/features/updater/store";
import { cn } from "@/lib/utils";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { moveTab, orderTabs, usePinnedTabs } from "./pinnedTabs";
import { tabShortcutLabel, useTabShortcuts } from "./tabShortcuts";

/** From this many tabs on, the tab list gets a filter box. */
const TAB_FILTER_MIN = 8;

function NewTabMenu() {
  const { t } = useTranslation();
  const recent = useRecentRepos();
  const tabs = useWorkspace((s) => s.tabs);
  const recentlyClosed = useWorkspace((s) => s.recentlyClosed);
  const candidates = recentCandidates(recentlyClosed, recent.data ?? [], tabs);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        aria-label={t("tabs.newTab")}
        title={t("tabs.newTab")}
        className="mb-1 rounded p-1 text-muted-foreground hover:bg-surface hover:text-foreground"
      >
        <Plus className="size-4" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-64">
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
        <DropdownMenuSeparator />
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <History />
            {t("tabs.recent")}
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="w-72" data-testid="recent-submenu">
            {candidates.length === 0 ? (
              <DropdownMenuItem disabled>{t("tabs.noRecent")}</DropdownMenuItem>
            ) : (
              candidates.map((r) => (
                <DropdownMenuItem key={r.id} onSelect={() => void openRepository(r.path)}>
                  <FolderGit2 />
                  <span className="flex min-w-0 flex-col">
                    <span className="truncate">{r.name}</span>
                    <span className="truncate text-[11px] text-muted-foreground">{r.path}</span>
                  </span>
                </DropdownMenuItem>
              ))
            )}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

export interface TabDropMark {
  id: number;
  after: boolean;
}

/**
 * One repo tab; its icon (and top edge when active) take the color of the repo's account.
 * Pinned tabs are narrower and show a pin instead of the close button.
 */
function RepoTabItem({
  tab,
  active,
  pinned,
  hasRight,
  hasOthers,
  drop,
  onDragStart,
  consumeDrag,
}: {
  tab: RepoTab;
  active: boolean;
  pinned: boolean;
  /** Unpinned tabs follow this one (for "Close tabs to the right"). */
  hasRight: boolean;
  /** Other unpinned tabs exist (for "Close other tabs"). */
  hasOthers: boolean;
  /** Where a dragged tab would land, when it is next to this one. */
  drop: "before" | "after" | null;
  onDragStart: (e: ReactPointerEvent<HTMLDivElement>, id: number) => void;
  /** True once after a drag ended on this tab: the click that follows is not a selection. */
  consumeDrag: () => boolean;
}) {
  const { t } = useTranslation();
  const setActive = useWorkspace((s) => s.setActive);
  const canReopen = useWorkspace((s) => s.recentlyClosed.length > 0);
  const { color, label } = useRepoAccount(tab.missing ? null : tab.id);
  const tip = tab.missing ? `${t("tabs.missing")}: ${tab.path}` : tab.path;
  const isMac = navigator.userAgent.includes("Mac");
  const shadows = [
    // Account color: a full top edge on the active tab, a faint bottom line on the others,
    // so tabs of the same account read as a group at a glance.
    color
      ? active
        ? `inset 0 2px 0 ${color}`
        : `inset 0 -2px 0 color-mix(in srgb, ${color} 55%, transparent)`
      : null,
    drop === "before" ? "inset 2px 0 0 var(--primary)" : null,
    drop === "after" ? "inset -2px 0 0 var(--primary)" : null,
  ].filter(Boolean);
  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <div
          role="tab"
          aria-selected={active}
          tabIndex={0}
          title={label ? `${tip} · ● ${label}` : tip}
          data-tab-id={tab.id}
          data-pinned={pinned || undefined}
          data-account-color={color ?? undefined}
          onPointerDown={(e) => onDragStart(e, tab.id)}
          onClick={() => {
            if (!consumeDrag()) setActive(tab.id);
          }}
          onKeyDown={(e) => e.key === "Enter" && setActive(tab.id)}
          onAuxClick={(e) => e.button === 1 && !pinned && void closeRepository(tab.id)}
          className={cn(
            "group flex h-8 shrink cursor-default items-center gap-2 rounded-t-md border border-b-0 text-xs select-none",
            pinned ? "max-w-32 min-w-16 px-2" : "max-w-48 min-w-24 px-3",
            active
              ? "bg-background font-medium"
              : "border-transparent text-muted-foreground hover:bg-surface",
          )}
          style={shadows.length ? { boxShadow: shadows.join(", ") } : undefined}
        >
          {tab.missing ? (
            <TriangleAlert className="size-3.5 shrink-0 text-warning" />
          ) : (
            <FolderGit2 className="size-3.5 shrink-0" style={color ? { color } : undefined} />
          )}
          <span className={cn("truncate", tab.missing && "line-through opacity-70")}>
            {tab.name}
          </span>
          {pinned ? (
            <Pin aria-label={t("tabs.pinned")} className="size-3 shrink-0 opacity-60" />
          ) : (
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
          )}
        </div>
      </ContextMenuTrigger>
      <ContextMenuContent className="w-60">
        <ContextMenuItem
          onSelect={() =>
            pinned ? usePinnedTabs.getState().unpin(tab.id) : usePinnedTabs.getState().pin(tab.id)
          }
        >
          {pinned ? <PinOff /> : <Pin />}
          {t(pinned ? "tabs.unpin" : "tabs.pin")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => void closeRepository(tab.id)}>
          <X />
          {t("tabs.close")}
        </ContextMenuItem>
        <ContextMenuItem disabled={!hasOthers} onSelect={() => void closeOtherRepositories(tab.id)}>
          {t("tabs.closeOthers")}
        </ContextMenuItem>
        <ContextMenuItem
          disabled={!hasRight}
          onSelect={() => void closeRepositoriesToTheRight(tab.id)}
        >
          {t("tabs.closeRight")}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => void closeAllRepositories()}>
          {t("tabs.closeAll")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled={!canReopen} onSelect={() => void reopenLastClosed()}>
          <Undo2 />
          {t("tabs.reopen")}
          <span className="ml-auto pl-3 font-mono text-[10px] text-muted-foreground">
            {tabShortcutLabel("reopen", isMac)}
          </span>
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => void navigator.clipboard.writeText(tab.path)}>
          <Copy />
          {t("tabs.copyPath")}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

/** Pixels the pointer must travel before a press on a tab becomes a drag. */
const DRAG_THRESHOLD = 5;

interface DragState {
  id: number;
  startX: number;
  moving: boolean;
  mark: TabDropMark | null;
}

/**
 * Drag a tab sideways to reorder (pointer events: HTML5 drag and drop is taken over by the
 * webview for file drops on Windows). Tabs only move within their group (pinned / not).
 */
function useTabDrag(order: readonly number[], pinned: readonly number[]) {
  const reorder = useWorkspace((s) => s.reorder);
  const [mark, setMark] = useState<TabDropMark | null>(null);
  const state = useRef<DragState | null>(null);
  const dragged = useRef(false);

  const onDragStart = (e: ReactPointerEvent<HTMLDivElement>, id: number) => {
    if (e.button !== 0 || (e.target as Element).closest("button")) return;
    state.current = { id, startX: e.clientX, moving: false, mark: null };
    const el = e.currentTarget;
    el.setPointerCapture?.(e.pointerId);
    const onMove = (ev: PointerEvent) => {
      const d = state.current;
      if (!d) return;
      if (!d.moving && Math.abs(ev.clientX - d.startX) < DRAG_THRESHOLD) return;
      d.moving = true;
      const over = document
        .elementFromPoint(ev.clientX, ev.clientY)
        ?.closest<HTMLElement>("[data-tab-id]");
      const overId = over ? Number(over.dataset.tabId) : null;
      let next: TabDropMark | null = null;
      if (
        over &&
        overId !== null &&
        overId !== d.id &&
        pinned.includes(overId) === pinned.includes(d.id)
      ) {
        const rect = over.getBoundingClientRect();
        next = { id: overId, after: ev.clientX > rect.left + rect.width / 2 };
      }
      d.mark = next;
      setMark(next);
    };
    const finish = (ev: PointerEvent) => {
      el.removeEventListener("pointermove", onMove);
      el.removeEventListener("pointerup", finish);
      el.removeEventListener("pointercancel", finish);
      const d = state.current;
      state.current = null;
      setMark(null);
      if (!d?.moving) return;
      dragged.current = true;
      if (ev.type === "pointerup" && d.mark) {
        const next = moveTab(order, pinned, d.id, d.mark.id, d.mark.after);
        if (next.some((x, i) => x !== order[i])) void reorder(next);
      }
    };
    el.addEventListener("pointermove", onMove);
    el.addEventListener("pointerup", finish);
    el.addEventListener("pointercancel", finish);
  };

  const consumeDrag = () => {
    const was = dragged.current;
    dragged.current = false;
    return was;
  };
  return { mark, onDragStart, consumeDrag };
}

function TabListRow({
  tab,
  active,
  onPick,
}: {
  tab: RepoTab;
  active: boolean;
  onPick: () => void;
}) {
  const { t } = useTranslation();
  const { color } = useRepoAccount(tab.missing ? null : tab.id);
  return (
    <li className="group flex items-center gap-1">
      <button
        type="button"
        role="option"
        aria-selected={active}
        onClick={onPick}
        title={tab.path}
        className={cn(
          "flex min-w-0 flex-1 items-center gap-2 rounded-sm px-2 py-1.5 text-left text-xs hover:bg-accent",
          active && "font-medium",
        )}
      >
        {tab.missing ? (
          <TriangleAlert className="size-3.5 shrink-0 text-warning" />
        ) : (
          <FolderGit2 className="size-3.5 shrink-0" style={color ? { color } : undefined} />
        )}
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate">{tab.name}</span>
          <span className="truncate text-[11px] text-muted-foreground">{tab.path}</span>
        </span>
        {active && <Check className="size-3.5 shrink-0 text-primary" />}
      </button>
      <button
        type="button"
        aria-label={`${t("tabs.close")}: ${tab.name}`}
        onClick={() => void closeRepository(tab.id)}
        className="rounded p-1 text-muted-foreground opacity-0 group-hover:opacity-100 hover:bg-surface-2 focus-visible:opacity-100"
      >
        <X className="size-3" />
      </button>
    </li>
  );
}

/**
 * "▾ N": every open tab in a list with a filter box, for the moment the strip is too crowded
 * to read. Shown from two tabs on.
 */
function TabListMenu() {
  const { t } = useTranslation();
  const pinned = usePinnedTabs((s) => s.pinned);
  const allTabs = useWorkspace((s) => s.tabs);
  const tabs = orderTabs(allTabs, pinned);
  const activeId = useWorkspace((s) => s.activeId);
  const setActive = useWorkspace((s) => s.setActive);
  const [open, setOpen] = useState(false);
  const [filter, setFilter] = useState("");
  if (tabs.length < 2) return null;

  const needle = filter.trim().toLowerCase();
  const shown = needle
    ? tabs.filter(
        (tab) => tab.name.toLowerCase().includes(needle) || tab.path.toLowerCase().includes(needle),
      )
    : tabs;
  const pick = (id: number) => {
    setActive(id);
    setOpen(false);
  };

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) setFilter("");
      }}
    >
      <PopoverTrigger
        aria-label={t("tabs.list")}
        title={t("tabs.list")}
        className="mb-1 flex items-center gap-0.5 rounded px-1 py-1 text-xs text-muted-foreground tabular-nums hover:bg-surface hover:text-foreground"
      >
        <ChevronDown className="size-4" />
        {tabs.length}
      </PopoverTrigger>
      <PopoverContent align="start" className="w-80 p-1">
        {tabs.length >= TAB_FILTER_MIN && (
          <input
            autoFocus
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && shown[0]) pick(shown[0].id);
            }}
            placeholder={t("tabs.filter")}
            aria-label={t("tabs.filter")}
            className="mb-1 h-7 w-full rounded-sm border bg-surface px-2 text-xs outline-hidden focus:border-primary/60"
          />
        )}
        <ul role="listbox" aria-label={t("tabs.list")} className="max-h-80 overflow-y-auto">
          {shown.map((tab) => (
            <TabListRow
              key={tab.id}
              tab={tab}
              active={tab.id === activeId}
              onPick={() => pick(tab.id)}
            />
          ))}
          {shown.length === 0 && (
            <li className="px-2 py-1.5 text-xs text-muted-foreground">{t("tabs.noMatch")}</li>
          )}
        </ul>
      </PopoverContent>
    </Popover>
  );
}

export function TopBar() {
  const { t } = useTranslation();
  const tabs = useWorkspace((s) => s.tabs);
  const activeId = useWorkspace((s) => s.activeId);
  const setSettingsOpen = useUi((s) => s.setSettingsOpen);
  const pendingUpdate = useUpdater((s) => s.pending?.update.version ?? null);
  const settingsLabel = pendingUpdate
    ? `${t("toolbar.settings")} · ${t("updater.pending.available", { version: versionLabel(pendingUpdate) })}`
    : t("toolbar.settings");
  const isMac = navigator.userAgent.includes("Mac");
  const tabsRef = useRef<HTMLElement>(null);
  const pinned = usePinnedTabs((s) => s.pinned);
  const ordered = orderTabs(tabs, pinned);
  const pinnedSet = new Set(pinned);
  const drag = useTabDrag(
    ordered.map((tab) => tab.id),
    pinned,
  );
  useTabShortcuts();

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
      {/* Tabs shrink to a minimum, then scroll sideways; "+", the tab list and the search box stay visible. */}
      <div className="flex min-w-0 flex-1 items-end gap-1 self-end">
        <nav
          ref={tabsRef}
          role="tablist"
          onWheel={onTabsWheel}
          className="flex min-w-0 items-end gap-1 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
        >
          {ordered.map((tab, i) => {
            const isPinned = pinnedSet.has(tab.id);
            const unpinnedAfter = ordered.slice(i + 1).some((x) => !pinnedSet.has(x.id));
            const unpinnedOthers = ordered.some((x) => x.id !== tab.id && !pinnedSet.has(x.id));
            const lastPinned = isPinned && !pinnedSet.has(ordered[i + 1]?.id ?? -1);
            return (
              <Fragment key={tab.id}>
                <RepoTabItem
                  tab={tab}
                  active={tab.id === activeId}
                  pinned={isPinned}
                  hasRight={unpinnedAfter}
                  hasOthers={unpinnedOthers}
                  drop={drag.mark?.id === tab.id ? (drag.mark.after ? "after" : "before") : null}
                  onDragStart={drag.onDragStart}
                  consumeDrag={drag.consumeDrag}
                />
                {lastPinned && i < ordered.length - 1 && (
                  <span aria-hidden className="mb-2 h-4 w-px shrink-0 self-end bg-border" />
                )}
              </Fragment>
            );
          })}
        </nav>
        <div className="flex shrink-0 items-end">
          <NewTabMenu />
          <TabListMenu />
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
      {/* Always reachable, with or without a repository (updates live in Settings). */}
      <button
        type="button"
        onClick={() => setSettingsOpen(true)}
        aria-label={settingsLabel}
        title={settingsLabel}
        className="relative shrink-0 rounded p-1.5 text-muted-foreground hover:bg-surface hover:text-foreground"
      >
        <Settings className="size-4" />
        {pendingUpdate && (
          <span
            aria-hidden
            data-testid="update-dot"
            className="absolute top-1 right-1 size-2 rounded-full bg-primary ring-2 ring-surface-2"
          />
        )}
      </button>
    </header>
  );
}
