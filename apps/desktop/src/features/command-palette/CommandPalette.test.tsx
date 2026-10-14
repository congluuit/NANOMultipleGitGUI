import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { useGraphStore } from "@/stores/graph";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { CommandPalette, openCommandPalette, registerCommands } from ".";
import { usePalette } from "./store";

function renderPalette() {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <CommandPalette />
    </QueryClientProvider>,
  );
}

const open = () => act(() => openCommandPalette());
const input = () => screen.getByRole("combobox");
const options = () => screen.getAllByRole("option");
const selected = () => options().find((o) => o.getAttribute("aria-selected") === "true");

describe("CommandPalette", () => {
  beforeEach(() => {
    initI18n("vi");
    useSettings.setState({ theme: "light", language: "vi" });
    useUi.setState({ view: "workspace", settingsOpen: false, notice: null });
    useWorkspace.setState({ tabs: [], activeId: null, loaded: true });
    useGraphStore.setState({ selection: {}, jump: null });
    usePalette.setState({ open: false, start: null });
  });

  const cleanups: (() => void)[] = [];
  afterEach(() => cleanups.splice(0).forEach((f) => f()));

  it("toggles with Ctrl+K and lists grouped commands", async () => {
    const user = userEvent.setup();
    renderPalette();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.keyboard("{Control>}k{/Control}");
    const dialog = await screen.findByRole("dialog", { name: "Bảng lệnh" });
    expect(input()).toHaveFocus();
    expect(within(dialog).getByRole("group", { name: "Ứng dụng" })).toBeInTheDocument();
    expect(within(dialog).getByRole("group", { name: "Tài khoản" })).toBeInTheDocument();
    await user.keyboard("{Control>}k{/Control}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("shows Switch account… disabled while there is no account", async () => {
    // The accounts feature replaces the placeholders (see features/accounts/commands.test.ts).
    const user = userEvent.setup();
    renderPalette();
    open();
    await user.type(input(), "chuyen tai khoan");
    const item = screen.getByRole("option", { name: /Chuyển tài khoản/ });
    expect(item).toHaveAttribute("aria-disabled", "true");
    expect(item).toHaveTextContent("chưa có tài khoản");
    await user.keyboard("{Enter}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("navigates with the arrow keys and runs with Enter", async () => {
    const user = userEvent.setup();
    const ran: string[] = [];
    cleanups.push(
      registerCommands("test.nav", (c) => [
        { id: "t.one", title: "Zeta một", group: "Test", run: () => void ran.push("one") },
        { id: "t.two", title: "Zeta hai", group: "Test", run: () => void ran.push(c.language) },
      ]),
    );
    renderPalette();
    open();
    await user.type(input(), "zeta");
    expect(options().map((o) => o.textContent)).toEqual(["Zeta một", "Zeta hai"]);
    expect(selected()).toHaveTextContent("Zeta một");
    await user.keyboard("{ArrowDown}");
    expect(selected()).toHaveTextContent("Zeta hai");
    expect(input()).toHaveAttribute("aria-activedescendant", selected()!.id);
    await user.keyboard("{ArrowDown}"); // wraps around
    expect(selected()).toHaveTextContent("Zeta một");
    await user.keyboard("{ArrowUp}{Enter}");
    expect(ran).toEqual(["vi"]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("opens a sub-list, goes back with Backspace and remembers the choice as recent", async () => {
    const user = userEvent.setup();
    renderPalette();
    open();
    await user.type(input(), "giao dien");
    expect(selected()).toHaveTextContent("Giao diện…");
    await user.keyboard("{Enter}");
    expect(options().map((o) => o.textContent)).toEqual(["Sáng", "Tối", "Theo hệ thống"]);
    expect(input()).toHaveValue("");

    await user.keyboard("{Backspace}");
    expect(screen.getByRole("group", { name: "Ứng dụng" })).toBeInTheDocument();

    await user.type(input(), "giao dien");
    await user.keyboard("{Enter}");
    await user.type(input(), "toi");
    await user.keyboard("{Enter}");
    expect(useSettings.getState().theme).toBe("dark");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    open();
    const recent = screen.getByRole("group", { name: "Dùng gần đây" });
    expect(within(recent).getAllByRole("option")[0]).toHaveTextContent("Giao diện: Tối");
  });

  it("offers flattened children in top-level search", async () => {
    const user = userEvent.setup();
    renderPalette();
    open();
    await user.type(input(), "english");
    expect(selected()).toHaveTextContent("Ngôn ngữ: English");
    await user.keyboard("{Enter}");
    expect(useSettings.getState().language).toBe("en");
  });

  it("shows an empty state", async () => {
    const user = userEvent.setup();
    renderPalette();
    open();
    await user.type(input(), "qqqqzzzz");
    expect(screen.queryAllByRole("option")).toHaveLength(0);
    expect(screen.getByText("Không tìm thấy lệnh nào")).toBeInTheDocument();
  });

  it("asks for a SHA and jumps to the commit", async () => {
    const user = userEvent.setup();
    useWorkspace.setState({
      tabs: [{ id: 7, name: "api", path: "D:/Work/api", missing: false }],
      activeId: 7,
    });
    renderPalette();
    open();
    await user.type(input(), "sha");
    await user.keyboard("{Enter}");
    expect(input()).toHaveAttribute("placeholder", "Nhập SHA (ít nhất 4 ký tự)…");
    await user.type(input(), "xyz{Enter}");
    expect(screen.getByText(/SHA chỉ gồm 4–40 ký tự hex/)).toBeInTheDocument();
    await user.clear(input());
    await user.type(input(), "abc1234{Enter}");
    expect(useGraphStore.getState().jump).toMatchObject({ repoId: 7, oid: "abc1234" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("can start inside a command and switch tabs from it", async () => {
    const user = userEvent.setup();
    useWorkspace.setState({
      tabs: [
        { id: 1, name: "api", path: "D:/Work/api", missing: false },
        { id: 2, name: "web", path: "D:/Work/web", missing: false },
      ],
      activeId: 1,
    });
    renderPalette();
    act(() => openCommandPalette({ commandId: "tab.switch" }));
    expect(options().map((o) => o.textContent)).toEqual(["apiTab 1", "webTab 2"]);
    await user.keyboard("{ArrowDown}{Enter}");
    expect(useWorkspace.getState().activeId).toBe(2);
  });

  it("reports a failing command in the notice bar", async () => {
    const user = userEvent.setup();
    cleanups.push(
      registerCommands("test.fail", () => [
        {
          id: "t.fail",
          title: "Qux hỏng",
          group: "Test",
          run: () => Promise.reject(new Error("boom")),
        },
      ]),
    );
    renderPalette();
    open();
    await user.type(input(), "qux{Enter}");
    await vi.waitFor(() =>
      expect(useUi.getState().notice).toEqual({ kind: "error", message: "boom" }),
    );
  });
});
