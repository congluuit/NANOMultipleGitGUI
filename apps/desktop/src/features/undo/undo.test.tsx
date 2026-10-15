import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import i18n from "i18next";
import { TooltipProvider } from "@/components/ui/tooltip";
import { initI18n } from "@/i18n";
import { commands, type UndoOutcome, type UndoState, type UndoStep } from "@/ipc";
import { useUi } from "@/stores/ui";
import { runUndo } from "./actions";
import { describeOp, stepTooltip } from "./label";
import { ownsUndoKeys, undoShortcut, useUndoShortcuts } from "./shortcuts";
import { useUndoStore } from "./store";
import { UndoRedoButtons } from "./UndoButtons";

beforeAll(() => {
  initI18n("vi");
});

const step = (extra: Partial<UndoStep> = {}): UndoStep => ({
  id: 1,
  op: { kind: "commit", subject: "abc" },
  time: 0,
  pushed: false,
  blocked: null,
  ...extra,
});

const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

describe("labels", () => {
  it("describes each operation", () => {
    const t = i18n.t;
    expect(describeOp(t, { kind: "commit", subject: "abc" })).toBe("commit “abc”");
    expect(describeOp(t, { kind: "checkout", from: "main", to: "topic" })).toBe(
      "chuyển từ main sang topic",
    );
    expect(describeOp(t, { kind: "deleteBranch", name: "feat" })).toBe("xóa nhánh feat");
    expect(describeOp(t, { kind: "commit", subject: "x".repeat(100) })).toMatch(/…”$/);
  });

  it("builds tooltips with the reason a step is blocked or pushed", () => {
    const t = i18n.t;
    expect(stepTooltip(t, step(), false)).toBe("Hoàn tác: commit “abc”");
    expect(stepTooltip(t, step(), true)).toBe("Làm lại: commit “abc”");
    expect(stepTooltip(t, null, false)).toBe("Không có gì để hoàn tác");
    expect(stepTooltip(t, step({ pushed: true }), false)).toContain("đã có trên remote");
    expect(stepTooltip(t, step({ blocked: "stateChanged" }), false)).toContain(
      "không thể tự hoàn tác",
    );
  });
});

describe("UndoRedoButtons", () => {
  it("enables only runnable steps and describes them", async () => {
    const user = userEvent.setup();
    const onUndo = vi.fn();
    const onRedo = vi.fn();
    const state: UndoState = {
      undo: step(),
      redo: step({ op: { kind: "reset", target: "HEAD~1" }, blocked: "stateChanged" }),
    };
    render(
      <TooltipProvider>
        <UndoRedoButtons state={state} onUndo={onUndo} onRedo={onRedo} />
      </TooltipProvider>,
    );
    const undo = screen.getByRole("button", { name: "Hoàn tác" });
    const redo = screen.getByRole("button", { name: "Làm lại" });
    expect(undo).toBeEnabled();
    expect(undo).toHaveAttribute("aria-description", "Hoàn tác: commit “abc”");
    expect(redo).toBeDisabled();
    expect(redo.getAttribute("aria-description")).toContain("Làm lại: reset về HEAD~1");
    await user.click(undo);
    expect(onUndo).toHaveBeenCalled();
    expect(onRedo).not.toHaveBeenCalled();
  });
});

describe("shortcuts", () => {
  it("maps Ctrl/Cmd+Z, Shift and Ctrl+Y", () => {
    expect(undoShortcut(key({ key: "z", code: "KeyZ", ctrlKey: true }), false)).toBe("undo");
    expect(
      undoShortcut(key({ key: "Z", code: "KeyZ", ctrlKey: true, shiftKey: true }), false),
    ).toBe("redo");
    expect(undoShortcut(key({ key: "y", code: "KeyY", ctrlKey: true }), false)).toBe("redo");
    expect(undoShortcut(key({ key: "z", code: "KeyZ", metaKey: true }), true)).toBe("undo");
    expect(undoShortcut(key({ key: "z", code: "KeyZ", ctrlKey: true }), true)).toBeNull();
    expect(undoShortcut(key({ key: "y", code: "KeyY", metaKey: true }), true)).toBeNull();
    expect(
      undoShortcut(key({ key: "z", code: "KeyZ", ctrlKey: true, altKey: true }), false),
    ).toBeNull();
    expect(undoShortcut(key({ key: "z", code: "KeyZ" }), false)).toBeNull();
  });

  it("leaves text fields, editors and dialogs alone", () => {
    document.body.innerHTML = `
      <input id="i" /><textarea id="t"></textarea>
      <div class="monaco-editor"><div id="m"></div></div>
      <div role="dialog"><button id="d"></button></div>
      <div id="e" contenteditable="true"></div>
      <button id="b"></button>`;
    const el = (id: string) => document.getElementById(id);
    for (const id of ["i", "t", "m", "d"]) expect(ownsUndoKeys(el(id)), id).toBe(true);
    expect(ownsUndoKeys(el("b"))).toBe(false);
    expect(ownsUndoKeys(null)).toBe(false);
    document.body.innerHTML = "";
  });
});

function Shortcuts({ repoId }: { repoId: number }) {
  useUndoShortcuts(repoId);
  return <input aria-label="ô nhập" />;
}

const outcome: UndoOutcome = {
  step: step(),
  head: "h",
  state: { undo: null, redo: step() },
};

describe("useUndoShortcuts and runUndo", () => {
  beforeEach(() => {
    useUi.setState({ notice: null });
    useUndoStore.setState({ byRepo: { 1: { undo: step(), redo: null } }, confirm: null });
  });
  afterEach(() => vi.restoreAllMocks());

  it("undoes on Ctrl+Z, but not while typing in an input", async () => {
    const user = userEvent.setup();
    const undo = vi
      .spyOn(commands, "undoRun")
      .mockResolvedValue({ status: "done", value: outcome });
    render(<Shortcuts repoId={1} />);
    await user.click(screen.getByLabelText("ô nhập"));
    await user.keyboard("{Control>}z{/Control}");
    expect(undo).not.toHaveBeenCalled();
    act(() => (document.activeElement as HTMLElement).blur());
    await user.keyboard("{Control>}z{/Control}");
    expect(undo).toHaveBeenCalledWith(1);
    await vi.waitFor(() =>
      expect(useUi.getState().notice?.message).toBe("Đã hoàn tác: commit “abc”"),
    );
    // The notice offers Redo.
    expect(useUi.getState().notice?.actions?.[0]?.label).toBe("Làm lại");
  });

  it("asks before undoing a pushed operation", async () => {
    const undo = vi.spyOn(commands, "undoRun");
    useUndoStore.setState({ byRepo: { 1: { undo: step({ pushed: true }), redo: null } } });
    await runUndo(1, false);
    expect(undo).not.toHaveBeenCalled();
    expect(useUndoStore.getState().confirm?.repoId).toBe(1);
    undo.mockResolvedValue({ status: "done", value: outcome });
    await runUndo(1, false, true);
    expect(undo).toHaveBeenCalledWith(1);
  });

  it("explains a refusal with how to fix it", async () => {
    vi.spyOn(commands, "undoRun").mockResolvedValue({
      status: "rejected",
      reason: "localChanges",
      message: "error: Entry 'a.txt' not uptodate. Cannot merge.",
    });
    await runUndo(1, false);
    const notice = useUi.getState().notice;
    expect(notice?.kind).toBe("error");
    expect(notice?.hint).toMatch(/Stash/);
  });
});
