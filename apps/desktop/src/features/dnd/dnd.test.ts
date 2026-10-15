import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { dropChoices } from "./choices";
import { branchAt, registerDropResolver, startBranchDrag, useDnd } from "./store";

const relation = (ahead: number, behind: number) => ({
  source: "s",
  target: "t",
  ahead,
  behind,
  canFastForward: ahead > 0 && behind === 0,
  current: "main",
});

describe("drop menu choices", () => {
  it("offers fast-forward only when possible and merge only when there is something new", () => {
    expect(dropChoices(relation(2, 0))).toEqual(["fastForward", "merge", "rebase"]);
    expect(dropChoices(relation(2, 3))).toEqual(["merge", "rebase"]);
    expect(dropChoices(relation(0, 3))).toEqual(["rebase"]);
    expect(dropChoices(null)).toEqual(["merge", "rebase"]);
  });
});

describe("branch drag", () => {
  let under: Element[] = [];
  beforeEach(() => {
    document.body.innerHTML = `
      <button id="main" data-drop-branch="main"><span id="inner">main</span></button>
      <div id="graph" data-drop-resolver="graph-1"></div>`;
    under = [];
    document.elementsFromPoint = () => under;
    useDnd.setState({ drag: null, drop: null });
  });
  afterEach(() => {
    document.body.innerHTML = "";
  });

  const move = (x: number, y: number) =>
    window.dispatchEvent(new MouseEvent("pointermove", { clientX: x, clientY: y }));
  const up = (x: number, y: number) =>
    window.dispatchEvent(new MouseEvent("pointerup", { clientX: x, clientY: y }));

  it("finds branches by attribute or through a registered resolver", () => {
    under = [document.getElementById("inner")!];
    expect(branchAt(0, 0)).toBe("main");
    under = [document.getElementById("graph")!];
    expect(branchAt(0, 0)).toBeNull();
    const off = registerDropResolver("graph-1", (x) => (x > 10 ? "topic" : null));
    expect(branchAt(20, 0)).toBe("topic");
    expect(branchAt(5, 0)).toBeNull();
    off();
    expect(branchAt(20, 0)).toBeNull();
  });

  it("opens the drop menu after a real drag onto another branch", () => {
    startBranchDrag({ clientX: 0, clientY: 0, button: 0 }, 1, "topic");
    move(2, 2); // below the threshold: still a click
    expect(useDnd.getState().drag).toBeNull();
    under = [document.getElementById("main")!];
    move(30, 30);
    expect(useDnd.getState().drag).toMatchObject({ source: "topic", over: "main" });
    up(30, 30);
    expect(useDnd.getState().drag).toBeNull();
    expect(useDnd.getState().drop).toEqual({
      repoId: 1,
      source: "topic",
      target: "main",
      x: 30,
      y: 30,
    });
    // The click that ends the drag is swallowed.
    const onClick = vi.fn();
    document.getElementById("main")!.addEventListener("click", onClick);
    document.getElementById("main")!.click();
    expect(onClick).not.toHaveBeenCalled();
  });

  it("ignores a plain click, a drop on itself, Escape and other buttons", () => {
    startBranchDrag({ clientX: 0, clientY: 0, button: 0 }, 1, "main");
    up(0, 0);
    expect(useDnd.getState().drop).toBeNull();

    startBranchDrag({ clientX: 0, clientY: 0, button: 0 }, 1, "main");
    under = [document.getElementById("main")!];
    move(40, 0);
    expect(useDnd.getState().drag?.over).toBeNull();
    up(40, 0);
    expect(useDnd.getState().drop).toBeNull();

    startBranchDrag({ clientX: 0, clientY: 0, button: 0 }, 1, "topic");
    move(40, 0);
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(useDnd.getState().drag).toBeNull();
    up(40, 0);
    expect(useDnd.getState().drop).toBeNull();

    startBranchDrag({ clientX: 0, clientY: 0, button: 2 }, 1, "topic");
    move(40, 0);
    expect(useDnd.getState().drag).toBeNull();
  });
});
