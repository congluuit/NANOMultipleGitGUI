/**
 * Recognises git problems the app can explain in plain words — and often fix in one click —
 * from git's (English) error output.
 */
export type Problem =
  /** `Unable to create '….lock': File exists`: another git is running, or one crashed. */
  | { kind: "lock"; path: string; file: string }
  /** `.git/index` is damaged (power loss, disk full, a crash while git wrote it). */
  | { kind: "indexCorrupt" }
  /** Windows: a path longer than 260 characters. */
  | { kind: "longPaths" }
  /** A big HTTPS push dropped by the server / a proxy (HTTP 413, "RPC failed"). */
  | { kind: "postBuffer" }
  /** The folder belongs to another user account (`safe.directory`). */
  | { kind: "dubiousOwnership"; path: string }
  /** The remote has its own first commit (README created on GitHub / GitLab). */
  | { kind: "unrelatedHistories" }
  /** No author name / e-mail configured. */
  | { kind: "identityUnknown" }
  /** Pushing a branch that has no commit yet. */
  | { kind: "noCommits" }
  /** A file is open in another program (Windows) or read-only. */
  | { kind: "fileBusy" };

export type ProblemKind = Problem["kind"];

const LOCK = /unable to create '([^'\n]+?\.lock)': file exists/i;
const CONFIG_LOCK = /could not lock config file ([^\n:]+?): file exists/i;
const DUBIOUS = /detected dubious ownership in repository at '([^'\n]+)'/i;
/** Our own wording (`GitError::DubiousOwnership`) when opening such a folder. */
const DUBIOUS_APP = /\(safe\.directory\): ([^\n]+)/i;

const fileName = (path: string) => path.split(/[\\/]/).pop() || path;

function lockProblem(path: string): Problem {
  return { kind: "lock", path, file: fileName(path) };
}

export function detectProblem(text: string): Problem | null {
  const lock = LOCK.exec(text)?.[1];
  if (lock) return lockProblem(lock);
  const configLock = CONFIG_LOCK.exec(text)?.[1];
  if (configLock) return lockProblem(`${configLock.trim()}.lock`);
  const dubious = (DUBIOUS.exec(text) ?? DUBIOUS_APP.exec(text))?.[1];
  if (dubious) return { kind: "dubiousOwnership", path: dubious.trim() };

  const s = text.toLowerCase();
  const any = (parts: string[]) => parts.some((p) => s.includes(p));
  if (
    any([
      "index file smaller than expected",
      "bad index file sha1 signature",
      "index file corrupt",
      "bad signature 0x",
    ])
  ) {
    return { kind: "indexCorrupt" };
  }
  if (s.includes("filename too long")) return { kind: "longPaths" };
  if (s.includes("refusing to merge unrelated histories")) return { kind: "unrelatedHistories" };
  if (
    any([
      "please tell me who you are",
      "author identity unknown",
      "committer identity unknown",
      "unable to auto-detect email address",
      "empty ident name",
    ])
  ) {
    return { kind: "identityUnknown" };
  }
  if (/src refspec .+ does not match any/.test(s)) return { kind: "noCommits" };
  if (
    any([
      "rpc failed",
      "http 413",
      "returned error: 413",
      "unexpected disconnect while reading sideband",
      "send-pack: unexpected disconnect",
    ])
  ) {
    return { kind: "postBuffer" };
  }
  if (
    !s.includes("publickey") &&
    any([
      "unable to unlink",
      "device or resource busy",
      "deletion of directory",
      "unable to create file",
      "): permission denied",
    ])
  ) {
    return { kind: "fileBusy" };
  }
  return null;
}
