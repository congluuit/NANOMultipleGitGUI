import type { ReactNode } from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { openUrl } from "@tauri-apps/plugin-opener";
import { initI18n } from "@/i18n";
import type { Account, Identity, Provider, ProviderAuthConfig, Rule } from "@/ipc";
import { AddAccountWizard } from "./AddAccountWizard";
import { emptyIdentity } from "./forms";
import { IdentityFormView } from "./IdentitiesSection";
import { RulesTab } from "./RulesTab";

const ipc = vi.hoisted(() => ({
  providerSave: vi.fn(),
  authConfig: vi.fn(),
  authAddPat: vi.fn(),
  authUpdateToken: vi.fn(),
  authDeviceStart: vi.fn(),
  authDeviceWait: vi.fn(),
  authCancel: vi.fn(async () => true),
  identityList: vi.fn(async () => [] as Identity[]),
  ruleReorder: vi.fn(async () => [] as Rule[]),
  ruleTest: vi.fn(),
}));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return { ...actual, commands: { ...actual.commands, ...ipc } };
});
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isTauri: () => true,
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));

beforeAll(() => {
  initI18n("vi");
});

beforeEach(() => {
  for (const f of Object.values(ipc)) f.mockClear();
});

function wrap(ui: ReactNode) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}>{ui}</QueryClientProvider>);
}

const github: Provider = {
  id: "github",
  kind: "github",
  name: "GitHub",
  baseUrl: "https://github.com",
  apiUrl: "https://api.github.com",
  oauthClientId: "Iv1.test",
  largeFileWarnMb: 50,
  largeFileBlockMb: 100,
  builtin: true,
};

const selfHosted: Provider = {
  id: "prv_1",
  kind: "gitlabSelfHosted",
  name: "GitLab git.acme.vn",
  baseUrl: "https://git.acme.vn",
  apiUrl: "https://git.acme.vn/api/v4",
  oauthClientId: null,
  largeFileWarnMb: 50,
  largeFileBlockMb: null,
  builtin: false,
};

const config = (over: Partial<ProviderAuthConfig>): ProviderAuthConfig => ({
  providerId: "github",
  hasClientId: true,
  builtinClientId: false,
  deviceFlow: true,
  pkce: false,
  loopbackPort: 47821,
  redirectUri: "http://127.0.0.1:47821/callback",
  patUrl: "https://github.com/settings/tokens/new?scopes=repo,read:user,user:email",
  patScopes: ["repo", "read:user", "user:email"],
  oauthAppsUrl: "https://github.com/settings/applications/new",
  ...over,
});

const account: Account = {
  id: "acc_1",
  providerId: "prv_1",
  username: "lan",
  displayName: "Lan Nguyen",
  avatarUrl: null,
  authMethod: "pat",
  scopes: ["read_user"],
  tokenExpiresAt: null,
  color: "#1c7ed6",
  label: "Công ty",
  defaultCloneDir: null,
  secrets: ["token"],
  sortOrder: 0,
  createdAt: 0,
  updatedAt: 0,
};

