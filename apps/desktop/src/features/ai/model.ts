import type { AiConfig, AiError, AiModels, AiProvider } from "@/ipc";

/** `AiConfig` with every field present (the backend fills defaults; TS sees them optional). */
export type FullAiConfig = Required<Omit<AiConfig, "models">> & { models: Required<AiModels> };

export const DEFAULT_OLLAMA_URL = "http://127.0.0.1:11434";
export const OLLAMA_DOWNLOAD_URL = "https://ollama.com/download";
export const OLLAMA_LINUX_COMMAND = "curl -fsSL https://ollama.com/install.sh | sh";

export const DEFAULT_MODELS: Required<AiModels> = {
  ollama: "qwen2.5-coder:1.5b",
  openai: "gpt-4.1-mini",
  anthropic: "claude-haiku-4-5",
  gemini: "gemini-2.5-flash-lite",
  compatible: "",
};

export const DEFAULT_CONFIG: FullAiConfig = {
  enabled: false,
  provider: "ollama",
  models: DEFAULT_MODELS,
  compatibleBaseUrl: "",
  ollamaUrl: DEFAULT_OLLAMA_URL,
  language: "app",
  style: "conventional",
  includeBody: true,
};

export function normalizeConfig(config: AiConfig | null | undefined): FullAiConfig {
  const c = config ?? {};
  return {
    ...DEFAULT_CONFIG,
    ...Object.fromEntries(Object.entries(c).filter(([, v]) => v !== undefined && v !== null)),
    models: {
      ...DEFAULT_MODELS,
      ...Object.fromEntries(
        Object.entries(c.models ?? {}).filter(([, v]) => v !== undefined && v !== null),
      ),
    },
  } as FullAiConfig;
}

/** Order of the service cards. Ollama first: free and private. */
export const PROVIDERS: readonly AiProvider[] = [
  "ollama",
  "openai",
  "anthropic",
  "gemini",
  "compatible",
];

export type CloudProvider = Exclude<AiProvider, "ollama">;

export const isCloudProvider = (p: AiProvider): p is CloudProvider => p !== "ollama";

export interface ModelSuggestion {
  id: string;
  note: "fastCheap" | "cheapest" | "smarter" | "openrouter" | "groq" | "lmstudio";
}

/** Cheap / fast defaults first; the user may type any other model. */
export const CLOUD_MODELS: Record<CloudProvider, readonly ModelSuggestion[]> = {
  openai: [
    { id: "gpt-4.1-mini", note: "fastCheap" },
    { id: "gpt-4.1-nano", note: "cheapest" },
    { id: "gpt-4o-mini", note: "fastCheap" },
    { id: "gpt-5-mini", note: "smarter" },
  ],
  anthropic: [
    { id: "claude-haiku-4-5", note: "fastCheap" },
    { id: "claude-3-5-haiku-latest", note: "cheapest" },
    { id: "claude-sonnet-4-5", note: "smarter" },
  ],
  gemini: [
    { id: "gemini-2.5-flash-lite", note: "fastCheap" },
    { id: "gemini-2.0-flash", note: "fastCheap" },
    { id: "gemini-2.5-flash", note: "smarter" },
  ],
  compatible: [
    { id: "openai/gpt-4.1-mini", note: "openrouter" },
    { id: "llama-3.1-8b-instant", note: "groq" },
    { id: "qwen2.5-coder-7b-instruct", note: "lmstudio" },
  ],
};

export const COMPATIBLE_PRESETS: readonly { label: string; url: string }[] = [
  { label: "OpenRouter", url: "https://openrouter.ai/api/v1" },
  { label: "Groq", url: "https://api.groq.com/openai/v1" },
  { label: "LM Studio", url: "http://localhost:1234/v1" },
  { label: "vLLM", url: "http://localhost:8000/v1" },
];

export const KEY_PAGES: Record<Exclude<CloudProvider, "compatible">, string> = {
  openai: "https://platform.openai.com/api-keys",
  anthropic: "https://console.anthropic.com/settings/keys",
  gemini: "https://aistudio.google.com/apikey",
};

export interface OllamaSuggestion {
  name: string;
  /** Download size in GB. */
  sizeGb: number;
  /** Memory the computer should have, in GB. */
  ramGb: number;
  recommended?: boolean;
  note: "qwenSmall" | "llamaSmall" | "gemma" | "qwenLarge" | "llamaLarge";
}

