import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type {
  Account,
  CloneDestination,
  CloneGuess,
  CloneRepoList,
  CloneRequest,
  Provider,
  RepoTab,
} from "@/ipc";
import { accountKeys } from "@/features/accounts/queries";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { queryClient } from "@/lib/queryClient";
import { useWorkspace } from "@/stores/workspace";
import { CloneDialogHost } from "./CloneDialog";
import { makeRepo } from "./testData";

const PROVIDERS: Provider[] = [
  {
    id: "gl",
    kind: "gitlabSelfHosted",
    name: "GitLab acme",
    baseUrl: "https://gitlab.acme.dev",
    apiUrl: "https://gitlab.acme.dev/api/v4",
    oauthClientId: null,
    largeFileWarnMb: 50,
    largeFileBlockMb: null,
    builtin: false,
  },
  {
    id: "gh",
    kind: "github",
    name: "GitHub",
    baseUrl: "https://github.com",
    apiUrl: "https://api.github.com",
    oauthClientId: null,
    largeFileWarnMb: 50,
    largeFileBlockMb: 100,
    builtin: true,
  },
];

function account(id: string, label: string, providerId: string, color: string): Account {
  return {
    id,
    providerId,
    username: id,
    displayName: null,
    avatarUrl: null,
    authMethod: "pat",
    scopes: [],
    tokenExpiresAt: null,
    color,
    label,
    defaultCloneDir: null,
    secrets: ["token"],
    sortOrder: 0,
    createdAt: 1,
    updatedAt: 1,
  };
}

const ACCOUNTS = [
  account("work", "Công ty", "gl", "#e8590c"),
  account("personal", "Cá nhân", "gh", "#5c7cfa"),
  account("oss", "OSS", "gh", "#2f9e44"),
];

const LIST: CloneRepoList = {
  accounts: [
    {
      accountId: "work",
      repos: [
        makeRepo({ fullName: "acme/backend-api", stars: 12, private: true, updatedAt: 9_000 }),
        makeRepo({ fullName: "acme/game-assets", lfsSizeBytes: 4.2 * 1024 ** 3 }),
      ],
      fetchedAt: 1,
      error: null,
      truncated: false,
      refreshing: false,
    },
    {
      accountId: "personal",
      repos: [
        makeRepo({
          fullName: "hoangnv/dotfiles",
          accountId: "personal",
          cloneHttps: "https://github.com/hoangnv/dotfiles.git",
          cloneSsh: "git@github.com:hoangnv/dotfiles.git",
        }),
      ],
      fetchedAt: 1,
      error: null,
      truncated: false,
      refreshing: false,
    },
    {
      accountId: "oss",
      repos: [],
      fetchedAt: 1,
      error: { code: "unauthorized", message: "401" },
      truncated: false,
      refreshing: false,
    },
  ],
};

const TAB: RepoTab = {
  id: 7,
  path: "D:\\Work\\acme\\backend-api",
  name: "backend-api",
  missing: false,
};

function destination(accountId: string | null, url: string, path: string | null): CloneDestination {
  const repo =
    url
      .replace(/\.git$/, "")
      .split(/[/:]/)
      .pop() ?? "";
  const owner = url.includes("hoangnv") ? "hoangnv" : "acme";
  const dir = accountId === "personal" ? "D:\\Me" : "D:\\Work";
  return {
    path: path ?? `${dir}\\${owner}\\${repo}`,
    template: "{clone_dir}/{owner}/{repo}",
    cloneDir: dir,
    owner,
    repo,
    host: "gitlab.acme.dev",
    account: accountId ?? "",
    freeBytes: 100 * 1024 ** 3,
    exists: path === "D:\\busy",
    usable: path !== "D:\\busy",
    sizeBytes: 2048,
    lfsSizeBytes: null,
  };
}

const ipc = vi.hoisted(() => ({
  cloneRepoList: vi.fn(),
  cloneGuess: vi.fn(),
  cloneDestination: vi.fn(),
  cloneSetTemplate: vi.fn(),
  cloneStart: vi.fn(),
  accountSettingsGet: vi.fn(async () => ({
    identityWriteToRepoConfig: true,
    sshWriteCoreSshCommand: false,
  })),
  accountList: vi.fn(),
  providerList: vi.fn(),
  repoOpen: vi.fn(),
  transferCancel: vi.fn(async () => true),
}));
const listen = vi.hoisted(() => vi.fn(async () => () => {}));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return {
    ...actual,
    commands: { ...actual.commands, ...ipc },
    events: {
      ...actual.events,
      cloneProgress: { listen },
      cloneListUpdated: { listen },
    },
  };
});
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isTauri: () => true,
}));

function renderHost() {
  return render(
    <QueryClientProvider client={queryClient}>
      <CloneDialogHost />
    </QueryClientProvider>,
  );
}

const rows = () => within(screen.getByRole("listbox")).queryAllByRole("option");

beforeAll(() => initI18n("vi"));

beforeEach(() => {
  queryClient.clear();
  queryClient.setQueryData(accountKeys.list, ACCOUNTS);
  queryClient.setQueryData(accountKeys.providers, PROVIDERS);
  ipc.cloneRepoList.mockResolvedValue(LIST);
  ipc.accountList.mockResolvedValue(ACCOUNTS);
  ipc.providerList.mockResolvedValue(PROVIDERS);
  ipc.cloneDestination.mockImplementation(async (a: string | null, u: string, p: string | null) =>
    destination(a, u, p),
  );
  ipc.repoOpen.mockResolvedValue(TAB);
  ipc.cloneStart.mockResolvedValue({
    status: "done",
    value: { tab: TAB, lfsPending: false, usesLfs: false, lfsError: null, identityWritten: true },
  });
  useWorkspace.setState({ tabs: [], activeId: null, loaded: true });
  useAccountDialogs.setState({ clone: { accountId: null, url: null } });
});

