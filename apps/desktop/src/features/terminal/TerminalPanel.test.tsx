import { act, render, renderHook, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import type React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { makeContext, seedAccounts, unboundContext } from "@/features/accounts/switcher/testData";
import { initI18n } from "@/i18n";
import { commands, type TerminalEvent } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { DEFAULT_HEIGHT, useTerminal } from "./store";
import { TerminalHost } from "./TerminalHost";
import TerminalPanel from "./TerminalPanel";
import { useTerminalShortcut } from "./useTerminalShortcut";

// The account header reads the repo's account context from the query cache.
const wrapper = ({ children }: { children: React.ReactNode }) => (
  <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
);

// jsdom has no canvas/layout: xterm.js and its fit addon are replaced by recording fakes.
const fakes = vi.hoisted(() => {
  class FakeTerminal {
    static instances: FakeTerminal[] = [];
    cols = 80;
    rows = 24;
    options: Record<string, unknown>;
    written: string[] = [];
    disposed = false;
    focused = false;
    onDataCb: ((data: string) => void) | null = null;
    onResizeCb: ((size: { cols: number; rows: number }) => void) | null = null;
    keyHandler: ((e: KeyboardEvent) => boolean) | null = null;
    constructor(options: Record<string, unknown>) {
      this.options = { ...options };
      FakeTerminal.instances.push(this);
    }
    loadAddon() {}
    open() {}
    write(data: string) {
      this.written.push(data);
    }
    reset() {
      this.written = [];
    }
    focus() {
      this.focused = true;
    }
    dispose() {
      this.disposed = true;
    }
    onData(cb: (data: string) => void) {
      this.onDataCb = cb;
      return { dispose: () => (this.onDataCb = null) };
    }
    onResize(cb: (size: { cols: number; rows: number }) => void) {
      this.onResizeCb = cb;
      return { dispose: () => (this.onResizeCb = null) };
    }
    attachCustomKeyEventHandler(handler: (e: KeyboardEvent) => boolean) {
      this.keyHandler = handler;
    }
    hasSelection() {
      return false;
    }
    getSelection() {
      return "";
    }
  }
  class FakeFitAddon {
    fit() {}
    proposeDimensions() {
      return { cols: 80, rows: 24 };
    }
  }
  class FakeChannel<T> {
    onmessage: (message: T) => void = () => {};
  }
  return { FakeTerminal, FakeFitAddon, FakeChannel };
});

vi.mock("@xterm/xterm", () => ({ Terminal: fakes.FakeTerminal }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: fakes.FakeFitAddon }));
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  Channel: fakes.FakeChannel,
}));

type Channel = InstanceType<typeof fakes.FakeChannel<TerminalEvent>>;
const terms = () => fakes.FakeTerminal.instances;

function mockCommands() {
  const channels: Channel[] = [];
  let nextId = 1;
  const open = vi
    .spyOn(commands, "terminalOpen")
    .mockImplementation((_repo, _cols, _rows, channel) => {
      channels.push(channel as unknown as Channel);
      return Promise.resolve({ id: nextId++, shell: "Git Bash" });
    });
  const write = vi.spyOn(commands, "terminalWrite").mockResolvedValue(null);
  const resize = vi.spyOn(commands, "terminalResize").mockResolvedValue(null);
  const close = vi.spyOn(commands, "terminalClose").mockResolvedValue(null);
  return { open, write, resize, close, channels };
}

