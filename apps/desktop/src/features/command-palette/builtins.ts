/** Built-in palette commands. Each group is its own source so it can be replaced separately. */
import {
  ArrowDownToLine,
  ArrowLeftRight,
  ArrowUpFromLine,
  Archive,
  ArchiveRestore,
  ChevronLeft,
  ChevronRight,
  CloudDownload,
  FolderGit2,
  FolderOpen,
  FolderPlus,
  GitBranch,
  GitBranchPlus,
  GitCommitHorizontal,
  GitMerge,
  Hash,
  History,
  Languages,
  LogIn,
  Palette,
  PencilLine,
  RefreshCw,
  Search,
  Settings,
  SquareTerminal,
  SunMoon,
  TriangleAlert,
  UserRound,
  X,
} from "lucide-react";
import { runOutcome } from "@/features/commits/outcome";
import { useRefDialog } from "@/features/commits/store";
import { pushPlan } from "@/features/remote/plan";
import {
  closeRepository,
  newRepositoryWithDialog,
  openRepository,
  openRepositoryWithDialog,
} from "@/features/repos/actions";
import { applyStash } from "@/features/stash/actions";
import { useStashUi } from "@/features/stash/store";
import { hasLocalChanges, topStash } from "@/features/stash/useStashToolbar";
import { useTerminal } from "@/features/terminal/store";
import { LANGUAGES } from "@/i18n";
import { commands } from "@/ipc";
import { useSettings } from "@/stores/settings";
import { useUi } from "@/stores/ui";
import { useWorkspace } from "@/stores/workspace";
import { THEME_MODES } from "@/theme/theme";
import {
  amendLastCommit,
  fetchAll,
  focusCommitSummary,
  focusGraphSearch,
  goToCommit,
  pull,
  push,
  requestForcePush,
  SHA_PATTERN,
} from "./actions";
import { modKey, registerCommands, type PaletteCommand, type PaletteContext } from "./registry";
// Step 3.3 (submodules, worktrees, LFS manager) registers "core.advanced" on import.
import "@/features/submodules/palette";

const hasRepo = (c: PaletteContext) => c.repo !== null;
const hasRemote = (c: PaletteContext) => !!c.refs && c.refs.remotes.length > 0;

function repoCommands(c: PaletteContext): PaletteCommand[] {
  const { t } = c;
  const group = t("cmd.group.repo");
  const openIds = new Set(c.tabs.map((tab) => tab.id));
  const recent = c.recentRepos.filter((r) => r.exists && !openIds.has(r.id));
  return [
    {
      id: "repo.open",
      title: t("cmd.repo.open"),
      group,
      icon: FolderOpen,
      keywords: ["open repository", "mo repo"],
      run: () => openRepositoryWithDialog(t("repos.pickOpen")),
    },
    {
      id: "repo.new",
      title: t("cmd.repo.new"),
      group,
      icon: FolderPlus,
      keywords: ["new repository", "init", "tao repo"],
      run: () => newRepositoryWithDialog(t("repos.pickNew")),
    },
    {
      id: "repo.recent",
      title: t("cmd.repo.recent"),
      group,
      icon: History,
      keywords: ["recent", "gan day"],
      when: () => recent.length > 0,
      flatten: true,
      children: () =>
        recent.map((r) => ({
          id: String(r.id),
          title: r.name,
          group: "",
          hint: r.path,
          icon: FolderGit2,
          run: () => openRepository(r.path),
        })),
    },
  ];
}

