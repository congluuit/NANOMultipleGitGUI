import { describe, expect, it } from "vitest";
import type { CloneRepo } from "@/ipc";
import { makeRepo } from "./testData";
import {
  filterRepos,
  flattenRepos,
  lfsMode,
  neededBytes,
  relativeTime,
  renderTemplate,
  sanitizeSegments,
  spaceWarning,
  urlForProtocol,
  validateForm,
} from "./model";

const REPOS = [
  makeRepo({ fullName: "acme/backend-api", updatedAt: 5_000, stars: 12 }),
  makeRepo({ fullName: "acme/game-assets", updatedAt: 9_000, lfsSizeBytes: 4.2e9 }),
  makeRepo({ fullName: "hoangnv/dotfiles", accountId: "personal", updatedAt: 7_000 }),
  makeRepo({
    fullName: "hoangnv/lib",
    accountId: "oss",
    fork: true,
    parentFullName: "some-org/lib",
    description: "API helpers",
    updatedAt: 1_000,
  }),
];

const names = (repos: CloneRepo[]) => repos.map((r) => r.fullName);

describe("filterRepos", () => {
  it("lists the most recently updated first without a query", () => {
    expect(names(filterRepos(REPOS, { query: "", accountId: null }))).toEqual([
      "acme/game-assets",
      "hoangnv/dotfiles",
      "acme/backend-api",
      "hoangnv/lib",
    ]);
  });

  it("filters by account", () => {
    expect(names(filterRepos(REPOS, { query: "", accountId: "personal" }))).toEqual([
      "hoangnv/dotfiles",
    ]);
  });

  it("needs every word and ranks name matches above owner/description matches", () => {
    expect(names(filterRepos(REPOS, { query: "API", accountId: null }))).toEqual([
      "acme/backend-api",
      "hoangnv/lib",
    ]);
    expect(names(filterRepos(REPOS, { query: "acme  assets", accountId: null }))).toEqual([
      "acme/game-assets",
    ]);
    expect(names(filterRepos(REPOS, { query: "lib", accountId: null }))).toEqual(["hoangnv/lib"]);
    expect(filterRepos(REPOS, { query: "nothing-like-this", accountId: null })).toEqual([]);
  });

  it("flattens per-account lists in account order", () => {
    const list = {
      accounts: [
        {
          accountId: "a",
          repos: [REPOS[0]!],
          fetchedAt: 1,
          error: null,
          truncated: false,
          refreshing: false,
        },
        {
          accountId: "b",
          repos: [],
          fetchedAt: null,
          error: { code: "noToken", message: "x" },
          truncated: false,
          refreshing: false,
        },
        {
          accountId: "c",
          repos: [REPOS[2]!],
          fetchedAt: 1,
          error: null,
          truncated: false,
          refreshing: false,
        },
      ],
    };
    expect(names(flattenRepos(list))).toEqual(["acme/backend-api", "hoangnv/dotfiles"]);
    expect(flattenRepos(undefined)).toEqual([]);
  });
});

describe("renderTemplate", () => {
  const vars = {
    cloneDir: "D:\\Work\\",
    owner: "acme/platform",
    repo: "api",
    host: "gitlab.acme.dev:8443",
    account: "hoangnv",
  };

  it("previews the default template with the clone folder's separator", () => {
    expect(renderTemplate("{clone_dir}/{owner}/{repo}", vars)).toBe(
      "D:\\Work\\acme\\platform\\api",
    );
    expect(
      renderTemplate("{clone_dir}/{owner}/{repo}", { ...vars, cloneDir: "/home/me/src" }),
    ).toBe("/home/me/src/acme/platform/api");
  });

  it("collapses a missing owner and puts relative templates under the clone folder", () => {
    expect(renderTemplate("{clone_dir}/{owner}/{repo}", { ...vars, owner: "" })).toBe(
      "D:\\Work\\api",
    );
    expect(renderTemplate("{host}/{account}/{repo}", vars)).toBe(
      "D:\\Work\\gitlab.acme.dev_8443\\hoangnv\\api",
    );
    expect(renderTemplate("E:/Clones/{repo}", vars)).toBe("E:\\Clones\\api");
  });

  it("sanitises values like the backend", () => {
    expect(sanitizeSegments("../..")).toBe("_/_");
    expect(sanitizeSegments("a:b*?.")).toBe("a_b__");
    expect(
      renderTemplate("{clone_dir}/{owner}/{repo}", { ...vars, owner: "../..", repo: "x" }),
    ).toBe("D:\\Work\\_\\_\\x");
  });
});

