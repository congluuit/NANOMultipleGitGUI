import type { FileStatus } from "@/ipc";

const LETTERS: Record<FileStatus, string> = {
  added: "A",
  modified: "M",
  deleted: "D",
  renamed: "R",
  copied: "C",
  typeChanged: "T",
  unknown: "?",
};

export const fileStatusLetter = (status: FileStatus) => LETTERS[status];

/** `src/auth/login.ts` → `{ dir: "src/auth/", name: "login.ts" }`. */
export function splitPath(path: string): { dir: string; name: string } {
  const i = path.lastIndexOf("/");
  return i < 0 ? { dir: "", name: path } : { dir: path.slice(0, i + 1), name: path.slice(i + 1) };
}
