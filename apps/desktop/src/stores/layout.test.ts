import { beforeEach, describe, expect, it } from "vitest";
import {
  CENTER_MIN,
  fitPanels,
  LAYOUT_KEY,
  loadLayout,
  maxWidthFor,
  PANEL_LIMITS,
  RAIL_WIDTH,
  useLayout,
} from "./layout";

const saved = {
  sidebar: PANEL_LIMITS.sidebar.default,
  right: PANEL_LIMITS.right.default,
  sidebarCollapsed: false,
  rightCollapsed: false,
};

beforeEach(() => {
  localStorage.clear();
  useLayout.setState({ ...saved, rowWidth: 0, hasRight: true, peek: null });
});

describe("fitPanels", () => {
  it("keeps the saved widths when there is room (or the row is not measured)", () => {
    expect(fitPanels(1600, saved)).toMatchObject({ sidebar: 240, right: 384, sidebarRail: false });
    expect(fitPanels(0, saved)).toMatchObject({ sidebar: 240, right: 384, rightRail: false });
  });

  it("shrinks both panels toward their minimum before folding anything", () => {
    const fit = fitPanels(960, saved);
    expect(fit.sidebar + fit.right + CENTER_MIN).toBe(960);
    expect(fit.sidebar).toBeGreaterThanOrEqual(PANEL_LIMITS.sidebar.min);
    expect(fit.right).toBeGreaterThanOrEqual(PANEL_LIMITS.right.min);
    expect(fit.sidebarRail || fit.rightRail).toBe(false);
  });

  it("folds the sidebar, then the right panel, when the window is too narrow", () => {
    const narrow = PANEL_LIMITS.sidebar.min + PANEL_LIMITS.right.min + CENTER_MIN - 1;
    expect(fitPanels(narrow, saved)).toMatchObject({
      sidebar: RAIL_WIDTH,
      sidebarRail: true,
      sidebarAuto: true,
      rightRail: false,
      // The room freed by the sidebar goes back to the right panel.
      right: Math.min(PANEL_LIMITS.right.default, narrow - RAIL_WIDTH - CENTER_MIN),
    });
    expect(fitPanels(500, saved)).toMatchObject({
      sidebarRail: true,
      rightRail: true,
      rightAuto: true,
      right: RAIL_WIDTH,
    });
  });

  it("a panel the user collapsed is a rail but not 'auto'", () => {
    expect(fitPanels(1600, { ...saved, sidebarCollapsed: true })).toMatchObject({
      sidebar: RAIL_WIDTH,
      sidebarRail: true,
      sidebarAuto: false,
    });
  });

  it("without a right panel the sidebar keeps its width", () => {
    expect(fitPanels(800, saved, false)).toMatchObject({ sidebar: 240, right: 0 });
  });
});

describe("maxWidthFor", () => {
  it("never squeezes the middle below its minimum", () => {
    expect(maxWidthFor("sidebar", 1280, 384)).toBe(1280 - 384 - CENTER_MIN);
    expect(maxWidthFor("sidebar", 4000, 384)).toBe(PANEL_LIMITS.sidebar.max);
    expect(maxWidthFor("sidebar", 900, 384)).toBe(PANEL_LIMITS.sidebar.min);
  });
});

describe("layout store", () => {
  it("remembers widths and collapsed state in localStorage", () => {
    useLayout.getState().setSize("sidebar", 300);
    useLayout.getState().setCollapsed("right", true);
    expect(JSON.parse(localStorage.getItem(LAYOUT_KEY)!)).toEqual({
      sidebar: 300,
      right: 384,
      sidebarCollapsed: false,
      rightCollapsed: true,
    });
    expect(loadLayout()).toMatchObject({ sidebar: 300, rightCollapsed: true });
  });

  it("does not save while dragging, only the final width", () => {
    useLayout.getState().setSize("sidebar", 310, false);
    expect(localStorage.getItem(LAYOUT_KEY)).toBeNull();
    expect(useLayout.getState().sidebar).toBe(310);
    useLayout.getState().setSize("sidebar", 320);
    expect(loadLayout().sidebar).toBe(320);
  });

  it("clamps sizes and ignores corrupt saved data", () => {
    useLayout.getState().setSize("right", 5000);
    expect(useLayout.getState().right).toBe(PANEL_LIMITS.right.max);
    localStorage.setItem(LAYOUT_KEY, "{not json");
    expect(loadLayout()).toEqual(saved);
    localStorage.setItem(LAYOUT_KEY, JSON.stringify({ sidebar: 10, right: "x" }));
    expect(loadLayout()).toMatchObject({ sidebar: PANEL_LIMITS.sidebar.min, right: 384 });
  });

  it("resetSize goes back to the default", () => {
    useLayout.getState().setSize("sidebar", 400);
    useLayout.getState().resetSize("sidebar");
    expect(useLayout.getState().sidebar).toBe(PANEL_LIMITS.sidebar.default);
    expect(loadLayout().sidebar).toBe(PANEL_LIMITS.sidebar.default);
  });

  it("togglePanel collapses / expands, or peeks when folded for lack of room", () => {
    const s = useLayout.getState();
    s.setRow(1600, true);
    s.togglePanel("sidebar");
    expect(useLayout.getState().sidebarCollapsed).toBe(true);
    useLayout.getState().togglePanel("sidebar");
    expect(useLayout.getState().sidebarCollapsed).toBe(false);

    useLayout.getState().setRow(700, true);
    useLayout.getState().togglePanel("sidebar");
    expect(useLayout.getState()).toMatchObject({ sidebarCollapsed: false, peek: "sidebar" });
    useLayout.getState().togglePanel("sidebar");
    expect(useLayout.getState().peek).toBeNull();
  });
});
