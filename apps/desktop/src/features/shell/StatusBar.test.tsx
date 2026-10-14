import { render, screen } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import {
  makeContext,
  makeRemote,
  PERSONAL,
  PERSONAL_MAIN,
  seedAccounts,
  unboundContext,
} from "@/features/accounts/switcher/testData";
import { repoKeys } from "@/features/repos/queries";
import { initI18n } from "@/i18n";
import type { RepoRefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { useTransferQueue } from "@/features/transfers/store";
import { useWorkspace } from "@/stores/workspace";
import { StatusBar } from "./StatusBar";

const REFS = {
  head: { branch: "main", oid: "a".repeat(40), detached: false, unborn: false },
  local: [
    {
      name: "main",
      oid: "a".repeat(40),
      isHead: true,
      upstream: "origin/main",
      upstreamGone: false,
      ahead: 2,
      behind: 0,
    },
  ],
  remotes: [{ name: "origin", url: "https://gitlab.acme.dev/platform/api.git", branches: [] }],
  tags: [],
  stashes: [],
} as unknown as RepoRefs;

function renderBar() {
  return render(
    <QueryClientProvider client={queryClient}>
      <StatusBar />
    </QueryClientProvider>,
  );
}

describe("StatusBar account", () => {
  beforeAll(() => {
    initI18n("vi");
  });

  beforeEach(() => {
    queryClient.clear();
    useTransferQueue.setState({ items: {}, order: [] });
    useWorkspace.setState({
      tabs: [{ id: 1, path: "D:/Work/api", name: "api", missing: false }],
      activeId: 1,
      loaded: true,
    });
    queryClient.setQueryData(repoKeys.refs(1), REFS);
  });

  it("shows label · provider · transport of the resolved account", () => {
    seedAccounts(queryClient, { contexts: [makeContext()] });
    renderBar();
    expect(screen.getByTestId("status-account")).toHaveTextContent("Công ty · GitLab acme · HTTPS");
    expect(screen.queryByText("Repo này chưa gắn tài khoản", { exact: false })).toBeNull();
  });

  it("follows an SSH remote bound to another account", () => {
    seedAccounts(queryClient, {
      contexts: [
        makeContext({
          account: PERSONAL,
          identity: PERSONAL_MAIN,
          perRemote: [
            makeRemote({
              url: "git@github.com:hoangnv/api.git",
              host: "github.com",
              transport: "ssh",
              account: PERSONAL,
              identity: PERSONAL_MAIN,
            }),
          ],
        }),
      ],
    });
    renderBar();
    expect(screen.getByTestId("status-account")).toHaveTextContent("Cá nhân · GitHub · SSH");
  });

  it("keeps the unbound warning when no account resolves", () => {
    seedAccounts(queryClient, { contexts: [unboundContext(1)] });
    renderBar();
    expect(screen.queryByTestId("status-account")).toBeNull();
    expect(screen.getByText(/chưa gắn tài khoản/)).toBeInTheDocument();
  });
});
