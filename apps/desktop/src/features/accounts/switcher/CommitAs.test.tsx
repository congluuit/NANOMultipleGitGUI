import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it } from "vitest";
import { worktreeKeys } from "@/features/worktree/queries";
import { initI18n } from "@/i18n";
import type { GitIdentity } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { CommitAs } from "./CommitAs";
import { peekOneShot, setOneShot, useAsAccount } from "./oneShot";
import { makeContext, PERSONAL, PERSONAL_ALT, seedAccounts, unboundContext } from "./testData";

function renderBlock(git: GitIdentity) {
  queryClient.setQueryData(worktreeKeys.identity(1), git);
  return render(
    <QueryClientProvider client={queryClient}>
      <CommitAs repoId={1} />
    </QueryClientProvider>,
  );
}

const block = () => screen.getByTestId("commit-as");

describe("CommitAs", () => {
  beforeEach(() => {
    initI18n("vi");
    queryClient.clear();
    useAsAccount.setState({ byRepo: {} });
  });

  it("shows the bound account's identity", () => {
    seedAccounts(queryClient, { contexts: [makeContext()] });
    renderBlock({ name: "Hoang Nguyen", email: "hoang@acme.dev" });
    expect(block()).toHaveTextContent("Commit với tư cách");
    expect(block()).toHaveTextContent("Công ty · Hoang Nguyen <hoang@acme.dev>");
    expect(screen.queryByTestId("commit-as-differs")).toBeNull();
  });

  it("warns when the repo's git config uses another email", () => {
    seedAccounts(queryClient, { contexts: [makeContext()] });
    renderBlock({ name: "Hoang", email: "me@home.dev" });
    expect(screen.getByTestId("commit-as-differs")).toHaveTextContent(
      "git config của repo đang dùng me@home.dev",
    );
  });

  it("falls back to the git config identity when unbound", () => {
    seedAccounts(queryClient, { contexts: [unboundContext(1)] });
    renderBlock({ name: "Me", email: "me@home.dev" });
    expect(block()).toHaveTextContent("Me <me@home.dev>");
    expect(block()).toHaveTextContent("Chưa gắn tài khoản — dùng git config của repo");
  });

  it("keeps the missing-identity warning when unbound", () => {
    seedAccounts(queryClient, { contexts: [unboundContext(1)] });
    renderBlock({ name: null, email: null });
    expect(block()).toHaveTextContent("Chưa có user.name / user.email");
    expect(block()).toHaveTextContent("gắn tài khoản cho repo này");
  });

  it("shows a one-time override for the next commit and lets it be cancelled", async () => {
    const user = userEvent.setup();
    seedAccounts(queryClient, { contexts: [makeContext()] });
    setOneShot(1, { accountId: PERSONAL.id, identityId: PERSONAL_ALT.id, op: "commit" });
    renderBlock({ name: "Hoang Nguyen", email: "hoang@acme.dev" });
    expect(screen.getByTestId("commit-as-once")).toHaveTextContent(
      "Cá nhân · Hoang (blog) <blog@hoangnv.dev>chỉ lần commit này",
    );
    await user.click(screen.getByRole("button", { name: "Dùng lại tài khoản của repo" }));
    expect(peekOneShot(1, "commit")).toBeNull();
    expect(block()).toHaveTextContent("Công ty · Hoang Nguyen");
  });

  it("ignores an override meant for push", () => {
    seedAccounts(queryClient, { contexts: [makeContext()] });
    setOneShot(1, { accountId: PERSONAL.id, identityId: null, op: "push" });
    renderBlock({ name: "Hoang Nguyen", email: "hoang@acme.dev" });
    expect(screen.queryByTestId("commit-as-once")).toBeNull();
  });
});
