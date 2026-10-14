import { beforeAll, describe, expect, it } from "vitest";
import i18n from "i18next";
import { initI18n } from "@/i18n";
import type { LfsFailure, LfsFailureKind, LfsTransferProgress } from "@/ipc";
import {
  failureTexts,
  formatBytes,
  formatEta,
  itemPercent,
  lfsSummaryText,
  percentOf,
  progressLine,
  summarize,
} from "./format";
import type { QueueItem } from "./store";

const MB = 1024 * 1024;
const GB = 1024 * MB;

beforeAll(() => {
  initI18n("vi");
});

function lfs(over: Partial<LfsTransferProgress> = {}): LfsTransferProgress {
  return {
    repoId: 1,
    opId: "op",
    direction: "upload",
    filesDone: 0,
    filesTotal: 0,
    bytesDone: 0,
    bytesTotal: 0,
    currentFile: null,
    bytesPerSec: 0,
    etaSecs: null,
    active: [],
    done: false,
    ...over,
  };
}

function item(over: Partial<QueueItem> = {}): QueueItem {
  return {
    opId: "op",
    repoId: 1,
    kind: "lfsUpload",
    subject: null,
    status: "running",
    cancelling: false,
    text: "",
    percent: null,
    lfs: null,
    failure: null,
    error: null,
    startedAt: 0,
    finishedAt: null,
    ...over,
  };
}

describe("formatBytes", () => {
  it("uses binary units with one decimal below 100", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(760 * MB)).toBe("760 MB");
    expect(formatBytes(1.8 * GB)).toBe("1.8 GB");
    expect(formatBytes(2 * GB)).toBe("2 GB");
    expect(formatBytes(-1)).toBe("?");
  });
});

describe("formatEta", () => {
  it("formats seconds, minutes and hours", () => {
    expect(formatEta(45)).toBe("45 giây");
    expect(formatEta(200)).toBe("3 phút 20 giây");
    expect(formatEta(3900)).toBe("1 giờ 05 phút");
  });

  it("follows the language", async () => {
    await i18n.changeLanguage("en");
    expect(formatEta(200)).toBe("3m 20s");
    await i18n.changeLanguage("vi");
  });
});

describe("summaries", () => {
  it("percentOf handles unknown totals", () => {
    expect(percentOf(1, 0)).toBeNull();
    expect(percentOf(42, 100)).toBe(42);
    expect(percentOf(200, 100)).toBe(100);
  });

  it("combines running LFS transfers across repositories", () => {
    const s = summarize([
      item({ opId: "a", lfs: lfs({ bytesDone: 500 * MB, bytesTotal: GB }) }),
      item({
        opId: "b",
        repoId: 2,
        kind: "lfsDownload",
        lfs: lfs({ bytesDone: 260 * MB, bytesTotal: 0.8 * GB }),
      }),
      item({ opId: "c", kind: "fetch", text: "Receiving objects 10%" }),
      item({ opId: "d", status: "failed" }),
      item({ opId: "e", status: "done" }),
    ]);
    expect(s.running).toBe(3);
    expect(s.failed).toBe(1);
    expect(s.finished).toBe(2);
    expect(s.git?.opId).toBe("c");
    expect(s.lfs).toEqual({ done: 760 * MB, total: 1.8 * GB, percent: 41 });
    expect(lfsSummaryText(s.lfs!)).toBe("LFS 41% · 760 MB/1.8 GB");
    expect(lfsSummaryText({ done: 5 * MB, total: 0, percent: null })).toBe("LFS · 5 MB");
  });

  it("describes item progress", () => {
    const running = item({
      lfs: lfs({
        bytesDone: 760 * MB,
        bytesTotal: 1.8 * GB,
        filesDone: 5,
        filesTotal: 12,
        bytesPerSec: 5 * MB,
        etaSecs: 200,
      }),
    });
    expect(progressLine(running)).toBe(
      "41% · 760 MB / 1.8 GB · 5/12 file · 5 MB/s · còn 3 phút 20 giây",
    );
    expect(itemPercent(running)).toBe(41);
    expect(itemPercent(item({ status: "done" }))).toBe(100);
    expect(progressLine(item({ kind: "push" }))).toBe("Đang bắt đầu…");
    expect(itemPercent(item({ kind: "push", percent: 30 }))).toBe(30);
  });
});

describe("failureTexts", () => {
  const failure = (kind: LfsFailureKind, over: Partial<LfsFailure> = {}): LfsFailure => ({
    kind,
    message: "raw",
    retryable: false,
    billingUrl: null,
    settingsUrl: null,
    limitBytes: null,
    object: null,
    ...over,
  });

  it("has a title and a hint for every kind in both languages", async () => {
    const kinds: LfsFailureKind[] = [
      "noPermission",
      "authRequired",
      "quotaExceeded",
      "objectTooLarge",
      "lfsDisabled",
      "notFound",
      "rateLimited",
      "network",
      "missingObject",
      "insufficientDiskSpace",
      "other",
    ];
    for (const lang of ["vi", "en"]) {
      await i18n.changeLanguage(lang);
      for (const kind of kinds) {
        const t = failureTexts(failure(kind), "Công ty");
        expect(t.title, kind).not.toContain("lfsErrors.");
        expect(t.hint, kind).not.toContain("lfsErrors.");
      }
    }
    await i18n.changeLanguage("vi");
  });

  it("names the account and adds details", () => {
    expect(failureTexts(failure("noPermission"), "Cá nhân").title).toBe(
      "Tài khoản Cá nhân không có quyền ghi LFS vào repo này.",
    );
    const big = failureTexts(
      failure("objectTooLarge", { limitBytes: 2 * GB, object: "abc123" }),
      "x",
    );
    expect(big.extra).toEqual(["Giới hạn: 2 GB.", "Object: abc123"]);
  });
});