describe("AddAccountWizard", () => {
  it("validates the self-hosted URL, creates the instance and adds a PAT", async () => {
    const user = userEvent.setup();
    ipc.providerSave.mockResolvedValue(selfHosted);
    ipc.authConfig.mockResolvedValue(
      config({
        providerId: "prv_1",
        hasClientId: false,
        pkce: true,
        patScopes: ["read_user", "read_api", "read_repository", "write_repository"],
      }),
    );
    ipc.authAddPat.mockResolvedValue(account);
    const onDone = vi.fn();
    wrap(<AddAccountWizard providers={[github]} onDone={onDone} onCancel={() => {}} />);

    await user.click(screen.getByRole("radio", { name: /GitLab tự host/ }));
    await user.click(screen.getByRole("button", { name: "Tiếp tục" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Nhập địa chỉ máy chủ");
    const url = screen.getByLabelText("Địa chỉ máy chủ");
    await user.type(url, "http://git.acme.vn");
    await user.click(screen.getByRole("button", { name: "Tiếp tục" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Chỉ hỗ trợ https://");
    await user.clear(url);
    await user.type(url, "git.acme.vn/{Enter}");
    expect(ipc.providerSave).toHaveBeenCalledWith({
      kind: "gitlabSelfHosted",
      name: "GitLab git.acme.vn",
      baseUrl: "https://git.acme.vn",
    });

    // No OAuth client id: browser/device sign-in disabled, PAT offered.
    expect(await screen.findByText(/chưa có OAuth client ID/)).toBeInTheDocument();
    const methods = within(screen.getByRole("list", { name: "Chọn cách đăng nhập" })).getAllByRole(
      "button",
    );
    expect(methods.map((m) => m.textContent)).toEqual([
      expect.stringContaining("Đăng nhập qua trình duyệt"),
      expect.stringContaining("Mã thiết bị"),
      expect.stringContaining("Token cá nhân"),
    ]);
    expect(methods[0]).toBeDisabled();
    expect(methods[1]).toBeDisabled();
    await user.click(methods[2]!);

    expect(screen.getByText("write_repository")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Kiểm tra và lưu" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Dán token vào đây");
    expect(ipc.authAddPat).not.toHaveBeenCalled();
    await user.type(screen.getByLabelText("Token"), "glpat-abc");
    await user.type(screen.getByLabelText("Nhãn"), "Công ty");
    await user.click(screen.getByRole("button", { name: "Kiểm tra và lưu" }));
    expect(ipc.authAddPat).toHaveBeenCalledWith("prv_1", "glpat-abc", "Công ty");

    expect(await screen.findByText("Đã thêm tài khoản")).toBeInTheDocument();
    expect(screen.getByText(/@lan/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Xong" }));
    expect(onDone).toHaveBeenCalledWith(account);
  });

  it("shows the device code and cancels the flow", async () => {
    const user = userEvent.setup();
    ipc.authConfig.mockResolvedValue(config({}));
    ipc.authDeviceStart.mockResolvedValue({
      flowId: "flow_1",
      prompt: {
        userCode: "WDJB-MJHT",
        verificationUri: "https://github.com/login/device",
        verificationUriComplete: null,
        expiresAt: Math.floor(Date.now() / 1000) + 900,
        intervalSecs: 5,
      },
    });
    ipc.authDeviceWait.mockReturnValue(new Promise(() => {}));
    wrap(<AddAccountWizard providers={[github]} onDone={() => {}} onCancel={() => {}} />);

    await user.click(screen.getByRole("button", { name: "Tiếp tục" }));
    const device = await screen.findByRole("button", { name: /Mã thiết bị/ });
    expect(device).toBeEnabled();
    await user.click(device);
    expect(await screen.findByLabelText("Mã xác nhận")).toHaveTextContent("WDJB-MJHT");
    expect(ipc.authDeviceStart).toHaveBeenCalledWith("github", null);
    expect(ipc.authDeviceWait).toHaveBeenCalledWith("flow_1");
    expect(openUrl).toHaveBeenCalledWith("https://github.com/login/device");
    expect(screen.getByRole("button", { name: /mở trình duyệt/ })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Hủy" }));
    expect(ipc.authCancel).toHaveBeenCalledWith("flow_1");
    const list = screen.getByRole("list", { name: "Chọn cách đăng nhập" });
    expect(within(list).getAllByRole("button")).toHaveLength(2);
  });

  it("explains a missing client id returned by the backend and offers a PAT", async () => {
    const user = userEvent.setup();
    ipc.authConfig.mockResolvedValue(config({}));
    ipc.authDeviceStart.mockRejectedValue({
      kind: "missingClientId",
      message: "GitHub has no OAuth client id yet",
      providerId: "github",
      patUrl: "https://github.com/settings/tokens/new",
      patScopes: ["repo"],
    });
    wrap(<AddAccountWizard providers={[github]} onDone={() => {}} onCancel={() => {}} />);
    await user.click(screen.getByRole("button", { name: "Tiếp tục" }));
    await user.click(await screen.findByRole("button", { name: /Mã thiết bị/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("chưa có OAuth client ID");
    await user.click(screen.getByRole("button", { name: "Dùng token cá nhân" }));
    expect(screen.getByLabelText("Token")).toBeInTheDocument();
  });
});

describe("IdentityFormView", () => {
  it("validates, suggests verified emails and submits", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn(async () => {});
    wrap(
      <IdentityFormView
        initial={emptyIdentity(true)}
        others={[
          {
            id: "idn_1",
            accountId: "acc_1",
            name: "Lan",
            email: "lan@acme.vn",
            signing: "none",
            signingKeyRef: null,
            isDefault: true,
            createdAt: 0,
          },
        ]}
        emails={[
          { email: "lan@acme.vn", verified: true, primary: true, commitEmail: false },
          { email: "lan@home.vn", verified: true, primary: false, commitEmail: false },
          { email: "spam@x.vn", verified: false, primary: false, commitEmail: false },
        ]}
        onSubmit={onSubmit}
        onCancel={() => {}}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Lưu" }));
    expect(screen.getByText("Nhập tên")).toBeInTheDocument();
    expect(screen.getByText("Nhập email")).toBeInTheDocument();

    await user.type(screen.getByLabelText("Tên"), "Lan Nguyen");
    await user.type(screen.getByLabelText("Email"), "LAN@acme.vn");
    await user.click(screen.getByRole("button", { name: "Lưu" }));
    expect(screen.getByText("Email này đã có trong tài khoản")).toBeInTheDocument();

    // Only verified emails not used yet are suggested.
    expect(screen.queryByRole("button", { name: "spam@x.vn" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "lan@acme.vn" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "lan@home.vn" }));
    await user.selectOptions(screen.getByLabelText("Ký commit"), "gpg");
    await user.click(screen.getByRole("button", { name: "Lưu" }));
    expect(screen.getByText("Nhập key dùng để ký")).toBeInTheDocument();
    await user.type(screen.getByLabelText("Key ký"), "3AA5C34371567BD2");
    await user.click(screen.getByRole("button", { name: "Lưu" }));
    expect(onSubmit).toHaveBeenCalledWith({
      name: "Lan Nguyen",
      email: "lan@home.vn",
      signing: "gpg",
      signingKeyRef: "3AA5C34371567BD2",
      isDefault: true,
    });
  });
});

describe("RulesTab", () => {
  const rule = (id: string, priority: number, host: string): Rule => ({
    id,
    priority,
    host,
    ownerOrGroup: null,
    pathGlob: null,
    remoteRegex: null,
    accountId: "acc_1",
    identityId: null,
    enabled: true,
    createdAt: 0,
  });

  it("lists rules by priority and reorders them", async () => {
    const user = userEvent.setup();
    wrap(
      <RulesTab
        rules={[rule("r3", 7, "c.example"), rule("r1", 0, "a.example"), rule("r2", 3, "b.example")]}
        accounts={[{ ...account, providerId: "github" }]}
        identities={[]}
      />,
    );
    const list = screen.getByRole("list", { name: "Quy tắc" });
    const items = within(list).getAllByRole("listitem");
    expect(items.map((li) => within(li).getByText(/\.example/).textContent)).toEqual([
      "a.example",
      "b.example",
      "c.example",
    ]);
    expect(within(items[0]!).getByRole("button", { name: "Ưu tiên cao hơn" })).toBeDisabled();
    await user.click(within(items[0]!).getByRole("button", { name: "Ưu tiên thấp hơn" }));
    expect(ipc.ruleReorder).toHaveBeenCalledWith(["r2", "r1", "r3"]);
  });

  it("validates a new rule", async () => {
    const user = userEvent.setup();
    wrap(<RulesTab rules={[]} accounts={[account]} identities={[]} />);
    await user.click(screen.getByRole("button", { name: "Thêm quy tắc" }));
    const form = screen.getByRole("form", { name: "Quy tắc" });
    await user.click(within(form).getByRole("button", { name: "Lưu" }));
    expect(within(form).getByRole("alert")).toHaveTextContent("Cần ít nhất một điều kiện");
  });
});