/** Light models that write good enough commit messages. */
export const OLLAMA_SUGGESTED: readonly OllamaSuggestion[] = [
  { name: "qwen2.5-coder:1.5b", sizeGb: 1, ramGb: 4, recommended: true, note: "qwenSmall" },
  { name: "llama3.2:1b", sizeGb: 1.3, ramGb: 4, note: "llamaSmall" },
  { name: "gemma3:1b", sizeGb: 0.8, ramGb: 3, note: "gemma" },
  { name: "qwen2.5-coder:3b", sizeGb: 1.9, ramGb: 8, note: "qwenLarge" },
  { name: "llama3.2:3b", sizeGb: 2, ramGb: 8, note: "llamaLarge" },
];

/** `llama3.2` is the same as `llama3.2:latest`. */
export function sameOllamaModel(a: string, b: string): boolean {
  const norm = (s: string) => (s.includes(":") ? s : `${s}:latest`);
  return norm(a.trim()) === norm(b.trim());
}

export function isLocalUrl(url: string): boolean {
  const m = /^https?:\/\/(\[[^\]]+\]|[^/:?#]+)/i.exec(url.trim());
  if (!m) return false;
  const host = (m[1] ?? "").toLowerCase().replace(/^\[|\]$/g, "");
  return (
    host === "localhost" ||
    host === "::1" ||
    host === "0.0.0.0" ||
    host.startsWith("127.") ||
    host.endsWith(".localhost")
  );
}

export function hostOf(url: string): string {
  const m = /^https?:\/\/(?:[^@/]*@)?(\[[^\]]+\]|[^/:?#]+)/i.exec(url.trim());
  return m?.[1] ?? url.trim();
}

/** The staged diff leaves this computer (show the privacy warning). */
export function sendsCodeAway(config: FullAiConfig): boolean {
  if (config.provider === "ollama") return false;
  if (config.provider === "compatible") return !isLocalUrl(config.compatibleBaseUrl);
  return true;
}

/** Name of the service for messages ("OpenAI", "openrouter.ai", ...). */
export function serviceName(config: FullAiConfig): string {
  switch (config.provider) {
    case "ollama":
      return "Ollama";
    case "openai":
      return "OpenAI";
    case "anthropic":
      return "Anthropic (Claude)";
    case "gemini":
      return "Google Gemini";
    case "compatible":
      return config.compatibleBaseUrl ? hostOf(config.compatibleBaseUrl) : "API";
  }
}

export const modelOf = (config: FullAiConfig) => config.models[config.provider];

/** Plain `http://` to another machine with a key: the backend refuses it. */
export function insecureRemote(url: string): boolean {
  return /^http:\/\//i.test(url.trim()) && !isLocalUrl(url);
}

export function isAiError(e: unknown): e is AiError {
  return (
    typeof e === "object" &&
    e !== null &&
    "kind" in e &&
    "message" in e &&
    typeof (e as { kind: unknown }).kind === "string"
  );
}

/** What the commit panel offers next to an error. */
export type AiFix = "settings" | "unstaged" | "retry" | null;

const SETTINGS_KINDS = new Set<AiError["kind"]>([
  "notEnabled",
  "missingKey",
  "missingModel",
  "missingBaseUrl",
  "invalidKey",
  "accessDenied",
  "quotaExceeded",
  "modelNotFound",
  "ollamaNotRunning",
  "ollamaNotInstalled",
  "invalidInput",
]);

const RETRY_KINDS = new Set<AiError["kind"]>([
  "rateLimited",
  "network",
  "timeout",
  "service",
  "emptyAnswer",
]);

export function errorFix(e: AiError): AiFix {
  if (e.kind === "nothingStaged") return e.hasUnstaged ? "unstaged" : null;
  if (SETTINGS_KINDS.has(e.kind)) return "settings";
  if (RETRY_KINDS.has(e.kind)) return "retry";
  return null;
}

/** Kinds whose `message` is technical detail worth showing collapsed. */
export function errorDetail(e: AiError): string | undefined {
  if (
    e.kind === "nothingStaged" ||
    e.kind === "notEnabled" ||
    e.kind === "cancelled" ||
    e.kind === "invalidInput"
  ) {
    return undefined;
  }
  return e.message || undefined;
}

export interface DraftText {
  summary: string;
  body: string;
}

export const hasText = (d: DraftText) => d.summary.trim() !== "" || d.body.trim() !== "";

export const sameText = (a: DraftText, b: DraftText) =>
  a.summary === b.summary && a.body === b.body;

export function newRequestId(prefix = "ai"): string {
  const c = globalThis.crypto as Crypto | undefined;
  const id =
    typeof c?.randomUUID === "function"
      ? c.randomUUID()
      : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
  return `${prefix}-${id}`;
}

/** `1.3 GB`, `850 MB` for the model sizes. */
export function formatGb(gb: number): string {
  return gb >= 1 ? `${gb.toFixed(1).replace(/\.0$/, "")} GB` : `${Math.round(gb * 1000)} MB`;
}