function tabCommands(c: PaletteContext): PaletteCommand[] {
  const { t, tabs, activeId } = c;
  const group = t("cmd.group.tabs");
  const index = tabs.findIndex((tab) => tab.id === activeId);
  const go = (step: number) => () => {
    const next = tabs[(index + step + tabs.length) % tabs.length];
    if (next) useWorkspace.getState().setActive(next.id);
  };
  return [
    {
      id: "tab.switch",
      title: t("cmd.tab.switch"),
      group,
      icon: ArrowLeftRight,
      keywords: ["switch tab", "go to tab", "chuyen tab"],
      when: () => tabs.length > 0,
      flatten: true,
      children: () =>
        tabs.map((tab, i) => ({
          id: String(tab.id),
          title: tab.name,
          group: "",
          hint: t("cmd.tab.number", { n: i + 1 }),
          icon: FolderGit2,
          checked: tab.id === activeId,
          run: () => useWorkspace.getState().setActive(tab.id),
        })),
    },
    {
      id: "tab.next",
      title: t("cmd.tab.next"),
      group,
      icon: ChevronRight,
      keywords: ["next tab"],
      when: () => tabs.length > 1,
      run: go(1),
    },
    {
      id: "tab.prev",
      title: t("cmd.tab.prev"),
      group,
      icon: ChevronLeft,
      keywords: ["previous tab"],
      when: () => tabs.length > 1,
      run: go(-1),
    },
    {
      id: "tab.close",
      title: t("cmd.tab.close"),
      group,
      icon: X,
      keywords: ["close tab", "dong tab"],
      when: () => activeId !== null,
      run: () => (activeId === null ? undefined : closeRepository(activeId)),
    },
  ];
}

function remoteCommands(c: PaletteContext): PaletteCommand[] {
  const { t, refs, repo } = c;
  if (!repo) return [];
  const id = repo.id;
  const group = t("cmd.group.sync");
  const canPull = (x: PaletteContext) => hasRemote(x) && !x.busy && !x.refs?.head.detached;
  const plan = refs ? pushPlan(refs) : null;
  const canPush = (x: PaletteContext) => !!plan && !x.busy;
  const target = plan ? `${plan.remote}/${plan.branch}` : undefined;
  return [
    {
      id: "remote.fetch",
      title: t("cmd.remote.fetch"),
      group,
      icon: RefreshCw,
      keywords: ["fetch", "remote", "dong bo"],
      when: (x) => hasRemote(x) && !x.busy,
      run: () => void fetchAll(id),
    },
    {
      id: "remote.pull",
      title: t("cmd.remote.pull"),
      group,
      icon: ArrowDownToLine,
      keywords: ["pull", "merge", "keo ve"],
      when: canPull,
      run: () => void pull(id, "merge"),
    },
    {
      id: "remote.pullRebase",
      title: t("cmd.remote.pullRebase"),
      group,
      icon: ArrowDownToLine,
      keywords: ["pull", "rebase"],
      when: canPull,
      run: () => void pull(id, "rebase"),
    },
    {
      id: "remote.pullFfOnly",
      title: t("cmd.remote.pullFfOnly"),
      group,
      icon: ArrowDownToLine,
      keywords: ["pull", "fast-forward", "ff-only"],
      when: canPull,
      run: () => void pull(id, "fastForwardOnly"),
    },
    {
      id: "remote.push",
      title: t("cmd.remote.push"),
      group,
      icon: ArrowUpFromLine,
      hint: target,
      keywords: ["push", "day len"],
      when: canPush,
      run: () => (refs ? push(id, refs, false) : undefined),
    },
    {
      id: "remote.forcePush",
      title: t("cmd.remote.forcePush"),
      group,
      icon: TriangleAlert,
      hint: target,
      destructive: true,
      keywords: ["force push", "force-with-lease"],
      when: canPush,
      run: () => (refs ? requestForcePush(id, refs) : undefined),
    },
  ];
}

