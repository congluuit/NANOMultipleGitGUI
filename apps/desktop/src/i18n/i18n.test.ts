import { describe, expect, it } from "vitest";
import en from "./locales/en.json";
import vi from "./locales/vi.json";
import { detectLanguage } from ".";

function keys(obj: object, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    v && typeof v === "object" ? keys(v as object, `${prefix}${k}.`) : [`${prefix}${k}`],
  );
}

describe("locales", () => {
  it("vi and en define exactly the same keys", () => {
    expect(keys(vi).sort()).toEqual(keys(en).sort());
  });

  it("uses the same interpolation variables in both languages", () => {
    const vars = (s: string) => [...s.matchAll(/\{\{(\w+)\}\}/g)].map((m) => m[1]).sort();
    const flat = (obj: object, path: string): string =>
      path.split(".").reduce<unknown>((o, k) => (o as Record<string, unknown>)[k], obj) as string;
    for (const key of keys(vi)) {
      expect(vars(flat(vi, key)), key).toEqual(vars(flat(en, key)));
    }
  });
});

describe("detectLanguage", () => {
  it("picks Vietnamese for vi locales and English otherwise", () => {
    expect(detectLanguage("vi-VN")).toBe("vi");
    expect(detectLanguage("vi")).toBe("vi");
    expect(detectLanguage("en-US")).toBe("en");
    expect(detectLanguage("ja-JP")).toBe("en");
    expect(detectLanguage(undefined)).toBe("en");
  });
});
