import type { Worktree } from "@/ipc";

/** Last segment of a folder path (any separator). */
export function folderName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** Folder that contains `path`. */
export function parentFolder(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const i = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  return i > 0 ? trimmed.slice(0, i) : trimmed;
}

/** Branch name made safe for a folder name (`feature/login` → `feature-login`). */
export function slug(branch: string): string {
  return branch
    .trim()
    .replace(/[\\/:*?"<>|\s]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/** Default folder for a new worktree: next to the main one, `<repo>-<branch>`. */
export function suggestFolder(mainPath: string, branch: string): string {
  const sep = mainPath.includes("\\") && !mainPath.includes("/") ? "\\" : "/";
  const name = slug(branch);
  return `${parentFolder(mainPath)}${sep}${folderName(mainPath)}${name ? `-${name}` : ""}`;
}

/** Same folder (case-insensitive, separator-agnostic: Windows paths). */
export function samePath(a: string, b: string): boolean {
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
  return norm(a) === norm(b);
}

/** What a worktree has checked out, for the row label. */
export function worktreeLabel(w: Worktree): string {
  if (w.branch) return w.branch;
  return w.head ? w.head.slice(0, 8) : "";
}

/** Branches checked out in some worktree, mapped to that worktree's folder. */
export function checkedOutBranches(list: readonly Worktree[]): Map<string, string> {
  const map = new Map<string, string>();
  for (const w of list) if (w.branch) map.set(w.branch, w.path);
  return map;
}
