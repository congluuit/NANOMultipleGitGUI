import { beforeEach, describe, expect, it } from "vitest";
import { useAccountDialogs } from "@/features/accounts/dialogs";
import { PERSONAL, seedAccounts } from "@/features/accounts/switcher/testData";
import { initI18n } from "@/i18n";
import type { PullCommandError } from "@/ipc";
import { queryClient } from "@/lib/queryClient";
import { describePullError, isPullCommandError } from "./errors";

const provider = (error: Extract<PullCommandError, { kind: "provider" }>["error"]) =>
  ({ kind: "provider", message: "x", accountId: PERSONAL.id, error }) as PullCommandError;

beforeEach(() => {
  initI18n("vi");
  queryClient.clear();
  seedAccounts(queryClient);
  useAccountDialogs.setState({ manager: null });
});

describe("describePullError", () => {
  it("offers to sign in again on a rejected token", () => {
    const d = describePullError(provider({ kind: "unauthorized", message: "Bad credentials" }));
    expect(d.message).toContain("Cá nhân");
    expect(d.reauth).toBe(true);
    d.actions[0]?.run();
    expect(useAccountDialogs.getState().manager).toEqual({
      accountId: PERSONAL.id,
      add: false,
      reauth: true,
    });
  });

  it("names the missing scope", () => {
    const d = describePullError(
      provider({
        kind: "forbidden",
        message: "",
        hint: { kind: "missingScopes", required: ["repo"], granted: ["read:user"] },
      }),
    );
    expect(d.message).toContain("repo");
    expect(d.actions).toHaveLength(1);
  });

  it("explains 404 as a visibility problem", () => {
    const d = describePullError(provider({ kind: "notFound", message: "Not Found", hint: null }));
    expect(d.message).toContain("không thấy");
    expect(d.hint).toBeTruthy();
  });

  it("gives the retry time of a rate limit", () => {
    const d = describePullError(
      provider({ kind: "rateLimited", resetAt: 1_999_999_999, retryAfterSecs: null }),
    );
    expect(d.message).toMatch(/\d{1,2}:\d{2}/);
  });

  it("falls back to the message of other errors", () => {
    const app: PullCommandError = {
      kind: "app",
      message: "no remote",
      error: { kind: "unsupported", message: "no remote" },
    };
    expect(isPullCommandError(app)).toBe(true);
    expect(describePullError(app).message).toBe("no remote");
    expect(describePullError(new Error("boom")).message).toBe("boom");
    expect(isPullCommandError({ kind: "git", message: "x" })).toBe(false);
  });
});
