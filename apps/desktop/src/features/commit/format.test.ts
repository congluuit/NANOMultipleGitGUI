import { describe, expect, it } from "vitest";
import { fileStatusLetter, splitPath } from "./format";

describe("splitPath", () => {
  it("splits directory and file name", () => {
    expect(splitPath("src/auth/login.ts")).toEqual({ dir: "src/auth/", name: "login.ts" });
    expect(splitPath("README.md")).toEqual({ dir: "", name: "README.md" });
    expect(splitPath("tài liệu/hướng dẫn.md")).toEqual({ dir: "tài liệu/", name: "hướng dẫn.md" });
  });
});

describe("fileStatusLetter", () => {
  it("maps statuses to git letters", () => {
    expect(fileStatusLetter("added")).toBe("A");
    expect(fileStatusLetter("renamed")).toBe("R");
    expect(fileStatusLetter("typeChanged")).toBe("T");
  });
});
