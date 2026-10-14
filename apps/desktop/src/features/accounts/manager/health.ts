import type { TFunction } from "i18next";
import type { AccountHealth, Feature } from "@/ipc";

export type BadgeTone = "ok" | "warn" | "danger" | "muted";

export interface HealthBadge {
  tone: BadgeTone;
  text: string;
  /** Show the "Re-authenticate" action next to it. */
  reauth?: boolean;
}

export const featureLabel = (t: TFunction, feature: Feature) => t(`accounts.feature.${feature}`);

/**
 * Badges of an account card, most severe first: "needs sign-in" / "expired", "expires in N
 * days", "missing scope X", or a single "OK".
 */
export function healthBadges(health: AccountHealth | undefined, t: TFunction): HealthBadge[] {
  if (!health) return [];
  const out: HealthBadge[] = [];
  if (health.expired) {
    out.push({ tone: "danger", text: t("accounts.health.expired"), reauth: true });
  } else if (health.needsSignIn) {
    out.push({ tone: "danger", text: t("accounts.health.needsSignIn"), reauth: true });
  }
  if (!health.expired && health.expiresSoon && health.daysLeft !== null) {
    out.push({
      tone: "warn",
      text:
        health.daysLeft === 0
          ? t("accounts.health.expiresToday")
          : t("accounts.health.expiresIn", { count: health.daysLeft }),
      reauth: true,
    });
  }
  for (const missing of health.missingScopes) {
    out.push({
      tone: "warn",
      text: t("accounts.health.missingScope", {
        scope: missing.anyOf.join(" / "),
        feature: featureLabel(t, missing.feature),
      }),
    });
  }
  if (out.length === 0) {
    if (!health.hasToken) out.push({ tone: "muted", text: t("accounts.health.sshOnly") });
    else out.push({ tone: "ok", text: t("accounts.health.ok") });
  }
  return out;
}

/** "Expires 2026-12-31" / "No expiry" for the details line. */
export function expiryText(expiresAtMs: number | null, autoRefresh: boolean, t: TFunction) {
  if (autoRefresh) return t("accounts.health.autoRefresh");
  if (expiresAtMs === null) return t("accounts.health.noExpiry");
  return t("accounts.health.expiresOn", {
    date: new Date(expiresAtMs).toISOString().slice(0, 10),
  });
}
