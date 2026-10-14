import type { TFunction } from "i18next";
import type { Account, SshImportResult, SshTestResult, SshUploadResult } from "@/ipc";

/** Account name as shown in titles: label, display name, then user name. */
export const accountName = (a: Pick<Account, "label" | "displayName" | "username">) =>
  a.label || a.displayName || a.username;

export type Tone = "ok" | "warn" | "danger";

export interface Message {
  tone: Tone;
  text: string;
  /** Offer "Sign in again" (opens the account manager on that account). */
  signIn?: boolean;
}

/** User-facing outcome of "Upload to provider". */
export function uploadMessage(t: TFunction, r: SshUploadResult): Message {
  switch (r.kind) {
    case "uploaded":
      return { tone: "ok", text: t("ssh.uploadResult.uploaded") };
    case "alreadyRegistered":
      return { tone: "ok", text: t("ssh.uploadResult.alreadyRegistered") };
    case "usedByAnotherAccount":
      return { tone: "danger", text: t("ssh.uploadResult.usedByAnotherAccount") };
    case "missingScope":
      return r.required.length > 0
        ? {
            tone: "warn",
            text: t("ssh.uploadResult.missingScope", { scopes: r.required.join(" / ") }),
            signIn: true,
          }
        : { tone: "warn", text: t("ssh.uploadResult.missingScopeUnknown"), signIn: true };
    case "tokenPermissionMissing":
      return { tone: "warn", text: t("ssh.uploadResult.tokenPermissionMissing"), signIn: true };
    case "noToken":
      return { tone: "warn", text: t("ssh.uploadResult.noToken"), signIn: true };
    case "unauthorized":
      return { tone: "danger", text: t("ssh.uploadResult.unauthorized"), signIn: true };
  }
}

/** Outcome of "Test connection": which user the key authenticates as. */
export function testMessage(t: TFunction, r: SshTestResult): Message {
  switch (r.status) {
    case "authenticated": {
      const user = r.username ?? "?";
      if (r.deployKey)
        return {
          tone: "warn",
          text: t("ssh.testResult.deployKey", { repo: user, expected: r.expectedUsername }),
        };
      if (!r.matchesAccount)
        return {
          tone: "warn",
          text: t("ssh.testResult.mismatch", { user, expected: r.expectedUsername }),
        };
      return { tone: "ok", text: t("ssh.testResult.authenticated", { user }) };
    }
    case "denied":
      return { tone: "danger", text: t("ssh.testResult.denied") };
    case "hostKeyFailed":
      return { tone: "danger", text: t("ssh.testResult.hostKeyFailed", { host: r.host }) };
    case "network":
      return { tone: "danger", text: t("ssh.testResult.network", { host: r.host }) };
    case "passphraseNeeded":
      return { tone: "warn", text: t("ssh.testResult.passphraseNeeded") };
    case "unknown":
      return { tone: "warn", text: t("ssh.testResult.unknown") };
  }
}

/** Outcome of "Import from file" for the form (`null` = imported without remarks). */
export function importMessage(t: TFunction, r: SshImportResult, accountId: string): Message | null {
  switch (r.kind) {
    case "imported":
      return r.publicFileMismatch
        ? { tone: "warn", text: t("ssh.importForm.publicMismatch") }
        : null;
    case "passphraseRequired":
      return { tone: "warn", text: t("ssh.importForm.passphraseRequired") };
    case "wrongPassphrase":
      return { tone: "danger", text: t("ssh.importForm.wrongPassphrase") };
    case "alreadyKnown":
      return r.accountId === accountId
        ? { tone: "warn", text: t("ssh.importForm.alreadyKnown") }
        : { tone: "danger", text: t("ssh.importForm.alreadyOther") };
  }
}

/** `SHA256:abcd…wxyz` shortened for narrow rows. */
export function shortFingerprint(fp: string, keep = 10): string {
  const colon = fp.indexOf(":");
  const algo = colon >= 0 ? fp.slice(0, colon) : "";
  const hash = colon >= 0 ? fp.slice(colon + 1) : fp;
  if (hash.length <= keep * 2 + 1) return fp;
  return `${algo ? `${algo}:` : ""}${hash.slice(0, keep)}…${hash.slice(-keep)}`;
}
