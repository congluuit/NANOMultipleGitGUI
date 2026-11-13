import { beforeEach, describe, expect, it } from "vitest";
import { repoKeys } from "@/features/repos/queries";
import type { RepoRefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { learnLimit, learnedLimit, parseRejectedPaths, parseServerLimit } from "./limits";

const MB = 1000 * 1000;
const MIB = 1024 * 1024;

describe("parseServerLimit", () => {
  it("reads the limit of GitHub, GitLab and Bitbucket refusals", () => {
    expect(
      parseServerLimit(
        "remote: error: File big.bin is 120.00 MB; this exceeds GitHub's file size limit of 100.00 MB",
      ),
    ).toBe(100 * MB);
    expect(
      parseServerLimit(
        "remote: GitLab: You are attempting to check in one or more blobs which exceed the 10.0MiB limit:",
      ),
    ).toBe(10 * MIB);
    expect(
      parseServerLimit(
        "Your push has been blocked because it includes at least one file that is 100 MB or larger.",
      ),
    ).toBe(100 * MB);
    expect(parseServerLimit("fatal: pack exceeds maximum allowed size")).toBeNull();
  });

  it("lists the files GitHub names", () => {
    const text = [
      "remote: error: File assets/intro.mp4 is 120.00 MB; this exceeds GitHub's file size limit of 100.00 MB",
      "remote: error: File Builds/game data.zip is 300.50 MB; this exceeds GitHub's file size limit of 100.00 MB",
      "remote: error: File assets/intro.mp4 is 120.00 MB; this exceeds GitHub's file size limit of 100.00 MB",
    ].join("\n");
    expect(parseRejectedPaths(text)).toEqual(["assets/intro.mp4", "Builds/game data.zip"]);
    expect(parseRejectedPaths("nothing here")).toEqual([]);
  });
});

describe("learned limits", () => {
  beforeEach(() => {
    localStorage.clear();
    queryClient.setQueryData<Partial<RepoRefs>>(repoKeys.refs(7), {
      remotes: [{ name: "origin", url: "https://git.example.com/me/app.git", branches: [] }],
    } as Partial<RepoRefs>);
  });

  it("remembers the smallest limit per host", () => {
    expect(learnedLimit(7, "origin")).toBeNull();
    learnLimit(7, "origin", 50 * MB);
    learnLimit(7, "origin", 80 * MB);
    expect(learnedLimit(7, "origin")).toBe(50 * MB);
    learnLimit(7, "origin", 10 * MB);
    expect(learnedLimit(7, "origin")).toBe(10 * MB);
    expect(learnedLimit(7, "missing")).toBeNull();
  });
});