function branchCommands(c: PaletteContext): PaletteCommand[] {
  const { t, refs, repo } = c;
  if (!repo) return [];
  const id = repo.id;
  const group = t("cmd.group.branch");
  const head = refs?.head;
  const current = head && !head.detached ? head.branch : null;
  const local = (refs?.local ?? []).filter((b) => !b.isHead);
  const remote = (refs?.remotes ?? []).flatMap((r) =>
    r.branches.filter((b) => b.name !== "HEAD").map((b) => `${r.name}/${b.name}`),
  );
  const branchItem = (name: string, isRemote: boolean, run: () => void): PaletteCommand => ({
    id: `${isRemote ? "remote" : "local"}:${name}`,
    title: name,
    group: "",
    icon: isRemote ? CloudDownload : GitBranch,
    hint: isRemote ? t("cmd.branch.remote") : undefined,
    run,
  });

  return [
    {
      id: "branch.create",
      title: t("cmd.branch.create"),
      group,
      icon: GitBranchPlus,
      keywords: ["create branch", "new branch", "tao nhanh"],
      when: () => !!head?.oid,
      run: () =>
        head?.oid &&
        useRefDialog.getState().open({
          kind: "createBranch",
          repoId: id,
          start: head.oid,
          label: head.branch ?? head.oid.slice(0, 7),
        }),
    },
    {
      id: "branch.checkout",
      title: t("cmd.branch.checkout"),
      group,
      icon: LogIn,
      keywords: ["checkout", "switch branch", "chuyen nhanh"],
      when: () => local.length + remote.length > 0,
      children: () => [
        ...local.map((b) =>
          branchItem(
            b.name,
            false,
            () =>
              void runOutcome(
                id,
                () => commands.branchCheckout(id, b.name),
                () => t("ops.checkedOut", { name: b.name }),
              ),
          ),
        ),
        ...remote.map((full) => {
          const slash = full.indexOf("/");
          return branchItem(
            full,
            true,
            () =>
              void runOutcome(
                id,
                () =>
                  commands.branchCheckoutRemote(
                    id,
                    full.slice(0, slash),
                    full.slice(slash + 1),
                    null,
                  ),
                (name) => t("ops.checkedOut", { name }),
              ),
          );
        }),
      ],
    },
    {
      id: "branch.merge",
      title: t("cmd.branch.merge", { branch: current ?? "HEAD" }),
      group,
      icon: GitMerge,
      keywords: ["merge", "gop nhanh"],
      when: () => current !== null && local.length + remote.length > 0,
      children: () =>
        [
          ...local.map((b) => [b.name, false] as const),
          ...remote.map((r) => [r, true] as const),
        ].map(([name, isRemote]) =>
          branchItem(name, isRemote, () =>
            useRefDialog.getState().open({ kind: "merge", repoId: id, rev: name, into: current }),
          ),
        ),
    },
  ];
}

function stashCommands(c: PaletteContext): PaletteCommand[] {
  const { t, repo } = c;
  if (!repo) return [];
  const id = repo.id;
  const group = t("cmd.group.stash");
  const top = topStash(c.refs?.stashes);
  return [
    {
      id: "stash.create",
      title: t("cmd.stash.create"),
      group,
      icon: Archive,
      keywords: ["stash", "cat tam"],
      when: (x) => hasLocalChanges(x.status ?? undefined),
      run: () => useStashUi.getState().openCreate(id),
    },
    {
      id: "stash.pop",
      title: t("cmd.stash.pop"),
      group,
      icon: ArchiveRestore,
      hint: top?.message,
      keywords: ["stash pop", "unstash"],
      when: () => top !== null,
      run: () => (top ? applyStash(id, top.index, true) : undefined),
    },
  ];
}

function commitCommands(c: PaletteContext): PaletteCommand[] {
  const { t, repo } = c;
  if (!repo) return [];
  const id = repo.id;
  const group = t("cmd.group.commit");
  return [
    {
      id: "commit.focus",
      title: t("cmd.commit.focus"),
      group,
      icon: GitCommitHorizontal,
      keywords: ["commit", "message"],
      run: () => void focusCommitSummary(id),
    },
    {
      id: "commit.amend",
      title: t("cmd.commit.amend"),
      group,
      icon: PencilLine,
      keywords: ["amend", "sua commit"],
      when: (x) => !!x.refs?.head.oid,
      run: () => void amendLastCommit(id),
    },
  ];
}

