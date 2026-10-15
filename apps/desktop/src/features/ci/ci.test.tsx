import { render, screen } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import i18n from "i18next";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { initI18n } from "@/i18n";
import type { CiJob, CiStatus, GraphRow, ProviderError, RepoRefs } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { CiBadge } from "./CiBadge";
import { CiChecksList } from "./CiChecksList";
import { drawCiBadge, type CiColors } from "./draw";
import { describeCiProblem, describeProviderError } from "./errors";
import {
  ciCandidateHashes,
  ciGlyph,
  ciTextClass,
  countJobs,
  entriesBySha,
  hasPending,
  jobDuration,
  safeUrl,
  sortJobs,
} from "./model";

const job = (name: string, state: CiJob["state"], extra: Partial<CiJob> = {}): CiJob => ({
  name,
  kind: "check",
  state,
  group: null,
  description: null,
  startedAt: null,
  finishedAt: null,
  url: null,
  allowFailure: false,
  ...extra,
});

const status = (state: CiStatus["state"], jobs: CiJob[] = []): CiStatus => ({
  sha: "a".repeat(40),
  state,
  jobs,
  pipelineUrl: null,
  truncated: false,
  checksError: null,
});

beforeAll(() => {
  initI18n("vi");
});

describe("ci model", () => {
  it("glyphs and colours", () => {
    expect(ciGlyph("success")).toBe("✓");
    expect(ciGlyph("failure")).toBe("✗");
    expect(ciGlyph("pending")).toBe("●");
    expect(ciGlyph("neutral")).toBe("–");
    expect(ciGlyph("none")).toBe("");
    expect(ciTextClass("success")).toBe("text-success");
    expect(ciTextClass("cancelled")).toBe("text-danger");
    expect(ciTextClass("running")).toBe("text-warning");
    expect(ciTextClass("skipped")).toBe("text-muted-foreground");
  });

  it("picks HEAD, branch tips and recent rows once, capped", () => {
    const refs = {
      head: { branch: "main", oid: "h1", detached: false, unborn: false },
      local: [
        { name: "main", oid: "h1" },
        { name: "dev", oid: "d1" },
      ],
    } as unknown as RepoRefs;
    const rows = Array.from({ length: 30 }, (_, i) => ({ hash: `r${i}` })) as GraphRow[];
    const out = ciCandidateHashes(refs, rows);
    expect(out.slice(0, 3)).toEqual(["h1", "d1", "r0"]);
    expect(out).toHaveLength(22);
    expect(ciCandidateHashes(refs, rows, 5)).toHaveLength(5);
    expect(ciCandidateHashes(null)).toEqual([]);
  });

  it("indexes entries and detects running ones", () => {
    const batch = {
      entries: [
        {
          remote: "origin",
          fullName: "a/b",
          accountId: "acc",
          status: { ...status("pending"), sha: "ABC1234" },
        },
      ],
      problem: null,
    };
    expect(entriesBySha(batch).get("abc1234")?.fullName).toBe("a/b");
    expect(hasPending(batch)).toBe(true);
    expect(hasPending(undefined)).toBe(false);
  });

  it("counts, sorts and times jobs", () => {
    const jobs = [
      job("ok", "success"),
      job("lint", "failure", { allowFailure: true }),
      job("run", "running"),
      job("bad", "failure"),
    ];
    expect(countJobs(jobs)).toEqual({ total: 4, success: 1, failure: 1, running: 1, other: 1 });
    expect(sortJobs(jobs).map((j) => j.name)).toEqual(["bad", "run", "ok", "lint"]);
    expect(jobDuration({ startedAt: 100, finishedAt: 142 }, 0)).toBe("42s");
    expect(jobDuration({ startedAt: 100, finishedAt: null }, 165)).toBe("1m 05s");
    expect(jobDuration({ startedAt: 0, finishedAt: 3720 }, 0)).toBe("1h 02m");
    expect(jobDuration({ startedAt: null, finishedAt: null }, 0)).toBeNull();
    expect(safeUrl("https://x/y")).toBe("https://x/y");
    expect(safeUrl("javascript:alert(1)")).toBeNull();
    expect(safeUrl("http://x")).toBeNull();
  });
});

