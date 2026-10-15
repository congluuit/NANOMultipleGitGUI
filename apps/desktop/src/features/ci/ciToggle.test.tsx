import { render, screen, waitFor } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { act } from "react";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type { CiBatch } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useSettings } from "@/stores/settings";
import { CiSummary } from "./CiSummary";

const ipc = vi.hoisted(() => ({ ciStatusFor: vi.fn() }));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return { ...actual, commands: { ...actual.commands, ...ipc } };
});
vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  isTauri: () => true,
}));

const SHA = "b".repeat(40);
const batch: CiBatch = {
  entries: [
    {
      remote: null,
      fullName: "o/r",
      accountId: "acc",
      status: {
        sha: SHA,
        state: "failure",
        jobs: [],
        pipelineUrl: null,
        truncated: false,
        checksError: null,
      },
    },
  ],
  problem: null,
};

const renderSummary = () =>
  render(
    <QueryClientProvider client={queryClient}>
      <CiSummary accountId="acc" fullName="o/r" sha={SHA} />
    </QueryClientProvider>,
  );

beforeAll(() => initI18n("en"));

beforeEach(() => {
  queryClient.clear();
  ipc.ciStatusFor.mockReset().mockResolvedValue(batch);
});

describe("CI setting", () => {
  it("is off by default: nothing is queried or shown", async () => {
    useSettings.setState({ ciEnabled: false });
    const { container } = renderSummary();
    await new Promise((r) => setTimeout(r, 20));
    expect(ipc.ciStatusFor).not.toHaveBeenCalled();
    expect(container).toBeEmptyDOMElement();
  });

  it("queries and shows CI once turned on", async () => {
    useSettings.setState({ ciEnabled: false });
    renderSummary();
    act(() => useSettings.setState({ ciEnabled: true }));
    await waitFor(() => expect(ipc.ciStatusFor).toHaveBeenCalledWith("acc", "o/r", SHA));
    expect(await screen.findByText(/CI: failed/)).toBeInTheDocument();
  });
});
