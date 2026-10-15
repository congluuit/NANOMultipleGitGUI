import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "@/App";
import { initI18n } from "@/i18n";
import { commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useGraphStore } from "@/stores/graph";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { useAccountDialogs } from "../dialogs";
import { useBannerDismissed } from "./bannerStore";
import { makeContext, OSS, PERSONAL, seedAccounts, unboundContext, WORK } from "./testData";
import { UnboundBanner } from "./UnboundBanner";

const wrap = (ui: React.ReactNode) =>
  render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>);

const banner = () => screen.queryByTestId("unbound-banner");

beforeEach(() => {
  initI18n("vi");
  queryClient.clear();
  useBannerDismissed.setState({ ids: {} });
  useUi.setState({ view: "workspace", settingsOpen: false, notice: null, initRequest: null });
  useAccountDialogs.setState({ manager: null });
  useWorkspace.setState({
    tabs: [
      { id: 1, path: "D:/Work/api", name: "api", missing: false },
      { id: 2, path: "D:/Work/web", name: "web", missing: false },
    ],
    activeId: 1,
    loaded: true,
  });
});
afterEach(() => vi.restoreAllMocks());

describe("UnboundBanner", () => {
  it("offers one quick button per account and binds on click", async () => {
    const user = userEvent.setup();
    seedAccounts(queryClient, { contexts: [unboundContext(1)] });
    const set = vi.spyOn(commands, "repoBindingSet").mockResolvedValue(makeContext());
    wrap(<UnboundBanner repoId={1} />);
    const bar = banner()!;
    expect(bar).toHaveTextContent("Repo này chưa gắn tài khoản — chọn tài khoản");
    const buttons = within(bar)
      .getAllByRole("button")
      .map((b) => b.textContent);
    expect(buttons.slice(0, 3)).toEqual(["CCông ty", "CCá nhân", "HHoang OSS"]);
    await user.click(within(bar).getByRole("button", { name: "Cá nhân" }));
    expect(set).toHaveBeenCalledWith(1, expect.objectContaining({ accountId: PERSONAL.id }));
  });

  it("is hidden while the context loads and for a bound repo", () => {
    seedAccounts(queryClient);
    const { unmount } = wrap(<UnboundBanner repoId={1} />);
    expect(banner()).toBeNull();
    unmount();
    queryClient.setQueryData(["accounts", "repoContext", 1], makeContext());
    wrap(<UnboundBanner repoId={1} />);
    expect(banner()).toBeNull();
  });

  it("can be dismissed for this repo for the session", async () => {
    const user = userEvent.setup();
    seedAccounts(queryClient, { contexts: [unboundContext(1), unboundContext(2)] });
    const { unmount } = wrap(<UnboundBanner repoId={1} />);
    await user.click(within(banner()!).getByRole("button", { name: "Ẩn" }));
    expect(banner()).toBeNull();
    unmount();
    const other = wrap(<UnboundBanner repoId={2} />);
    expect(banner()).not.toBeNull();
    other.unmount();
    wrap(<UnboundBanner repoId={1} />);
    expect(banner()).toBeNull();
  });

  it("opens the manager to add an account or create a rule", async () => {
    const user = userEvent.setup();
    seedAccounts(queryClient, { accounts: [], identities: [], contexts: [unboundContext(1)] });
    wrap(<UnboundBanner repoId={1} />);
    await user.click(screen.getByRole("button", { name: "Thêm tài khoản…" }));
    expect(useAccountDialogs.getState().manager).toEqual({
      accountId: null,
      add: true,
      reauth: false,
    });
    await user.click(screen.getByRole("button", { name: "Tạo rule cho thư mục/host này…" }));
    expect(useAccountDialogs.getState().manager).toEqual({
      accountId: null,
      add: false,
      reauth: false,
    });
  });
});

describe("AppShell account colors", () => {
  function renderApp() {
    useSettings.setState({ theme: "light", language: "vi" });
    useGraphStore.setState({ selection: {}, jump: null });
    return wrap(<App />);
  }

  it("colors the window border and the repo tabs by account", () => {
    seedAccounts(queryClient, {
      contexts: [makeContext({ repoId: 1 }), makeContext({ repoId: 2, account: OSS })],
    });
    renderApp();
    expect(screen.getByTestId("app-shell")).toHaveStyle({ borderTopColor: WORK.color });
    const [api, web] = screen.getAllByRole("tab");
    expect(api).toHaveAttribute("data-account-color", WORK.color);
    expect(web).toHaveAttribute("data-account-color", OSS.color);
    expect(banner()).toBeNull();
  });

  it("keeps a neutral border and shows the banner for an unbound repo", () => {
    seedAccounts(queryClient, { contexts: [unboundContext(1)] });
    renderApp();
    expect(screen.getByTestId("app-shell").style.borderTopColor).toBe("var(--border)");
    expect(screen.getAllByRole("tab")[0]).not.toHaveAttribute("data-account-color");
    expect(banner()).not.toBeNull();
    // Read-only UI stays available.
    expect(screen.getByRole("complementary", { name: "Thay đổi" })).toBeInTheDocument();
  });
});
