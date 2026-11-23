import type { SecretFinding, SecretReport } from "@/ipc";

let seq = 0;

/** A finding as the backend sends it (masked value, 32-hex fingerprint). */
export function secretFinding(over: Partial<SecretFinding> & { path: string }): SecretFinding {
  seq += 1;
  return {
    ruleId: "github-pat",
    ruleName: "GitHub personal access token",
    line: 1,
    masked: "ghp_1a2B****yz",
    fingerprint: seq.toString(16).padStart(32, "0"),
    ...over,
  };
}

export const secretReport = (
  findings: SecretFinding[],
  over: Partial<SecretReport> = {},
): SecretReport => ({
  findings,
  filesScanned: findings.length,
  ignored: 0,
  truncated: false,
  ...over,
});
