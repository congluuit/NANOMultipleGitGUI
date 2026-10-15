import { afterEach, describe, expect, it, vi } from "vitest";
import { useSettings } from "@/stores/settings";

const { switchCheck } = vi.hoisted(() => ({ switchCheck: vi.fn() }));
vi.mock("@/ipc", async (orig) => {
  const real = await orig<typeof import("@/ipc")>();
  return { ...real, commands: { ...real.commands, branchSwitchCheck: switchCheck } };
});

import { requestSwitch } from "./actions";
import { useBranchSwitchUi } from "./store";

describe("requestSwitch", () => {
  afterEach(() => {
    useBranchSwitchUi.getState().closeConfirm();
    useSettings.getState().setConfirmBranchSwitch(true);
    switchCheck.mockReset();
  });

  it("asks before switching", () => {
    requestSwitch(1, { kind: "local", name: "feature" });
    expect(useBranchSwitchUi.getState().confirm).toEqual({
      repoId: 1,
      target: { kind: "local", name: "feature" },
    });
    expect(switchCheck).not.toHaveBeenCalled();
  });

  it("switches right away when the confirmation is turned off", () => {
    switchCheck.mockRejectedValue(new Error("stop"));
    useSettings.getState().setConfirmBranchSwitch(false);
    requestSwitch(1, { kind: "local", name: "feature" });
    expect(useBranchSwitchUi.getState().confirm).toBeNull();
    expect(switchCheck).toHaveBeenCalledWith(1, { kind: "local", name: "feature" });
  });
});
