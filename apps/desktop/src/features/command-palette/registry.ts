/**
 * Command registry of the command palette (`Ctrl/Cmd+K`).
 *
 * Any feature can contribute commands without touching the palette:
 *
 * ```ts
 * // src/features/accounts/commands.ts
 * import { UserRound } from "lucide-react";
 * import { registerCommands } from "@/features/command-palette/registry";
 *
 * registerCommands("accounts", (ctx) => [
 *   {
 *     id: "accounts.switch",
 *     title: ctx.t("accounts.switch"),
 *     group: ctx.t("cmd.group.accounts"),
 *     icon: UserRound,
 *     when: (c) => c.repo !== null,
 *     children: (c) => myAccounts().map((a) => ({
 *       id: a.id,
 *       title: a.label,
 *       group: "",
 *       run: () => bindAccount(c.repo!.id, a.id),
 *     })),
 *   },
 * ]);
 * ```
 *
 * then add `import "@/features/accounts/commands";` to `command-palette/index.ts` (import from
 * `registry` rather than the index to avoid an import cycle).
 *
 * - A provider is a plain function (no hooks) called each time the palette renders, with the
 *   current {@link PaletteContext}. Return `[]` when nothing applies.
 * - Registering the same `source` again replaces the previous provider (the returned function
 *   unregisters it). Built-in placeholders use `{ fallback: true }`, so a real provider for the
 *   same source wins whatever the import order is.
 */
import { useSyncExternalStore } from "react";
import type { LucideIcon } from "lucide-react";
import type { TFunction } from "i18next";
import type { Language } from "@/i18n";
import type { RecentRepo, RepoRefs, RepoTab, WorkingTreeStatus } from "@/ipc";
import type { ThemeMode } from "@/theme/theme";

/** Everything a provider may need, gathered by the palette when it opens. */
export interface PaletteContext {
  t: TFunction;
  language: Language;
  theme: ThemeMode;
  isMac: boolean;
  tabs: RepoTab[];
  activeId: number | null;
  /** Active repository when it is usable (open and its folder exists), else `null`. */
  repo: RepoTab | null;
  /** Refs of `repo` (`null` while loading or without a repo). */
  refs: RepoRefs | null;
  /** Working tree status of `repo`. */
  status: WorkingTreeStatus | null;
  /** A fetch/pull/push/LFS transfer is running for `repo`. */
  busy: boolean;
  recentRepos: RecentRepo[];
}

/** Free-text step (e.g. "Go to commit by SHA…" asks for the SHA). */
export interface PaletteInput {
  placeholder: string;
  /** Error text for the current value, `null` when it can be submitted. */
  validate?: (value: string) => string | null;
  /** May return a promise; a rejection is shown in the notice bar. */
  run: (value: string, ctx: PaletteContext) => unknown;
}

export interface PaletteCommand {
  /** Unique and stable (recent commands are remembered by id). */
  id: string;
  title: string;
  /** Heading the command is listed under (already translated). Ignored inside a sub-list. */
  group: string;
  /** Extra search terms (English words are useful in the Vietnamese UI and vice versa). */
  keywords?: readonly string[];
  /** Hint shown on the right, e.g. `Ctrl+`` ` (keys joined by `+`). Display only. */
  shortcut?: string;
  icon?: LucideIcon;
  /** Muted text after the title ("coming soon", "current", a path…). */
  hint?: string;
  /** Marks the current choice in a sub-list (theme, language…). */
  checked?: boolean;
  destructive?: boolean;
  /** Enabled state; disabled commands stay visible but cannot run. Default: enabled. */
  when?: (ctx: PaletteContext) => boolean;
  /** Runs as the palette closes. May return a promise; a rejection is shown in the notice bar. */
  run?: (ctx: PaletteContext) => unknown;
  /** Second step: a sub-list (takes precedence over `run`). */
  children?: (ctx: PaletteContext) => PaletteCommand[];
  /** Second step: a text prompt. */
  input?: PaletteInput;
  /** Also offer the children in top-level search results as "Title: Child" (small lists). */
  flatten?: boolean;
}

export type CommandProvider = (ctx: PaletteContext) => PaletteCommand[];

interface Entry {
  provider: CommandProvider;
  fallback: boolean;
}

const entries = new Map<string, Entry>();
let snapshot: readonly CommandProvider[] = [];
const listeners = new Set<() => void>();

function changed() {
  snapshot = [...entries.values()].map((e) => e.provider);
  listeners.forEach((l) => l());
}

/**
 * Registers (or replaces) the commands of `source`. Returns a function that unregisters them.
 * With `fallback: true` the provider is only used until a non-fallback one registers.
 */
export function registerCommands(
  source: string,
  provider: CommandProvider,
  options: { fallback?: boolean } = {},
): () => void {
  const fallback = options.fallback ?? false;
  const existing = entries.get(source);
  if (fallback && existing && !existing.fallback) return () => {};
  entries.set(source, { provider, fallback });
  changed();
  return () => {
    if (entries.get(source)?.provider === provider) {
      entries.delete(source);
      changed();
    }
  };
}

/** Registered sources (for tests and debugging). */
export const registeredSources = () => [...entries.keys()];

/** Active providers, in registration order. */
export const currentProviders = () => snapshot;

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};

/** Current providers; re-renders when a source registers or unregisters. */
export const useCommandProviders = () => useSyncExternalStore(subscribe, currentProviders);

/** Calls every provider; a failing provider is logged and skipped. Duplicate ids keep the first. */
export function collectCommands(
  providers: readonly CommandProvider[],
  ctx: PaletteContext,
): PaletteCommand[] {
  const seen = new Set<string>();
  const out: PaletteCommand[] = [];
  for (const provider of providers) {
    let list: PaletteCommand[];
    try {
      list = provider(ctx);
    } catch (e) {
      console.error("command palette provider failed", e);
      continue;
    }
    for (const cmd of list) {
      if (seen.has(cmd.id)) continue;
      seen.add(cmd.id);
      out.push(cmd);
    }
  }
  return out;
}

/** Commands of every registered source for `ctx`. */
export function usePaletteCommands(ctx: PaletteContext): PaletteCommand[] {
  return collectCommands(useCommandProviders(), ctx);
}

export const isEnabled = (cmd: PaletteCommand, ctx: PaletteContext) => !cmd.when || cmd.when(ctx);

/** `Ctrl` or `⌘` for shortcut hints. */
export const modKey = (ctx: Pick<PaletteContext, "isMac">) => (ctx.isMac ? "⌘" : "Ctrl");
