import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it } from "vitest";
import App from "./App";
import { usePalette } from "./features/command-palette/store";
import { initI18n } from "./i18n";
import type { RepoTab } from "./ipc";
import { useGraphStore } from "./stores/graph";
import { useSettings } from "./stores/settings";
import { useUi } from "./stores/ui";
import { useWorkspace } from "./stores/workspace";

function renderApp() {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <App />
    </QueryClientProvider>,
  );
}

const repo = (over: Partial<RepoTab> = {}): RepoTab => ({
  id: 1,
  path: "D:/Work/api",
  name: "api",
  missing: false,
  ...over,
});

function openTabs(...tabs: RepoTab[]) {
  useWorkspace.setState({ tabs, activeId: tabs[0]?.id ?? null, loaded: true });
}

describe("App", () => {
  beforeEach(() => {
    initI18n("vi");
    useSettings.setState({ theme: "light", language: "vi" });
    useUi.setState({ view: "workspace", settingsOpen: false, notice: null, initRequest: null });
    useWorkspace.setState({ tabs: [], activeId: null, loaded: true });
    useGraphStore.setState({ selection: {}, jump: null });
  });

  it("shows the welcome screen when no repository is open", () => {
    renderApp();
    expect(screen.getByText("Bắt đầu với một repo")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Mở repo/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Tạo repo mới/ })).toBeInTheDocument();
  });

  it("renders the main layout for an open repository", () => {
    openTabs(repo(), repo({ id: 2, name: "web", path: "D:/Work/web" }));
    renderApp();
    const tabs = screen.getAllByRole("tab");
    expect(tabs.map((t) => t.textContent)).toEqual(["api", "web"]);
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("complementary", { name: "Thay đổi" })).toBeInTheDocument();
    expect(screen.getByText("Hoàn tác")).toBeInTheDocument();
    expect(screen.queryByText("Bắt đầu với một repo")).not.toBeInTheDocument();
  });

  it("switches the active tab on click", async () => {
    const user = userEvent.setup();
    openTabs(repo(), repo({ id: 2, name: "web", path: "D:/Work/web" }));
    renderApp();
    await user.click(screen.getByRole("tab", { name: /web/ }));
    expect(useWorkspace.getState().activeId).toBe(2);
  });

  it("offers recovery actions for a repository whose folder is gone", () => {
    openTabs(repo({ missing: true }));
    renderApp();
    expect(screen.getByText("Không tìm thấy thư mục")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Thử mở lại" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Xóa khỏi danh sách" })).toBeInTheDocument();
  });

  it("asks before initializing a folder that is not a repository", async () => {
    const user = userEvent.setup();
    renderApp();
    act(() => useUi.getState().requestInit({ path: "D:/Work/new", reason: "notRepository" }));
    expect(await screen.findByText("Thư mục này chưa phải repo git")).toBeInTheDocument();
    const create = screen.getByRole("button", { name: "Tạo repo" });
    expect(create).toBeEnabled();
    const input = screen.getByLabelText("Tên nhánh khởi tạo");
    await user.clear(input);
    await user.type(input, "bad name");
    expect(create).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Hủy" }));
    expect(useUi.getState().initRequest).toBeNull();
  });

  it("shows commit details for the selected commit and closes them", async () => {
    const user = userEvent.setup();
    openTabs(repo());
    renderApp();
    expect(screen.getByRole("complementary", { name: "Thay đổi" })).toBeInTheDocument();
    act(() => useGraphStore.getState().select(1, { hash: "a".repeat(40), index: 3 }));
    expect(
      await screen.findByRole("complementary", { name: "Chi tiết commit" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Đóng chi tiết/ }));
    expect(useGraphStore.getState().selection[1]).toBeNull();
    expect(screen.getByRole("complementary", { name: "Thay đổi" })).toBeInTheDocument();
  });

  it("keeps selections per repository", () => {
    openTabs(repo(), repo({ id: 2, name: "web", path: "D:/Work/web" }));
    renderApp();
    act(() => useGraphStore.getState().select(2, { hash: "b".repeat(40), index: 0 }));
    expect(screen.getByRole("complementary", { name: "Thay đổi" })).toBeInTheDocument();
    act(() => useWorkspace.getState().setActive(2));
    expect(screen.getByRole("complementary", { name: "Chi tiết commit" })).toBeInTheDocument();
  });

  it("applies the chosen theme to <html data-theme>", () => {
    renderApp();
    expect(document.documentElement.dataset.theme).toBe("light");
    act(() => useSettings.getState().setTheme("dark"));
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("switches language at runtime", async () => {
    renderApp();
    await act(async () => useSettings.getState().setLanguage("en"));
    expect(await screen.findByText("Start with a repository")).toBeInTheDocument();
    expect(document.documentElement.lang).toBe("en");
  });

  it("opens settings and changes theme through the dialog", async () => {
    const user = userEvent.setup();
    openTabs(repo());
    renderApp();
    await user.click(screen.getAllByRole("button", { name: "Cài đặt" })[0]!);
    await user.click(await screen.findByRole("radio", { name: "Tối" }));
    expect(useSettings.getState().theme).toBe("dark");
  });

  it("shows an error notice that can be dismissed", async () => {
    const user = userEvent.setup();
    renderApp();
    act(() => useUi.getState().showNotice({ kind: "error", message: "boom" }));
    expect(screen.getByRole("alert")).toHaveTextContent("boom");
    await user.click(screen.getByRole("button", { name: "Ẩn" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows the palette page", async () => {
    renderApp();
    act(() => useUi.getState().setView("palette"));
    expect(await screen.findByText("Bảng màu & design tokens")).toBeInTheDocument();
  });
});

describe("App command palette", () => {
  beforeEach(() => {
    initI18n("vi");
    useSettings.setState({ theme: "light", language: "vi" });
    useUi.setState({ view: "workspace", settingsOpen: false, notice: null, initRequest: null });
    useWorkspace.setState({ tabs: [], activeId: null, loaded: true });
    useGraphStore.setState({ selection: {}, jump: null });
    usePalette.setState({ open: false, start: null });
  });

  it("opens with Ctrl+K and runs a command", async () => {
    const user = userEvent.setup();
    openTabs(repo());
    renderApp();
    await user.keyboard("{Control>}k{/Control}");
    const input = await screen.findByRole("combobox", { name: "Gõ lệnh hoặc tìm kiếm…" });
    await user.type(input, "cai dat{Enter}");
    expect(useUi.getState().settingsOpen).toBe(true);
    expect(screen.queryByRole("dialog", { name: "Bảng lệnh" })).not.toBeInTheDocument();
    expect(await screen.findByRole("dialog", { name: "Cài đặt" })).toBeInTheDocument();
  });

  it("opens from the search pill in the top bar", async () => {
    const user = userEvent.setup();
    renderApp();
    await user.click(screen.getByRole("button", { name: /Tìm kiếm \/ lệnh/ }));
    expect(await screen.findByRole("dialog", { name: "Bảng lệnh" })).toBeInTheDocument();
  });
});
