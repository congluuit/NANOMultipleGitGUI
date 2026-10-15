import { create } from "zustand";
import { useShallow } from "zustand/react/shallow";

/**
 * Widths of the main window's side panels (step 3B.5).
 *
 * Kept in localStorage rather than the settings DB: it is read synchronously before the first
 * paint (no jump from default to saved width), a drag never round-trips through IPC, and
 * there is a single main window per app instance, so "per window" and "per origin" coincide.
 */
export type PanelId = "sidebar" | "right";

export const PANEL_LIMITS: Record<PanelId, { min: number; max: number; default: number }> = {
  sidebar: { min: 180, max: 480, default: 240 },
  right: { min: 300, max: 640, default: 384 },
};
/** Width of a panel collapsed to its icon rail. */
export const RAIL_WIDTH = 44;
/** The graph / diff in the middle never gets narrower than this while a panel can give way. */
export const CENTER_MIN = 440;

export const LAYOUT_KEY = "nanomgg.layout.v1";

interface Persisted {
  sidebar: number;
  right: number;
  sidebarCollapsed: boolean;
  rightCollapsed: boolean;
}

interface LayoutState extends Persisted {
  /** Width of the panel row, measured by the shell (0 = not measured yet). Not saved. */
  rowWidth: number;
  /** The current view has a right panel (a missing repo has none). Not saved. */
  hasRight: boolean;
  /** Panel folded only for lack of room, shown temporarily over the middle. Not saved. */
  peek: PanelId | null;
  setRow: (rowWidth: number, hasRight: boolean) => void;
  setPeek: (peek: PanelId | null) => void;
  /**
   * Toggle button / shortcut: collapses or expands the panel; when it is folded only because
   * the window is too narrow, shows it temporarily over the middle instead.
   */
  togglePanel: (id: PanelId) => void;
  /** `persist: false` while dragging; the final width is saved on release. */
  setSize: (id: PanelId, px: number, persist?: boolean) => void;
  resetSize: (id: PanelId) => void;
  setCollapsed: (id: PanelId, collapsed: boolean) => void;
  toggleCollapsed: (id: PanelId) => void;
}

const clampTo = (id: PanelId, px: number) =>
  Math.round(Math.min(PANEL_LIMITS[id].max, Math.max(PANEL_LIMITS[id].min, px)));

function defaults(): Persisted {
  return {
    sidebar: PANEL_LIMITS.sidebar.default,
    right: PANEL_LIMITS.right.default,
    sidebarCollapsed: false,
    rightCollapsed: false,
  };
}

export function loadLayout(): Persisted {
  const base = defaults();
  try {
    const raw: unknown = JSON.parse(globalThis.localStorage?.getItem(LAYOUT_KEY) ?? "null");
    if (!raw || typeof raw !== "object") return base;
    const r = raw as Partial<Record<keyof Persisted, unknown>>;
    return {
      sidebar: typeof r.sidebar === "number" ? clampTo("sidebar", r.sidebar) : base.sidebar,
      right: typeof r.right === "number" ? clampTo("right", r.right) : base.right,
      sidebarCollapsed: r.sidebarCollapsed === true,
      rightCollapsed: r.rightCollapsed === true,
    };
  } catch {
    return base;
  }
}

function save(s: Persisted) {
  try {
    const { sidebar, right, sidebarCollapsed, rightCollapsed } = s;
    globalThis.localStorage?.setItem(
      LAYOUT_KEY,
      JSON.stringify({ sidebar, right, sidebarCollapsed, rightCollapsed }),
    );
  } catch {
    // Storage full or unavailable: the layout just is not remembered.
  }
}

const collapsedKey = (id: PanelId) => (id === "sidebar" ? "sidebarCollapsed" : "rightCollapsed");

