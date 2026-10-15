import type { Submodule, SubmoduleAccount, SubmoduleAccountSource, SubmoduleInfo } from "@/ipc";

export type SubmoduleTone = "ok" | "warning" | "danger" | "muted";
export type SubmoduleStatusKey =
  "uninitialized" | "upToDate" | "outOfDate" | "dirty" | "conflicted";

/** i18n key suffix (`submodule.state.<x>`) and colour of a submodule's state. */
export function submoduleStatus(s: Submodule): { key: SubmoduleStatusKey; tone: SubmoduleTone } {
  switch (s.state) {
    case "uninitialized":
      return { key: "uninitialized", tone: "muted" };
    case "conflicted":
      return { key: "conflicted", tone: "danger" };
    case "outOfDate":
      return { key: "outOfDate", tone: "warning" };
    default:
      return s.modified || s.untracked
        ? { key: "dirty", tone: "warning" }
        : { key: "upToDate", tone: "ok" };
  }
}

/** Last path segment (`lib/my sub` → `my sub`). */
export function baseName(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** Short commit id. */
export const short = (oid: string | null | undefined) => (oid ? oid.slice(0, 8) : "–");

/** Absolute path of a submodule from the repository root (any separator style). */
export function absolutePath(root: string, path: string): string {
  const sep = root.includes("\\") && !root.includes("/") ? "\\" : "/";
  const base = root.replace(/[\\/]+$/, "");
  return `${base}${sep}${sep === "\\" ? path.replaceAll("/", "\\") : path}`;
}

/** Submodules that "Update all" would initialise (not checked out yet). */
export function needsInit(list: readonly SubmoduleInfo[]): number {
  return list.filter((s) => s.submodule.state === "uninitialized").length;
}

/** Submodules whose checkout differs from the commit recorded by the parent. */
export function outOfDate(list: readonly SubmoduleInfo[]): number {
  return list.filter((s) => s.submodule.state === "outOfDate").length;
}

/** Default folder for a new submodule from its URL (`…/team/lib.git` → `lib`). */
export function pathFromUrl(url: string): string {
  const trimmed = url.trim().replace(/[\\/]+$/, "");
  const last = trimmed.split(/[\\/:]/).pop() ?? "";
  return last.replace(/\.git$/i, "");
}

/** i18n key of the account note for a submodule (`submodule.account.<x>`). */
export function accountNoteKey(a: SubmoduleAccount): SubmoduleAccountSource | "mismatch" {
  if (a.hostMismatch) return "mismatch";
  return a.source;
}
