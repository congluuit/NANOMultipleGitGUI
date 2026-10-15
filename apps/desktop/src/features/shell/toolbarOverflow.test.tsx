import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { initI18n } from "@/i18n";
import { queryClient } from "@/lib/queryClient";
import { PANEL_LIMITS, useLayout } from "@/stores/layout";
import { useWorkspace } from "@/stores/workspace";
import { Toolbar } from "./Toolbar";
import { chooseHidden, MORE_WIDTH, useToolbarOverflow } from "./toolbarOverflow";

describe("chooseHidden", () => {
  const items = ["a", "b", "c", "d"].map((id, index) => ({ id, width: 50, priority: 10, index }));

  it("hides nothing when everything fits", () => {
    expect(chooseHidden(items, 10, 210)).toEqual([]);
    // Not measured (jsdom / detached): nothing hidden.
    expect(chooseHidden(items, 10, 0)).toEqual([]);
  });

  it("hides the rightmost buttons first and keeps room for the ⋯ button", () => {
    // 210 needed: hiding one button frees 50 px, but the ⋯ button takes MORE_WIDTH of it.
    expect(chooseHidden(items, 10, 210 - 50 + MORE_WIDTH)).toEqual(["d"]);
    expect(chooseHidden(items, 10, 210 - 50 + MORE_WIDTH - 1)).toEqual(["c", "d"]);
  });

  it("follows priorities and never hides pinned buttons", () => {
    const mixed = [
      { id: "fetch", width: 50, priority: 10, index: 0 },
      { id: "panel", width: 50, priority: 0, index: 1 },
      { id: "terminal", width: 50, priority: 5, index: 2 },
      { id: "settings", width: 50, priority: Number.POSITIVE_INFINITY, index: 3 },
    ];
    expect(chooseHidden(mixed, 0, 160)).toEqual(["panel", "terminal"]);
    expect(chooseHidden(mixed, 0, 10)).toEqual(["fetch", "panel", "terminal"]);
  });
});

// Fake layout: every toolbar button is 60 px, the row is `rowWidth` px.
let rowWidth = 2000;
const observers: { cb: ResizeObserverCallback; self: ResizeObserver }[] = [];
class MockResizeObserver {
  constructor(cb: ResizeObserverCallback) {
    observers.push({ cb, self: this as unknown as ResizeObserver });
  }
  observe() {}
  unobserve() {}
  disconnect() {}
}
const original = {
  ro: globalThis.ResizeObserver,
  offsetWidth: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "offsetWidth"),
  clientWidth: Object.getOwnPropertyDescriptor(Element.prototype, "clientWidth"),
};

function resize(width: number) {
  rowWidth = width;
  act(() => observers.forEach((o) => o.cb([], o.self)));
}

function renderToolbar() {
  return render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <Toolbar />
      </TooltipProvider>
    </QueryClientProvider>,
  );
}

const wrapperOf = (label: string) =>
  screen.getByRole("button", { name: label }).closest("[data-toolbar-item]");

describe("Toolbar overflow", () => {
  beforeEach(() => {
    initI18n("en");
    queryClient.clear();
    observers.length = 0;
    rowWidth = 2000;
    globalThis.ResizeObserver = MockResizeObserver as unknown as typeof ResizeObserver;
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
      configurable: true,
      get(this: HTMLElement) {
        if (this.dataset.toolbarItem) return this.classList.contains("hidden") ? 0 : 60;
        return this.getAttribute("data-slot") === "separator" ? 9 : 0;
      },
    });
    Object.defineProperty(Element.prototype, "clientWidth", {
      configurable: true,
      get(this: Element) {
        return this.getAttribute("role") === "toolbar" ? rowWidth : 0;
      },
    });
    useToolbarOverflow.setState({ items: {}, hidden: [] });
    useLayout.setState({
      sidebar: PANEL_LIMITS.sidebar.default,
      right: PANEL_LIMITS.right.default,
      sidebarCollapsed: false,
      rightCollapsed: false,
      rowWidth: 0,
      hasRight: true,
      peek: null,
    });
    useWorkspace.setState({
      tabs: [{ id: 1, path: "D:/Work/api", name: "api", missing: false }],
      activeId: 1,
      loaded: true,
    });
  });

  afterEach(() => {
    globalThis.ResizeObserver = original.ro;
    if (original.offsetWidth) {
      Object.defineProperty(HTMLElement.prototype, "offsetWidth", original.offsetWidth);
    }
    if (original.clientWidth) {
      Object.defineProperty(Element.prototype, "clientWidth", original.clientWidth);
    }
  });

  it("shows every button and no ⋯ menu when the row is wide", () => {
    renderToolbar();
    expect(screen.queryByRole("button", { name: "More actions" })).not.toBeInTheDocument();
    for (const name of ["Fetch", "Pull", "Push", "Branch", "Stash", "Undo", "Settings"]) {
      expect(wrapperOf(name)).not.toHaveClass("hidden");
    }
  });

  it("moves the buttons that do not fit into ⋯, without duplicating them", async () => {
    renderToolbar();
    resize(400);
    // Lowest priority first (panel toggles, Terminal), then from the right of the main group.
    expect(useToolbarOverflow.getState().hidden).toEqual([
      "stash",
      "pop",
      "undo",
      "redo",
      "mywork",
      "sidebar",
      "rightPanel",
      "terminal",
    ]);
    expect(wrapperOf("Stash")).toHaveClass("hidden");
    expect(wrapperOf("Settings")).not.toHaveClass("hidden");
    expect(wrapperOf("Fetch")).not.toHaveClass("hidden");
    // One DOM button per action (hidden in place, not cloned).
    expect(screen.getAllByRole("button", { name: "Stash" })).toHaveLength(1);

    await userEvent.click(screen.getByRole("button", { name: "More actions" }));
    const menu = screen.getByRole("menu");
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((i) => i.textContent)).toEqual([
      "Stash",
      "Pop",
      "Undo",
      "Redo",
      "My work",
      "Sidebar",
      "Panel",
      "Terminal",
    ]);
    // Disabled state carried over (no repo data: nothing to stash / undo).
    expect(within(menu).getByRole("menuitem", { name: "Stash" })).toHaveAttribute("data-disabled");
    expect(within(menu).getByRole("menuitem", { name: "Undo" })).toHaveAttribute("data-disabled");
    // Same action as the button.
    await userEvent.click(within(menu).getByRole("menuitem", { name: "Sidebar" }));
    expect(useLayout.getState().sidebarCollapsed).toBe(true);
  });

  it("puts the buttons back when the row widens again", () => {
    renderToolbar();
    resize(400);
    expect(useToolbarOverflow.getState().hidden.length).toBeGreaterThan(0);
    resize(1600);
    expect(useToolbarOverflow.getState().hidden).toEqual([]);
    expect(wrapperOf("Stash")).not.toHaveClass("hidden");
    expect(screen.queryByRole("button", { name: "More actions" })).not.toBeInTheDocument();
  });

  it("keeps the dropdown of a hidden button as a submenu", async () => {
    renderToolbar();
    resize(150);
    expect(useToolbarOverflow.getState().hidden).toContain("pull");
    await userEvent.click(screen.getByRole("button", { name: "More actions" }));
    expect(screen.getByRole("menuitem", { name: "Pull options" })).toBeInTheDocument();
  });
});