describe("drawCiBadge", () => {
  const colors: CiColors = {
    success: "#0a0",
    danger: "#a00",
    warning: "#aa0",
    muted: "#888",
    bg: "#fff",
    font: "sans",
  };
  const fakeCtx = () => {
    const calls: string[] = [];
    const ctx = {
      save: () => calls.push("save"),
      restore: () => calls.push("restore"),
      beginPath: vi.fn(),
      arc: vi.fn(),
      fill: vi.fn(),
      stroke: vi.fn(),
      fillText: (text: string) => calls.push(`text:${text}`),
      fillStyle: "",
      strokeStyle: "",
      lineWidth: 0,
      font: "",
      textAlign: "",
      textBaseline: "",
    };
    return { ctx: ctx as unknown as CanvasRenderingContext2D, calls, raw: ctx };
  };

  it("draws a ring with the glyph in the state colour", () => {
    const { ctx, calls, raw } = fakeCtx();
    expect(drawCiBadge(ctx, 10, 10, "failure", colors)).toBe(14);
    expect(calls).toEqual(["save", "text:✗", "restore"]);
    expect(raw.strokeStyle).toBe("#a00");
  });

  it("draws a dot for running and nothing for none", () => {
    const { ctx, calls, raw } = fakeCtx();
    drawCiBadge(ctx, 10, 10, "pending", colors, 12);
    expect(calls).toEqual(["save", "restore"]);
    expect(raw.fillStyle).toBe("#aa0");
    const none = fakeCtx();
    expect(drawCiBadge(none.ctx, 0, 0, "none", colors)).toBe(0);
    expect(none.calls).toEqual([]);
  });
});

describe("ci errors", () => {
  const t = i18n.t.bind(i18n);
  const label = () => "Công ty";

  it("explains missing scopes with a sign-in-again action", () => {
    const error: ProviderError = {
      kind: "forbidden",
      message: "x",
      hint: { kind: "missingScopes", required: ["repo"], granted: ["read:user"] },
    };
    const d = describeProviderError(t, error, "acc", "Công ty");
    expect(d.text).toContain("repo");
    expect(d.action?.label).toBe("Đăng nhập lại");
  });

  it("explains 404 and rate limits", () => {
    expect(
      describeProviderError(t, { kind: "notFound", message: "Not Found", hint: null }, "a", "X")
        .text,
    ).toContain("không thấy repo");
    const rl = describeProviderError(
      t,
      { kind: "rateLimited", resetAt: 1_999_999_999, retryAfterSecs: null },
      "a",
      "X",
    );
    expect(rl.text).toContain("Thử lại sau");
  });

  it("stays silent without a hosted remote", () => {
    expect(describeCiProblem(t, { kind: "noHostedRemote" }, label)).toBeNull();
    expect(describeCiProblem(t, null, label)).toBeNull();
    expect(describeCiProblem(t, { kind: "unbound", remote: "origin" }, label)?.text).toContain(
      "origin",
    );
    expect(describeCiProblem(t, { kind: "noToken", accountId: "a" }, label)?.action).not.toBeNull();
  });
});

describe("CI components", () => {
  const wrap = (ui: React.ReactNode) =>
    render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>);

  it("badge has a tooltip and nothing for none", () => {
    const { container } = wrap(
      <>
        <CiBadge state="success" jobs={[job("a", "success"), job("b", "skipped")]} />
        <CiBadge state="none" />
      </>,
    );
    const badge = screen.getByRole("img");
    expect(badge.getAttribute("title")).toBe("CI: thành công · 1/2 job đạt");
    expect(container.querySelectorAll("[data-ci-state]")).toHaveLength(1);
  });

  it("lists jobs (failed first) with log links and the checks hint", () => {
    const st: CiStatus = {
      ...status("failure", [
        job("build", "success", { url: "https://ci/1", startedAt: 0, finishedAt: 30 }),
        job("<b>test</b>", "failure", { group: "GitHub Actions", url: "javascript:x" }),
      ]),
      checksError: { kind: "forbidden", message: "no", hint: { kind: "tokenPermissionMissing" } },
    };
    wrap(<CiChecksList status={st} accountId="acc" />);
    const items = screen.getAllByRole("listitem");
    expect(items[0]?.textContent).toContain("<b>test</b>");
    expect(items[1]?.textContent).toContain("30s");
    // Only the https link is offered.
    expect(screen.getAllByRole("button", { name: /Mở log của/ })).toHaveLength(1);
    expect(screen.getByRole("note").textContent).toContain("check runs");
  });
});
