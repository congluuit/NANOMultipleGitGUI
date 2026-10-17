/**
 * Dev-only fake backend so the web build (`vite`, no Tauri) shows the shell with a repository:
 * `http://localhost:1437/?mock=shell&lang=en`. Used by the layout screenshots of step 3B.5
 * (`.agent/phase3b/layout/`). Imported only behind `import.meta.env.DEV`, so release builds
 * drop it. Commands it does not know resolve to `null` and are listed in `window.__mockMisses`.
 */
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { GraphRow, RepoRefs, StatusEntry, WorkingTreeStatus, WorkspaceState } from "@/ipc";

const HEAD = "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678";
const hash = (i: number) => (i.toString(16).padStart(8, "0") + HEAD).slice(0, 40);

const workspace: WorkspaceState = {
  tabs: [
    { id: 1, path: "D:/Work/backend-api", name: "backend-api", missing: false },
    { id: 2, path: "D:/Work/game-assets", name: "game-assets", missing: false },
    {
      id: 3,
      path: "D:/Work/infrastructure-terraform",
      name: "infrastructure-terraform",
      missing: false,
    },
  ],
  activeId: 1,
};

const refs: RepoRefs = {
  head: { branch: "main", oid: hash(0), detached: false, unborn: false },
  local: ["main", "feature/login", "fix/token-refresh", "chore/deps"].map((name, i) => ({
    name,
    oid: hash(i * 3),
    upstream: i < 2 ? `origin/${name}` : null,
    ahead: i === 0 ? 1 : 0,
    behind: i === 1 ? 2 : 0,
    upstreamGone: false,
    isHead: i === 0,
  })),
  remotes: [
    {
      name: "origin",
      url: "https://github.com/acme/backend-api.git",
      branches: ["main", "feature/login", "release/1.2"].map((name, i) => ({
        name,
        oid: hash(i * 3 + 1),
      })),
    },
  ],
  tags: [{ name: "v1.2.0", oid: hash(5) }],
  stashes: [
    { index: 0, message: "WIP on main: experiment", time: 1_760_000_000, branch: "main" },
    {
      index: 1,
      message: "On feature/login: half-done form",
      time: 1_759_900_000,
      branch: "feature/login",
    },
  ],
};

const SUBJECTS = [
  "feat: add login",
  "fix: token refresh",
  "Merge branch 'feature/x'",
  "chore: deps",
  "docs: update README with setup steps for new contributors",
  "refactor: split account switcher",
];
const rows = (offset: number, limit: number): GraphRow[] =>
  Array.from({ length: Math.max(0, Math.min(limit, 200 - offset)) }, (_, k) => {
    const i = offset + k;
    return {
      index: i,
      hash: hash(i),
      subject: SUBJECTS[i % SUBJECTS.length] ?? "",
      authorName: i % 3 ? "Lan" : "Hoang",
      authorEmail: i % 3 ? "lan@acme.dev" : "hoang@acme.dev",
      authorTime: 1_760_000_000 - i * 3600,
      refs: i === 0 ? ["HEAD -> main", "origin/main"] : i === 5 ? ["tag: v1.2.0"] : [],
      isMerge: i % 6 === 2,
      lane: 0,
      color: 0,
      segments: [],
    };
  });

const entry = (path: string, kind: StatusEntry["kind"]): StatusEntry => ({
  path,
  oldPath: null,
  kind,
  conflict: null,
  submodule: false,
  size: 1200,
});
const status: WorkingTreeStatus = {
  branch: "main",
  headOid: hash(0),
  unborn: false,
  upstream: "origin/main",
  ahead: 1,
  behind: 0,
  staged: [entry("src/b.ts", "added")],
  unstaged: [
    entry("src/a.ts", "modified"),
    entry("assets/intro.mp4", "added"),
    entry("docs/very/long/path/to/some/deeply/nested/file-name.md", "modified"),
  ],
  conflicted: [],
};

const tool = (major: number, minor: number) => ({
  version: { major, minor, patch: 0 },
  raw: `${major}.${minor}.0`,
  minimum: { major: 2, minor: 0, patch: 0 },
  ok: true,
});

// The URL overrides of main.tsx (?lang=…&theme=…) win over the "saved" settings.
const params = new URLSearchParams(location.search);
const settings = { theme: params.get("theme") ?? "light", language: params.get("lang") };

const handlers: Record<string, (args: Record<string, unknown>) => unknown> = {
  get_settings: () => settings,
  update_settings: () => settings,
  workspace_state: () => workspace,
  workspace_set_active: (a) => {
    workspace.activeId = a.id as number;
    return null;
  },
  startup_args: () => ({ repo: null, benchGraph: false }),
  recent_repos: () => [],
  repo_refs: () => refs,
  graph_open: (a) => ({
    path: workspace.tabs.find((t) => t.id === a.repoId)?.path ?? "",
    total: 200,
    maxLanes: 1,
    loadMs: 5,
    refresh: "full",
    added: 0,
    version: 1,
  }),
  graph_rows: (a) => rows(a.offset as number, a.limit as number),
  graph_find: () => null,
  graph_search: () => [],
  worktree_status: () => status,
  undo_state: () => ({ undo: null, redo: null }),
  tool_status: () => ({ git: tool(2, 47), gitLfs: tool(3, 5) }),
  submodule_list: () => [],
  linked_worktree_list: () => [],
  accounts_list: () => [],
  identities_list: () => [],
  providers_list: () => [],
  repo_account: () => null,
};

declare global {
  interface Window {
    __mockMisses?: string[];
  }
}

export function installShellMock() {
  (globalThis as { isTauri?: boolean }).isTauri = true;
  window.__mockMisses = [];
  mockWindows("main");
  mockIPC(
    (cmd, args) => {
      const handler = handlers[cmd];
      if (handler) return handler((args ?? {}) as Record<string, unknown>);
      if (!window.__mockMisses?.includes(cmd)) window.__mockMisses?.push(cmd);
      return null;
    },
    { shouldMockEvents: true },
  );
}
