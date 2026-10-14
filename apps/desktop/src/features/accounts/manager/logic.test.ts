import i18n from "i18next";
import { beforeAll, describe, expect, it } from "vitest";
import { initI18n } from "@/i18n";
import type { AccountHealth, Identity, Provider, Rule } from "@/ipc";
import {
  contrastRatio,
  emptyIdentity,
  moveRule,
  ruleInput,
  sortRules,
  textOn,
  validateIdentity,
  validateRule,
} from "./forms";
import { expiryText, healthBadges } from "./health";
import {
  countdown,
  findProvider,
  instanceName,
  methodsFor,
  normalizeBaseUrl,
  validateBaseUrl,
  validateToken,
} from "./wizard";

beforeAll(() => {
  initI18n("vi");
});

const health = (over: Partial<AccountHealth> = {}): AccountHealth => ({
  accountId: "acc_1",
  hasToken: true,
  autoRefresh: false,
  scopesKnown: true,
  missingScopes: [],
  optionalMissing: [],
  expiresAt: null,
  expired: false,
  expiresSoon: false,
  daysLeft: null,
  needsSignIn: false,
  lastError: null,
  tokenKind: null,
  ...over,
});

describe("healthBadges", () => {
  const t = (lng: "vi" | "en") => i18n.getFixedT(lng);

  it("is OK for a healthy token and 'SSH only' without one", () => {
    expect(healthBadges(health(), t("vi"))).toEqual([{ tone: "ok", text: "Hoạt động tốt" }]);
    expect(healthBadges(health({ hasToken: false }), t("en"))).toEqual([
      { tone: "muted", text: "SSH only" },
    ]);
    expect(healthBadges(undefined, t("vi"))).toEqual([]);
  });

  it("formats 'expires in N days' with plurals", () => {
    const soon = health({ expiresSoon: true, daysLeft: 3 });
    expect(healthBadges(soon, t("vi"))[0]).toEqual({
      tone: "warn",
      text: "Hết hạn sau 3 ngày",
      reauth: true,
    });
    expect(healthBadges(soon, t("en"))[0]?.text).toBe("Expires in 3 days");
    expect(healthBadges(health({ expiresSoon: true, daysLeft: 1 }), t("en"))[0]?.text).toBe(
      "Expires in 1 day",
    );
    expect(healthBadges(health({ expiresSoon: true, daysLeft: 0 }), t("vi"))[0]?.text).toBe(
      "Hết hạn hôm nay",
    );
  });

  it("puts 'needs sign-in' / 'expired' first and lists missing scopes", () => {
    const badges = healthBadges(
      health({
        needsSignIn: true,
        missingScopes: [{ feature: "gitHttps", anyOf: ["repo"] }],
      }),
      t("vi"),
    );
    expect(badges.map((b) => b.text)).toEqual([
      "Cần đăng nhập lại",
      "Thiếu quyền repo (git qua HTTPS)",
    ]);
    expect(badges[0]).toMatchObject({ tone: "danger", reauth: true });
    const expired = healthBadges(
      health({ expired: true, needsSignIn: true, expiresSoon: true, daysLeft: 0 }),
      t("en"),
    );
    expect(expired.map((b) => b.text)).toEqual(["Token expired"]);
  });

  it("describes expiry", () => {
    expect(expiryText(null, false, t("vi"))).toBe("Không hết hạn");
    expect(expiryText(Date.UTC(2026, 11, 31), false, t("en"))).toBe("Expires 2026-12-31");
    expect(expiryText(123, true, t("en"))).toBe("Renews automatically");
  });
});

const identity = (email: string): Identity => ({
  id: `idn_${email}`,
  accountId: "acc_1",
  name: "Lan",
  email,
  signing: "none",
  signingKeyRef: null,
  isDefault: false,
  createdAt: 0,
});

describe("validateIdentity", () => {
  it("requires a name and a valid, unique email", () => {
    expect(validateIdentity(emptyIdentity(true), [])).toEqual({
      name: "required",
      email: "required",
    });
    const form = { ...emptyIdentity(false), name: "Lan <x>", email: "lan@" };
    expect(validateIdentity(form, [])).toEqual({ name: "invalid", email: "invalid" });
    const dup = { ...emptyIdentity(false), name: "Lan", email: "LAN@acme.vn " };
    expect(validateIdentity(dup, [identity("lan@acme.vn")])).toEqual({ email: "duplicate" });
    expect(validateIdentity({ ...dup, email: "lan@home.vn" }, [identity("lan@acme.vn")])).toEqual(
      {},
    );
  });

  it("requires a key when signing", () => {
    const form = {
      ...emptyIdentity(false),
      name: "Lan",
      email: "lan@acme.vn",
      signing: "ssh" as const,
    };
    expect(validateIdentity(form, [])).toEqual({ signingKeyRef: "required" });
    expect(validateIdentity({ ...form, signingKeyRef: "~/.ssh/id.pub" }, [])).toEqual({});
  });
});