describe("TerminalPanel", () => {
  beforeEach(() => {
    initI18n("en");
    fakes.FakeTerminal.instances = [];
    useUi.setState({ resolvedTheme: "light" });
    useWorkspace.setState({ activeId: 1 });
    queryClient.clear();
    seedAccounts(queryClient, { contexts: [makeContext()] });
    useTerminal.setState({ open: false, height: DEFAULT_HEIGHT, tabs: [], active: {}, seq: 0 });
  });
  afterEach(() => vi.restoreAllMocks());

  it("opens a shell in the repo, streams output and forwards input/resizes", async () => {
    const ipc = mockCommands();
    act(() => useTerminal.getState().toggle(1));
    render(<TerminalPanel repoId={1} />, { wrapper });

    // Header: account + identity of the repo, tab named after the shell.
    expect(screen.getByTestId("terminal-account")).toHaveTextContent("Công ty");
    expect(screen.getByTestId("terminal-account")).toHaveTextContent("hoang@acme.dev");
    expect(await screen.findByRole("tab", { name: "1: Git Bash" })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    expect(ipc.open).toHaveBeenCalledWith(1, 80, 24, expect.anything());
    const term = terms()[0]!;
    expect(term.options.fontFamily).toBeTruthy();
    expect(term.focused).toBe(true);

    act(() => ipc.channels[0]!.onmessage({ kind: "output", data: "$ " }));
    expect(term.written).toContain("$ ");

    term.onDataCb!("ls\r");
    expect(ipc.write).toHaveBeenCalledWith(1, "ls\r");
    term.onResizeCb!({ cols: 120, rows: 30 });
    expect(ipc.resize).toHaveBeenCalledWith(1, 120, 30);
  });

  it("offers a restart after the shell exits", async () => {
    const ipc = mockCommands();
    act(() => useTerminal.getState().toggle(1));
    render(<TerminalPanel repoId={1} />, { wrapper });
    await screen.findByRole("tab", { name: "1: Git Bash" });
    const term = terms()[0]!;

    act(() => ipc.channels[0]!.onmessage({ kind: "exit", code: 0 }));
    expect(term.written.join("")).toContain("Process exited with code 0");
    term.onDataCb!("x");
    expect(ipc.write).not.toHaveBeenCalled();

    term.onDataCb!("\r");
    expect(ipc.open).toHaveBeenCalledTimes(2);
  });

  it("re-themes the terminals when the app theme changes", async () => {
    mockCommands();
    act(() => useTerminal.getState().toggle(1));
    render(<TerminalPanel repoId={1} />, { wrapper });
    await screen.findByRole("tab", { name: "1: Git Bash" });
    const term = terms()[0]!;
    const before = term.options.theme;
    act(() => useUi.setState({ resolvedTheme: "dark" }));
    expect(term.options.theme).not.toBe(before);
  });

  it("adds and closes tabs; closing kills the session", async () => {
    const user = userEvent.setup();
    const ipc = mockCommands();
    act(() => useTerminal.getState().toggle(1));
    render(<TerminalPanel repoId={1} />, { wrapper });
    await screen.findByRole("tab", { name: "1: Git Bash" });

    await user.click(screen.getByRole("button", { name: "New terminal" }));
    expect(await screen.findByRole("tab", { name: "2: Git Bash" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(ipc.open).toHaveBeenCalledTimes(2);

    await user.click(screen.getByRole("button", { name: "Close terminal 2: Git Bash" }));
    expect(ipc.close).toHaveBeenCalledWith(2);
    expect(terms()[1]!.disposed).toBe(true);
    expect(screen.getByRole("tab", { name: "1: Git Bash" })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    await user.click(screen.getByRole("button", { name: "Close terminal 1: Git Bash" }));
    expect(ipc.close).toHaveBeenCalledWith(1);
    expect(useTerminal.getState().open).toBe(false);
  });

  it("keeps other repos' shells alive but hidden, and shows an empty state", async () => {
    const ipc = mockCommands();
    act(() => useTerminal.getState().toggle(1));
    const { rerender } = render(<TerminalPanel repoId={1} />, { wrapper });
    await screen.findByRole("tab", { name: "1: Git Bash" });

    rerender(<TerminalPanel repoId={2} />);
    expect(screen.queryByRole("tab")).toBeNull();
    expect(screen.getByText("No terminal for this repository yet.")).toBeInTheDocument();
    expect(ipc.close).not.toHaveBeenCalled();
    expect(terms()[0]!.disposed).toBe(false);
  });

  it("shows when no account is bound", () => {
    mockCommands();
    seedAccounts(queryClient, { contexts: [unboundContext(1)] });
    act(() => useTerminal.getState().toggle(1));
    render(<TerminalPanel repoId={1} />, { wrapper });
    expect(screen.getByTestId("terminal-account")).toHaveTextContent("No account bound");
  });
});

describe("TerminalHost", () => {
  beforeEach(() => {
    initI18n("en");
    fakes.FakeTerminal.instances = [];
    useTerminal.setState({ open: false, height: DEFAULT_HEIGHT, tabs: [], active: {}, seq: 0 });
    useWorkspace.setState({
      tabs: [{ id: 1, path: "D:/w/api", name: "api", missing: false }],
      activeId: 1,
      loaded: true,
    });
  });
  afterEach(() => vi.restoreAllMocks());

  it("loads the panel lazily and ends terminals of closed repos", async () => {
    const ipc = mockCommands();
    const { container } = render(<TerminalHost />, { wrapper });
    expect(container).toBeEmptyDOMElement(); // xterm chunk not requested yet

    act(() => useTerminal.getState().toggle(1));
    expect(await screen.findByRole("tab", { name: "1: Git Bash" })).toBeInTheDocument();
    expect(ipc.open).toHaveBeenCalledWith(1, 80, 24, expect.anything());

    act(() => useWorkspace.setState({ tabs: [] }));
    expect(useTerminal.getState().tabs).toEqual([]);
    expect(ipc.close).toHaveBeenCalledWith(1);
  });
});

describe("Ctrl+` shortcut", () => {
  beforeEach(() => {
    useTerminal.setState({ open: false, height: DEFAULT_HEIGHT, tabs: [], active: {}, seq: 0 });
  });

  it("toggles the panel only while a repo is usable", () => {
    const press = (init: KeyboardEventInit) =>
      act(() => {
        window.dispatchEvent(new KeyboardEvent("keydown", { cancelable: true, ...init }));
      });
    const { rerender, unmount } = renderHook(({ id }) => useTerminalShortcut(id), {
      initialProps: { id: null as number | null },
    });
    press({ code: "Backquote", ctrlKey: true });
    expect(useTerminal.getState().open).toBe(false);

    rerender({ id: 3 });
    press({ code: "Backquote", ctrlKey: true, shiftKey: true });
    expect(useTerminal.getState().open).toBe(false);
    press({ code: "Backquote", ctrlKey: true });
    expect(useTerminal.getState().open).toBe(true);
    expect(useTerminal.getState().tabs.map((t) => t.repoId)).toEqual([3]);
    press({ code: "Backquote", ctrlKey: true });
    expect(useTerminal.getState().open).toBe(false);
    unmount();
  });
});
