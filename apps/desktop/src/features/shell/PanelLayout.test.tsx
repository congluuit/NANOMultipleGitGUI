import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { repoKeys } from "@/features/repos/queries";
import { initI18n } from "@/i18n";
import type { RepoRefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { CENTER_MIN, LAYOUT_KEY, PANEL_LIMITS, RAIL_WIDTH, useLayout } from "@/stores/layout";
import { useWorkspace } from "@/stores/workspace";
import { layoutShortcut, useLayoutShortcuts } from "./layoutShortcuts";
import { PanelRow, SidePanel } from "./PanelLayout";
import { Sidebar } from "./Sidebar";

function Shell() {
  useLayoutShortcuts();
  return (
    <TooltipProvider>
      <PanelRow hasRight>
        <SidePanel id="sidebar" rail={<span>sidebar rail</span>}>
          <span>sidebar full</span>
        </SidePanel>
        <div>middle</div>
        <SidePanel id="right" rail={<span>right rail</span>}>
          <span>right full</span>
        </SidePanel>
      </PanelRow>
    </TooltipProvider>
  );
}

const sidebarHandle = () => screen.queryByRole("separator", { name: /Resize sidebar/ });
const panel = (id: string) => screen.getByTestId(`panel-${id}`);
const press = (init: KeyboardEventInit, target: EventTarget = window) =>
  act(() => {
    target.dispatchEvent(
      new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }),
    );
  });

beforeEach(() => {
  initI18n("en");
  localStorage.clear();
  useLayout.setState({
    sidebar: PANEL_LIMITS.sidebar.default,
    right: PANEL_LIMITS.right.default,
    sidebarCollapsed: false,
    rightCollapsed: false,
    rowWidth: 0,
    hasRight: true,
    peek: null,
  });
});

describe("side panels", () => {
  it("sizes the panels from the store and resizes them from the handle", () => {
    render(<Shell />);
    expect(panel("sidebar")).toHaveStyle({ width: "240px" });
    expect(panel("right")).toHaveStyle({ width: "384px" });
    fireEvent.keyDown(sidebarHandle()!, { key: "ArrowRight" });
    expect(panel("sidebar")).toHaveStyle({ width: "256px" });
    expect(JSON.parse(localStorage.getItem(LAYOUT_KEY)!).sidebar).toBe(256);
    fireEvent.doubleClick(sidebarHandle()!);
    expect(panel("sidebar")).toHaveStyle({ width: "240px" });
  });

  it("Ctrl+B folds the sidebar to its rail and keeps the content mounted", () => {
    render(<Shell />);
    press({ key: "b", code: "KeyB", ctrlKey: true });
    expect(useLayout.getState().sidebarCollapsed).toBe(true);
    expect(screen.getByText("sidebar rail")).toBeInTheDocument();
    expect(screen.getByText("sidebar full")).toBeInTheDocument();
    expect(panel("sidebar")).toHaveClass("hidden");
    expect(sidebarHandle()).not.toBeInTheDocument();
    press({ key: "b", code: "KeyB", ctrlKey: true });
    expect(screen.queryByText("sidebar rail")).not.toBeInTheDocument();
    expect(panel("sidebar")).not.toHaveClass("hidden");
  });

  it("Ctrl+Alt+B folds the right panel", () => {
    render(<Shell />);
    press({ key: "b", code: "KeyB", ctrlKey: true, altKey: true });
    expect(useLayout.getState()).toMatchObject({ rightCollapsed: true, sidebarCollapsed: false });
    expect(screen.getByText("right rail")).toBeInTheDocument();
  });

  it("leaves Ctrl+B to the integrated terminal", () => {
    render(<Shell />);
    const xterm = document.createElement("div");
    xterm.className = "xterm";
    document.body.append(xterm);
    press({ key: "b", code: "KeyB", ctrlKey: true }, xterm);
    expect(useLayout.getState().sidebarCollapsed).toBe(false);
    xterm.remove();
  });

  it("folds for lack of room and then peeks over the middle instead of resizing", () => {
    render(<Shell />);
    act(() => useLayout.getState().setRow(800, true));
    expect(screen.getByText("sidebar rail")).toBeInTheDocument();
    expect(panel("right")).toHaveStyle({ width: `${800 - RAIL_WIDTH - CENTER_MIN}px` });
    press({ key: "b", code: "KeyB", ctrlKey: true });
    expect(useLayout.getState()).toMatchObject({ peek: "sidebar", sidebarCollapsed: false });
    expect(panel("sidebar")).toHaveClass("absolute");
    expect(panel("sidebar")).toHaveStyle({ left: `${RAIL_WIDTH}px`, width: "240px" });
    press({ key: "Escape" });
    expect(useLayout.getState().peek).toBeNull();
    expect(panel("sidebar")).toHaveClass("hidden");
  });
});

