import { afterEach, describe, expect, it, vi } from "vitest";
import type { TFunction } from "i18next";
import { recentId, subSections, topLevelSections } from "./model";
import {
  collectCommands,
  currentProviders,
  registerCommands,
  registeredSources,
  type CommandProvider,
  type PaletteCommand,
  type PaletteContext,
} from "./registry";
import { pushRecent, readRecent } from "./store";

const ctx: PaletteContext = {
  t: ((k: string) => k) as unknown as TFunction,
  language: "vi",
  theme: "system",
  isMac: false,
  tabs: [],
  activeId: null,
  repo: null,
  refs: null,
  status: null,
  busy: false,
  recentRepos: [],
};

const cmd = (id: string, title: string, group = "G", over: Partial<PaletteCommand> = {}) =>
  ({ id, title, group, run: () => {}, ...over }) satisfies PaletteCommand;

const cleanups: (() => void)[] = [];
afterEach(() => {
  cleanups.splice(0).forEach((f) => f());
});

describe("registerCommands", () => {
  it("adds, replaces and removes a source", () => {
    const a: CommandProvider = () => [cmd("a", "A")];
    const b: CommandProvider = () => [cmd("b", "B")];
    const offA = registerCommands("test.src", a);
    expect(registeredSources()).toContain("test.src");
    const offB = registerCommands("test.src", b);
    cleanups.push(offB);
    // The stale unregister of the replaced provider is a no-op.
    offA();
    expect(registeredSources()).toContain("test.src");
    offB();
    expect(registeredSources()).not.toContain("test.src");
  });

  it("lets a real provider win over a fallback in either order", () => {
    const real: CommandProvider = () => [cmd("x.real", "Real")];
    const placeholder: CommandProvider = () => [cmd("x.placeholder", "Placeholder")];

    cleanups.push(registerCommands("test.fallback1", placeholder, { fallback: true }));
    cleanups.push(registerCommands("test.fallback1", real));
    cleanups.push(registerCommands("test.fallback2", real));
    cleanups.push(registerCommands("test.fallback2", placeholder, { fallback: true }));

    expect(currentProviders().filter((p) => p === real)).toHaveLength(2);
    expect(currentProviders()).not.toContain(placeholder);
  });

  it("registers the built-in accounts placeholders as a fallback", async () => {
    await import("./builtins");
    expect(registeredSources()).toEqual(
      expect.arrayContaining(["core.repo", "core.remote", "core.app", "accounts"]),
    );
    const { accountPlaceholders } = await import("./builtins");
    expect(currentProviders()).toContain(accountPlaceholders);
    const placeholders = accountPlaceholders(ctx);
    expect(placeholders.map((c) => c.id)).toEqual([
      "accounts.switch",
      "accounts.commitAs",
      "accounts.pushAs",
      "accounts.cloneAs",
    ]);
    expect(placeholders.every((c) => c.when?.(ctx) === false && c.hint)).toBe(true);

    const real: CommandProvider = () => [cmd("accounts.switch", "Real")];
    cleanups.push(registerCommands("accounts", real));
    // A later fallback registration (e.g. hot reload of builtins) does not undo the real one.
    registerCommands("accounts", accountPlaceholders, { fallback: true });
    expect(currentProviders()).toContain(real);
    expect(currentProviders()).not.toContain(accountPlaceholders);
  });
});

describe("collectCommands", () => {
  it("skips failing providers and duplicate ids", () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const out = collectCommands(
      [
        () => [cmd("a", "A"), cmd("b", "B")],
        () => {
          throw new Error("boom");
        },
        () => [cmd("a", "A again"), cmd("c", "C")],
      ],
      ctx,
    );
    expect(out.map((c) => c.title)).toEqual(["A", "B", "C"]);
    expect(error).toHaveBeenCalledOnce();
    error.mockRestore();
  });
});

describe("topLevelSections", () => {
  const theme = cmd("theme", "Giao diện…", "App", {
    run: undefined,
    flatten: true,
    children: () => [cmd("dark", "Tối", ""), cmd("light", "Sáng", "")],
  });
  const commands = [
    cmd("fetch", "Fetch", "Sync"),
    cmd("push", "Push", "Sync"),
    cmd("soon", "Chuyển tài khoản…", "Accounts", { when: () => false }),
    theme,
  ];

  it("groups by group in registration order, recent commands first", () => {
    const sections = topLevelSections(commands, ctx, "", ["push", "theme/dark", "gone"], "Recent");
    expect(sections.map((s) => s.group)).toEqual(["Recent", "Sync", "Accounts", "App"]);
    expect(sections[0]!.items.map((i) => i.title)).toEqual(["Push", "Giao diện: Tối"]);
    expect(sections[1]!.items.map((i) => i.key)).toEqual(["fetch"]);
  });

  it("does not list disabled commands as recent", () => {
    const sections = topLevelSections(commands, ctx, "", ["soon"], "Recent");
    expect(sections[0]!.group).toBe("Sync");
  });

  it("searches flattened children and marks disabled items", () => {
    const sections = topLevelSections(commands, ctx, "toi", [], "Recent");
    expect(sections.flatMap((s) => s.items.map((i) => i.title))).toEqual(["Giao diện: Tối"]);
    const soon = topLevelSections(commands, ctx, "tai khoan", [], "Recent")[0]!.items[0]!;
    expect(soon.enabled).toBe(false);
  });

  it("finds commands by id words and boosts recent ones", () => {
    const both = [
      cmd("remote.pull", "Kéo về", "Sync"),
      cmd("remote.pullRebase", "Kéo về (rebase)", "Sync"),
    ];
    const plain = topLevelSections(both, ctx, "pull", [], "R")[0]!.items.map((i) => i.key);
    expect(plain[0]).toBe("remote.pull");
    // Similar matches: the recently used one comes first.
    const twins = [cmd("pull.a", "Pull A", "Sync"), cmd("pull.b", "Pull B", "Sync")];
    const boosted = topLevelSections(twins, ctx, "pull", ["pull.b"], "R")[0]!.items;
    expect(boosted.map((i) => i.key)).toEqual(["pull.b", "pull.a"]);
  });
});

describe("subSections", () => {
  it("lists and filters the children of a step", () => {
    const parent = cmd("checkout", "Checkout…", "Branch", {
      children: () => [cmd("main", "main", ""), cmd("feature/đăng-nhập", "feature/đăng-nhập", "")],
    });
    expect(subSections(parent, ctx, "")[0]!.items).toHaveLength(2);
    expect(subSections(parent, ctx, "dang")[0]!.items.map((i) => i.title)).toEqual([
      "feature/đăng-nhập",
    ]);
    expect(subSections(parent, ctx, "zzz")).toEqual([]);
  });
});

describe("recent ids", () => {
  it("remembers flattened children individually and other sub-steps by parent", () => {
    const flat = cmd("theme", "Theme", "App", { flatten: true });
    const nested = cmd("checkout", "Checkout", "Branch");
    expect(recentId(cmd("dark", "Dark"), flat)).toBe("theme/dark");
    expect(recentId(cmd("main", "main"), nested)).toBe("checkout");
    expect(recentId(nested, null)).toBe("checkout");
  });

  it("persists newest first without duplicates", () => {
    pushRecent("a");
    pushRecent("b");
    pushRecent("a");
    expect(readRecent()).toEqual(["a", "b"]);
    localStorage.setItem("nanomgg.palette.recent", "not json");
    expect(readRecent()).toEqual([]);
  });
});