function historyCommands(c: PaletteContext): PaletteCommand[] {
  const { t, repo } = c;
  if (!repo) return [];
  const id = repo.id;
  const group = t("cmd.group.history");
  return [
    {
      id: "graph.goto",
      title: t("cmd.graph.goto"),
      group,
      icon: Hash,
      keywords: ["go to commit", "sha", "hash", "jump"],
      input: {
        placeholder: t("cmd.graph.gotoPlaceholder"),
        validate: (v) => (SHA_PATTERN.test(v.trim()) ? null : t("cmd.graph.gotoInvalid")),
        run: (v) => goToCommit(id, v),
      },
    },
    {
      id: "graph.search",
      title: t("cmd.graph.search"),
      group,
      icon: Search,
      shortcut: `${modKey(c)}+F`,
      keywords: ["search commits", "find", "tim commit"],
      run: () => void focusGraphSearch(id),
    },
  ];
}

function appCommands(c: PaletteContext): PaletteCommand[] {
  const { t } = c;
  const group = t("cmd.group.app");
  const themeLabel = {
    light: "settings.themeLight",
    dark: "settings.themeDark",
    system: "settings.themeSystem",
  } as const;
  return [
    {
      id: "terminal.toggle",
      title: t("cmd.app.terminal"),
      group,
      icon: SquareTerminal,
      shortcut: "Ctrl+`",
      keywords: ["terminal", "shell", "console"],
      when: hasRepo,
      run: (x) => x.repo && useTerminal.getState().toggle(x.repo.id),
    },
    {
      id: "theme",
      title: t("cmd.app.theme"),
      group,
      icon: SunMoon,
      keywords: ["theme", "giao dien"],
      flatten: true,
      children: (x) =>
        THEME_MODES.map((mode) => ({
          id: mode,
          title: t(themeLabel[mode]),
          group: "",
          keywords: [mode],
          checked: x.theme === mode,
          run: () => useSettings.getState().setTheme(mode),
        })),
    },
    {
      id: "language",
      title: t("cmd.app.language"),
      group,
      icon: Languages,
      keywords: ["language", "ngon ngu"],
      flatten: true,
      children: (x) =>
        LANGUAGES.map((l) => ({
          id: l.code,
          title: l.label,
          group: "",
          checked: x.language === l.code,
          run: () => useSettings.getState().setLanguage(l.code),
        })),
    },
    {
      id: "app.settings",
      title: t("cmd.app.settings"),
      group,
      icon: Settings,
      keywords: ["settings", "preferences", "cai dat"],
      run: () => useUi.getState().setSettingsOpen(true),
    },
    {
      id: "app.palette",
      title: t("cmd.app.colorPalette"),
      group,
      icon: Palette,
      keywords: ["color palette", "design tokens", "bang mau"],
      run: () => useUi.getState().setView("palette"),
    },
  ];
}

/**
 * Placeholders for PLAN §4.5, shown disabled until the accounts feature registers the real
 * commands under the same source: `registerCommands("accounts", provider)` replaces these.
 */
export function accountPlaceholders(c: PaletteContext): PaletteCommand[] {
  const { t } = c;
  const group = t("cmd.group.accounts");
  const soon = t("cmd.comingSoon");
  const item = (
    id: "switch" | "commitAs" | "pushAs" | "cloneAs",
    keywords: string[],
  ): PaletteCommand => ({
    id: `accounts.${id}`,
    title: t(`cmd.accounts.${id}`),
    group,
    icon: UserRound,
    hint: soon,
    keywords,
    when: () => false,
  });
  return [
    item("switch", ["switch account", "chuyen tai khoan"]),
    item("commitAs", ["commit as"]),
    item("pushAs", ["push as"]),
    item("cloneAs", ["clone as"]),
  ];
}

registerCommands("core.repo", repoCommands);
registerCommands("core.tabs", tabCommands);
registerCommands("core.remote", remoteCommands);
registerCommands("core.branch", branchCommands);
registerCommands("core.stash", stashCommands);
registerCommands("core.commit", commitCommands);
registerCommands("core.history", historyCommands);
registerCommands("core.app", appCommands);
registerCommands("accounts", accountPlaceholders, { fallback: true });