describe("layoutShortcut", () => {
  const ev = (init: KeyboardEventInit) => new KeyboardEvent("keydown", { code: "KeyB", ...init });
  it("maps Ctrl/Cmd+B and Ctrl/Cmd+Alt+B, nothing else", () => {
    expect(layoutShortcut(ev({ ctrlKey: true }), false)).toBe("sidebar");
    expect(layoutShortcut(ev({ ctrlKey: true, altKey: true }), false)).toBe("right");
    expect(layoutShortcut(ev({ metaKey: true }), true)).toBe("sidebar");
    expect(layoutShortcut(ev({ ctrlKey: true }), true)).toBeNull();
    expect(layoutShortcut(ev({ ctrlKey: true, shiftKey: true }), false)).toBeNull();
    expect(layoutShortcut(ev({}), false)).toBeNull();
    expect(
      layoutShortcut(new KeyboardEvent("keydown", { code: "KeyK", ctrlKey: true }), false),
    ).toBeNull();
  });
});

describe("sidebar rail", () => {
  const refs = {
    head: { branch: "main", oid: "a".repeat(40), detached: false, unborn: false },
    local: [
      {
        name: "main",
        oid: "a".repeat(40),
        isHead: true,
        upstream: null,
        upstreamGone: false,
        ahead: 0,
        behind: 0,
      },
      {
        name: "feature/login",
        oid: "b".repeat(40),
        isHead: false,
        upstream: null,
        upstreamGone: false,
        ahead: 0,
        behind: 0,
      },
    ],
    remotes: [],
    tags: [{ name: "v1.0", oid: "c".repeat(40) }],
    stashes: [],
  } as RepoRefs;

  beforeEach(() => {
    queryClient.clear();
    queryClient.setQueryData(repoKeys.refs(1), refs);
    useWorkspace.setState({
      tabs: [{ id: 1, path: "D:/Work/api", name: "api", missing: false }],
      activeId: 1,
      loaded: true,
    });
  });

  const renderSidebar = (rail: boolean) =>
    render(
      <QueryClientProvider client={queryClient}>
        <TooltipProvider>
          <Sidebar rail={rail} />
        </TooltipProvider>
      </QueryClientProvider>,
    );

  it("shows one icon per section and opens its list in a popover", async () => {
    renderSidebar(true);
    for (const name of ["Local (2)", "Remote (0)", "Tags (1)", "Stashes (0)", "LFS", "Worktrees"]) {
      expect(
        screen.getByRole("button", { name: new RegExp(`^${name.replace(/[()]/g, "\\$&")}`) }),
      ).toBeInTheDocument();
    }
    expect(screen.queryByText("feature/login")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Local (2)" }));
    expect(await screen.findByText("feature/login")).toBeInTheDocument();
  });

  it("the expand button of the rail and the collapse button of the full sidebar toggle it", async () => {
    const { unmount } = renderSidebar(false);
    await userEvent.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(useLayout.getState().sidebarCollapsed).toBe(true);
    unmount();
    renderSidebar(true);
    await userEvent.click(screen.getByRole("button", { name: "Show sidebar" }));
    expect(useLayout.getState().sidebarCollapsed).toBe(false);
  });
});
