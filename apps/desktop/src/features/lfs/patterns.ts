import type { LfsPattern, LfsStatus } from "@/ipc";

/**
 * Same rules as the backend (`nanomgg_lfs::setup::validate_pattern`): not empty, no
 * surrounding whitespace, not option-like, not a comment / negation, no quotes or control
 * characters.
 */
export function isValidPattern(pattern: string): boolean {
  return (
    pattern.length > 0 &&
    pattern.length <= 4096 &&
    pattern.trim() === pattern &&
    !/^[-#!]/.test(pattern) &&
    !pattern.includes('"') &&
    // eslint-disable-next-line no-control-regex
    !/[\u0000-\u001f\u007f]/.test(pattern)
  );
}

/** `git lfs untrack` edits the root `.gitattributes` only. */
export function canUntrack(pattern: LfsPattern): boolean {
  return pattern.file === ".gitattributes";
}

/**
 * Offer `git lfs install --local`: the backend flag, or patterns declared while the hooks
 * are missing (e.g. installed by hand and later removed).
 */
export function shouldOfferInstall(status: LfsStatus): boolean {
  return (
    lfsVersion(status) !== null &&
    (status.needsInstall || (status.usesLfs && !status.hooksInstalled))
  );
}

/** `3.6.1`, or `null` when git-lfs is not installed. */
export function lfsVersion(status: LfsStatus): string | null {
  const v = status.gitLfs.version;
  return v ? `${v.major}.${v.minor}.${v.patch}` : null;
}
