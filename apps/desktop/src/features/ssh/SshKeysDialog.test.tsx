import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { TFunction } from "i18next";
import { QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { accountKeys } from "@/features/accounts/queries";
import { PROVIDERS, account } from "@/features/onboarding/testData";
import { initI18n } from "@/i18n";
import { commands, type SshKeyInfo } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { shortFingerprint, testMessage, uploadMessage } from "./messages";
import { sshKeys } from "./queries";
import { SshKeysDialog } from "./SshKeysDialog";

const ME = account("acc_me", "github", "an-dev");

function key(over: Partial<SshKeyInfo> & { id: string }): SshKeyInfo {
  return {
    accountId: "acc_me",
    keyType: "ed25519",
    fingerprint: "SHA256:abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG",
    publicKey: "ssh-ed25519 AAAAC3Nz an@personal.dev",
    comment: "an@personal.dev",
    privateKeyPath: "C:/AppData/nanomgg/ssh/" + over.id,
    hasPassphrase: false,
    passphraseSaved: false,
    uploadedToProvider: false,
    providerKeyId: null,
    createdAt: 1_700_000_000_000,
    isDefault: false,
    explicitDefault: false,
    fileExists: true,
    managed: true,
    ...over,
  };
}

function seed(keys: SshKeyInfo[]) {
  queryClient.setQueryData(accountKeys.list, [ME]);
  queryClient.setQueryData(accountKeys.providers, PROVIDERS);
  queryClient.setQueryData(sshKeys.list("acc_me"), keys);
}

function renderDialog() {
  useAccountDialogs.getState().openSshKeys("acc_me");
  return render(
    <QueryClientProvider client={queryClient}>
      <SshKeysDialog />
    </QueryClientProvider>,
  );
}

describe("SshKeysDialog", () => {
  beforeEach(() => {
    initI18n("vi");
    queryClient.clear();
    useAccountDialogs.setState({ sshKeys: null, manager: null });
  });
  afterEach(() => vi.restoreAllMocks());

  it("lists keys with badges and explains SSH transport", () => {
    seed([
      key({ id: "key_a", isDefault: true, uploadedToProvider: true }),
      key({ id: "key_b", hasPassphrase: true, comment: "work", fileExists: false }),
    ]);
    renderDialog();
    expect(screen.getByRole("dialog", { name: /Khóa SSH — an-dev/ })).toBeInTheDocument();
    expect(screen.getByText(/GIT_SSH_COMMAND/)).toBeInTheDocument();
    const a = within(screen.getByTestId("ssh-key-key_a"));
    expect(a.getByText("Mặc định")).toBeInTheDocument();
    expect(a.getByText("Đã tải lên")).toBeInTheDocument();
    expect(a.queryByRole("button", { name: /Đặt mặc định/ })).toBeNull();
    const b = within(screen.getByTestId("ssh-key-key_b"));
    expect(b.getByText("Passphrase chưa lưu")).toBeInTheDocument();
    expect(b.getByText("Thiếu file khóa")).toBeInTheDocument();
    expect(b.getByRole("button", { name: /Kiểm tra kết nối/ })).toBeDisabled();
  });

  it("shows the empty state", () => {
    seed([]);
    renderDialog();
    expect(screen.getByText(/chưa có khóa SSH/)).toBeInTheDocument();
  });

  it("generates a key with a confirmed passphrase", async () => {
    seed([]);
    const generate = vi.spyOn(commands, "sshKeyGenerate").mockResolvedValue(key({ id: "key_n" }));
    const user = userEvent.setup();
    renderDialog();
    await user.click(screen.getByRole("button", { name: /Tạo khóa mới/ }));
    await user.type(screen.getByLabelText("Passphrase (tùy chọn)"), "pw1");
    await user.type(screen.getByLabelText("Nhập lại passphrase"), "pw2");
    expect(screen.getByText("Hai passphrase không khớp.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Tạo khóa" })).toBeDisabled();
    await user.clear(screen.getByLabelText("Nhập lại passphrase"));
    await user.type(screen.getByLabelText("Nhập lại passphrase"), "pw1");
    await user.click(screen.getByRole("button", { name: "Tạo khóa" }));
    expect(generate).toHaveBeenCalledWith("acc_me", null, "pw1");
    expect(await screen.findByText("Đã tạo khóa mới.")).toBeInTheDocument();
  });

  it("tests the connection and shows who the key authenticates as", async () => {
    seed([key({ id: "key_a", isDefault: true })]);
    vi.spyOn(commands, "sshKeyCheckPermissions").mockResolvedValue({
      keyId: "key_a",
      restricted: true,
      entries: [],
      problem: null,
      repairable: true,
    });
    vi.spyOn(commands, "sshTestConnection").mockResolvedValue({
      status: "authenticated",
      host: "github.com",
      username: "an-dev",
      expectedUsername: "an-dev",
      matchesAccount: true,
      deployKey: false,
      keyId: "key_a",
      output: "Hi an-dev! You've successfully authenticated",
    });
    const user = userEvent.setup();
    renderDialog();
    await user.click(screen.getByRole("button", { name: /Kiểm tra kết nối/ }));
    expect(await screen.findByText("Đã xác thực là @an-dev")).toBeInTheDocument();
  });

  it("asks for confirmation before deleting", async () => {
    seed([key({ id: "key_a" })]);
    const del = vi.spyOn(commands, "sshKeyDelete").mockResolvedValue(true);
    const user = userEvent.setup();
    renderDialog();
    await user.click(screen.getByRole("button", { name: /Xóa/ }));
    expect(del).not.toHaveBeenCalled();
    await user.click(await screen.findByRole("button", { name: "Xóa khóa" }));
    expect(del).toHaveBeenCalledWith("key_a");
  });
});

describe("ssh messages", () => {
  beforeEach(() => initI18n("vi"));
  const t = ((k: string, o?: Record<string, unknown>) =>
    `${k}${o ? JSON.stringify(o) : ""}`) as unknown as TFunction;

  it("maps upload results to actionable messages", () => {
    const scope = uploadMessage(t, {
      kind: "missingScope",
      required: ["admin:public_key", "write:public_key"],
      granted: ["repo"],
    });
    expect(scope.tone).toBe("warn");
    expect(scope.signIn).toBe(true);
    expect(scope.text).toContain("admin:public_key / write:public_key");
    expect(uploadMessage(t, { kind: "missingScope", required: [], granted: null }).text).toContain(
      "missingScopeUnknown",
    );
    expect(uploadMessage(t, { kind: "usedByAnotherAccount" }).tone).toBe("danger");
    expect(uploadMessage(t, { kind: "noToken" }).signIn).toBe(true);
  });

  it("warns when the key authenticates as somebody else", () => {
    const base = {
      status: "authenticated" as const,
      host: "github.com",
      expectedUsername: "an-dev",
      keyId: "k",
      output: "",
    };
    expect(
      testMessage(t, { ...base, username: "bob", matchesAccount: false, deployKey: false }).text,
    ).toContain("mismatch");
    expect(
      testMessage(t, { ...base, username: "acme/api", matchesAccount: false, deployKey: true })
        .text,
    ).toContain("deployKey");
    expect(
      testMessage(t, {
        ...base,
        status: "denied",
        username: null,
        matchesAccount: false,
        deployKey: false,
      }).tone,
    ).toBe("danger");
  });

  it("shortens fingerprints", () => {
    expect(shortFingerprint("SHA256:abcdefghijklmnopqrstuvwxyz", 4)).toBe("SHA256:abcd…wxyz");
    expect(shortFingerprint("SHA256:short", 4)).toBe("SHA256:short");
  });
});
