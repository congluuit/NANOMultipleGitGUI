import type { CommitSignature, GpgKey, SignatureStatus, SshKeyInfo } from "@/ipc";

export type SignatureTone = "ok" | "warning" | "danger" | "muted";

/** Colour of a verification status: green verified, amber "signed but cannot vouch", red bad. */
export function signatureTone(status: SignatureStatus): SignatureTone {
  switch (status) {
    case "verified":
      return "ok";
    case "bad":
    case "revoked":
      return "danger";
    case "unsigned":
      return "muted";
    default:
      return "warning";
  }
}

/** Who signed, for the badge tooltip (`signer` · key). */
export function signerText(sig: CommitSignature): string {
  return [sig.signer, sig.key].filter(Boolean).join(" · ");
}

/** Option label of a GPG key: first user id + long key id. */
export function gpgKeyLabel(k: GpgKey): string {
  return `${k.uids[0] ?? "?"} — ${k.keyId}`;
}

/** Keys that can actually sign come first. */
export function usableGpgKeys(keys: readonly GpgKey[]): GpgKey[] {
  return [...keys].sort((a, b) => Number(b.canSign) - Number(a.canSign));
}

/**
 * Signing key reference stored on the identity for an app-managed SSH key: the `.pub` next to
 * its private key (the resolver signs with the private key when it exists).
 */
export function sshKeyRef(key: SshKeyInfo): string | null {
  return key.privateKeyPath ? `${key.privateKeyPath}.pub` : null;
}

/** Label of an SSH key in the picker. */
export function sshKeyLabel(key: SshKeyInfo): string {
  return `${key.comment ?? key.keyType} — ${key.fingerprint}`;
}
