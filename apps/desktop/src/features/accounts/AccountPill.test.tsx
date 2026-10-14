import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { commands } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { AccountPill } from "./AccountPill";
import { useAccountDialogs } from "./dialogs";
import { peekOneShot, setOneShot, useAsAccount } from "./switcher/oneShot";
import {
  OSS,
  PERSONAL,
  PERSONAL_ALT,
  WORK,
  makeContext,
  makeRemote,
  makeRule,
  seedAccounts,
  unboundContext,
} from "./switcher/testData";

function renderPill() {
  return render(
    <QueryClientProvider client={queryClient}>
      <AccountPill />
    </QueryClientProvider>,
  );
}

const pill = () => screen.getByTestId("account-pill");
const menu = () => screen.getByRole("menu");
const item = (name: RegExp | string) => within(menu()).getByRole("menuitem", { name });

async function openMenu(user: ReturnType<typeof userEvent.setup>) {
  await user.click(pill());
  return menu();
}

function mockBinding() {
  return vi.spyOn(commands, "repoBindingSet").mockImplementation((repoId, input) =>
    Promise.resolve(
      makeContext({
        repoId,
        account: PERSONAL,
        identity: null,
        source: "manual",
        ...(input.locked ? { source: "locked" } : {}),
      }),
    ),
  );
}

