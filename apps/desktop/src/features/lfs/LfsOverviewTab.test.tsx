import { QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { initI18n } from "@/i18n";
import type { LfsFile, LfsServerInfo, LfsStorage } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { OverviewTab } from "./LfsOverviewTab";
import { lfsKeys } from "./queries";

const REPO = 42;

beforeAll(() => {
  initI18n("en");
});

afterEach(() => queryClient.clear());

const file = (path: string, size: number, checkedOut = true): LfsFile => ({
  path,
  oid: "0".repeat(64),
  size,
  downloaded: checkedOut,
  checkedOut,
  lockable: false,
});

function setup(server: LfsServerInfo, files: LfsFile[]) {
  queryClient.setQueryData(lfsKeys.files(REPO), files);
  queryClient.setQueryData<LfsStorage>(lfsKeys.storage(REPO), {
    mediaDir: "D:/work/game/.git/lfs/objects",
    objects: 3,
    bytes: 3 * 1024 ** 3,
  });
  queryClient.setQueryData(lfsKeys.server(REPO), server);
  render(
    <QueryClientProvider client={queryClient}>
      <OverviewTab repoId={REPO} />
    </QueryClientProvider>,
  );
}

describe("OverviewTab", () => {
  it("explains storage, shows sizes and the GitHub quota when usage cannot be read", () => {
    setup(
      {
        remote: "origin",
        host: "github.com",
        fullName: "acme/game",
        kind: "github",
        usage: { status: "unavailable", reason: "noPermission", message: "403" },
      },
      [file("Assets/intro.mp4", 3 * 1e9), file("Art/hero.psd", 80 * 1024 ** 2, false)],
    );
    expect(screen.getByText("Where do LFS files live?")).toBeInTheDocument();
    expect(screen.getByText("D:/work/game/.git/lfs/objects")).toBeInTheDocument();
    expect(screen.getByText("Free / Pro")).toBeInTheDocument();
    expect(screen.getAllByText("10 GiB / month").length).toBeGreaterThan(0);
    expect(
      screen.getByText(/only the owner or an organization admin sees billing/),
    ).toBeInTheDocument();
    expect(screen.getByText(/1 file is bigger than 2 GB/)).toBeInTheDocument();
    expect(screen.getByText("Assets/intro.mp4")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Open usage page/ })).toBeInTheDocument();
    expect(screen.getByText(/Only deleting the repository/)).toBeInTheDocument();
  });

  it("shows GitLab project statistics against the Free limit", () => {
    setup(
      {
        remote: "origin",
        host: "gitlab.com",
        fullName: "acme/game",
        kind: "gitlab",
        usage: {
          status: "done",
          value: {
            source: "gitlabProject",
            lfsBytes: 4 * 1024 ** 3,
            repositoryBytes: 1024 ** 3,
            storageBytes: 6 * 1024 ** 3,
          },
        },
      },
      [file("a.bin", 1024)],
    );
    expect(screen.getByText(/LFS files: 4 GB\. Git repository: 1 GB/)).toBeInTheDocument();
    expect(screen.getByText(/5 GB of 10 GiB on the Free plan \(50%\)/)).toBeInTheDocument();
    expect(screen.queryByText(/is bigger than/)).toBeNull();
  });

  it("says when there is no hosting server", () => {
    setup(
      {
        remote: null,
        host: null,
        fullName: null,
        kind: null,
        usage: { status: "unavailable", reason: "noRemote", message: null },
      },
      [],
    );
    expect(screen.getByText(/has no remote on a hosting server/)).toBeInTheDocument();
  });
});
