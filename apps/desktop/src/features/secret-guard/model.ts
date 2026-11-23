import type { SecretFinding } from "@/ipc";

/** What to do with one finding before committing. */
export type SecretChoice = "unstage" | "once" | "always";

export const SECRET_CHOICES: readonly SecretChoice[] = ["unstage", "once", "always"];

/** Safe default: the file with the secret is left out of the commit. */
export const DEFAULT_CHOICE: SecretChoice = "unstage";

/** Identity of a row in the dialog (the same secret can appear on several lines). */
export const findingKey = (f: SecretFinding): string => `${f.path}\n${f.line}\n${f.fingerprint}`;

export interface SecretPlan {
  /** Staged paths to unstage, deduplicated, in finding order. */
  unstage: string[];
  /** Fingerprints to remember as "always ignore" in this repository. */
  always: string[];
}

/**
 * Turns the per-finding choices into actions. A finding whose file is unstaged anyway is not
 * remembered as "always ignore" (the user did not say the secret is fine).
 */
export function planFor(
  findings: SecretFinding[],
  choices: ReadonlyMap<string, SecretChoice>,
): SecretPlan {
  const choiceOf = (f: SecretFinding) => choices.get(findingKey(f)) ?? DEFAULT_CHOICE;
  const unstage: string[] = [];
  for (const f of findings) {
    if (choiceOf(f) === "unstage" && !unstage.includes(f.path)) unstage.push(f.path);
  }
  const always: string[] = [];
  for (const f of findings) {
    if (choiceOf(f) !== "always" || unstage.includes(f.path)) continue;
    if (!always.includes(f.fingerprint)) always.push(f.fingerprint);
  }
  return { unstage, always };
}

/** Staged paths still in the commit after unstaging `unstaged`. */
export const remainingPaths = (staged: string[], unstaged: string[]): string[] =>
  staged.filter((p) => !unstaged.includes(p));

/** Findings of the same file next to each other, files in first-seen order, lines ascending. */
export function sortFindings(findings: SecretFinding[]): SecretFinding[] {
  const order = new Map<string, number>();
  for (const f of findings) if (!order.has(f.path)) order.set(f.path, order.size);
  return [...findings].sort(
    (a, b) => (order.get(a.path) ?? 0) - (order.get(b.path) ?? 0) || a.line - b.line,
  );
}