describe("AccountPill", () => {
  beforeEach(() => {
    initI18n("vi");
    queryClient.clear();
    useUi.setState({ notice: null });
    useAsAccount.setState({ byRepo: {} });
    useAccountDialogs.setState({ manager: null, clone: null, importWizard: false });
    useWorkspace.setState({
      tabs: [{ id: 1, path: "D:/Work/api", name: "api", missing: false }],
      activeId: 1,
      loaded: true,
    });
  });
  afterEach(() => vi.restoreAllMocks());

  it("shows the bound account and lists accounts with identities in switcher order", async () => {
    const user = userEvent.setup();
    seedAccounts(queryClient, { contexts: [makeContext({ source: "locked" })] });
    renderPill();
    expect(pill()).toHaveTextContent("Công ty");
    expect(pill()).toHaveAttribute("title", expect.stringContaining("đã khóa"));

    await openMenu(user);
    expect(screen.getByTestId("account-source")).toHaveTextContent(
      "Công ty · đã khóa · hoang@acme.dev",
    );
    const work = item(/^Công ty ?\(GitLab acme\)/);
    const personal = item(/^Cá nhân ?\(GitHub\)/);
    const oss = item(/^Hoang OSS/);
    const follows = (a: Element, b: Element) =>
      (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
    expect(follows(work, personal) && follows(personal, oss)).toBe(true);
    expect(work).toHaveTextContent("Ctrl+Alt+1");
    expect(oss).toHaveTextContent("Ctrl+Alt+3");
    // Personal has two identities → listed under it; Work has one → shown inline.
    expect(work).toHaveTextContent("hoang@acme.dev");
    expect(item(/Hoang \(blog\)/)).toHaveTextContent(PERSONAL_ALT.email);
    expect(item(/^Công ty/)).toHaveAttribute("data-current", "true");
    // "Ghi nhớ" reflects the locked binding.
    expect(within(menu()).getByRole("menuitemcheckbox", { name: /Ghi nhớ/ })).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });

  it("binds the repo with 'Dùng cho repo này', optionally locked", async () => {
    const user = userEvent.setup();
    seedAccounts(queryClient, { contexts: [makeContext()] });
    const set = mockBinding();
    renderPill();

    await openMenu(user);
    await user.click(item(/Hoang \(blog\)/));
    expect(set).toHaveBeenCalledWith(1, {
      remote: null,
      accountId: PERSONAL.id,
      identityId: PERSONAL_ALT.id,
      locked: false,
      source: "manual",
    });

    await openMenu(user);
    await user.click(within(menu()).getByRole("menuitemcheckbox", { name: /Ghi nhớ/ }));
    expect(menu()).toBeInTheDocument(); // stays open
    await user.click(item(/^Hoang OSS/));
    expect(set).toHaveBeenLastCalledWith(
      1,
      expect.objectContaining({ accountId: OSS.id, identityId: null, locked: true }),
    );
  });

  it("sets a one-time override with 'Chỉ cho thao tác này'", async () => {
    const user = userEvent.setup();
    seedAccounts(queryClient, { contexts: [makeContext()] });
    const set = mockBinding();
    renderPill();

    await openMenu(user);
    await user.click(within(menu()).getByRole("menuitemradio", { name: "Chỉ cho thao tác này" }));
    expect(within(menu()).queryByRole("menuitemcheckbox")).not.toBeInTheDocument();
    await user.click(item(/^Hoang OSS/));
    expect(set).not.toHaveBeenCalled();
    expect(peekOneShot(1, "push")).toEqual({ accountId: OSS.id, identityId: null, op: "any" });
    expect(pill()).toHaveTextContent("1×");

    // The pending override can be cancelled from the menu.
    await openMenu(user);
    await user.click(item(/kế tiếp dùng Hoang OSS/));
    expect(peekOneShot(1, "push")).toBeNull();
  });

  it("shows the matched rule, unbinds and opens the account manager", async () => {
    const user = userEvent.setup();
    seedAccounts(queryClient, {
      contexts: [makeContext({ source: "rule", matchedRule: makeRule(), bindings: [] })],
    });
    const clear = vi.spyOn(commands, "repoBindingClear").mockResolvedValue(unboundContext(1));
    renderPill();

    await openMenu(user);
    expect(screen.getByTestId("account-source")).toHaveTextContent(
      "theo rule gitlab.acme.dev/platform/*",
    );
    // A rule result is not a binding: nothing to unbind.
    expect(within(menu()).queryByRole("menuitem", { name: "Bỏ gắn" })).not.toBeInTheDocument();
    await user.click(item("Quản lý tài khoản…"));
    expect(useAccountDialogs.getState().manager).toEqual({ accountId: null, add: false });

    queryClient.setQueryData(["accounts", "repoContext", 1], makeContext());
    await openMenu(user);
    await user.click(item("Bỏ gắn"));
    expect(clear).toHaveBeenCalledWith(1, null);

    await openMenu(user);
    await user.click(item("Thêm tài khoản…"));
    expect(useAccountDialogs.getState().manager).toEqual({ accountId: null, add: true });
  });

  it("lists remotes bound differently", async () => {
    const user = userEvent.setup();
    seedAccounts(queryClient, {
      contexts: [
        makeContext({
          perRemote: [
            makeRemote(),
            makeRemote({
              remote: "upstream",
              url: "https://github.com/oss/api.git",
              account: PERSONAL,
              inherited: false,
            }),
          ],
        }),
      ],
    });
    renderPill();
    await openMenu(user);
    expect(within(menu()).getByText("Theo từng remote")).toBeInTheDocument();
    const upstream = within(menu())
      .getAllByRole("menuitem")
      .find((r) => r.textContent?.startsWith("upstream"));
    expect(upstream).toHaveTextContent(/→.*Cá nhân · chọn thủ công/);
    const origin = within(menu())
      .getAllByRole("menuitem")
      .find((r) => r.textContent?.startsWith("origin"));
    expect(origin).toHaveTextContent("theo repo");
  });

  it("shows an unbound repo, an empty account list and no repo", async () => {
    const user = userEvent.setup();
    seedAccounts(queryClient, { accounts: [], identities: [], contexts: [unboundContext(1)] });
    const { unmount } = renderPill();
    expect(pill()).toHaveTextContent("Chưa gắn tài khoản");
    await openMenu(user);
    expect(screen.getByTestId("account-source")).toHaveTextContent("chưa gắn");
    expect(within(menu()).getByText("Chưa có tài khoản nào")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    unmount();

    useWorkspace.setState({ tabs: [], activeId: null });
    seedAccounts(queryClient);
    renderPill();
    await openMenu(user);
    expect(within(menu()).getByText("Mở một repo để chọn tài khoản")).toBeInTheDocument();
    expect(item(/^Công ty/)).toHaveAttribute("aria-disabled", "true");
  });

  it("marks a pending override set elsewhere (palette)", () => {
    seedAccounts(queryClient, { contexts: [makeContext()] });
    setOneShot(1, { accountId: WORK.id, identityId: null, op: "commit" });
    renderPill();
    expect(pill()).toHaveTextContent("1×");
  });
});
