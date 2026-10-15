import { describe, expect, it } from "vitest";
import type { TFunction } from "i18next";
import type { AccountPreview, AccountsImportReport } from "@/ipc";
import en from "@/i18n/locales/en.json";
import vi from "@/i18n/locales/vi.json";
import {
  defaultFileName,
  defaultSelection,
  importSummary,
  passwordProblem,
  problemText,
  toggleKey,
} from "./model";

/** Echoes the key and its options, so tests check which text was picked. */
const t = ((key: string, opts?: Record<string, unknown>) =>
  opts ? `${key} ${JSON.stringify(opts)}` : key) as unknown as TFunction;

function preview(key: string, existing: string | null): AccountPreview {
  return {
    key,
    username: key,
    displayName: null,
    label: null,
    color: "#336699",
    providerName: "GitHub",
    providerKind: "github",
    host: "github.com",
    authMethod: "pat",
    identities: 1,
    rules: 0,
    sshKeys: 0,
    sshKeyFiles: 0,
    hasToken: false,
    existingAccountId: existing,
    newProvider: false,
  };
}

describe("passwordProblem", () => {
  it("needs eight characters and the same text twice", () => {
    expect(passwordProblem("", "")).toBe("tooShort");
    expect(passwordProblem("1234567", "1234567")).toBe("tooShort");
    expect(passwordProblem("12345678", "1234567")).toBe("mismatch");
    expect(passwordProblem("12345678", "12345678")).toBeNull();
    // Characters, not UTF-16 units or bytes.
    expect(passwordProblem("mậtkhẩu1", "mậtkhẩu1")).toBeNull();
    expect(passwordProblem("🔑🔑🔑🔑🔑🔑🔑", "🔑🔑🔑🔑🔑🔑🔑")).toBe("tooShort");
  });
});

describe("problemText", () => {
  it("explains the wrong kind of file for each dialog", () => {
    expect(problemText(t, "wrongKind", "accounts").message).toBe(
      "backup.problem.wrongKindAccounts",
    );
    expect(problemText(t, "wrongKind", "full").message).toBe("backup.problem.wrongKindFull");
    const wrong = problemText(t, "wrongPassword", "full");
    expect(wrong.hint).toBe("backup.problem.wrongPasswordHint");
  });

  it("has a text for every problem in both languages", () => {
    for (const problem of [
      "wrongPassword",
      "notABackup",
      "newerVersion",
      "tooLarge",
      "corrupt",
      "weakPassword",
    ] as const) {
      const key = problemText(t, problem, "full").message.replace("backup.problem.", "");
      expect(vi.backup.problem).toHaveProperty(key);
      expect(en.backup.problem).toHaveProperty(key);
    }
  });
});

describe("files and selection", () => {
  it("suggests a dated file name with the right extension", () => {
    const day = new Date(2026, 9, 5);
    expect(defaultFileName("accounts", day)).toBe("nanomgg-accounts-2026-10-05.nanomgg-accounts");
    expect(defaultFileName("full", day)).toBe("nanomgg-backup-2026-10-05.nanomgg-backup");
  });

  it("ticks only accounts that are not here yet and keeps the file order", () => {
    const list = [preview("a", null), preview("b", "acc_b"), preview("c", null)];
    expect(defaultSelection(list)).toEqual(["a", "c"]);
    const order = list.map((a) => a.key);
    expect(toggleKey(["a", "c"], "b", order)).toEqual(["a", "b", "c"]);
    expect(toggleKey(["a", "b", "c"], "a", order)).toEqual(["b", "c"]);
  });
});

describe("importSummary", () => {
  const report: AccountsImportReport = {
    added: 2,
    updated: 0,
    skipped: 1,
    providersAdded: 1,
    tokens: 0,
    sshKeys: 0,
    needSignIn: ["acc_1", "acc_2"],
    signingDisabled: 0,
  };

  it("says what was added and skipped, and who must sign in again", () => {
    const s = importSummary(t, report);
    expect(s.message).toBe('backup.accountsImport.result {"added":2,"skipped":1}');
    expect(s.lines).toEqual(['backup.accountsImport.needSignIn {"count":2}']);
  });

  it("mentions updates and disabled signing only when they happened", () => {
    const s = importSummary(t, { ...report, updated: 1, needSignIn: [], signingDisabled: 1 });
    expect(s.message).toContain("backup.accountsImport.resultUpdated");
    expect(s.lines).toEqual(['backup.accountsImport.signingDisabled {"count":1}']);
  });
});
