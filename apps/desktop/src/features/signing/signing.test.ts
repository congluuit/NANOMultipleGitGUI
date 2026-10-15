import { beforeAll, describe, expect, it } from "vitest";
import type { GpgKey, SshKeyInfo } from "@/ipc";
import { initI18n } from "@/i18n";
import { commitErrorNotice, isSigningError } from "./commitError";
import {
  gpgKeyLabel,
  signatureTone,
  signerText,
  sshKeyLabel,
  sshKeyRef,
  usableGpgKeys,
} from "./model";

beforeAll(() => {
  initI18n("en");
});

const gpg = (keyId: string, canSign: boolean): GpgKey => ({
  keyId,
  fingerprint: null,
  uids: [`${keyId} <a@b.dev>`],
  created: null,
  expires: null,
  canSign,
  invalid: !canSign,
});

describe("signing model", () => {
  it("maps verification status to a tone", () => {
    expect(signatureTone("verified")).toBe("ok");
    expect(signatureTone("bad")).toBe("danger");
    expect(signatureTone("revoked")).toBe("danger");
    expect(signatureTone("unknownKey")).toBe("warning");
    expect(signatureTone("unsigned")).toBe("muted");
    expect(
      signerText({
        status: "verified",
        kind: "ssh",
        signer: "a@b.dev",
        key: "SHA256:x",
        fingerprint: null,
        trust: null,
        message: null,
      }),
    ).toBe("a@b.dev · SHA256:x");
  });

  it("lists usable GPG keys first", () => {
    const keys = usableGpgKeys([gpg("OLD", false), gpg("NEW", true)]);
    expect(keys.map((k) => k.keyId)).toEqual(["NEW", "OLD"]);
    expect(gpgKeyLabel(keys[0]!)).toBe("NEW <a@b.dev> — NEW");
  });

  it("refers to app SSH keys by their .pub path", () => {
    const key = {
      id: "k1",
      keyType: "ed25519",
      fingerprint: "SHA256:abc",
      comment: "work",
      privateKeyPath: "C:/data/ssh/k1",
    } as SshKeyInfo;
    expect(sshKeyRef(key)).toBe("C:/data/ssh/k1.pub");
    expect(sshKeyRef({ ...key, privateKeyPath: null })).toBeNull();
    expect(sshKeyLabel(key)).toBe("work — SHA256:abc");
  });

  it("recognises signing failures of a commit", () => {
    const gpgError = {
      kind: "git",
      message:
        "git commit failed: error: gpg failed to sign the data:\ngpg: signing failed: No secret key\nfatal: failed to write commit object",
    };
    expect(isSigningError(gpgError.message)).toBe(true);
    const notice = commitErrorNotice(gpgError, 1);
    expect(notice.message).toBe("The commit could not be signed");
    expect(notice.hint).toMatch(/Test signing/);
    expect(notice.detail).toContain("No secret key");
    const other = commitErrorNotice({ kind: "git", message: "hook failed" }, 1);
    expect(other).toEqual({ kind: "error", message: "hook failed" });
  });
});
