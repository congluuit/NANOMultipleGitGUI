import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { peekOneShot, setOneShot, useAsAccount } from "@/features/accounts/switcher/oneShot";
import {
  makeContext,
  PERSONAL,
  PERSONAL_ALT,
  seedAccounts,
} from "@/features/accounts/switcher/testData";
import { initI18n } from "@/i18n";
import { commands, type CommitResult } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useUi } from "@/stores/ui";
import { EMPTY_DRAFT, useWorktreeStore } from "@/stores/worktree";
import { CommitBox } from "./CommitBox";
import { worktreeKeys } from "./queries";

vi.mock("./queries", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./queries")>()),
  invalidateRepo: vi.fn(),
}));

const RESULT: CommitResult = { hash: "abcdef1234567", summary: "Fix it" };

function renderBox() {
  return render(
    <QueryClientProvider client={queryClient}>
      <CommitBox repoId={1} stagedCount={1} conflictCount={0} hasHead />
    </QueryClientProvider>,
  );
}

const commitButton = () => screen.getByRole("button", { name: /^Commit/ });

describe("CommitBox one-shot override", () => {
  beforeAll(() => {
    initI18n("vi");
  });

  beforeEach(() => {
    vi.restoreAllMocks();
    queryClient.clear();
    useAsAccount.setState({ byRepo: {} });
    useWorktreeStore.setState({ drafts: { 1: { ...EMPTY_DRAFT, summary: "Fix it" } } });
    useUi.setState({ notice: null });
    seedAccounts(queryClient, { contexts: [makeContext()] });
    queryClient.setQueryData(worktreeKeys.identity(1), {
      name: "Hoang Nguyen",
      email: "hoang@acme.dev",
    });
  });

  it("commits as the repo account when there is no override", async () => {
    const create = vi.spyOn(commands, "commitCreate").mockResolvedValue(RESULT);
    renderBox();
    await userEvent.click(commitButton());
    expect(create).toHaveBeenCalledWith(
      1,
      "Fix it",
      { amend: false, signoff: false, allowEmpty: false },
      null,
    );
  });

  it("consumes the 'Commit as…' override and passes it as asAccount", async () => {
    const create = vi.spyOn(commands, "commitCreate").mockResolvedValue(RESULT);
    setOneShot(1, { accountId: PERSONAL.id, identityId: PERSONAL_ALT.id, op: "commit" });
    renderBox();
    await userEvent.click(commitButton());
    expect(create).toHaveBeenCalledWith(1, "Fix it", expect.anything(), {
      accountId: PERSONAL.id,
      identityId: PERSONAL_ALT.id,
    });
    expect(peekOneShot(1, "commit")).toBeNull();
    expect(useUi.getState().notice?.kind).toBe("info");
  });

  it("gives the override back when the commit fails", async () => {
    vi.spyOn(commands, "commitCreate").mockRejectedValue({
      kind: "git",
      message: "pre-commit hook failed",
    });
    const shot = { accountId: PERSONAL.id, identityId: null, op: "any" } as const;
    setOneShot(1, shot);
    renderBox();
    await userEvent.click(commitButton());
    expect(useUi.getState().notice).toMatchObject({
      kind: "error",
      message: "pre-commit hook failed",
    });
    expect(peekOneShot(1, "commit")).toEqual(shot);
  });

  it("leaves a push-only override for the push", async () => {
    const create = vi.spyOn(commands, "commitCreate").mockResolvedValue(RESULT);
    setOneShot(1, { accountId: PERSONAL.id, identityId: null, op: "push" });
    renderBox();
    await userEvent.click(commitButton());
    expect(create).toHaveBeenCalledWith(1, "Fix it", expect.anything(), null);
    expect(peekOneShot(1, "push")).not.toBeNull();
  });
});
