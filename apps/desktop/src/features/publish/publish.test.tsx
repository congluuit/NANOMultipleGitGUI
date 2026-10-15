import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { RemoteButtons } from "@/features/remote/RemoteButtons";
import { repoKeys } from "@/features/repos/queries";
import { accountKeys } from "@/features/accounts/queries";
import { initI18n } from "@/i18n";
import type { Account, Provider, RepoRefs } from "@/ipc";
import { useWorkspace } from "@/stores/workspace";
import { queryClient } from "@/lib/queryClient";
import { publishNewRepo, publishToUrl } from "./actions";
import { FirstStepsBanner } from "./FirstStepsBanner";
import { PublishDialog } from "./PublishDialog";
import {
  firstSteps,
  plausibleRemoteUrl,
  remoteHost,
  repoUrl,
  showFirstSteps,
  suggestRepoName,
  validRepoName,
} from "./model";
import { usePublishDialog } from "./store";

const ipc = vi.hoisted(() => ({
  publishCreateRepo: vi.fn(),
  publishAddRemote: vi.fn(),
  remotePush: vi.fn(),
}));
vi.mock("@/ipc", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/ipc")>();
  return { ...actual, commands: { ...actual.commands, ...ipc } };
});

const OID = "a".repeat(40);

function refs(opts: {
  unborn?: boolean;
  remote?: boolean;
  upstream?: boolean;
  remoteBranches?: boolean;
}): RepoRefs {
  return {
    head: { branch: "main", oid: opts.unborn ? null : OID, detached: false, unborn: !!opts.unborn },
    local: opts.unborn
      ? []
      : [
          {
            name: "main",
            oid: OID,
            isHead: true,
            upstream: opts.upstream ? "origin/main" : null,
            upstreamGone: false,
            ahead: 0,
            behind: 0,
          },
        ],
    remotes: opts.remote
      ? [
          {
            name: "origin",
            url: "https://github.com/me/app.git",
            branches: opts.remoteBranches ? [{ name: "main", oid: OID }] : [],
          },
        ]
      : [],
    tags: [],
    stashes: [],
  } as unknown as RepoRefs;
}

const created = {
  fullName: "me/app",
  cloneHttps: "https://github.com/me/app.git",
  cloneSsh: "git@github.com:me/app.git",
  webUrl: "https://github.com/me/app",
  defaultBranch: null,
  private: true,
  existed: false,
};

beforeEach(() => {
  initI18n("en");
  queryClient.clear();
  localStorage.clear();
  usePublishDialog.setState({ preset: null });
  for (const f of Object.values(ipc)) f.mockReset();
});

describe("publish model", () => {
  it("suggests a valid repository name from the folder", () => {
    expect(suggestRepoName("D:\\Code\\Dự án mới")).toBe("Du-an-moi");
    expect(suggestRepoName("/home/me/my app/")).toBe("my-app");
    expect(suggestRepoName("/home/me/lib.git")).toBe("lib");
    expect(suggestRepoName("/home/me/---")).toBe("my-project");
    expect(validRepoName("my-app_2.0")).toBe(true);
    for (const bad of ["", ".", "..", "-x", "a b", "x.git", "tiếng"]) {
      expect(validRepoName(bad)).toBe(false);
    }
  });

  it("builds and reads remote URLs", () => {
    expect(repoUrl("https://github.com/", "me", "app")).toBe("https://github.com/me/app.git");
    expect(remoteHost("git@gitlab.acme.vn:grp/app.git")).toBe("gitlab.acme.vn");
    expect(remoteHost("https://GitHub.com/me/app")).toBe("github.com");
    expect(remoteHost("nonsense")).toBeNull();
    expect(plausibleRemoteUrl("https://github.com/me/app.git")).toBe(true);
    expect(plausibleRemoteUrl("git@github.com:me/app.git")).toBe(true);
    expect(plausibleRemoteUrl("--upload-pack=x")).toBe(false);
    expect(plausibleRemoteUrl("github.com/me/app")).toBe(false);
  });

  it("walks the first steps of a new project", () => {
    expect(firstSteps(refs({ unborn: true })).next).toBe("commit");
    expect(firstSteps(refs({})).next).toBe("publish");
    expect(firstSteps(refs({ remote: true })).next).toBe("push");
    expect(firstSteps(refs({ remote: true, upstream: true })).next).toBeNull();
    expect(showFirstSteps(refs({ unborn: true }))).toBe(true);
    expect(showFirstSteps(refs({ remote: true }))).toBe(true);
    // An ordinary repository with a remote that has branches: no checklist.
    expect(showFirstSteps(refs({ remote: true, remoteBranches: true }))).toBe(false);
  });
});

