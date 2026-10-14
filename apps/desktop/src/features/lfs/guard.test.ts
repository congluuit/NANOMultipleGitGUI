import { describe, expect, it } from "vitest";
import type { GuardFinding, GuardReport } from "@/ipc";
import {
  choosePatterns,
  displaySize,
  guessProvider,
  providerFromUrl,
  relevantFindings,
  remoteHost,
  summarize,
} from "./guard";

const MB = 1024 * 1024;

function finding(over: Partial<GuardFinding> & { path: string }): GuardFinding {
  const ext = over.path.includes(".") ? `*.${over.path.split(".").pop()}` : null;
  return {
    size: 10 * MB,
    diskSize: 10 * MB,
    stagedSize: 10 * MB,
    level: "none",
    alreadyLfs: false,
    stagedPointer: false,
    needsRenormalize: false,
    looksBinary: false,
    looksLikeBuildOutput: false,
    patternByExtension: ext,
    patternByPath: `/${over.path}`,
    ...over,
  };
}

const report = (findings: GuardFinding[]): GuardReport => ({
  thresholds: { warnBytes: 50 * MB, blockBytes: 100 * MB },
  findings,
  suggestCount: 0,
  warnCount: 0,
  blockCount: 0,
});

describe("remoteHost", () => {
  it("reads URLs with a scheme", () => {
    expect(remoteHost("https://github.com/acme/api.git")).toBe("github.com");
    expect(remoteHost("https://user@GitLab.Example.com:8443/g/p.git")).toBe("gitlab.example.com");
    expect(remoteHost("ssh://git@ssh.github.com:443/acme/api.git")).toBe("ssh.github.com");
  });

  it("reads scp-like SSH remotes", () => {
    expect(remoteHost("git@github.com:acme/api.git")).toBe("github.com");
    expect(remoteHost("gitlab.com:group/project.git")).toBe("gitlab.com");
  });

  it("returns null for local paths", () => {
    expect(remoteHost("D:/Work/remote.git")).toBeNull();
    expect(remoteHost("../remote.git")).toBeNull();
    expect(remoteHost("/srv/git/remote.git")).toBeNull();
    expect(remoteHost("file:///srv/git/remote.git")).toBeNull();
  });
});

describe("providerFromUrl", () => {
  it("maps hosts to guard providers", () => {
    expect(providerFromUrl("https://github.com/acme/api.git")).toBe("github");
    expect(providerFromUrl("git@github.com:acme/api.git")).toBe("github");
    expect(providerFromUrl("https://gitlab.com/acme/api.git")).toBe("gitlab");
    expect(providerFromUrl("git@gitlab.acme.vn:team/api.git")).toBe("gitlab");
    expect(providerFromUrl("https://bitbucket.org/acme/api.git")).toBe("other");
    expect(providerFromUrl("https://github.acme.com/acme/api.git")).toBe("other");
    expect(providerFromUrl(null)).toBe("other");
    expect(providerFromUrl("D:/Work/remote.git")).toBe("other");
  });
});

describe("guessProvider", () => {
  it("prefers origin, else the first remote with a URL", () => {
    expect(
      guessProvider({
        remotes: [
          { name: "backup", url: "https://gitlab.com/a/b.git", branches: [] },
          { name: "origin", url: "https://github.com/a/b.git", branches: [] },
        ],
      }),
    ).toBe("github");
    expect(
      guessProvider({
        remotes: [
          { name: "nourl", url: null, branches: [] },
          { name: "upstream", url: "git@gitlab.com:a/b.git", branches: [] },
        ],
      }),
    ).toBe("gitlab");
    expect(guessProvider({ remotes: [] })).toBe("other");
    expect(guessProvider(undefined)).toBe("other");
  });
});

describe("relevantFindings / summarize", () => {
  const findings = [
    finding({ path: "src/a.ts" }),
    finding({ path: "icon.png", level: "suggest", looksBinary: true }),
    finding({ path: "video.mp4", level: "block", size: 300 * MB }),
    finding({ path: "design.psd", level: "warn", size: 80 * MB }),
    finding({ path: "old.bin", alreadyLfs: true, needsRenormalize: true }),
  ];

  it("keeps findings worth showing, most severe first", () => {
    expect(relevantFindings(report(findings)).map((f) => f.path)).toEqual([
      "video.mp4",
      "design.psd",
      "icon.png",
      "old.bin",
    ]);
  });

  it("counts levels and blocks on a hard-limit file", () => {
    const s = summarize(relevantFindings(report(findings)));
    expect(s).toEqual({ block: 1, warn: 1, suggest: 1, over: 2, blocked: true });
    expect(summarize([finding({ path: "a.zip", level: "suggest" })]).blocked).toBe(false);
  });
});

describe("choosePatterns", () => {
  const findings = [
    finding({ path: "Assets/intro.mp4" }),
    finding({ path: "Assets/outro.mp4" }),
    finding({ path: "tools/blob" }),
    finding({ path: "lfs/already.psd", alreadyLfs: true }),
  ];

  it("tracks by extension, falling back to the path without one", () => {
    expect(choosePatterns(findings, "extension")).toEqual(["*.mp4", "/tools/blob"]);
  });

  it("tracks by anchored path", () => {
    expect(choosePatterns(findings, "path")).toEqual([
      "/Assets/intro.mp4",
      "/Assets/outro.mp4",
      "/tools/blob",
    ]);
  });

  it("adds nothing for files already routed through LFS", () => {
    expect(choosePatterns([findings[3]!], "extension")).toEqual([]);
  });
});

describe("displaySize", () => {
  it("prefers the file size on disk", () => {
    expect(displaySize(finding({ path: "a", diskSize: 5, size: 0 }))).toBe(5);
    expect(displaySize(finding({ path: "a", diskSize: null, size: 7 }))).toBe(7);
  });
});
