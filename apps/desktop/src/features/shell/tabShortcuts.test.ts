import { beforeEach, describe, expect, it } from "vitest";
import { useWorkspace } from "@/stores/workspace";
import { neighbourTab, tabShortcut } from "./tabShortcuts";

const key = (code: string, mods: Partial<Parameters<typeof tabShortcut>[0]> = {}) => ({
  code,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

describe("tabShortcut", () => {
  it("cycles tabs with Ctrl+Tab on every platform", () => {
    expect(tabShortcut(key("Tab", { ctrlKey: true }), false)).toBe("next");
    expect(tabShortcut(key("Tab", { ctrlKey: true, shiftKey: true }), true)).toBe("prev");
    expect(tabShortcut(key("Tab"), false)).toBeNull();
    expect(tabShortcut(key("Tab", { metaKey: true }), true)).toBeNull();
  });

  it("reopens the last closed tab with Ctrl+Shift+T (⌘⇧T on macOS)", () => {
    expect(tabShortcut(key("KeyT", { ctrlKey: true, shiftKey: true }), false)).toBe("reopen");
    expect(tabShortcut(key("KeyT", { metaKey: true, shiftKey: true }), true)).toBe("reopen");
    expect(tabShortcut(key("KeyT", { ctrlKey: true, shiftKey: true }), true)).toBeNull();
    expect(tabShortcut(key("KeyT", { ctrlKey: true }), false)).toBeNull();
    expect(
      tabShortcut(key("KeyT", { ctrlKey: true, shiftKey: true, altKey: true }), false),
    ).toBeNull();
  });
});

describe("neighbourTab", () => {
  beforeEach(() =>
    useWorkspace.setState({
      tabs: [1, 2, 3].map((id) => ({ id, path: `D:/w/${id}`, name: String(id), missing: false })),
      activeId: 3,
      loaded: true,
    }),
  );

  it("wraps around in both directions", () => {
    expect(neighbourTab(1)).toBe(1);
    expect(neighbourTab(-1)).toBe(2);
    useWorkspace.setState({ activeId: 1 });
    expect(neighbourTab(-1)).toBe(3);
  });

  it("is null with a single tab", () => {
    useWorkspace.setState({ tabs: [{ id: 1, path: "D:/w/1", name: "1", missing: false }] });
    expect(neighbourTab(1)).toBeNull();
  });
});
