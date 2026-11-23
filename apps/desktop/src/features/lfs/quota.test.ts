import { describe, expect, it } from "vitest";
import type { LfsFile } from "@/ipc";
import {
  filesOver,
  largestFiles,
  lfsHostOf,
  percentOf,
  providerQuota,
  smallestFileMax,
  usagePageUrl,
} from "./quota";

const file = (path: string, size: number): LfsFile => ({
  path,
  oid: "0".repeat(64),
  size,
  downloaded: true,
  checkedOut: true,
  lockable: false,
});

describe("lfsHostOf", () => {
  it("knows the big hosts by name", () => {
    expect(lfsHostOf("github.com", null)).toBe("github");
    expect(lfsHostOf("GitLab.com", "gitlab")).toBe("gitlab");
    expect(lfsHostOf("bitbucket.org", null)).toBe("bitbucket");
    expect(lfsHostOf("codeberg.org", "gitea")).toBe("codeberg");
    expect(lfsHostOf("ssh.dev.azure.com", null)).toBe("azure");
    expect(lfsHostOf("acme.visualstudio.com", null)).toBe("azure");
  });

  it("uses the account kind for self-hosted servers", () => {
    expect(lfsHostOf("git.acme.io", "gitlab")).toBe("gitlabSelfHosted");
    expect(lfsHostOf("git.acme.io", "gitlabSelfHosted")).toBe("gitlabSelfHosted");
    expect(lfsHostOf("git.acme.io", "githubEnterprise")).toBe("githubEnterprise");
    expect(lfsHostOf("git.acme.io", "gitea")).toBe("gitea");
  });

  it("guesses from the host name without an account", () => {
    expect(lfsHostOf("gitlab.acme.io", null)).toBe("gitlabSelfHosted");
    expect(lfsHostOf("forgejo.acme.io", undefined)).toBe("gitea");
    expect(lfsHostOf("git.acme.io", null)).toBe("unknown");
    expect(lfsHostOf(null, null)).toBe("unknown");
  });
});

describe("quota table", () => {
  it("has the documented GitHub numbers", () => {
    const gh = providerQuota("github");
    expect(gh.plans[0]).toMatchObject({
      storage: { label: "10 GiB", bytes: 10 * 1024 ** 3 },
      bandwidth: { label: "10 GiB" },
      fileMax: { label: "2 GB", bytes: 2e9 },
    });
    expect(smallestFileMax(gh)?.label).toBe("2 GB");
  });

  it("has no per-file limit where none is documented", () => {
    expect(smallestFileMax(providerQuota("gitlab"))).toBeNull();
    expect(smallestFileMax(providerQuota("bitbucket"))).toBeNull();
    expect(providerQuota("bitbucket").plans.map((p) => p.storage?.label)).toEqual([
      "1 GB",
      "5 GB",
      "10 GB",
    ]);
  });

  it("links every provider to an https documentation page", () => {
    for (const host of [
      "github",
      "githubEnterprise",
      "gitlab",
      "gitlabSelfHosted",
      "bitbucket",
      "azure",
      "codeberg",
      "gitea",
      "unknown",
    ] as const) {
      expect(providerQuota(host).docsUrl).toMatch(/^https:\/\//);
    }
  });
});

describe("usagePageUrl", () => {
  it("points GitHub organizations and users at their billing page", () => {
    expect(usagePageUrl("github", "github.com", "acme/game", true)).toBe(
      "https://github.com/organizations/acme/settings/billing/usage",
    );
    expect(usagePageUrl("github", "github.com", "me/game", false)).toBe(
      "https://github.com/settings/billing/usage",
    );
  });

  it("points GitLab at the project's usage quotas", () => {
    expect(usagePageUrl("gitlabSelfHosted", "git.acme.io", "g/sub/p", null)).toBe(
      "https://git.acme.io/g/sub/p/-/usage_quotas",
    );
    expect(usagePageUrl("gitlab", null, "g/p", null)).toBeNull();
    expect(usagePageUrl("bitbucket", "bitbucket.org", "w/r", null)).toBeNull();
  });
});

describe("size helpers", () => {
  const files = [file("a.psd", 300), file("b.mp4", 5_000_000_000), file("c.fbx", 300)];

  it("lists the largest files first, ties by path", () => {
    expect(largestFiles(files, 2).map((f) => f.path)).toEqual(["b.mp4", "a.psd"]);
    expect(largestFiles([])).toEqual([]);
  });

  it("finds files over a limit", () => {
    expect(filesOver(files, { bytes: 2e9, label: "2 GB" }).map((f) => f.path)).toEqual(["b.mp4"]);
    expect(filesOver(files, null)).toEqual([]);
  });

  it("computes a clamped percentage", () => {
    expect(percentOf(5, 10)).toBe(50);
    expect(percentOf(1, 3)).toBe(33.3);
    expect(percentOf(20, 10)).toBe(100);
    expect(percentOf(5, 0)).toBe(0);
  });
});
