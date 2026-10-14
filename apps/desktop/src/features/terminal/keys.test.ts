import { describe, expect, it, vi } from "vitest";
import { terminalKeyHandler } from "./keys";

describe("terminal keys", () => {
  const key = (code: string, mods: Partial<KeyboardEventInit> = {}, type = "keydown") =>
    new KeyboardEvent(type, { code, cancelable: true, ...mods });

  it("copies the selection on Ctrl+Shift+C without reaching the shell", () => {
    const write = vi.fn(() => Promise.resolve());
    const term = { hasSelection: () => true, getSelection: () => "abc" };
    const handle = terminalKeyHandler(term, write);
    const e = key("KeyC", { ctrlKey: true, shiftKey: true });
    expect(handle(e)).toBe(false);
    expect(e.defaultPrevented).toBe(true);
    expect(write).toHaveBeenCalledWith("abc");
  });

  it("leaves Ctrl+Shift+V to the native paste and Ctrl+` to the panel toggle", () => {
    const write = vi.fn(() => Promise.resolve());
    const handle = terminalKeyHandler({ hasSelection: () => false, getSelection: () => "" }, write);
    const paste = key("KeyV", { ctrlKey: true, shiftKey: true });
    expect(handle(paste)).toBe(false);
    expect(paste.defaultPrevented).toBe(false);
    expect(handle(key("Backquote", { ctrlKey: true }))).toBe(false);
    // Plain Ctrl+C (SIGINT) and typing go to the shell.
    expect(handle(key("KeyC", { ctrlKey: true }))).toBe(true);
    expect(handle(key("KeyA"))).toBe(true);
    expect(handle(key("KeyC", { ctrlKey: true, shiftKey: true }))).toBe(false);
    expect(write).not.toHaveBeenCalled(); // nothing selected
  });
});
