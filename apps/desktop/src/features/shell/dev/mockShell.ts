/**
 * Dev-only fake backend so the web build (`vite`, no Tauri) shows the shell with a repository:
 * `http://localhost:1437/?mock=shell&lang=en`. Used by the layout screenshots of step 3B.5
 * (`.agent/phase3b/layout/`). Imported only behind `import.meta.env.DEV`, so release builds
 * drop it. Commands it does not know resolve to `null` and are listed in `window.__mockMisses`.
 */
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { accountHandlers } from "./mockAccounts";
import { aiHandlers } from "./mockAi";
import type {
  DiffRequest,
  GraphRow,
  RepoRefs,
  StatusEntry,
  WorkingTreeStatus,
  WorkspaceState,
} from "@/ipc";

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

// Index and working-tree text of the changed sample file, for the diff and file editor views.
const lines = (...l: string[]) => l.join(String.fromCharCode(10));
const files: Record<string, { index: string; worktree: string }> = {
  "src/a.ts": {
    index: lines(
      'import { fetchToken } from "./auth";',
      "",
      "export interface Session {",
      "  user: string;",
      "  token: string;",
      "  expires: number;",
      "}",
      "",
      "export async function login(user: string, password: string): Promise<Session> {",
      "  const token = await fetchToken(user, password);",
      "  return { user, token, expires: Date.now() + 3600_000 };",
      "}",
      "",
      "export function isExpired(session: Session): boolean {",
      "  return session.expires < Date.now();",
      "}",
      "",
      "// Old helper kept for the CLI.",
      "export function describe(session: Session): string {",
      "  return `${session.user} (${session.token.slice(0, 4)}...)`;",
      "}",
      "",
    ),
    worktree: lines(
      'import { fetchToken } from "./auth";',
      "",
      "const SESSION_MS = 60 * 60 * 1000;",
      "",
      "export interface Session {",
      "  user: string;",
      "  token: string;",
      "  expires: number;",
      "}",
      "",
      "export async function login(user: string, password: string): Promise<Session> {",
      "  const token = await fetchToken(user, password);",
      "  return { user, token, expires: Date.now() + SESSION_MS };",
      "}",
      "",
      "export function isExpired(session: Session): boolean {",
      "  // A minute of margin for clock drift.",
      "  return session.expires - 60_000 < Date.now();",
      "}",
      "",
      "export function describe(session: Session): string {",
      "  return `${session.user} (${session.token.slice(0, 4)}...)`;",
      "}",
      "",
    ),
  },
};
for (const [path, text] of [
  ["README.md", lines("# backend-api", "", "Session helpers and the login flow.", "")],
  ["src/b.ts", lines("export const b = 1;", "")],
  ["src/lib/util.ts", lines("export function login() {}", "export const SESSION = 1;", "")],
  ["package.json", lines("{", '  "name": "backend-api"', "}", "")],
] as const) {
  files[path] = { index: text, worktree: text };
}
const head: Record<string, string> = Object.fromEntries(
  Object.entries(files).map(([p, f]) => [p, f.index]),
);
const created = new Set<string>();
const version = (text: string) =>
  `v${[...text].reduce((h, ch) => (h * 31 + ch.charCodeAt(0)) | 0, 7)}`;

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
  file_contents: (a) => {
    const r = a.request as DiffRequest;
    const f = files[r.path];
    if (!f) return { original: null, modified: null, binary: false, tooLarge: false };
    return r.side === "staged"
      ? { original: head[r.path] ?? null, modified: f.index, binary: false, tooLarge: false }
      : { original: f.index, modified: f.worktree, binary: false, tooLarge: false };
  },
  repo_files: () => ({
    tracked: Object.keys(files)
      .filter((p) => !created.has(p))
      .sort(),
    untracked: [...created].sort(),
    folders: [],
    truncated: false,
  }),
  repo_search: (a) => {
    const q = String(a.query).toLowerCase();
    const matches = Object.entries(files).flatMap(([path, f]) =>
      f.worktree.split(String.fromCharCode(10)).flatMap((text, i) => {
        const at = text.toLowerCase().indexOf(q);
        return at < 0 ? [] : [{ path, line: i + 1, column: at + 1, text, textStart: 0 }];
      }),
    );
    return { matches, truncated: false };
  },
  file_create: (a) => {
    const path = a.path as string;
    if (files[path]) throw new Error(`${path} already exists`);
    files[path] = { index: "", worktree: "" };
    created.add(path);
    return null;
  },
  folder_create: () => null,
  path_rename: (a) => {
    const from = a.from as string;
    const to = a.to as string;
    const f = files[from];
    if (!f) throw new Error(`${from} does not exist`);
    files[to] = f;
    delete files[from];
    if (created.delete(from)) created.add(to);
    return null;
  },
  path_delete: (a) => {
    delete files[a.path as string];
    created.delete(a.path as string);
    return null;
  },
  index_stage_text: (a) => {
    const f = files[a.path as string];
    if (!f || a.expected !== f.index) return false;
    f.index = a.text as string;
    return true;
  },
  file_at_commit: (a) => {
    const text = head[a.path as string];
    return {
      exists: text !== undefined,
      size: text?.length ?? 0,
      text: text ?? null,
      binary: false,
      tooLarge: false,
      lfs: null,
      image: false,
    };
  },
  blame_file: (a) => {
    const f = files[a.path as string];
    const text = f?.worktree ?? "";
    const all = text.split(String.fromCharCode(10));
    const base = (f?.index ?? "").split(String.fromCharCode(10));
    return {
      path: a.path,
      rev: null,
      lines: all,
      lineCommits: all.map((l) => (base.includes(l) ? (l.length % 2 ? 0 : 1) : 2)),
      origLines: all.map((_, i) => i + 1),
      commits: [
        {
          oid: hash(1),
          author: "Lan",
          email: "lan@acme.dev",
          time: 1_760_000_000,
          summary: "feat: add login",
          filename: a.path,
          previous: null,
          previousPath: null,
          boundary: false,
          uncommitted: false,
        },
        {
          oid: hash(2),
          author: "Hoang",
          email: "hoang@acme.dev",
          time: 1_750_000_000,
          summary: "fix: token refresh",
          filename: a.path,
          previous: null,
          previousPath: null,
          boundary: false,
          uncommitted: false,
        },
        {
          oid: "0".repeat(40),
          author: "Not Committed Yet",
          email: "",
          time: 0,
          summary: "",
          filename: a.path,
          previous: null,
          previousPath: null,
          boundary: false,
          uncommitted: true,
        },
      ],
      tooLarge: false,
      binary: false,
    };
  },
  worktree_file_read: (a) => {
    const f = files[a.path as string];
    return {
      text: f?.worktree ?? null,
      exists: !!f,
      binary: false,
      tooLarge: false,
      notUtf8: false,
      symlink: false,
      version: f ? version(f.worktree) : "missing",
    };
  },
  worktree_file_write: (a) => {
    const f = files[a.path as string];
    if (!f) throw new Error("no such file");
    if (a.expected != null && a.expected !== version(f.worktree)) {
      return { saved: false, version: version(f.worktree) };
    }
    f.worktree = a.text as string;
    return { saved: true, version: version(f.worktree) };
  },
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
  // `&accounts=gitlab`: fake accounts on top of the sample repos (see mockAccounts.ts).
  const extra =
    params.get("accounts") === "gitlab"
      ? accountHandlers((cmd, args) => handlers[cmd]?.(args))
      : {};
  if (params.has("ai")) Object.assign(extra, aiHandlers());
  mockIPC(
    (cmd, args) => {
      const handler = extra[cmd] ?? handlers[cmd];
      if (handler) return handler((args ?? {}) as Record<string, unknown>);
      if (!window.__mockMisses?.includes(cmd)) window.__mockMisses?.push(cmd);
      return null;
    },
    { shouldMockEvents: true },
  );
}
