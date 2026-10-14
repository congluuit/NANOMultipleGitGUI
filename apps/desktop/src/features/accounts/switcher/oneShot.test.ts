import { beforeEach, describe, expect, it } from "vitest";
import {
  clearOneShot,
  peekOneShot,
  setOneShot,
  takeOneShot,
  useAsAccount,
  type OneShot,
} from "./oneShot";

const shot = (op: OneShot["op"]): OneShot => ({ accountId: "acc-oss", identityId: null, op });

describe("one-shot account override", () => {
  beforeEach(() => useAsAccount.setState({ byRepo: {} }));

  it("is consumed once by the matching operation", () => {
    setOneShot(1, shot("commit"));
    expect(takeOneShot(1, "push")).toBeNull();
    expect(peekOneShot(1, "commit")).toEqual(shot("commit"));
    expect(takeOneShot(1, "commit")).toEqual(shot("commit"));
    expect(takeOneShot(1, "commit")).toBeNull();
  });

  it("'any' applies to the next commit or push, never to fetch", () => {
    setOneShot(1, shot("any"));
    expect(takeOneShot(1, "fetch")).toBeNull();
    expect(takeOneShot(1, "push")).toEqual(shot("any"));
    expect(useAsAccount.getState().byRepo).toEqual({});
  });

  it("'fetch' is only consumed by fetch", () => {
    setOneShot(1, shot("fetch"));
    expect(takeOneShot(1, "commit")).toBeNull();
    expect(takeOneShot(1, "fetch")).toEqual(shot("fetch"));
  });

  it("is kept per repository and can be replaced or cleared", () => {
    setOneShot(1, shot("commit"));
    setOneShot(2, shot("push"));
    setOneShot(1, { ...shot("push"), identityId: "id-x" });
    expect(takeOneShot(2, "push")?.accountId).toBe("acc-oss");
    expect(peekOneShot(1, "push")?.identityId).toBe("id-x");
    clearOneShot(1);
    expect(peekOneShot(1, "push")).toBeNull();
  });
});
