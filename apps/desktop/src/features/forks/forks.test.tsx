import { fireEvent, render, screen } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import i18n from "i18next";
import { useState } from "react";
import { beforeAll, describe, expect, it } from "vitest";
import { initI18n } from "@/i18n";
import type { Account, CloneGuess, CloneRepo, Provider } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { CloneForkOption, type ForkChoice } from "./CloneForkOption";
import {
  accountsOnHost,
  canOfferFork,
  describeForkFailure,
  forkSourceOf,
  freeRemoteName,
  hostOf,
  ownedBy,
  urlLike,
  validRemoteName,
  type ForkSource,
} from "./model";

const account = (id: string, username: string, providerId = "gh"): Account =>
  ({ id, username, providerId, color: "#123456", label: null, displayName: null }) as Account;

const providers = [
  { id: "gh", baseUrl: "https://github.com" },
  { id: "gl", baseUrl: "https://gitlab.acme.vn:8443" },
] as Provider[];

const source: ForkSource = {
  fullName: "some-org/lib",
  owner: "some-org",
  host: "github.com",
  cloneHttps: "https://github.com/some-org/lib.git",
  cloneSsh: "git@github.com:some-org/lib.git",
};

beforeAll(() => {
  initI18n("vi");
});

describe("forks model", () => {
  it("derives the source from a listed repo or a pasted URL", () => {
    const repo = {
      fullName: "acme/api",
      owner: "acme",
      cloneHttps: "https://github.com/acme/api.git",
      cloneSsh: "git@github.com:acme/api.git",
    } as CloneRepo;
    expect(forkSourceOf(repo, undefined)?.host).toBe("github.com");
    const guess = {
      valid: true,
      local: false,
      host: "gitlab.acme.vn",
      owner: "grp/sub",
      repo: "api",
      httpsUrl: "https://gitlab.acme.vn/grp/sub/api.git",
      sshUrl: null,
    } as CloneGuess;
    expect(forkSourceOf(null, guess)).toMatchObject({ fullName: "grp/sub/api", cloneSsh: "" });
    expect(forkSourceOf(null, { ...guess, local: true })).toBeNull();
    expect(forkSourceOf(null, undefined)).toBeNull();
  });

  it("offers forking only for repositories the account does not own", () => {
    expect(ownedBy({ owner: "HoangNV" }, account("a", "hoangnv"))).toBe(true);
    expect(canOfferFork(source, account("a", "hoangnv"))).toBe(true);
    expect(canOfferFork({ ...source, owner: "hoangnv" }, account("a", "hoangnv"))).toBe(false);
    expect(canOfferFork(null, account("a", "x"))).toBe(false);
    expect(canOfferFork(source, undefined)).toBe(false);
  });

  it("filters accounts by provider host", () => {
    const list = [account("a", "x", "gh"), account("b", "y", "gl")];
    expect(accountsOnHost(list, providers, "github.com").map((a) => a.id)).toEqual(["a"]);
    expect(accountsOnHost(list, providers, "gitlab.acme.vn").map((a) => a.id)).toEqual(["b"]);
    expect(accountsOnHost(list, providers, null)).toEqual([]);
    expect(hostOf("not a url")).toBeNull();
  });

  it("names and URLs", () => {
    expect(freeRemoteName(["origin"])).toBe("fork");
    expect(freeRemoteName(["fork", "fork-2"])).toBe("fork-3");
    expect(validRemoteName("fork")).toBe(true);
    for (const bad of ["", "-x", ".x", "a b", "a/b", "x.lock"])
      expect(validRemoteName(bad)).toBe(false);
    expect(urlLike("git@github.com:me/r.git", "https://h", "git@s")).toBe("git@s");
    expect(urlLike("https://github.com/me/r", "https://h", "git@s")).toBe("https://h");
    expect(urlLike("ssh://git@h/r", "https://h", "")).toBe("https://h");
  });

  it("explains fork failures", () => {
    const t = i18n.t.bind(i18n);
    const disabled = describeForkFailure(
      t,
      {
        status: "failed",
        error: { kind: "forbidden", message: "forking is disabled", hint: null },
        problem: "forkingDisabled",
      },
      "a",
      "Cá nhân",
    );
    expect(disabled.text).toContain("không cho phép fork");
    const noToken = describeForkFailure(t, { status: "noToken", accountId: "a" }, "a", "Cá nhân");
    expect(noToken.text).toContain("Cá nhân");
    expect(noToken.action).not.toBeNull();
    const scope = describeForkFailure(
      t,
      {
        status: "failed",
        error: {
          kind: "notFound",
          message: "Not Found",
          hint: { kind: "missingScopes", required: ["public_repo", "repo"], granted: [] },
        },
        problem: null,
      },
      "a",
      "Cá nhân",
    );
    expect(scope.text).toContain("public_repo, repo");
  });
});

describe("CloneForkOption", () => {
  function Harness({ owner }: { owner: string }) {
    const [choice, setChoice] = useState<ForkChoice | null>(null);
    return (
      <QueryClientProvider client={queryClient}>
        <div>
          <CloneForkOption
            source={{ ...source, owner }}
            account={account("a", "hoangnv")}
            value={choice}
            onChange={setChoice}
          />
        </div>
      </QueryClientProvider>
    );
  }

  it("shows the account and reveals the namespace when checked", () => {
    render(<Harness owner="some-org" />);
    const box = screen.getByRole("checkbox");
    expect(box.closest("label")?.textContent).toContain("Fork về tài khoản");
    expect(box.closest("label")?.textContent).toContain("hoangnv");
    expect(screen.queryByText(/Tạo fork của some-org\/lib/)).toBeNull();
    fireEvent.click(box);
    expect(screen.getByText(/Tạo fork của some-org\/lib/)).toBeTruthy();
  });

  it("is hidden for the account's own repository", () => {
    render(<Harness owner="hoangnv" />);
    expect(screen.queryByRole("checkbox")).toBeNull();
  });
});
