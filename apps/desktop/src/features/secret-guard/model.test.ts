import { describe, expect, it } from "vitest";
import { findingKey, planFor, remainingPaths, sortFindings, type SecretChoice } from "./model";
import { secretFinding } from "./testData";

const env = secretFinding({ path: "app/.env", line: 3 });
const envAgain = secretFinding({ path: "app/.env", line: 9 });
const conf = secretFinding({ path: "src/config.ts", line: 12 });
const confSame = { ...conf, line: 40 };

const choices = (entries: [typeof env, SecretChoice][]) =>
  new Map(entries.map(([f, c]) => [findingKey(f), c]));

describe("secret guard plan", () => {
  it("unstages every file by default", () => {
    expect(planFor([env, envAgain, conf], new Map())).toEqual({
      unstage: ["app/.env", "src/config.ts"],
      always: [],
    });
  });

  it("remembers always-ignored findings once, and nothing for ignore-once", () => {
    const plan = planFor(
      [env, conf, confSame],
      choices([
        [env, "once"],
        [conf, "always"],
        [confSame, "always"],
      ]),
    );
    expect(plan).toEqual({ unstage: [], always: [conf.fingerprint] });
  });

  it("does not remember a finding whose file is unstaged anyway", () => {
    const plan = planFor(
      [env, envAgain],
      choices([
        [env, "always"],
        [envAgain, "unstage"],
      ]),
    );
    expect(plan).toEqual({ unstage: ["app/.env"], always: [] });
  });

  it("keeps rows of the same secret on different lines apart", () => {
    expect(findingKey(conf)).not.toBe(findingKey(confSame));
  });

  it("lists what stays staged", () => {
    expect(remainingPaths(["a", "app/.env", "b"], ["app/.env"])).toEqual(["a", "b"]);
    expect(remainingPaths(["app/.env"], ["app/.env"])).toEqual([]);
  });

  it("groups findings by file and sorts lines", () => {
    const sorted = sortFindings([envAgain, conf, env]);
    expect(sorted.map((f) => `${f.path}:${f.line}`)).toEqual([
      "app/.env:3",
      "app/.env:9",
      "src/config.ts:12",
    ]);
  });
});
