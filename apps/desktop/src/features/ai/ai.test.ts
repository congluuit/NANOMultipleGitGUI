import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import i18n, { initI18n } from "@/i18n";
import type { AiError } from "@/ipc";
import { EMPTY_DRAFT, useWorktreeStore } from "@/stores/worktree";
import {
  DEFAULT_CONFIG,
  errorFix,
  formatGb,
  hasText,
  insecureRemote,
  isLocalUrl,
  normalizeConfig,
  sameOllamaModel,
  sendsCodeAway,
  serviceName,
} from "./model";
import { useAiUi } from "./store";
import { aiErrorNotice, aiErrorText, applySuggestion, undoSuggestion } from "./suggest";

beforeAll(() => {
  initI18n("vi");
});

describe("AI config model", () => {
  it("fills defaults for a partial config", () => {
    const c = normalizeConfig({ enabled: true, models: { openai: "gpt-x" } });
    expect(c.enabled).toBe(true);
    expect(c.provider).toBe("ollama");
    expect(c.models.openai).toBe("gpt-x");
    expect(c.models.ollama).toBe("qwen2.5-coder:1.5b");
    expect(normalizeConfig(null)).toEqual(DEFAULT_CONFIG);
  });

  it("knows when code leaves the computer", () => {
    expect(sendsCodeAway({ ...DEFAULT_CONFIG, provider: "ollama" })).toBe(false);
    expect(sendsCodeAway({ ...DEFAULT_CONFIG, provider: "openai" })).toBe(true);
    const compat = { ...DEFAULT_CONFIG, provider: "compatible" as const };
    expect(sendsCodeAway({ ...compat, compatibleBaseUrl: "http://localhost:1234/v1" })).toBe(false);
    expect(sendsCodeAway({ ...compat, compatibleBaseUrl: "https://openrouter.ai/api/v1" })).toBe(
      true,
    );
    expect(serviceName({ ...compat, compatibleBaseUrl: "https://openrouter.ai/api/v1" })).toBe(
      "openrouter.ai",
    );
    expect(isLocalUrl("http://127.0.0.1:11434")).toBe(true);
    expect(isLocalUrl("http://[::1]:8000/v1")).toBe(true);
    expect(isLocalUrl("https://api.groq.com/openai/v1")).toBe(false);
    expect(insecureRemote("http://example.com/v1")).toBe(true);
    expect(insecureRemote("http://localhost:1234/v1")).toBe(false);
    expect(insecureRemote("https://example.com/v1")).toBe(false);
  });

  it("matches Ollama model names with an implicit :latest", () => {
    expect(sameOllamaModel("llama3.2", "llama3.2:latest")).toBe(true);
    expect(sameOllamaModel("llama3.2:1b", "llama3.2:3b")).toBe(false);
    expect(formatGb(1)).toBe("1 GB");
    expect(formatGb(1.3)).toBe("1.3 GB");
    expect(formatGb(0.8)).toBe("800 MB");
  });

  it("offers the right quick fix for each error", () => {
    const e = (kind: string, extra: object = {}) => ({ kind, message: "x", ...extra }) as AiError;
    expect(errorFix(e("invalidKey"))).toBe("settings");
    expect(errorFix(e("ollamaNotRunning"))).toBe("settings");
    expect(errorFix(e("modelNotFound", { model: "m" }))).toBe("settings");
    expect(errorFix(e("rateLimited"))).toBe("retry");
    expect(errorFix(e("network"))).toBe("retry");
    expect(errorFix(e("nothingStaged", { hasUnstaged: true }))).toBe("unstaged");
    expect(errorFix(e("nothingStaged", { hasUnstaged: false }))).toBeNull();
    expect(errorFix(e("git"))).toBeNull();
  });
});

describe("AI errors in plain words", () => {
  it("explains a wrong key and links to the settings", () => {
    const n = aiErrorNotice(
      i18n.t,
      { kind: "invalidKey", message: "Incorrect API key provided" },
      vi.fn(),
      false,
    );
    expect(n?.kind).toBe("error");
    expect(n?.message).toContain("API key không đúng");
    expect(n?.detail).toBe("Incorrect API key provided");
    expect(n?.actions?.[0]?.label).toBe("Mở cài đặt AI");
  });

  it("offers the unstaged changes when nothing is staged", () => {
    const retry = vi.fn();
    const n = aiErrorNotice(
      i18n.t,
      { kind: "nothingStaged", message: "nothing", hasUnstaged: true },
      retry,
      false,
    );
    expect(n?.kind).toBe("info");
    expect(n?.message).toContain("Hãy stage thay đổi trước");
    n?.actions?.[0]?.run();
    expect(retry).toHaveBeenCalledWith(true);
  });

  it("stays silent on cancel and interpolates details", () => {
    expect(aiErrorNotice(i18n.t, { kind: "cancelled", message: "" }, vi.fn(), false)).toBeNull();
    expect(
      aiErrorText(i18n.t, { kind: "modelNotFound", message: "", model: "gemma3:1b" }),
    ).toContain("gemma3:1b");
    expect(aiErrorText(i18n.t, { kind: "service", message: "", status: 503 })).toContain("503");
    expect(aiErrorText(i18n.t, "boom")).toBe("boom");
  });
});

describe("filling the draft", () => {
  beforeEach(() => {
    useWorktreeStore.setState({ drafts: {} });
    useAiUi.setState({ undo: {}, generating: {} });
  });

  it("replaces the draft once and can undo it", () => {
    useWorktreeStore.getState().updateDraft(7, { summary: "wip", body: "notes" });
    applySuggestion(7, { summary: "feat: add login", body: "- form" });
    const d = useWorktreeStore.getState().drafts[7] ?? EMPTY_DRAFT;
    expect(d.summary).toBe("feat: add login");
    expect(d.body).toBe("- form");
    expect(useAiUi.getState().undo[7]?.previous).toEqual({ summary: "wip", body: "notes" });

    undoSuggestion(7);
    const back = useWorktreeStore.getState().drafts[7] ?? EMPTY_DRAFT;
    expect(back.summary).toBe("wip");
    expect(back.body).toBe("notes");
    expect(useAiUi.getState().undo[7]).toBeUndefined();
    expect(hasText(back)).toBe(true);
  });
});
