/**
 * Dev-only fake data for the tools of the web mock: all repositories, reflog, statistics,
 * sparse checkout, patches, bisect (`&bisect` starts with one running) and the AI review.
 */
import type {
  AiReview,
  BisectState,
  PatchInfo,
  ReflogEntry,
  RepoOverview,
  RepoStats,
  SparseState,
} from "@/ipc";

type Handler = (args: Record<string, unknown>) => unknown;

const NOW = Math.floor(Date.now() / 1000);
const oid = (i: number) =>
  (i.toString(16).padStart(7, "0") + "a1b2c3d4e5f60718293a4b5c6d7e8f901").slice(0, 40);
const done = <T>(value: T) => ({ status: "done", value });
const later = <T>(value: T, ms = 700) =>
  new Promise<T>((resolve) => setTimeout(() => resolve(value), ms));

const overview: RepoOverview[] = [
  {
    repoId: 1,
    name: "backend-api",
    path: "D:/Work/backend-api",
    missing: false,
    branch: "main",
    upstream: "origin/main",
    ahead: 1,
    behind: 3,
    changes: 0,
    conflicts: 0,
    stashes: 2,
    operation: null,
    error: null,
  },
  {
    repoId: 2,
    name: "game-assets",
    path: "D:/Work/game-assets",
    missing: false,
    branch: "feature/new-map",
    upstream: "origin/feature/new-map",
    ahead: 0,
    behind: 5,
    changes: 12,
    conflicts: 0,
    stashes: 0,
    operation: null,
    error: null,
  },
  {
    repoId: 3,
    name: "infrastructure-terraform",
    path: "D:/Work/infrastructure-terraform",
    missing: false,
    branch: "main",
    upstream: "origin/main",
    ahead: 0,
    behind: 0,
    changes: 0,
    conflicts: 0,
    stashes: 0,
    operation: null,
    error: null,
  },
];

const reflog: ReflogEntry[] = [
  ["checkout", "moving from feature/login to main", "Merge pull request #41"],
  ["commit", "Add login form validation", "Add login form validation"],
  ["commit (amend)", "Fix token refresh race", "Fix token refresh race"],
  ["reset", "moving to HEAD~1", "Bump dependencies"],
  ["rebase (finish)", "returning to refs/heads/feature/login", "Add login form"],
  ["pull", "Fast-forward", "Release 1.2.0"],
  ["checkout", "moving from main to feature/login", "Add login form"],
].map(([action, message, subject], index) => ({
  index,
  oid: oid(index + 1),
  action: action!,
  message: message!,
  subject: subject!,
  time: NOW - (index + 1) * 5400,
}));

const stats: RepoStats = {
  totalCommits: 1284,
  authorCount: 7,
  authors: [
    { name: "Hoang Nguyen", email: "hoang@acme.vn", commits: 612 },
    { name: "Linh Tran", email: "linh@acme.vn", commits: 344 },
    { name: "Pret", email: "pret@acme.vn", commits: 201 },
    { name: "dependabot[bot]", email: "bot@github.com", commits: 127 },
  ],
  weeks: Array.from({ length: 52 }, (_, i) => Math.round(6 + 5 * Math.sin(i / 3) + (i % 7))),
  lastWeekStart: NOW - 7 * 24 * 3600,
  files: [
    { path: "src/auth/session.ts", changes: 48 },
    { path: "src/api/routes.ts", changes: 37 },
    { path: "package.json", changes: 29 },
    { path: "src/auth/token.ts", changes: 21 },
    { path: "README.md", changes: 9 },
  ],
  firstCommit: NOW - 900 * 24 * 3600,
  lastCommit: NOW - 3600,
};

let sparse: SparseState = {
  enabled: false,
  folders: [],
  available: ["assets", "docs", "scripts", "src", "tests"],
};

const patch: PatchInfo = {
  commits: 2,
  subjects: ["Add login form validation", "Fix token refresh race"],
  files: [
    { path: "src/auth/login.ts", added: 42, removed: 7, binary: false },
    { path: "src/auth/token.ts", added: 9, removed: 3, binary: false },
    { path: "assets/logo.png", added: 0, removed: 0, binary: true },
  ],
  applies: false,
  problem: "error: patch failed: src/auth/token.ts:12",
};

const idle: BisectState = {
  active: false,
  current: null,
  currentSubject: null,
  bad: null,
  good: [],
  remaining: 0,
  firstBad: null,
  firstBadSubject: null,
  start: null,
};

const review: AiReview = {
  text: [
    "1. src/auth/token.ts, dòng 48: refreshToken() không kiểm tra token rỗng trước khi gọi API. Thêm kiểm tra và trả về lỗi sớm.",
    "2. src/auth/login.ts, dòng 12: còn console.log(password) dùng để debug. Xóa trước khi commit.",
    "3. src/api/routes.ts: thêm route /admin nhưng chưa có test đi kèm.",
  ].join(String.fromCharCode(10)),
  source: "staged",
  files: 3,
  truncated: false,
  provider: "ollama",
  model: "qwen2.5-coder:3b",
};

export function toolHandlers(params: URLSearchParams): Record<string, Handler> {
  let bisect: BisectState = params.has("bisect")
    ? {
        ...idle,
        active: true,
        current: oid(40),
        currentSubject: "Refactor session storage",
        bad: oid(1),
        good: [oid(90)],
        remaining: 23,
        start: "main",
      }
    : idle;
  const step = () => {
    bisect =
      bisect.remaining <= 3
        ? { ...bisect, remaining: 1, firstBad: oid(33), firstBadSubject: "Cache tokens in memory" }
        : {
            ...bisect,
            remaining: Math.ceil(bisect.remaining / 2),
            current: oid(bisect.remaining),
            currentSubject: "Update session handling",
          };
    return done(bisect);
  };
  return {
    workspace_overview: () => later(overview, 300),
    reflog_list: () => later(reflog, 300),
    reflog_recent_branches: () => ["feature/login", "fix/token-refresh"],
    repo_stats: () => later(stats, 400),
    sparse_state: () => sparse,
    sparse_set: (a) => {
      sparse = { ...sparse, enabled: true, folders: a.folders as string[] };
      return done(sparse);
    },
    sparse_disable: () => {
      sparse = { ...sparse, enabled: false, folders: [] };
      return done(sparse);
    },
    patch_inspect: () => later(done(patch), 300),
    patch_apply: () => done({ conflicts: [], pending: null, head: oid(1) }),
    patch_save_commit: () => done(null),
    patch_save_changes: () => done(null),
    bisect_state: () => bisect,
    bisect_start: (a) => {
      bisect = {
        ...idle,
        active: true,
        current: oid(40),
        currentSubject: "Refactor session storage",
        bad: String(a.bad),
        good: [String(a.good)],
        remaining: 23,
        start: "main",
      };
      return done(bisect);
    },
    bisect_mark: step,
    bisect_reset: () => {
      bisect = idle;
      return done(null);
    },
    ai_review: () => later(review, 1500),
    server_limits: () => ({}),
    server_limit_learn: () => ({}),
    os_notify: () => false,
  };
}
