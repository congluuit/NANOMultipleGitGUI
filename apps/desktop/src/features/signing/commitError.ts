import { errorMessage } from "@/ipc";
import { failureNotice } from "@/features/commits/outcome";
import type { Notice } from "@/stores/ui";

/** Same phrases as the backend's `failure_kind` → `signingFailed` (gpg / ssh-keygen output). */
const SIGNING_PATTERNS = [
  "failed to sign the data",
  "signing failed",
  "couldn't load public key",
  "ssh-keygen -y sign is needed",
];

export function isSigningError(message: string): boolean {
  const m = message.toLowerCase();
  return SIGNING_PATTERNS.some((p) => m.includes(p));
}

/**
 * Notice for a failed `commit_create`: signing failures get the "how to fix" help of
 * `failure.signingFailed`; anything else keeps the plain message.
 */
export function commitErrorNotice(e: unknown, repoId: number): Notice {
  const message = errorMessage(e);
  return isSigningError(message)
    ? failureNotice("signingFailed", message, repoId)
    : { kind: "error", message };
}