describe("relativeTime", () => {
  const now = 1_700_000_000_000;
  it("formats in the UI language", () => {
    expect(relativeTime(now / 1000 - 2 * 3600, now, "en")).toBe("2 hours ago");
    expect(relativeTime(now / 1000 - 2 * 3600, now, "vi")).toBe("2 giờ trước");
    expect(relativeTime(now / 1000 - 3 * 86400, now, "en")).toBe("3 days ago");
    expect(relativeTime(now / 1000 - 5, now, "en")).toBe("this minute");
  });
});

describe("disk space", () => {
  it("counts LFS objects twice only when they are downloaded now", () => {
    const d = { sizeBytes: 100, lfsSizeBytes: 1000 };
    expect(neededBytes(d, "all")).toBe(2100);
    expect(neededBytes(d, "later")).toBe(100);
    expect(neededBytes({ sizeBytes: null, lfsSizeBytes: null }, "all")).toBeNull();
  });

  it("warns when the clone does not fit or the drive is nearly full", () => {
    expect(spaceWarning(500, 2100)).toBe("insufficient");
    expect(spaceWarning(500, null)).toBe("low");
    expect(spaceWarning(10 * 1024 ** 3, 2100)).toBeNull();
    expect(spaceWarning(null, 2100)).toBeNull();
  });
});

describe("form", () => {
  const ok = { urlValid: true, destUsable: true };
  const form = { url: "https://x/y.git", dest: "D:/Work/y", lfs: "all" as const, lfsInclude: "" };

  it("accepts a complete form", () => {
    expect(validateForm(form, ok)).toEqual([]);
  });

  it("reports missing or invalid fields", () => {
    expect(validateForm({ ...form, url: " ", dest: "" }, ok)).toEqual([
      "urlRequired",
      "destRequired",
    ]);
    expect(validateForm(form, { urlValid: false, destUsable: false })).toEqual([
      "urlInvalid",
      "destNotEmpty",
    ]);
    for (const lfsInclude of ["", "--all", "a/**,,b"]) {
      expect(validateForm({ ...form, lfs: "partial", lfsInclude }, ok)).toEqual(["patternInvalid"]);
    }
    expect(validateForm({ ...form, lfs: "partial", lfsInclude: "Assets/UI/**" }, ok)).toEqual([]);
  });

  it("builds the LFS mode and picks the URL of the chosen protocol", () => {
    expect(lfsMode({ lfs: "partial", lfsInclude: " Assets/** " })).toEqual({
      mode: "partial",
      include: "Assets/**",
    });
    expect(lfsMode({ lfs: "later", lfsInclude: "x" })).toEqual({ mode: "later" });
    const guess = { httpsUrl: "https://h/a/b.git", sshUrl: "git@h:a/b.git", local: false };
    expect(urlForProtocol("h/a/b", guess, "ssh")).toBe("git@h:a/b.git");
    expect(urlForProtocol("h/a/b", guess, "https")).toBe("https://h/a/b.git");
    expect(urlForProtocol(" C:/r.git ", { ...guess, local: true }, "ssh")).toBe("C:/r.git");
    expect(urlForProtocol(" x ", undefined, "ssh")).toBe("x");
  });
});
