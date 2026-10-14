import { beforeEach, describe, expect, it } from "vitest";
import { DEFAULT_HEIGHT, MAX_HEIGHT, MIN_HEIGHT, useTerminal } from "./store";

const state = () => useTerminal.getState();
const keysOf = (repoId: number) =>
  state()
    .tabs.filter((t) => t.repoId === repoId)
    .map((t) => t.key);

describe("terminal store", () => {
  beforeEach(() => {
    useTerminal.setState({ open: false, height: DEFAULT_HEIGHT, tabs: [], active: {}, seq: 0 });
  });

  it("toggle opens the panel with a first terminal, then hides it without closing shells", () => {
    state().toggle(1);
    expect(state().open).toBe(true);
    expect(keysOf(1)).toHaveLength(1);
    expect(state().active[1]).toBe(keysOf(1)[0]);

    state().toggle(1);
    expect(state().open).toBe(false);
    expect(keysOf(1)).toHaveLength(1);

    state().toggle(1);
    expect(keysOf(1)).toHaveLength(1); // reuses the running terminal
  });

  it("toggle for another repo starts that repo's own terminal", () => {
    state().toggle(1);
    state().setOpen(false);
    state().toggle(2);
    expect(keysOf(1)).toHaveLength(1);
    expect(keysOf(2)).toHaveLength(1);
  });

  it("closing the active tab selects its neighbour; the last one hides the panel", () => {
    const a = state().addTab(1);
    const b = state().addTab(1);
    const c = state().addTab(1);
    state().addTab(2);
    expect(state().active[1]).toBe(c);

    state().activate(1, b);
    state().closeTab(b);
    expect(keysOf(1)).toEqual([a, c]);
    expect(state().active[1]).toBe(c);

    state().closeTab(c);
    expect(state().active[1]).toBe(a);
    expect(state().open).toBe(true);

    state().closeTab(a);
    expect(keysOf(1)).toEqual([]);
    expect(state().active[1]).toBeUndefined();
    expect(state().open).toBe(false);
    expect(keysOf(2)).toHaveLength(1);

    const before = state();
    state().closeTab("missing");
    expect(state()).toBe(before);
  });

  it("drops terminals of closed repos only", () => {
    state().addTab(1);
    state().addTab(2);
    const before = state().tabs;
    state().retainRepos([1, 2]);
    expect(state().tabs).toBe(before); // no change, no re-render
    state().retainRepos([2]);
    expect(keysOf(1)).toEqual([]);
    expect(keysOf(2)).toHaveLength(1);
    expect(Object.keys(state().active)).toEqual(["2"]);
  });

  it("records the shell name and clamps the height", () => {
    const key = state().addTab(1);
    state().setTitle(key, "Git Bash");
    expect(state().tabs[0]?.title).toBe("Git Bash");
    state().setHeight(10);
    expect(state().height).toBe(MIN_HEIGHT);
    state().setHeight(99999);
    expect(state().height).toBe(MAX_HEIGHT);
    state().setHeight(300.4);
    expect(state().height).toBe(300);
  });

  it("persists only the panel height", () => {
    state().addTab(1);
    state().setHeight(333);
    const saved = JSON.parse(localStorage.getItem("nanomgg.terminal") ?? "{}");
    expect(saved.state).toEqual({ height: 333 });
  });
});
