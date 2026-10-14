import { describe, expect, it } from "vitest";
import { fuzzyFilter, normalize, scoreCommand, scoreToken } from "./fuzzy";

describe("normalize", () => {
  it("strips Vietnamese diacritics, đ and case", () => {
    expect(normalize("Chuyển nhánh")).toBe("chuyen nhanh");
    expect(normalize("Đóng tab hiện tại")).toBe("dong tab hien tai");
    expect(normalize("Tiếng Việt")).toBe("tieng viet");
    expect(normalize("Ngôn ngữ…")).toBe("ngon ngu…");
  });

  it("handles precomposed and decomposed input alike", () => {
    expect(normalize("e\u0301")).toBe(normalize("é"));
  });
});

describe("scoreToken", () => {
  it("ranks exact > prefix > word start > substring > acronym > subsequence", () => {
    const exact = scoreToken("push", "push")!;
    const prefix = scoreToken("pu", "push")!;
    const word = scoreToken("pu", "force push")!;
    const sub = scoreToken("us", "push")!;
    const acro = scoreToken("cb", "create branch")!;
    const subseq = scoreToken("fpsh", "force push")!;
    expect(exact).toBeGreaterThan(prefix);
    expect(prefix).toBeGreaterThan(word);
    expect(word).toBeGreaterThan(sub);
    expect(sub).toBeGreaterThan(acro);
    expect(acro).toBeGreaterThan(subseq);
    expect(subseq).toBeGreaterThan(0);
  });

  it("returns null when characters are missing or out of order", () => {
    expect(scoreToken("xyz", "push")).toBeNull();
    expect(scoreToken("hsup", "push")).toBeNull();
  });

  it("prefers tight subsequences", () => {
    expect(scoreToken("fch", "fetch")!).toBeGreaterThan(scoreToken("fch", "fxxxxxxxcxxxxxxh")!);
  });
});

describe("scoreCommand", () => {
  it("matches without diacritics in either the query or the title", () => {
    expect(scoreCommand("chuyen nhanh", { title: "Chuyển nhánh (checkout)…" })).not.toBeNull();
    expect(scoreCommand("chuyển", { title: "Chuyen tab" })).not.toBeNull();
    expect(scoreCommand("dong", { title: "Đóng tab hiện tại" })).not.toBeNull();
  });

  it("requires every token to match the title or a keyword", () => {
    const item = { title: "Chuyển nhánh…", keywords: ["checkout", "switch branch"] };
    expect(scoreCommand("checkout", item)).not.toBeNull();
    expect(scoreCommand("nhanh checkout", item)).not.toBeNull();
    expect(scoreCommand("nhanh zzz", item)).toBeNull();
  });

  it("weights title hits above keyword hits", () => {
    expect(scoreCommand("push", { title: "Push" })!).toBeGreaterThan(
      scoreCommand("push", { title: "Đồng bộ", keywords: ["push"] })!,
    );
  });

  it("gives an empty query a neutral score", () => {
    expect(scoreCommand("  ", { title: "anything" })).toBe(0);
  });
});

describe("fuzzyFilter", () => {
  const items = [
    { title: "Force push…" },
    { title: "Pull (rebase)" },
    { title: "Push" },
    { title: "Mở cài đặt" },
  ];

  it("sorts the best match first and drops non-matches", () => {
    expect(fuzzyFilter("push", items).map((i) => i.title)).toEqual(["Push", "Force push…"]);
    expect(fuzzyFilter("cai dat", items).map((i) => i.title)).toEqual(["Mở cài đặt"]);
  });

  it("keeps the original order for an empty query and for ties", () => {
    expect(fuzzyFilter("", items)).toEqual(items);
    const twins = [{ title: "Tab a" }, { title: "Tab b" }];
    expect(fuzzyFilter("tab", twins)).toEqual(twins);
  });
});