describe("publish flow", () => {
  it("creates the repository, adds it as origin bound to the account, then pushes", async () => {
    ipc.publishCreateRepo.mockResolvedValue({ status: "done", value: created });
    ipc.publishAddRemote.mockResolvedValue({
      remote: "origin",
      url: created.cloneHttps,
      added: true,
      renamedTo: null,
    });
    ipc.remotePush.mockResolvedValue({ status: "done", value: { upToDate: false } });
    const phases: string[] = [];
    const step = await publishNewRepo(
      { repoId: 1, branch: "main", remoteName: "origin", accountId: "acc" },
      { name: "app", namespace: null, description: null, private: true },
      "https",
      (p) => phases.push(p),
    );
    expect(step).toMatchObject({ status: "done", pushed: true });
    expect(phases).toEqual(["creating", "linking", "pushing"]);
    expect(ipc.publishAddRemote).toHaveBeenCalledWith(1, "origin", created.cloneHttps, "acc");
    expect(ipc.remotePush).toHaveBeenCalledWith(
      1,
      expect.any(String),
      {
        remote: "origin",
        branch: "main",
        remoteBranch: null,
        setUpstream: true,
        forceWithLease: false,
      },
      null,
    );
  });

  it("uses the SSH URL when asked and skips the push without a commit", async () => {
    ipc.publishCreateRepo.mockResolvedValue({ status: "done", value: created });
    ipc.publishAddRemote.mockResolvedValue({
      remote: "origin",
      url: created.cloneSsh,
      added: true,
      renamedTo: null,
    });
    const step = await publishNewRepo(
      { repoId: 1, branch: null, remoteName: "origin", accountId: "acc" },
      { name: "app" },
      "ssh",
      () => {},
    );
    expect(step).toMatchObject({ status: "done", pushed: false });
    expect(ipc.publishAddRemote).toHaveBeenCalledWith(1, "origin", created.cloneSsh, "acc");
    expect(ipc.remotePush).not.toHaveBeenCalled();
  });

  it("stops when the server refuses to create the repository", async () => {
    ipc.publishCreateRepo.mockResolvedValue({
      status: "failed",
      error: { kind: "alreadyExists", message: "name already exists on this account" },
      problem: "nameTaken",
    });
    const step = await publishNewRepo(
      { repoId: 1, branch: "main", remoteName: "origin", accountId: "acc" },
      { name: "app" },
      "https",
      () => {},
    );
    expect(step.status).toBe("createFailed");
    expect(ipc.publishAddRemote).not.toHaveBeenCalled();
  });

  it("links a pasted URL and reports a failed push separately", async () => {
    ipc.publishAddRemote.mockResolvedValue({
      remote: "origin",
      url: "https://gitlab.com/me/app.git",
      added: true,
      renamedTo: null,
    });
    ipc.remotePush.mockResolvedValue({ status: "rejected", reason: "auth", message: "denied" });
    const step = await publishToUrl(
      { repoId: 1, branch: "main", remoteName: "origin", accountId: null },
      "  https://gitlab.com/me/app.git ",
      () => {},
    );
    expect(step.status).toBe("pushFailed");
    expect(ipc.publishAddRemote).toHaveBeenCalledWith(
      1,
      "origin",
      "https://gitlab.com/me/app.git",
      null,
    );
  });
});

function renderWith(r: RepoRefs, ui: React.ReactNode) {
  queryClient.setQueryData(repoKeys.refs(1), r);
  return render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>{ui}</TooltipProvider>
    </QueryClientProvider>,
  );
}

describe("entry points", () => {
  it("turns Push into Publish when the repository has no remote", async () => {
    renderWith(refs({}), <RemoteButtons repoId={1} />);
    await userEvent.click(screen.getByRole("button", { name: "Publish" }));
    expect(usePublishDialog.getState().preset).toEqual({ repoId: 1, mode: "new" });
  });

  it("labels the first push of a branch as Publish branch", () => {
    renderWith(refs({ remote: true }), <RemoteButtons repoId={1} />);
    expect(screen.getByRole("button", { name: "Publish branch" })).toBeEnabled();
  });

  it("guides a new project to the next step and can be hidden", async () => {
    const view = renderWith(refs({}), <FirstStepsBanner repoId={1} />);
    expect(screen.getByText("New project")).toBeInTheDocument();
    await userEvent.click(screen.getByTestId("first-steps-action"));
    expect(usePublishDialog.getState().preset?.repoId).toBe(1);
    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText("New project")).toBeNull();
    view.unmount();
    renderWith(refs({}), <FirstStepsBanner repoId={1} />);
    await waitFor(() => expect(screen.queryByText("New project")).toBeNull());
  });
});

describe("PublishDialog", () => {
  it("creates the repository with the suggested name and shows the result", async () => {
    useWorkspace.setState({
      tabs: [{ id: 1, path: "D:\\Code\\My App", name: "My App", missing: false }],
    });
    queryClient.setQueryData(accountKeys.providers, [
      { id: "gh", name: "GitHub", baseUrl: "https://github.com" },
    ] as Provider[]);
    queryClient.setQueryData(accountKeys.list, [
      {
        id: "acc",
        username: "me",
        providerId: "gh",
        color: "#123456",
        label: null,
        displayName: null,
        secrets: ["token"],
      },
    ] as unknown as Account[]);
    ipc.publishCreateRepo.mockResolvedValue({ status: "done", value: created });
    ipc.publishAddRemote.mockResolvedValue({
      remote: "origin",
      url: created.cloneHttps,
      added: true,
      renamedTo: null,
    });
    ipc.remotePush.mockResolvedValue({ status: "done", value: { upToDate: false } });
    renderWith(refs({}), <PublishDialog repoId={1} initialMode="new" onClose={() => {}} />);
    expect(screen.getByLabelText("Repository name")).toHaveValue("My-App");
    await userEvent.click(screen.getByRole("button", { name: "Create and push" }));
    await waitFor(() => expect(screen.getByText(/Done! Published me\/app/)).toBeInTheDocument());
    expect(ipc.publishCreateRepo).toHaveBeenCalledWith("acc", {
      name: "My-App",
      namespace: null,
      description: null,
      private: true,
    });
  });
});