export const useLayout = create<LayoutState>()((set, get) => ({
  ...loadLayout(),
  rowWidth: 0,
  hasRight: true,
  peek: null,
  setRow: (rowWidth, hasRight) => {
    if (rowWidth !== get().rowWidth || hasRight !== get().hasRight) set({ rowWidth, hasRight });
  },
  setPeek: (peek) => set({ peek }),
  togglePanel: (id) => {
    const s = get();
    const fit = fitPanels(s.rowWidth, s, s.hasRight);
    if (id === "sidebar" ? fit.sidebarAuto : fit.rightAuto) {
      set({ peek: s.peek === id ? null : id });
    } else {
      set({ peek: null });
      s.toggleCollapsed(id);
    }
  },
  setSize: (id, px, persist = true) => {
    set({ [id]: clampTo(id, px) } as Pick<Persisted, PanelId>);
    if (persist) save(get());
  },
  resetSize: (id) => {
    set({ [id]: PANEL_LIMITS[id].default } as Pick<Persisted, PanelId>);
    save(get());
  },
  setCollapsed: (id, collapsed) => {
    set({ [collapsedKey(id)]: collapsed } as Partial<Persisted>);
    save(get());
  },
  toggleCollapsed: (id) => get().setCollapsed(id, !get()[collapsedKey(id)]),
}));

export interface FittedPanels {
  sidebar: number;
  right: number;
  /** Shown as icon rail (collapsed by the user, or because the window is too narrow). */
  sidebarRail: boolean;
  rightRail: boolean;
  /** Collapsed only because the window is too narrow (the user's choice is unchanged). */
  sidebarAuto: boolean;
  rightAuto: boolean;
}

/**
 * Effective panel widths for a row `available` px wide: the saved widths shrink toward their
 * minimum first, then the sidebar and finally the right panel fold to their rails, so the
 * middle keeps `CENTER_MIN`. Nothing here is saved: widening the window restores the layout.
 */
export function fitPanels(available: number, s: Persisted, hasRight = true): FittedPanels {
  // Not measured yet (first render, jsdom): use the saved layout as is.
  if (available <= 0) available = Number.POSITIVE_INFINITY;
  let sidebarRail = s.sidebarCollapsed;
  let rightRail = !hasRight || s.rightCollapsed;
  let sidebar = sidebarRail ? RAIL_WIDTH : s.sidebar;
  let right = !hasRight ? 0 : rightRail ? RAIL_WIDTH : s.right;
  let over = sidebar + right + CENTER_MIN - available;
  if (over > 0) {
    const slackS = sidebarRail ? 0 : sidebar - PANEL_LIMITS.sidebar.min;
    const slackR = rightRail ? 0 : right - PANEL_LIMITS.right.min;
    const slack = slackS + slackR;
    if (slack > 0) {
      const take = Math.min(over, slack);
      const fromS = Math.round((take * slackS) / slack);
      sidebar -= fromS;
      right -= take - fromS;
      over -= take;
    }
  }
  let sidebarAuto = false;
  let rightAuto = false;
  if (over > 0 && !sidebarRail) {
    over -= sidebar - RAIL_WIDTH;
    sidebar = RAIL_WIDTH;
    sidebarRail = sidebarAuto = true;
    // The room the sidebar gave up goes back to the right panel, up to its saved width.
    if (!rightRail && over < 0) {
      const grow = Math.min(-over, s.right - right);
      right += grow;
      over += grow;
    }
  }
  if (over > 0 && !rightRail) {
    right = RAIL_WIDTH;
    rightRail = rightAuto = true;
  }
  return { sidebar, right, sidebarRail, rightRail, sidebarAuto, rightAuto };
}

/** Largest width `id` may be dragged to without squeezing the middle below `CENTER_MIN`. */
export function maxWidthFor(id: PanelId, available: number, other: number): number {
  const room = available - other - CENTER_MIN;
  return Math.max(PANEL_LIMITS[id].min, Math.min(PANEL_LIMITS[id].max, room));
}

/** Effective panel widths for the measured row (stable object while nothing changes). */
export function useFittedPanels(): FittedPanels {
  return useLayout(useShallow((s) => fitPanels(s.rowWidth, s, s.hasRight)));
}

/** The panel is shown in full (expanded, or peeking over the middle). */
export function usePanelShown(id: PanelId): boolean {
  return useLayout((s) => {
    const fit = fitPanels(s.rowWidth, s, s.hasRight);
    return !(id === "sidebar" ? fit.sidebarRail : fit.rightRail) || s.peek === id;
  });
}
