import type { Identity, Rule, RuleInput, SigningMethod } from "@/ipc";

// ----- identities --------------------------------------------------------------------------

export interface IdentityForm {
  name: string;
  email: string;
  signing: SigningMethod;
  signingKeyRef: string;
  isDefault: boolean;
}

export type FieldProblem = "required" | "invalid" | "duplicate";

export type IdentityErrors = Partial<Record<"name" | "email" | "signingKeyRef", FieldProblem>>;

export const emptyIdentity = (isDefault: boolean): IdentityForm => ({
  name: "",
  email: "",
  signing: "none",
  signingKeyRef: "",
  isDefault,
});

export const identityToForm = (i: Identity): IdentityForm => ({
  name: i.name,
  email: i.email,
  signing: i.signing,
  signingKeyRef: i.signingKeyRef ?? "",
  isDefault: i.isDefault,
});

export function isValidEmail(email: string): boolean {
  const e = email.trim();
  if (/[<>\s]/.test(e)) return false;
  const at = e.indexOf("@");
  return at > 0 && at < e.length - 1 && e.indexOf("@", at + 1) === -1;
}

/**
 * Same rules as the backend (`Identity::validate` + unique email per account): name without
 * `<>`, a plausible email, a key reference when signing is on. `others` = the account's other
 * identities.
 */
export function validateIdentity(form: IdentityForm, others: readonly Identity[]): IdentityErrors {
  const errors: IdentityErrors = {};
  const name = form.name.trim();
  if (!name) errors.name = "required";
  else if (/[<>\n]/.test(name)) errors.name = "invalid";
  const email = form.email.trim();
  if (!email) errors.email = "required";
  else if (!isValidEmail(email)) errors.email = "invalid";
  else if (others.some((o) => o.email.toLowerCase() === email.toLowerCase()))
    errors.email = "duplicate";
  if (form.signing !== "none" && !form.signingKeyRef.trim()) errors.signingKeyRef = "required";
  return errors;
}

// ----- rules -------------------------------------------------------------------------------

/** Evaluation order: priority, then creation time (what the backend does). */
export function sortRules(rules: readonly Rule[]): Rule[] {
  return [...rules].sort((a, b) => a.priority - b.priority || a.createdAt - b.createdAt);
}

/** Ids after moving the rule at `index` by `delta` (clamped); unchanged order → same ids. */
export function moveRule(ids: readonly string[], index: number, delta: number): string[] {
  const target = Math.max(0, Math.min(ids.length - 1, index + delta));
  const next = [...ids];
  if (index < 0 || index >= ids.length || target === index) return next;
  const [item] = next.splice(index, 1);
  if (item !== undefined) next.splice(target, 0, item);
  return next;
}

export interface RuleForm {
  host: string;
  ownerOrGroup: string;
  pathGlob: string;
  remoteRegex: string;
  accountId: string;
  identityId: string;
  enabled: boolean;
}

export type RuleErrors = Partial<Record<"match" | "remoteRegex" | "accountId", FieldProblem>>;

export const emptyRule = (accountId: string): RuleForm => ({
  host: "",
  ownerOrGroup: "",
  pathGlob: "",
  remoteRegex: "",
  accountId,
  identityId: "",
  enabled: true,
});

export const ruleToForm = (r: Rule): RuleForm => ({
  host: r.host ?? "",
  ownerOrGroup: r.ownerOrGroup ?? "",
  pathGlob: r.pathGlob ?? "",
  remoteRegex: r.remoteRegex ?? "",
  accountId: r.accountId,
  identityId: r.identityId ?? "",
  enabled: r.enabled,
});

export function validateRule(form: RuleForm): RuleErrors {
  const errors: RuleErrors = {};
  if (![form.host, form.ownerOrGroup, form.pathGlob, form.remoteRegex].some((v) => v.trim()))
    errors.match = "required";
  if (form.remoteRegex.trim()) {
    try {
      new RegExp(form.remoteRegex.trim());
    } catch {
      errors.remoteRegex = "invalid";
    }
  }
  if (!form.accountId) errors.accountId = "required";
  return errors;
}

const opt = (s: string) => (s.trim() ? s.trim() : null);

export function ruleInput(form: RuleForm, id: string | null): RuleInput {
  return {
    id,
    host: opt(form.host)?.toLowerCase() ?? null,
    ownerOrGroup: opt(form.ownerOrGroup),
    pathGlob: opt(form.pathGlob),
    remoteRegex: opt(form.remoteRegex),
    accountId: form.accountId,
    identityId: opt(form.identityId),
    enabled: form.enabled,
  };
}

// ----- colors ------------------------------------------------------------------------------

function luminance(hex: string): number {
  const m = /^#([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m?.[1]) return 0;
  const n = parseInt(m[1], 16);
  const lin = (c: number) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin((n >> 16) & 255) + 0.7152 * lin((n >> 8) & 255) + 0.0722 * lin(n & 255);
}

/** WCAG contrast ratio of two `#rrggbb` colors (1–21). */
export function contrastRatio(a: string, b: string): number {
  const la = luminance(a);
  const lb = luminance(b);
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
}

/** White or near-black text, whichever reads better on `hex`. */
export function textOn(hex: string): "#ffffff" | "#111111" {
  return contrastRatio(hex, "#ffffff") >= contrastRatio(hex, "#111111") ? "#ffffff" : "#111111";
}
