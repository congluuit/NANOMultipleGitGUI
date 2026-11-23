import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, useEffect } from "react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { SecretGuardSettingsSection } from "./SecretGuardSettingsSection";
import { secretFinding, secretReport } from "./testData";
import { useSecretGuard } from "./useSecretGuard";

const ipc = vi.hoisted(() => ({
  secretGuardCheck: vi.fn(),
  secretGuardIgnore: vi.fn(),
  secretGuardPrefs: vi.fn(),
  secretGuardSetEnabled: vi.fn(),
  secretGuardClearIgnored: vi.fn(),
  unstageFiles: vi.fn(),
}));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return { ...actual, commands: { ...actual.commands, ...ipc } };
});
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isTauri: () => true,
}));

const env = secretFinding({ path: "app/.env", line: 2 });
const conf = secretFinding({ path: "src/config.ts", line: 7 });

type Guard = ReturnType<typeof useSecretGuard>;
let guard: Guard | null = null;
const keep = (g: Guard) => {
  guard = g;
};

function Harness({ onRender }: { onRender: (g: Guard) => void }) {
  const g = useSecretGuard(1);
  useEffect(() => onRender(g));
  return g.dialog;
}

const renderHarness = () =>
  render(
    <QueryClientProvider client={queryClient}>
      <Harness onRender={keep} />
    </QueryClientProvider>,
  );

beforeAll(() => initI18n("en"));

beforeEach(() => {
  guard = null;
  queryClient.clear();
  for (const fn of Object.values(ipc)) fn.mockReset();
  ipc.secretGuardIgnore.mockResolvedValue({ enabled: true, ignoredCount: 1 });
  ipc.unstageFiles.mockResolvedValue(null);
});

async function intercept(staged: string[]) {
  const proceed = vi.fn();
  let held = false;
  await act(async () => {
    held = await guard!.intercept(staged, proceed);
  });
  return { held, proceed };
}

describe("useSecretGuard", () => {
  it("lets the commit through when the guard is off or finds nothing", async () => {
    renderHarness();
    ipc.secretGuardCheck.mockResolvedValueOnce({ enabled: false, report: secretReport([]) });
    expect((await intercept(["a"])).held).toBe(false);
    ipc.secretGuardCheck.mockResolvedValueOnce({ enabled: true, report: secretReport([]) });
    expect((await intercept(["a"])).held).toBe(false);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("fails open when the scan itself fails", async () => {
    renderHarness();
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    ipc.secretGuardCheck.mockRejectedValueOnce(new Error("boom"));
    expect((await intercept(["a"])).held).toBe(false);
    warn.mockRestore();
  });

  it("holds the commit, applies the choices, then continues with what is still staged", async () => {
    const user = userEvent.setup();
    renderHarness();
    ipc.secretGuardCheck.mockResolvedValueOnce({
      enabled: true,
      report: secretReport([env, conf]),
    });
    const { held, proceed } = await intercept(["app/.env", "src/config.ts", "README.md"]);
    expect(held).toBe(true);
    expect(screen.getByRole("dialog")).toBeInTheDocument();

    const confRow = screen.getByRole("radiogroup", {
      name: "What to do with src/config.ts line 7",
    });
    await user.click(within(confRow).getByRole("radio", { name: "Always ignore in this repo" }));
    await user.click(screen.getByRole("button", { name: "Apply and commit" }));

    await waitFor(() => expect(proceed).toHaveBeenCalledWith(["src/config.ts", "README.md"]));
    expect(ipc.unstageFiles).toHaveBeenCalledWith(1, ["app/.env"]);
    expect(ipc.secretGuardIgnore).toHaveBeenCalledWith(1, [conf.fingerprint]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("commits anyway with every staged path", async () => {
    const user = userEvent.setup();
    renderHarness();
    ipc.secretGuardCheck.mockResolvedValueOnce({ enabled: true, report: secretReport([env]) });
    const { proceed } = await intercept(["app/.env"]);
    await user.click(screen.getByRole("button", { name: "Commit anyway" }));
    expect(proceed).toHaveBeenCalledWith(["app/.env"]);
    expect(ipc.unstageFiles).not.toHaveBeenCalled();
  });

  it("stops the commit when unstaging fails", async () => {
    const user = userEvent.setup();
    const showNotice = vi.fn();
    useUi.setState({ showNotice });
    renderHarness();
    ipc.secretGuardCheck.mockResolvedValueOnce({ enabled: true, report: secretReport([env]) });
    ipc.unstageFiles.mockRejectedValueOnce(new Error("index locked"));
    const { proceed } = await intercept(["app/.env"]);
    await user.click(screen.getByRole("button", { name: "Apply and commit" }));
    await waitFor(() => expect(showNotice).toHaveBeenCalled());
    expect(showNotice.mock.calls[0]![0]).toMatchObject({ kind: "error" });
    expect(proceed).not.toHaveBeenCalled();
  });
});

describe("SecretGuardSettingsSection", () => {
  const renderSection = () =>
    render(
      <QueryClientProvider client={queryClient}>
        <SecretGuardSettingsSection />
      </QueryClientProvider>,
    );

  it("turns the guard off", async () => {
    const user = userEvent.setup();
    ipc.secretGuardPrefs.mockResolvedValue({ enabled: true, ignoredCount: 0 });
    ipc.secretGuardSetEnabled.mockResolvedValue({ enabled: false, ignoredCount: 0 });
    renderSection();
    const toggle = await screen.findByRole("switch", { name: /Secret Guard/ });
    await waitFor(() => expect(toggle).toBeChecked());
    expect(screen.queryByRole("button", { name: /Forget/ })).not.toBeInTheDocument();
    await user.click(toggle);
    expect(ipc.secretGuardSetEnabled).toHaveBeenCalledWith(false);
    await waitFor(() => expect(toggle).not.toBeChecked());
  });

  it("forgets the always-ignored findings", async () => {
    const user = userEvent.setup();
    ipc.secretGuardPrefs.mockResolvedValue({ enabled: true, ignoredCount: 2 });
    ipc.secretGuardClearIgnored.mockResolvedValue({ enabled: true, ignoredCount: 0 });
    renderSection();
    await user.click(
      await screen.findByRole("button", { name: "Forget 2 always-ignored findings" }),
    );
    expect(ipc.secretGuardClearIgnored).toHaveBeenCalledOnce();
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: /Forget/ })).not.toBeInTheDocument(),
    );
  });
});