afterEach(() => vi.clearAllMocks());

describe("CloneDialog", () => {
  it("lists repositories of every account with badges and per-account errors", async () => {
    const user = userEvent.setup();
    renderHost();
    await waitFor(() => expect(rows()).toHaveLength(3));
    const first = rows()[0]!;
    expect(first).toHaveTextContent("Công ty (GitLab acme)");
    expect(first).toHaveTextContent("acme/backend-api");
    expect(first).toHaveTextContent("12");
    expect(within(first).getByLabelText("Riêng tư")).toBeInTheDocument();
    expect(screen.getByText(/LFS 4\.2 GB/)).toBeInTheDocument();
    expect(
      screen.getByText(/Không tải được OSS \(GitHub\): token không hợp lệ/),
    ).toBeInTheDocument();

    await user.type(screen.getByRole("textbox", { name: /Tìm repo/ }), "dotfiles");
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(rows()[0]).toHaveTextContent("Cá nhân (GitHub)");
  });

  it("clones a selected repository as its account and opens the tab", async () => {
    const user = userEvent.setup();
    renderHost();
    await waitFor(() => expect(rows()).toHaveLength(3));
    await user.click(rows()[0]!);
    expect(screen.getByLabelText("Hoặc dán URL")).toHaveValue(
      "https://gitlab.acme.dev/acme/backend-api.git",
    );
    const dest = screen.getByLabelText("Thư mục");
    await waitFor(() => expect(dest).toHaveValue("D:\\Work\\acme\\backend-api"));
    expect(screen.getByText(/Ổ này còn trống 100 GB/)).toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "SSH" }));
    await user.click(screen.getByRole("checkbox", { name: /Shallow/ }));
    await user.click(screen.getByRole("radio", { name: "Tải sau" }));
    const submit = screen.getByRole("button", { name: "Clone" });
    await waitFor(() => expect(submit).toBeEnabled());
    await user.click(submit);

    await waitFor(() => expect(ipc.cloneStart).toHaveBeenCalledTimes(1));
    const request = ipc.cloneStart.mock.calls[0]![1] as CloneRequest;
    expect(request).toMatchObject({
      url: "git@gitlab.acme.dev:acme/backend-api.git",
      accountId: "work",
      dest: "D:\\Work\\acme\\backend-api",
      depth: 1,
      recurseSubmodules: false,
      lfs: { mode: "later" },
      writeIdentity: true,
    });
    await waitFor(() => expect(useAccountDialogs.getState().clone).toBeNull());
    expect(ipc.repoOpen).toHaveBeenCalledWith(TAB.path);
    expect(useWorkspace.getState().activeId).toBe(TAB.id);
  });

  it("guesses the account of a pasted URL but keeps a preset account (Clone as…)", async () => {
    const guess: CloneGuess = {
      valid: true,
      local: false,
      host: "gitlab.acme.dev",
      owner: "acme",
      repo: "tool",
      transport: "https",
      httpsUrl: "https://gitlab.acme.dev/acme/tool.git",
      sshUrl: "git@gitlab.acme.dev:acme/tool.git",
      candidates: [{ accountId: "work", identityId: null, reason: "rule", ruleId: "r1" }],
    };
    ipc.cloneGuess.mockResolvedValue(guess);
    useAccountDialogs.setState({ clone: { accountId: "personal", url: null } });
    const user = userEvent.setup();
    renderHost();
    // The account filter starts on the preset account.
    await waitFor(() => expect(rows()).toHaveLength(1));
    await user.type(screen.getByLabelText("Hoặc dán URL"), guess.httpsUrl!);
    expect(await screen.findByText("→ tài khoản: Công ty (GitLab acme)")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByLabelText("Thư mục")).toHaveValue("D:\\Me\\acme\\tool"));
    await user.click(screen.getByRole("button", { name: "Clone" }));
    await waitFor(() => expect(ipc.cloneStart).toHaveBeenCalled());
    expect(ipc.cloneStart.mock.calls[0]![1]).toMatchObject({
      url: "https://gitlab.acme.dev/acme/tool.git",
      accountId: "personal",
      lfs: { mode: "all" },
    });
  });

  it("blocks a non-empty destination and an empty LFS pattern", async () => {
    const user = userEvent.setup();
    renderHost();
    await waitFor(() => expect(rows()).toHaveLength(3));
    await user.click(rows()[1]!);
    const dest = screen.getByLabelText("Thư mục");
    await waitFor(() => expect(dest).toHaveValue("D:\\Work\\acme\\game-assets"));
    await user.clear(dest);
    await user.type(dest, "D:\\busy");
    expect(await screen.findByText("Thư mục đã tồn tại và không trống.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clone" })).toBeDisabled();

    await user.clear(dest);
    await user.type(dest, "D:\\free");
    await user.click(screen.getByRole("radio", { name: "Chỉ tải:" }));
    await waitFor(() =>
      expect(screen.queryByText("Thư mục đã tồn tại và không trống.")).not.toBeInTheDocument(),
    );
    expect(screen.getByRole("button", { name: "Clone" })).toBeDisabled();
    await user.type(screen.getByLabelText("Mẫu file LFS cần tải"), "Assets/UI/**");
    await waitFor(() => expect(screen.getByRole("button", { name: "Clone" })).toBeEnabled());
    expect(ipc.cloneStart).not.toHaveBeenCalled();
  });
});