const rule = (id: string, priority: number, createdAt = 0): Rule => ({
  id,
  priority,
  host: "github.com",
  ownerOrGroup: null,
  pathGlob: null,
  remoteRegex: null,
  accountId: "acc_1",
  identityId: null,
  enabled: true,
  createdAt,
});

describe("rules", () => {
  it("sorts by priority then creation time", () => {
    const sorted = sortRules([rule("c", 2), rule("b", 1, 5), rule("a", 1, 1)]);
    expect(sorted.map((r) => r.id)).toEqual(["a", "b", "c"]);
  });

  it("moves a rule up/down and clamps at the ends", () => {
    expect(moveRule(["a", "b", "c"], 2, -1)).toEqual(["a", "c", "b"]);
    expect(moveRule(["a", "b", "c"], 0, 1)).toEqual(["b", "a", "c"]);
    expect(moveRule(["a", "b", "c"], 0, -1)).toEqual(["a", "b", "c"]);
    expect(moveRule(["a", "b", "c"], 2, 1)).toEqual(["a", "b", "c"]);
  });

  it("validates conditions and builds the input", () => {
    const empty = {
      host: " ",
      ownerOrGroup: "",
      pathGlob: "",
      remoteRegex: "",
      accountId: "",
      identityId: "",
      enabled: true,
    };
    expect(validateRule(empty)).toEqual({ match: "required", accountId: "required" });
    expect(validateRule({ ...empty, remoteRegex: "(", accountId: "acc_1" })).toEqual({
      remoteRegex: "invalid",
    });
    expect(ruleInput({ ...empty, host: " GitHub.com ", accountId: "acc_1" }, null)).toEqual({
      id: null,
      host: "github.com",
      ownerOrGroup: null,
      pathGlob: null,
      remoteRegex: null,
      accountId: "acc_1",
      identityId: null,
      enabled: true,
    });
  });
});

describe("colors", () => {
  it("picks readable text on account colors", () => {
    expect(textOn("#7048e8")).toBe("#ffffff");
    expect(textOn("#f08c00")).toBe("#111111");
    expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 0);
  });
});

const provider = (over: Partial<Provider>): Provider => ({
  id: "github",
  kind: "github",
  name: "GitHub",
  baseUrl: "https://github.com",
  apiUrl: "https://api.github.com",
  oauthClientId: null,
  largeFileWarnMb: 50,
  largeFileBlockMb: 100,
  builtin: true,
  ...over,
});

describe("wizard helpers", () => {
  it("validates and normalizes base URLs", () => {
    expect(validateBaseUrl("")).toBe("required");
    expect(validateBaseUrl("http://git.acme.vn")).toBe("insecure");
    expect(validateBaseUrl("http://127.0.0.1:8080")).toBeNull();
    expect(validateBaseUrl("https://u:p@git.acme.vn")).toBe("invalid");
    expect(validateBaseUrl("git.acme.vn/gitlab/")).toBeNull();
    expect(normalizeBaseUrl(" git.acme.vn/gitlab/ ")).toBe("https://git.acme.vn/gitlab");
    expect(instanceName("gitlabSelfHosted", "git.acme.vn")).toBe("GitLab git.acme.vn");
  });

  it("finds built-in and existing self-hosted providers", () => {
    const gl = provider({
      id: "prv_1",
      kind: "gitlabSelfHosted",
      baseUrl: "https://git.acme.vn",
      builtin: false,
    });
    const list = [provider({}), gl];
    expect(findProvider(list, "github", "")?.id).toBe("github");
    expect(findProvider(list, "gitlabSelfHosted", "GIT.acme.vn/")?.id).toBe("prv_1");
    expect(findProvider(list, "gitlabSelfHosted", "other.vn")).toBeNull();
    expect(findProvider(list, "gitlab", "")).toBeNull();
  });

  it("offers PKCE for GitLab only, PAT everywhere", () => {
    expect(methodsFor("github")).toEqual(["device", "pat"]);
    expect(methodsFor("gitlabSelfHosted")).toEqual(["pkce", "device", "pat"]);
  });

  it("checks tokens and formats the countdown", () => {
    expect(validateToken("  ")).toBe("required");
    expect(validateToken("ghp_a b")).toBe("malformed");
    expect(validateToken(" ghp_ok ")).toBeNull();
    expect(countdown(1000, 1000 * 1000 - 125_000)).toBe("2:05");
    expect(countdown(1000, 1000 * 1000)).toBeNull();
  });
});
